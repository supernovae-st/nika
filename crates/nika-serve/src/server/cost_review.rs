// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The cost-review door (C6 · R4 111 · B12): `POST /v{1,2}/cost-reviews`,
//! `GET /v{1,2}/cost-reviews/{id}`, `POST /v{1,2}/cost-reviews/{id}/decision`, and the
//! one job admission that consumes an approved review. A review is a fresh,
//! single-use decision held in memory: it never creates a job, a run, a file or
//! an effect. Its authority comes from this server's startup composition
//! (`--cost-review`, the per-run ceiling), never from a request, and every
//! judgment is the shared host evaluator's (`nika_cli_host::run_cost`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Instant;

use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use nika_cli_host::run_cost::{self, ReviewedRun, RunCost, RunCostPlan};
use nika_dap::cost_journal::{self as custody, KeyAnswer, Replay, ReviewRefusal};
use nika_execution::{AdmittedExecution, ExecutionService, ExecutionSession};
use nika_providers::admission::{CapEvidence, CostHostEvidence, HardMonetaryCap};
use nika_service_execution::ExecutionAccessPlan;
use serde_json::{Value, json};

use super::AppState;
use super::error::{ResponseBody, json_error};
use super::route::{self, JobByName};
use crate::{Admission, IdempotencyKey, RequestDigest};

const POLICY: &str = "nika serve composition: no monetary policy source is supported by this build";
const OCCURRENCE: &str =
    "authenticated manual job request (JobOrigin::Manual), not a scheduled occurrence";
const PROJECT_BASIS: &str = "a host-local display identity of the served root's canonical path: not authenticated and not globally unique";

/// The reviewed authority one job's run carries: the plan its review judged and
/// the account it confirmed. Only this server's cost-review door creates one.
#[non_exhaustive]
pub struct CostAuthority {
    pub(super) plan: ExecutionAccessPlan,
    pub(super) cost: RunCost,
}

impl std::fmt::Debug for CostAuthority {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CostAuthority").finish_non_exhaustive()
    }
}

/// What a live review holds: the framed review (its lease), the captured world
/// the job will run and the plan it was judged under. Never serialized.
type Held = (Box<ReviewedRun>, ExecutionSession, ExecutionAccessPlan);

/// Admitted jobs' reviewed world and authority until their run claims it; the
/// custody (one claim, duplicates never run) is DAP's.
pub(super) type Reviewed = custody::Claims<Box<(ExecutionSession, CostAuthority)>>;
/// What a job's run finds for its id (its world and authority, or none).
pub(super) type Claim = custody::Claim<Box<(ExecutionSession, CostAuthority)>>;

/// The door a server started with `--cost-review` seats.
pub(super) struct Door {
    store: Mutex<custody::Reviews<Held>>,
    root: PathBuf,
    ceiling: Option<f64>,
    /// Seconds added to the monotonic clock (tests drive the lifetime).
    #[cfg(test)]
    pub(super) skew: std::sync::atomic::AtomicU64,
    /// The plan a review judges in tests (the machine's probes otherwise).
    #[cfg(test)]
    pub(super) plan: Mutex<Option<ExecutionAccessPlan>>,
}

impl Door {
    /// Seat the door over the served root under the per-run ceiling, and sweep
    /// expired reviews (releasing their leases) while it lives.
    pub(super) fn open(root: PathBuf, ceiling: Option<f64>) -> Arc<Self> {
        let door = Arc::new(Self {
            store: Mutex::new(custody::Reviews::default()),
            root,
            ceiling,
            #[cfg(test)]
            skew: std::sync::atomic::AtomicU64::new(0),
            #[cfg(test)]
            plan: Mutex::new(None),
        });
        tokio::spawn(sweep(Arc::downgrade(&door)));
        door
    }
    #[cfg_attr(not(test), allow(clippy::unused_self))] // the test offset is its only reader
    fn now(&self) -> Instant {
        #[cfg(test)]
        let skew = self.skew.load(std::sync::atomic::Ordering::SeqCst);
        #[cfg(not(test))]
        let skew = 0;
        Instant::now() + std::time::Duration::from_secs(skew)
    }
    fn store(&self) -> MutexGuard<'_, custody::Reviews<Held>> {
        let mut store = self
            .store
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        store.sweep(self.now());
        store
    }
}

