// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One generation-2 round on a server that seats native authoring: a fresh round under the
//! operator's seat, or a zero-call replay of a round this server kept.
//!
//! A fresh round takes a compile slot, then a place for the round it may leave, before any
//! provider call; both live inside the blocking work, so a caller that disconnected or timed
//! out never frees a slot or a place still in use. An explicit deadline is absolute, fixed at
//! admission: a round that starts late (a busy blocking pool) never gets a fresh window, and a
//! round that must already stop never begins. The stop — that deadline, or the server stopping
//! — is checked before the work begins, raced against it, and checked again before every
//! provider call; an outcome that arrives once the round must stop is never answered or kept.
//! The work revalidates the pinned snapshot, composes the pack for the request's intent (host
//! paths stripped from the recorded identity), reads the caller's admitted observation, tries
//! candidates on its trial inputs, has a seated decision model judge, and calls the seat's
//! provider through a gate
//! that counts both model invocations and physical requests under the operator's explicit
//! authority, with no redirect or uncounted resend, and hands the core fixed safe reasons. The answer is the
//! core's own document; a document that carries a withheld value is refused whole. No job,
//! run, approval, trace, file or permission is created.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderName, HeaderValue};
use hyper::{Response, StatusCode};
use nika_kernel::ai::provider::{InferRequest, InferResponse, ProviderError, ProviderInferDyn};
use nika_onboard::compile::authority::{Seat as CountedSeat, Wire};
use nika_onboard::compile::{
    Cognition, CompileOutcome, Strategy, compile_with_cognition_composed, outcome_document,
    revise_intent,
};
use nika_onboard::remote_door::trial::TrialProject;
use nika_providers::ProviderRegistry;
use serde_json::value::RawValue;
use tokio::time::Instant;

use super::super::AppState;
use super::super::error::{ApiError, ResponseBody, ServerError, once_body};
use super::native::{ContextChanged, Seat};
use super::replay::Reservation;
use super::v2::{self, Action, Bounds, CLARIFICATION, Input};
use nika_onboard::remote_door::decision::Judge;

/// The header that carries a kept round's token (S09). Never logged, never reflected.
pub(super) const REPLAY_HEADER: HeaderName = HeaderName::from_static("nika-compile-replay");
/// How long past its own deadline a handler waits for the blocking work to hand its answer.
const HANDOFF: Duration = Duration::from_secs(5);

type Answers = BTreeMap<String, Box<RawValue>>;

/// Why a round ended without the core's document.
enum Refusal {
    /// The pinned snapshot no longer reads as pinned: refused before any provider call.
    Context,
    /// The round's deadline passed: its work stopped, or never began.
    Deadline,
    /// The server is stopping: the round stopped with it.
    Stopping,
    /// The compiler or the provider transport could not run.
    Machinery,
}

impl From<ContextChanged> for Refusal {
    fn from(_: ContextChanged) -> Self {
        Self::Context
    }
}

/// When a round must stop: at any explicit absolute deadline it was admitted with, or when
/// its server stops.
#[derive(Clone)]
struct Stop {
    deadline: Option<Instant>,
    halt: tokio::sync::watch::Receiver<bool>,
}

impl Stop {
    /// Why the round must stop now, if it must.
    fn now(&self) -> Option<Refusal> {
        if *self.halt.borrow() {
            Some(Refusal::Stopping)
        } else if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            Some(Refusal::Deadline)
        } else {
            None
        }
    }

    /// Ready the moment the round must stop, and at once when it already must: a first poll
    /// never lets expired work begin.
    async fn reached(mut self) -> Refusal {
        if let Some(refusal) = self.now() {
            return refusal;
        }
        tokio::select! {
            () = async {
                match self.deadline {
                    Some(deadline) => tokio::time::sleep_until(deadline).await,
                    None => std::future::pending().await,
                }
            } => Refusal::Deadline,
            _ = self.halt.wait_for(|halted| *halted) => Refusal::Stopping,
        }
    }
}

