// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One generation-2 round on a server that seats native authoring: a fresh round under the
//! operator's seat, or a zero-call replay of a round this server kept.
//!
//! A fresh round takes a compile slot, then a place for the round it may leave, before any
//! provider call; both live inside the blocking work, so a caller that disconnected or timed
//! out never frees a slot or a place still in use. Its deadline is ABSOLUTE, fixed when it is
//! admitted: a round that starts late (a busy blocking pool) never gets a fresh window, and a
//! round that must already stop never begins. The stop — that deadline, or the server stopping
//! — is checked before the work begins, raced against it, and checked again before every
//! provider call; an outcome that arrives once the round must stop is never answered or kept.
//! The work revalidates the pinned snapshot, composes the pack for the request's intent (host
//! paths stripped from the recorded identity), and calls the seat's provider through a gate
//! that makes at most `1 + repairs` logical calls (the provider transport may resend one after
//! a 429, 503 or 529) and hands the core only fixed, safe failure reasons. The answer is the
//! core's own document; a document that carries a withheld value is refused whole. No job,
//! run, approval, trace, file or permission is created.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use bytes::Bytes;
use hyper::header::{CACHE_CONTROL, CONTENT_TYPE, HeaderName, HeaderValue};
use hyper::{Response, StatusCode};
use nika_kernel::ai::provider::{InferRequest, InferResponse, ProviderError, ProviderInferDyn};
use nika_onboard::compile::{
    AuthoringPolicy, Cognition, CompileOutcome, NativeMode, Strategy, compile_with_cognition,
    outcome_document, revise_intent,
};
use nika_providers::ProviderRegistry;
use serde_json::value::RawValue;
use tokio::time::Instant;

use super::super::AppState;
use super::super::error::{ApiError, ResponseBody, ServerError, once_body};
use super::native::{ContextChanged, Seat};
use super::replay::Reservation;
use super::v2::{self, Action, Bounds, CLARIFICATION, Input};

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

/// When a round must stop: at the absolute deadline it was admitted with, or when its server
/// stops.
#[derive(Clone)]
struct Stop {
    deadline: Instant,
    halt: tokio::sync::watch::Receiver<bool>,
}