async fn sweep(door: Weak<Door>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let Some(door) = door.upgrade() else { return };
        drop(door.store());
    }
}

/// This server's startup evidence: no policy source, its per-run ceiling as the
/// machine layer (a present ceiling is a hard cap every unknown cost refuses
/// under; only an explicit startup disarm is observed absent), a manual job.
pub(super) fn evidence(ceiling: Option<f64>) -> CostHostEvidence {
    let cap = ceiling.map(|usd| nika_providers::admission::monetary_default(Some(usd)));
    let machine = match cap {
        Some(Ok(Some(cap))) => CapEvidence::Observed {
            cap: HardMonetaryCap::Capped(cap),
            origin: format!("this server's per-run ceiling ({cap} USD, `--run-cost-ceiling`): a hard cap every manual job runs under"),
        },
        Some(_) => CapEvidence::Unknown,
        None => CapEvidence::Observed {
            cap: HardMonetaryCap::Absent,
            origin: "this server's per-run ceiling was explicitly disarmed at startup (`--run-cost-ceiling none`)".into(),
        },
    };
    let policy = CapEvidence::NotApplicable {
        origin: POLICY.into(),
    };
    let occurrence = CapEvidence::NotApplicable {
        origin: OCCURRENCE.into(),
    };
    CostHostEvidence::new(true, policy, machine, occurrence)
}

fn unavailable() -> Response<ResponseBody> {
    json_error(
        StatusCode::FORBIDDEN,
        "cost_review_unavailable",
        "this server's operator did not seat the cost-review door (nika serve --cost-review); health lists costReviewV1 and costReviewV2 when it is present",
    )
}

fn refused(refusal: ReviewRefusal) -> Response<ResponseBody> {
    let status = match refusal {
        ReviewRefusal::Unknown => StatusCode::NOT_FOUND,
        ReviewRefusal::Expired => StatusCode::GONE,
        _ => StatusCode::CONFLICT,
    };
    json_error(status, refusal.code(), refusal.message())
}

fn internal() -> Response<ResponseBody> {
    json_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal_error",
        "request could not be completed",
    )
}

/// `POST /v{version}/cost-reviews`: capture the named world, judge it with the
/// shared evaluator, and hold one pending review (or answer that none is needed).
pub(super) async fn create(
    request: Request<Incoming>,
    state: Arc<AppState>,
    version: u64,
) -> Response<ResponseBody> {
    let Some(door) = state.cost_review.clone() else {
        return unavailable();
    };
    let (key, digest, body) = match route::intake(request, &state, route::idempotency_key).await {
        Ok(parts) => parts,
        Err(response) => return response,
    };
    // A key binds its version with its bytes: one key never answers across versions.
    let digest = format!("v{version}:{}", digest.as_str());
    let replay = door.store().replay(key.as_str(), &digest);
    match replay {
        Some(Replay::Same(KeyAnswer::Review(id))) => {
            return respond(&door, &id, StatusCode::OK, version);
        }
        Some(Replay::Same(KeyAnswer::NotRequired(answer))) => {
            return route::json_response(StatusCode::OK, &answer);
        }
        Some(_) => {
            let why = "idempotency key is already bound to another request";
            return json_error(StatusCode::CONFLICT, "idempotency_conflict", why);
        }
        None => {}
    }
    let answer = match open_review(&state, &door, &body, version).await {
        Ok(answer) => answer,
        Err(response) => return response,
    };
    let key = key.as_str().to_owned();
    match answer {
        Ok(id) => {
            let bound = KeyAnswer::Review(id.clone());
            door.store().bind_key(key, digest, bound);
            respond(&door, &id, StatusCode::CREATED, version)
        }
        Err(answer) => {
            let bound = KeyAnswer::NotRequired(answer.clone());
            door.store().bind_key(key, digest, bound);
            route::json_response(StatusCode::OK, &answer)
        }
    }
}