/// Run a round unless it must already stop, raced against its stop (which wins a tie); an
/// outcome that arrives once the round must stop is refused, never answered or kept.
fn run<F>(
    runtime: &tokio::runtime::Handle,
    stop: &Stop,
    round: F,
) -> Result<CompileOutcome, Refusal>
where
    F: Future<Output = Result<CompileOutcome, Refusal>>,
{
    if let Some(refusal) = stop.now() {
        return Err(refusal);
    }
    let authored = runtime.block_on(async {
        tokio::select! {
            biased;
            refusal = stop.clone().reached() => Err(refusal),
            authored = round => authored,
        }
    });
    stop.now().map_or(authored, Err)
}

/// A stopping server stops every native round of its seat, then waits — within `grace` — until
/// every compile slot is free: each slot belongs to a round's work, so a free slot is a round
/// that has settled. A server without a seat has nothing to settle.
pub(in crate::server) async fn settle(
    state: &AppState,
    grace: Duration,
) -> Result<(), ServerError> {
    let Some(seat) = &state.native else {
        return Ok(());
    };
    seat.halt();
    let slots = u32::try_from(state.limits.max_compile_requests()).unwrap_or(u32::MAX);
    match tokio::time::timeout(grace, state.compile_slots.acquire_many(slots)).await {
        Ok(Ok(_settled)) => Ok(()),
        Ok(Err(_)) | Err(_) => Err(ServerError::ShutdownTimeout),
    }
}

/// A generation-2 body, on a server that seats native authoring. `deadline` is the request's
/// own (the replay's bound); a fresh round is bounded by its seat deadline instead.
pub(super) async fn handle(
    body: &Bytes,
    state: Arc<AppState>,
    deadline: Instant,
) -> Response<ResponseBody> {
    let Some(seat) = state.native.clone() else {
        return ApiError::internal().into_response();
    };
    let request = match v2::parse(body, seat.bounds) {
        Ok(request) => request,
        Err(error) => return error.into_response(),
    };
    if seat.trials.is_none() && request.input.trial().is_some() {
        let refusal = nika_onboard::compile::remote::Refusal::TrialShape;
        return v2::refused(refusal).into_response();
    }
    match request.action {
        Action::Author(bounds) => {
            let round = (request.input, request.answers, bounds);
            fresh(&state, seat, round, None).await
        }
        Action::Judge(token, bounds) => {
            let Some(kept) = seat.replays.get(&token) else {
                return replay_unavailable().into_response();
            };
            if kept.input != request.input {
                return input_changed().into_response();
            }
            let round = (request.input, request.answers, bounds);
            fresh(&state, seat, round, Some((token, kept.plan.clone()))).await
        }
        Action::Replay(token) => {
            replay(
                &state,
                seat,
                request.input,
                request.answers,
                &token,
                deadline,
            )
            .await
        }
    }
}