impl Stop {
    /// Why the round must stop now, if it must.
    fn now(&self) -> Option<Refusal> {
        if *self.halt.borrow() {
            Some(Refusal::Stopping)
        } else if Instant::now() >= self.deadline {
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
            () = tokio::time::sleep_until(self.deadline) => Refusal::Deadline,
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
    match request.action {
        Action::Author(bounds) => fresh(&state, seat, request.input, request.answers, bounds).await,
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

async fn fresh(
    state: &Arc<AppState>,
    seat: Arc<Seat>,
    input: Input,
    answers: Answers,
    bounds: Bounds,
) -> Response<ResponseBody> {
    let Ok(permit) = Arc::clone(&state.compile_slots).try_acquire_owned() else {
        return super::busy().into_response();
    };
    let Some(place) = seat.replays.reserve() else {
        return replay_capacity().into_response();
    };
    // One absolute deadline, fixed at admission: a late start never gets a fresh window.
    let stop = Stop {
        deadline: Instant::now() + bounds.deadline,
        halt: seat.halted(),
    };
    let handoff = stop.deadline + HANDOFF;
    #[cfg(test)]
    let before_compile = Arc::clone(&state.before_compile);
    let runtime = tokio::runtime::Handle::current();
    let work = tokio::task::spawn_blocking(move || {
        // The slot and the place live exactly as long as the owned work.
        let _permit = permit;
        #[cfg(test)]
        super::super::test_support::before_compile(&before_compile);
        let authored = run(
            &runtime,
            &stop,
            author(&seat, &input, &answers, bounds, &stop),
        );
        conclude(&seat, authored, input, place)
    });
    // Past the deadline and its handoff the caller hears « stopped »: the work, if it has not
    // begun, never will (its stop is absolute), and nothing it produces is answered or kept.
    match tokio::time::timeout_at(handoff, work).await {
        Ok(Ok(response)) => response,
        Ok(Err(_)) => ApiError::internal().into_response(),
        Err(_) => deadline_exceeded().into_response(),
    }
}

/// The fresh round itself: the core under the seat's policy, the pack beside the card.
async fn author(
    seat: &Seat,
    input: &Input,
    answers: &Answers,
    bounds: Bounds,
    stop: &Stop,
) -> Result<CompileOutcome, Refusal> {
    let mut request = input.request(answers).with_authoring_policy(
        AuthoringPolicy::new(seat.model.as_str(), bounds.max_tokens, bounds.call_timeout)
            .with_native(NativeMode::Only)
            .with_repairs(bounds.repairs)
            .with_samples(1),
    );
    if let Some((snapshot, exclude)) = seat.context()? {
        let intent = match input {
            Input::Create { intent, .. } => Some(intent.clone()),
            Input::Revise { .. } => revise_intent(&request),
            Input::Constant { .. } => None,
        };
        if let Some(intent) = intent.filter(|intent| !intent.trim().is_empty()) {
            let mut pack = snapshot
                .pack(&intent, exclude)
                .map_err(|_| Refusal::Context)?;
            public_identity(&mut pack.identity);
            request = request.with_authoring_knowledge(pack);
        }
    }
    let http = nika_runtime::compose::provider_http().map_err(|_| Refusal::Machinery)?;
    let provider = ProviderRegistry::new(Arc::new(http), seat.providers.clone())
        .resolve(&seat.model)
        .map_err(|_| Refusal::Machinery)?;
    let gate = Gate {
        provider,
        calls: AtomicU32::new(bounds.repairs.saturating_add(1)),
        stop: stop.clone(),
    };
    let cognition = Cognition {
        provider: Some(&gate),
        seat: None,
    };
    let mut outcome = Box::pin(compile_with_cognition(&request, cognition))
        .await
        .map_err(|_| Refusal::Machinery)?;
    if let Some(receipt) = outcome.provenance.authoring.as_mut() {
        receipt.backend = Some(serde_json::json!({
            "kind": "direct_api",
            "provider": seat.provider,
            "cost_basis": "measured_by_tokens_at_catalog_price",
        }));
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
        deadline,
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

/// The ONE provider this request may reach, behind its stop and its call budget: once the
/// round must stop, or past `1 + repairs` logical calls, nothing is sent; every failure reaches
/// the core as a fixed reason — never the provider's own text, which can carry an endpoint, a
/// request body or a credential. One logical call is one `infer`: the provider transport may
/// resend its request after a 429, 503 or 529 inside it.
struct Gate<P> {
    provider: P,
    calls: AtomicU32,
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
        if self
            .calls
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_err()
        {
            return Err(ProviderError::Other {
                reason: "this request's authoring calls are spent; nothing was sent".to_owned(),
            });
        }
        self.provider
            .infer(request)
            .await
            .map_err(|error| ProviderError::Other {
                reason: safe_reason(&error),
            })
    }
}

fn safe_reason(error: &ProviderError) -> String {
    match error {
        ProviderError::HttpResponse { details } => {
            format!("the authoring provider answered HTTP {}", details.status())
        }
        ProviderError::Api { status, .. } => {
            format!("the authoring provider answered HTTP {status}")
        }
        ProviderError::RateLimited { .. } => "the authoring provider rate-limited the call".to_owned(),
        ProviderError::AuthFailed { .. } => {
            "the authoring provider refused the operator's credentials".to_owned()
        }
        ProviderError::ModelNotFound { .. } => {
            "the authoring provider does not serve the seated model".to_owned()
        }
        ProviderError::Connection { .. } => {
            "the connection to the authoring provider failed or was cut; the call may still be billed"
                .to_owned()
        }
        _ => "the authoring provider call failed".to_owned(),
    }
}

/// The snapshot identity as an answer may carry it: every hash, count and selection, no host
/// path (the snapshot directory, the files root).
fn public_identity(identity: &mut serde_json::Value) {
    if let Some(identity) = identity.as_object_mut() {
        identity.remove("dir");
        if let Some(verification) = identity
            .get_mut("verification")
            .and_then(serde_json::Value::as_object_mut)
        {
            verification.remove("files_root");
        }
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
        "every kept answer round is in use; nothing was authored or spent — retry later",
    )
}

fn replay_unavailable() -> ApiError {
    ApiError::new(
        StatusCode::CONFLICT,
        "compile_replay_unavailable",
        "this server keeps no round under that token (unknown, expired, or kept by another server run); author again with explicitProvider",
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