/// A new review's id, or the no-review answer.
async fn open_review(
    state: &AppState,
    door: &Arc<Door>,
    body: &[u8],
    version: u64,
) -> Result<Result<String, Value>, Response<ResponseBody>> {
    let job = match route::named_job(body) {
        Ok(Some(job)) if job.cost_review.is_none() => job,
        Ok(_) => {
            let why = "a review takes the by-name job form only: workflow with optional literal inputs and access";
            return Err(json_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "malformed_snapshot",
                why,
            ));
        }
        Err(error) => return Err(error.into_response()),
    };
    let admitted = route::admit_by_name(&job.workflow, state).await?;
    let inputs = job.inputs.clone().unwrap_or_default();
    super::inputs::validate(&admitted, &inputs).map_err(super::error::ApiError::into_response)?;
    let request = request_value(&job);
    let (service, root, ceiling) = (state.service, door.root.clone(), door.ceiling);
    let access = job.access;
    #[cfg(test)]
    let forced = door.plan.lock().ok().and_then(|plan| plan.clone());
    #[cfg(not(test))]
    let forced = None;
    let framed = tokio::task::spawn_blocking(move || {
        let access = (access.as_deref(), forced);
        frame(service, admitted, &root, ceiling, &inputs, access, version)
    })
    .await
    .map_err(|_| internal())?;
    // Only the cap's own refusal teaches the cap's remedy (C6 defect 1).
    let framed = framed.map_err(|refused| {
        let (why, hint) = match refused {
            Refused::Capped(why) => (why, ceiling.map_or(String::new(), |usd| format!(" · this server's per-run ceiling ({usd} USD) is a hard cap: an operator may disarm it explicitly at startup (--run-cost-ceiling none)"))),
            Refused::Other(why) => (why, String::new()),
        };
        json_error(StatusCode::UNPROCESSABLE_ENTITY, "cost_review_refused", &format!("{why}{hint}"))
    })?;
    let ((review, session, plan), execution) = match framed {
        Framed::Review(held) => *held,
        Framed::NotRequired(observer, reason) => {
            return Ok(Err(
                json!({"cost_review_version": version, "review_required": false,
                "observer": observer, "reason": reason}),
            ));
        }
    };
    let id = format!("rev-{}", uuid::Uuid::new_v4());
    let snapshot = session.context().snapshot().digest().to_owned();
    let Some(view) = document(
        &id,
        (&review, &plan),
        &request,
        (&execution, &snapshot),
        (door, version),
    ) else {
        let why = "no fresh private witness nonce could be drawn: nothing was held";
        return Err(json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            why,
        ));
    };
    let now = door.now();
    let held = (review, session, plan);
    door.store().insert(id.clone(), (view, request), held, now);
    Ok(Ok(id))
}