/// A round under the operator's seat: a fresh authoring round, or, with `kept` (a kept round's
/// token and plan), that plan replayed with these answers and judged by the seat, with no
/// authoring call. A judged candidate the seat did not accept forgets that token.
async fn fresh(
    state: &Arc<AppState>,
    seat: Arc<Seat>,
    (input, answers, bounds): (Input, Answers, Bounds),
    kept: Option<(String, serde_json::Value)>,
) -> Response<ResponseBody> {
    let Ok(permit) = Arc::clone(&state.compile_slots).try_acquire_owned() else {
        return super::busy().into_response();
    };
    let Some(place) = seat.replays.reserve() else {
        return replay_capacity().into_response();
    };
    // One absolute deadline, fixed at admission: a late start never gets a fresh window.
    let deadline = match bounds.deadline {
        Some(wait) => match Instant::now().checked_add(wait) {
            Some(deadline) => Some(deadline),
            None => return super::limit().into_response(),
        },
        None => None,
    };
    let stop = Stop {
        deadline,
        halt: seat.halted(),
    };
    let handoff = deadline.map(|deadline| deadline.checked_add(HANDOFF).unwrap_or(deadline));
    #[cfg(test)]
    let before_compile = Arc::clone(&state.before_compile);
    let runtime = tokio::runtime::Handle::current();
    let work = tokio::task::spawn_blocking(move || {
        // The slot and the place live exactly as long as the owned work.
        let _permit = permit;
        #[cfg(test)]
        super::super::test_support::before_compile(&before_compile);
        let (judged, plan) = kept.unzip();
        let authored = run(
            &runtime,
            &stop,
            author(&seat, (&input, &answers, plan.as_ref()), bounds, &stop),
        );
        if let Some(judged) = judged.as_deref()
            && authored.as_ref().is_ok_and(held)
        {
            seat.replays.forget(judged);
        }
        conclude(&seat, authored, input, place)
    });
    // Past the deadline and its handoff the caller hears « stopped »: the work, if it has not
    // begun, never will (its stop is absolute), and nothing it produces is answered or kept.
    let joined = match handoff {
        Some(deadline) => tokio::time::timeout_at(deadline, work)
            .await
            .map_err(|_| ()),
        None => Ok(work.await),
    };
    match joined {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => ApiError::internal().into_response(),
        Err(()) => deadline_exceeded().into_response(),
    }
}

/// Whether the round's judge did not accept its candidate (the core's `verify_held` finding).
fn held(outcome: &CompileOutcome) -> bool {
    (outcome.diagnostics.iter()).any(|d| {
        d.kind == nika_onboard::compile::DiagnosticKind::Applied && d.target == "verify_held"
    })
}