/// The reviewed Run in its world (and its execution), or no review: whether
/// the job binds a per-Run observer, and why.
enum Framed {
    Review(Box<(Held, String)>),
    NotRequired(bool, &'static str),
}

/// Version 1's answer to a Run only version 2 can show.
const V1_MULTIPLIED: &str = "this Run fans out or retries: its finite dispatch bound is reviewed only at POST /v2/cost-reviews (health costReviewV2); version 1 reviews one sequential single-attempt Run";
const OBSERVED: &str = "exact declared-free or run-time routes: the job binds a per-Run observer account, as nika run does";
const ZERO: &str = "zero physical requests on its unknown-cost route: the job binds a per-Run observer account that sends nothing, as nika run does";
const UNNEEDED: &str = "no unknown-cost route: the job keeps its composition";

/// Why framing refused, in the evaluator's words: this server's own hard cap
/// (the one refusal whose remedy it teaches) or any other cause.
enum Refused {
    Capped(String),
    Other(String),
}

fn frame(
    service: ExecutionService,
    admitted: AdmittedExecution,
    root: &Path,
    ceiling: Option<f64>,
    inputs: &BTreeMap<String, Value>,
    (access, forced): (Option<&str>, Option<ExecutionAccessPlan>),
    version: u64,
) -> Result<Framed, Refused> {
    let session = service.begin(admitted);
    let driver = nika_service_execution::ServiceExecutionDriver::new(session.context(), root)
        .ok_or_else(|| Refused::Other("workflow world could not be composed".into()))?;
    let plan = forced.unwrap_or_else(|| driver.resolve_access_plan(None, access));
    let execution = session.context().execution_id().to_string();
    let ask = || Ok(());
    let prepared = run_cost::prepare(
        root,
        Some(root),
        driver.root_source(),
        execution.clone(),
        driver.workflow(),
        None,
        &plan,
        inputs,
        None,
        (evidence(ceiling), &ask),
    )
    .map_err(Refused::Other)?;
    // Version 1 documents only a single-attempt sequential Run: a fan or an
    // authored retry, and a fan that sends nothing, are version 2's.
    let (v1, refuse_v1) = (version == 1, || Refused::Other(V1_MULTIPLIED.into()));
    Ok(match prepared {
        RunCostPlan::Review(r) if v1 && r.dispatch_bound().multiplied() => return Err(refuse_v1()),
        RunCostPlan::Review(r) => Framed::Review(Box::new(((r, session, plan), execution))),
        RunCostPlan::Observer(c) if v1 && c.dispatch_bound().is_some() => return Err(refuse_v1()),
        RunCostPlan::Observer(c) if c.dispatch_bound().is_some() => Framed::NotRequired(true, ZERO),
        RunCostPlan::Observer(_) => Framed::NotRequired(true, OBSERVED),
        RunCostPlan::HardCapped(why) => return Err(Refused::Capped(why)),
        _ => Framed::NotRequired(false, UNNEEDED),
    })
}

fn request_value(job: &JobByName) -> Value {
    json!({"workflow": job.workflow, "inputs": job.inputs.clone().unwrap_or_default(), "access": job.access})
}

/// The public document: what the decision shows, never the endpoint path, a
/// credential or the private nonce. The witness digests a fresh private nonce
/// with the exact private binding, so no caller can recompute or forge it;
/// `None` when no fresh nonce can be drawn (nothing is held then).
fn document(
    id: &str,
    (review, plan): (&ReviewedRun, &ExecutionAccessPlan),
    request: &Value,
    (execution, snapshot): (&str, &str),
    (door, version): (&Door, u64),
) -> Option<Value> {
    let challenge = review.challenge();
    let (requests, tokens, timeout) = review.bounds();
    let [invocation, project] = review.defaults();
    let prior = review.prior_journal();
    let created = jiff::Timestamp::now();
    let expires = created
        .checked_add(jiff::SignedDuration::from_secs(300))
        .unwrap_or(created);
    let usd = |v: Option<f64>| v.map(|v| v.to_string());
    let names: Vec<&String> = request["inputs"]
        .as_object()
        .map(|m| m.keys().collect())
        .unwrap_or_default();
    let mut view = json!({"cost_review_version": version, "review_id": id, "state": "pending",
        "created_at": created.to_string(), "expires_at": expires.to_string(),
        "workflow": request["workflow"], "access": request["access"], "execution_id": execution,
        "project": {"root_fingerprint": nika_runtime::project_root_fingerprint(&door.root), "basis": PROJECT_BASIS},
        "program": {"snapshot_digest": snapshot, "source_sha256": challenge.source_sha256},
        "inputs": {"names": names, "source": "api_caller", "sha256": challenge.inputs_sha256},
        "route": {"provider": challenge.route.provider, "model": challenge.route.model, "origin": challenge.route.origin()},
        "price": {"state": "unknown", "native": challenge.native_price},
        "question": challenge.question,
        "bounds": {"max_requests": requests, "max_output_tokens": tokens, "request_timeout_seconds": timeout.as_secs(), "retries": 0},
        "defaults": {"invocation_usd": usd(invocation), "project_usd": usd(project),
            "basis": "invocation: none (this server's per-run ceiling is a hard cap, never a default); project: the project's `ceiling:`, overridden once as nika run does; no hard cap is ever overridden"},
        "host": {"authority": "operator_started_cost_review", "evidence": evidence(door.ceiling).view(),
            "credential_custody": custody(plan, &challenge.route.provider)},
        "prior_journal": {"length": prior.length, "sha256": prior.sha256},
        "effects": ["holds this project's cost lease until decline, expiry or one job admission",
            "may create .nika/ and the cost journal", "may record an earlier Run whose writer the lease proves gone as UNKNOWN",
            "writes no Run row: the prepared row is written only at the accepted job admission"],
        "grants": "one explicit POST /v1/jobs of this exact witness and request; approving creates no job, run, file or effect"});
    if version == 2 {
        // The typed bound the approval confirms; the witness below covers it.
        let d = review.dispatch_bound();
        view["bounds"] = json!({"max_requests": requests, "max_in_flight": d.max_in_flight, "max_output_tokens": tokens,
            "request_timeout_seconds": timeout.as_secs(), "transport_retries": 0});
        view["dispatch"] = json!({"requests": d.requests, "max_in_flight": d.max_in_flight, "authored_retry": d.authored_retry(),
            "tasks": d.tasks.iter().map(|t| json!({"task": t.task, "items": t.items, "attempts": t.attempts,
                "calls_per_attempt": t.calls_per_attempt, "max_parallel": t.max_parallel, "requests": t.requests})).collect::<Vec<_>>()});
    }
    let mut nonce = [0_u8; 32];
    getrandom::fill(&mut nonce).ok()?;
    let private = json!([view, challenge.candidate, challenge.route.endpoint, request]);
    view["witness_sha256"] = json!(custody::review_witness(&nonce, &private.to_string()));
    Some(view)
}

/// Where the route's credential is held: an admitted API lane reads its key
/// from this server process's environment and keeps it in memory; anything
/// else is not observed here.
fn custody(plan: &ExecutionAccessPlan, provider: &str) -> &'static str {
    if plan.admits_api_lane(provider) {
        "HOST_SERVER_MEMORY"
    } else {
        "UNKNOWN"
    }
}

/// A review answers only at the version that created it: elsewhere its id is unknown.
fn respond(door: &Door, id: &str, status: StatusCode, version: u64) -> Response<ResponseBody> {
    match door.store().view(id) {
        Ok(view) if view["cost_review_version"] == version => route::json_response(status, &view),
        Ok(_) => refused(ReviewRefusal::Unknown),
        Err(refusal) => refused(refusal),
    }
}

/// `GET /v{version}/cost-reviews/{id}`: reading approves nothing.
pub(super) fn get(id: &str, state: &AppState, version: u64) -> Response<ResponseBody> {
    match &state.cost_review {
        Some(door) => respond(door, id, StatusCode::OK, version),
        None => unavailable(),
    }
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    witness_sha256: String,
    decision: String,
}

/// `POST /v{version}/cost-reviews/{id}/decision`: one explicit decision, never a job.
pub(super) async fn decide(
    request: Request<Incoming>,
    id: String,
    state: Arc<AppState>,
    version: u64,
) -> Response<ResponseBody> {
    let Some(door) = state.cost_review.clone() else {
        return unavailable();
    };
    let body = match route::intake(request, &state, |_| Ok(())).await {
        Ok(((), _, body)) => body,
        Err(response) => return response,
    };
    if let Ok(view) = door.store().view(&id)
        && view["cost_review_version"] != version
    {
        return refused(ReviewRefusal::Unknown);
    }
    let (approve, witness) = match serde_json::from_slice::<Decision>(&body) {
        Ok(d) if d.decision == "approve_once" => (true, d.witness_sha256),
        Ok(d) if d.decision == "decline" => (false, d.witness_sha256),
        _ => {
            let why =
                "send exactly {\"witness_sha256\", \"decision\": \"approve_once\" | \"decline\"}";
            return json_error(StatusCode::UNPROCESSABLE_ENTITY, "malformed_decision", why);
        }
    };
    let stamp = (door.now(), jiff::Timestamp::now().to_string());
    let decided = door
        .store()
        .decide(&id, &witness, approve, (stamp.0, &stamp.1));
    match decided {
        Ok(view) => route::json_response(StatusCode::OK, &view),
        Err(refusal) => refused(refusal),
    }
}