/// The round itself: the core under the seat's policy, the pack beside the card; with a kept
/// `plan`, that plan replayed and judged, no pack read (no authoring call reads one).
async fn author(
    seat: &Seat,
    (input, answers, plan): (&Input, &Answers, Option<&serde_json::Value>),
    bounds: Bounds,
    stop: &Stop,
) -> Result<CompileOutcome, Refusal> {
    // The shared producer: the seat's default strategy and its reasoning effort (R4 B16).
    let policy = seat
        .authoring
        .policy(&seat.model, bounds.max_tokens, bounds.call_timeout);
    let policy = policy
        .map_err(|_| Refusal::Machinery)?
        .with_initial_max_tokens(bounds.initial_tokens.min(bounds.max_tokens));
    let policy = match bounds.repairs {
        Some(repairs) => policy.with_repairs(repairs),
        None => policy.with_unbounded_repairs(),
    };
    let mut request = input.request(answers).with_authoring_policy(policy);
    // The pinned snapshot, revalidated, kept for the compile: lent as the catalogue of this
    // request, its excluded corpus held out of the reach as it is out of the pack.
    let context = seat.context()?;
    if let Some(plan) = plan {
        request = request.with_plan(plan.clone());
    } else if let Some((snapshot, exclude)) = &context {
        let exclude = *exclude;
        let intent = match input {
            Input::Create { intent, .. } => Some(intent.clone()),
            Input::Revise { .. } => revise_intent(&request),
            // A structured constant states no intent to retrieve knowledge for.
            _ => None,
        };
        if let Some(intent) = intent.filter(|intent| !intent.trim().is_empty()) {
            let mut pack = snapshot
                .pack(&intent, exclude)
                .map_err(|_| Refusal::Context)?;
            nika_onboard::knowledge::redact_host_paths(&mut pack.identity);
            request = request.with_authoring_knowledge(pack);
        }
    }
    let authority = v2::authority(bounds).map_err(|_| Refusal::Machinery)?;
    let invocations = authority.envelope();
    let requests = authority.envelope();
    let http = nika_cli_host::compile::authoring_http_with_deadline(bounds.call_timeout)
        .map_err(|_| Refusal::Machinery)?;
    let wire = Wire::new(http, Arc::clone(&requests));
    let registry = ProviderRegistry::new(Arc::new(wire), seat.providers.clone());
    let mut backend = nika_cli_host::compile::authoring_backend(&registry, &seat.model);
    backend["provider"] = serde_json::json!(seat.provider);
    let provider = registry
        .resolve(&seat.model)
        .map_err(|_| Refusal::Machinery)?;
    let gate = Gate {
        provider: CountedSeat::new(provider, Arc::clone(&invocations)),
        stop: stop.clone(),
    };
    let decision = (seat.decision.as_ref()).map(|d| d.resolve(&seat.providers));
    let decision = decision
        .transpose()
        .map_err(|_| Refusal::Machinery)?
        .flatten();
    let decision = decision.map(|provider| Gate {
        provider,
        stop: stop.clone(),
    });
    let limits = (bounds.call_timeout, bounds.max_tokens);
    let judge = (seat.decision.as_ref())
        .and_then(|d| d.judge(decision.as_ref(), limits, seat.authoring.reasoning));
    let cognition = Cognition {
        provider: Some(&gate),
        seat: judge.as_ref().map(Judge::seat),
    };
    let room =
        (seat.trials.as_ref().zip(input.trial())).map(|(jq, trial)| TrialProject::room(trial, jq));
    let room = room.transpose().map_err(|_| Refusal::Machinery)?;
    let host =
        (room.as_ref()).map(|(_, room)| room as &dyn nika_onboard::compile::rehearse::Rehearse);
    let catalogue = (context.as_ref()).map(|(snapshot, exclude)| snapshot.catalogue(*exclude));
    let lent = (catalogue.as_ref())
        .map(|catalogue| catalogue as &dyn nika_onboard::knowledge::ComponentCatalog);
    let mut outcome = Box::pin(compile_with_cognition_composed(
        &request, cognition, host, lent,
    ))
    .await
    .map_err(|_| Refusal::Machinery)?;
    if let Some(receipt) = outcome.provenance.authoring.as_mut() {
        backend["usage_complete"] = serde_json::json!(
            nika_onboard::compile::authority::usage_complete(&receipt.context)
        );
        backend["observed_models"] = serde_json::json!(gate.provider.observed());
        backend["unreported_models"] = serde_json::json!(gate.provider.unreported());
        backend["authority"] = authority.record(&invocations, Some(&requests));
        if let Some(decision) = &seat.decision {
            let note = nika_cli_host::compile::decision_seat_note(&decision.model());
            decision.stamp(&mut backend, note);
        }
        receipt.backend = Some(backend);
    }
    Ok(outcome)
}

/// The answer of a fresh round: the core's document, its round kept when it left a native plan.
fn conclude(
    seat: &Seat,
    authored: Result<CompileOutcome, Refusal>,
    input: Input,
    place: Reservation,
) -> Response<ResponseBody> {
    let outcome = match authored {
        Ok(outcome) => outcome,
        Err(refusal) => return refused(&refusal),
    };
    let Ok(document) = serde_json::to_vec(&outcome_document(&outcome)) else {
        return ApiError::internal().into_response();
    };
    if seat.discloses(&document) {
        return disclosure_refused().into_response();
    }
    let token = (outcome.provenance.strategy == Some(Strategy::Native))
        .then_some(outcome.provenance.plan)
        .flatten()
        .and_then(|plan| place.keep(input, plan));
    respond(document, token.as_deref())
}