/// The one job admission a review allows. The request must be the reviewed
/// one; the world is captured again by name and must be the reviewed bytes;
/// the host confirms after re-observing everything the review bound. Any
/// refusal after the review was taken spends it: nothing is ever refunded.
pub(super) async fn admit(
    state: Arc<AppState>,
    key: IdempotencyKey,
    digest: RequestDigest,
    job: JobByName,
    (id, witness): (String, String),
) -> Response<ResponseBody> {
    let Some(door) = state.cost_review.clone() else {
        return unavailable();
    };
    let now = door.now();
    let taken = door.store().take(&id, &witness, &request_value(&job), now);
    let held = match taken {
        Ok(held) => held,
        Err(refusal) => return refused(refusal),
    };
    let _verdict = Admitting(&door, &id);
    let (session, plan, cost, world) = match confirm(&state, &door, &id, &job.workflow, held).await
    {
        Ok(confirmed) => confirmed,
        Err(response) => return response,
    };
    let account = cost.account.clone();
    let authority = (session, CostAuthority { plan, cost });
    let inputs = job.inputs.unwrap_or_default();
    let admitted = state
        .coordinator
        .admit_manual_inputs(
            key,
            digest,
            job.workflow,
            world,
            job.access,
            inputs,
            Some(authority),
        )
        .await;
    if let Ok(Admission::Created(record)) = &admitted {
        let job = (record.id().as_str(), account_view(account));
        door.store().settle(&id, "consumed", Some(job), None);
    } else {
        let why = "the confirmed review created no job; its account settled with nothing sent";
        door.store()
            .settle(&id, "failed", None, Some(("review_admission_failed", why)));
    }
    match admitted {
        Ok(admission) => route::admission_response(admission),
        Err(error) => route::admission_error(&error),
    }
}

/// A taken review whose admission ends without a verdict (the request
/// deadline dropped it) still ends: `failed`, never back to approved.
struct Admitting<'a>(&'a Door, &'a str);

impl Drop for Admitting<'_> {
    fn drop(&mut self) {
        let why = "the admission ended before its verdict; if the job exists, replaying its Idempotency-Key answers it";
        self.0.store().settle(
            self.1,
            "failed",
            None,
            Some(("review_admission_failed", why)),
        );
    }
}

/// A consumed review reads its account live: admission-account accounting,
/// never proof of physical dispatch.
fn account_view(account: nika_providers::InferenceAdmission) -> custody::AccountView {
    Box::new(move || {
        let receipt = account.snapshot().ok()?;
        let sent = receipt.unknown_attempts.iter().filter(|a| a.sent).count();
        Some(
            json!({"attempts": receipt.unknown_attempts.len(), "marked_sent": sent,
            "unknown_charge_attempts": receipt.unknown_calls, "state": format!("{:?}", receipt.state),
            "basis": "admission-account accounting, not proof of physical dispatch"}),
        )
    })
}

type Confirmed = (ExecutionSession, ExecutionAccessPlan, RunCost, String);

/// Capture the named world again and require the reviewed bytes, then let the
/// host confirm after re-observing everything the review bound.
async fn confirm(
    state: &AppState,
    door: &Door,
    id: &str,
    workflow: &str,
    (review, session, plan): Held,
) -> Result<Confirmed, Response<ResponseBody>> {
    let spend = |word: &str, code: &'static str, message: &str| {
        door.store().settle(id, word, None, Some((code, message)));
        json_error(StatusCode::CONFLICT, code, message)
    };
    let current = match route::admit_by_name(workflow, state).await {
        Ok(admitted) => admitted.snapshot().digest().to_owned(),
        Err(response) => {
            let why = "the reviewed workflow is no longer served";
            door.store()
                .settle(id, "refused", None, Some(("review_witness_changed", why)));
            return Err(response);
        }
    };
    let context = session.context();
    if current != context.snapshot().digest() {
        let why = "the workflow changed since review: request a fresh review";
        return Err(spend("refused", "review_witness_changed", why));
    }
    let snapshot = context.snapshot();
    let source = snapshot
        .text(snapshot.root())
        .unwrap_or_default()
        .to_owned();
    let world = snapshot.encode();
    let confirmed = tokio::task::spawn_blocking(move || {
        let answer = review.challenge().response(true);
        review.confirm(&answer, &source)
    })
    .await;
    let cost = match confirmed {
        Ok(Ok(cost)) => cost,
        Ok(Err(why)) if door.store().expired_at(id, door.now()) => {
            door.store()
                .settle(id, "expired", None, Some(("review_expired", &why)));
            let why = "the review expired before its admission: request a fresh review";
            return Err(json_error(StatusCode::GONE, "review_expired", why));
        }
        Ok(Err(why)) => return Err(spend("refused", "review_witness_changed", &why)),
        Err(_) => {
            let why = "the admission could not confirm";
            return Err(spend("failed", "review_admission_failed", why));
        }
    };
    let world = world.map_err(|_| {
        spend(
            "failed",
            "review_admission_failed",
            "the reviewed world could not be encoded",
        )
    })?;
    Ok((session, plan, cost, world))
}