async fn replay(
    state: &Arc<AppState>,
    seat: Arc<Seat>,
    input: Input,
    answers: Answers,
    token: &str,
    deadline: Instant,
) -> Response<ResponseBody> {
    let Some(kept) = seat.replays.get(token) else {
        return replay_unavailable().into_response();
    };
    if kept.input != input || answers.contains_key(CLARIFICATION) {
        return input_changed().into_response();
    }
    let Ok(permit) = Arc::clone(&state.compile_slots).try_acquire_owned() else {
        return super::busy().into_response();
    };
    let stop = Stop {
        deadline: Some(deadline),
        halt: seat.halted(),
    };
    #[cfg(test)]
    let before_compile = Arc::clone(&state.before_compile);
    let work = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        #[cfg(test)]
        super::super::test_support::before_compile(&before_compile);
        // Zero calls, but no work for a caller already answered or a server stopping.
        if let Some(refusal) = stop.now() {
            return Err(refusal);
        }
        seat.context()?;
        let request = input.request(&answers).with_plan(kept.plan.clone());
        let outcome = nika_onboard::compile::compile(&request).map_err(|_| Refusal::Machinery)?;
        let document =
            serde_json::to_vec(&outcome_document(&outcome)).map_err(|_| Refusal::Machinery)?;
        Ok::<_, Refusal>((!seat.discloses(&document)).then_some(document))
    });
    match tokio::time::timeout_at(deadline, work).await {
        Ok(Ok(Ok(Some(document)))) => respond(document, None),
        Ok(Ok(Ok(None))) => disclosure_refused().into_response(),
        Ok(Ok(Err(refusal))) => refused(&refusal),
        Ok(Err(_)) => ApiError::internal().into_response(),
        Err(_) => super::super::route::request_timeout().into_response(),
    }
}

/// Stop a round before another invocation and sanitize provider failures. The enclosed
/// counted seat and wire enforce independent invocation and physical-request ceilings;
/// provider retries consume the request grant; redirects are disabled. Provider text never reaches the core:
/// it can contain an endpoint, request body or credential. A local refusal is the engine's own words
/// and keeps them: a spent grant names its remedy, an unqualified effort its route (R4 B17).
struct Gate<P> {
    provider: P,
    stop: Stop,
}

impl<P: ProviderInferDyn> ProviderInferDyn for Gate<P> {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if self.stop.now().is_some() {
            return Err(ProviderError::Other {
                reason: "this round stopped (its deadline passed or its server is stopping); nothing was sent"
                    .to_owned(),
            });
        }
        let result = self.provider.infer(request).await;
        result.map_err(|error| match error {
            ProviderError::AdmissionDenied { .. } => error,
            other => nika_cli_host::compile::redact_authoring_error(other, ""),
        })
    }
}

fn respond(document: Vec<u8>, token: Option<&str>) -> Response<ResponseBody> {
    let mut response = Response::new(once_body(document));
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if let Some(token) = token.and_then(|token| HeaderValue::from_str(token).ok()) {
        headers.insert(REPLAY_HEADER, token);
    }
    response
}

fn refused(refusal: &Refusal) -> Response<ResponseBody> {
    match refusal {
        Refusal::Context => context_changed(),
        Refusal::Deadline => deadline_exceeded(),
        Refusal::Stopping => stopping(),
        Refusal::Machinery => ApiError::internal(),
    }
    .into_response()
}

fn stopping() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "stopping",
        "the server is stopping: the native round stopped and nothing was kept",
    )
}

fn replay_capacity() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "compile_replay_capacity",
        "the operator's explicit kept-round capacity is full; nothing was authored or spent",
    )
}

fn replay_unavailable() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "compile_replay_unavailable",
        "this server run keeps no round under that token (unknown, past an explicit lifetime, or kept by another server run); author again with explicitProvider",
    )
}

fn input_changed() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "compile_replay_input_changed",
        "a replay repeats its round's exact input (mode, intent or source, change, original_intent, workflow_id) and never replaces the intent; author the new request with explicitProvider",
    )
}

fn context_changed() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "compile_context_changed",
        "the knowledge snapshot this server pinned at start no longer reads as pinned; nothing was sent to the provider — the operator restarts the server on the snapshot it means",
    )
}

fn deadline_exceeded() -> ApiError {
    ApiError::new(
        StatusCode::REQUEST_TIMEOUT,
        "compile_deadline_exceeded",
        "the native round reached its deadline: its work stopped or never began, no outcome was kept, and a provider call in flight may still be billed",
    )
}

fn disclosure_refused() -> ApiError {
    ApiError::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        "compile_disclosure_refused",
        "the outcome would carry a value this server withholds; it was refused whole",
    )
}
