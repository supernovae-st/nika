use std::path::Path;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::header::{CONTENT_ENCODING, ETAG, HeaderValue, IF_MATCH, IF_NONE_MATCH};
use hyper::{Request, Response, StatusCode};
use jiff::Zoned;
use nika_cadence::{
    AfterSkip, MissPolicy, Overlap, ScheduleDecisionState, ScheduleDefinition, ScheduleDraft,
    ScheduleDueVerdict, ScheduleFinding, ScheduleJitter, ScheduleOrigin, SchedulePlanError,
    ScheduleRevision, ScheduleSlot, ScheduleWhen, ScheduleWhenDraft, Shift, plan_schedule,
};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::schedule::{ScheduleDecisionRecord, ScheduleSlotAction};
use crate::{ScheduleApplyOutcome, ScheduleApplyPrecondition, ScheduleStoreError};

use super::AppState;
use super::error::{ApiError, ResponseBody, json_error};
use super::route::{collect_body, is_json, json_response};

const MAX_SCHEDULE_BODY_BYTES: usize = 8 * 1024;
const MAX_ETAG_HEADER_BYTES: usize = 96;
const STATUS_SLOT_LIMIT: usize = 8;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchedulePutBody {
    workflow: String,
    when: ScheduleWhenBody,
    max_cost_usd: f64,
    missed: MissedBody,
    #[serde(default)]
    max_lateness_seconds: Option<u64>,
    #[serde(default)]
    overlap: Option<OverlapBody>,
    #[serde(default)]
    after_skip: Option<AfterSkipBody>,
    #[serde(default)]
    jitter: Option<JitterBody>,
    #[serde(default)]
    tolerance: Option<String>,
    #[serde(default)]
    active: Option<bool>,
    #[serde(default)]
    pause_reason: Option<String>,
    #[serde(default)]
    pause_until: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ScheduleWhenBody {
    Once { at: String },
    Cadence { expression: String },
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum MissedBody {
    CatchUp,
    CatchUpOnce,
    Skip,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OverlapBody {
    Skip,
    Queue,
    Replace,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AfterSkipBody {
    NextSlot,
    OnCompletion,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum JitterBody {
    Hash,
}

impl SchedulePutBody {
    fn lower(self, id: String) -> ScheduleDraft {
        let when = match self.when {
            ScheduleWhenBody::Once { at } => ScheduleWhenDraft::Once { at },
            ScheduleWhenBody::Cadence { expression } => ScheduleWhenDraft::Cadence { expression },
        };
        let missed = match self.missed {
            MissedBody::CatchUp => MissPolicy::Rattraper,
            MissedBody::CatchUpOnce => MissPolicy::RattraperUneFois,
            MissedBody::Skip => MissPolicy::Sauter,
        };
        let mut draft = ScheduleDraft::new(id, self.workflow, when, self.max_cost_usd, missed);
        draft.max_lateness_seconds = self.max_lateness_seconds;
        draft.overlap = self.overlap.map(|overlap| match overlap {
            OverlapBody::Skip => Overlap::Sauter,
            OverlapBody::Queue => Overlap::File,
            OverlapBody::Replace => Overlap::Remplacer,
        });
        draft.after_skip = self.after_skip.map(|after| match after {
            AfterSkipBody::NextSlot => AfterSkip::ProchainCreneau,
            AfterSkipBody::OnCompletion => AfterSkip::ACompletion,
        });
        draft.jitter = self.jitter.map(|JitterBody::Hash| ScheduleJitter::Hash);
        draft.tolerance = self.tolerance;
        draft.active = self.active;
        draft.pause_reason = self.pause_reason;
        draft.pause_until = self.pause_until;
        draft
    }
}

pub(super) async fn put(
    request: Request<Incoming>,
    id: String,
    state: Arc<AppState>,
) -> Response<ResponseBody> {
    let precondition = match precondition(request.headers()) {
        Ok(precondition) => precondition,
        Err(PreconditionError) => return precondition_failed(None),
    };
    if !is_json(request.headers()) || request.headers().contains_key(CONTENT_ENCODING) {
        return ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_schedule_media",
            "schedule bodies require uncompressed application/json",
        )
        .into_response();
    }
    let body = match collect_body(request, MAX_SCHEDULE_BODY_BYTES).await {
        Ok(body) => body,
        Err(error) => return error.into_response(),
    };
    let wire: SchedulePutBody = match serde_json::from_slice(&body) {
        Ok(wire) => wire,
        Err(_) => {
            return json_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "schedule.body",
                "schedule JSON is malformed or contains an unknown field",
            );
        }
    };
    let draft = wire.lower(id);
    let candidate = match draft.clone().validate() {
        Ok(candidate) => candidate,
        Err(finding) => return finding_response(&finding),
    };
    if let Err(error) = plan_schedule(
        &candidate,
        &state.clock.now(),
        &ScheduleDecisionState::empty(),
        1,
    ) {
        return planner_error_response(&error);
    }
    if let Err(response) = validate_workflow(&candidate, &state).await {
        return response;
    }
    let schedules = Arc::clone(&state.schedules);
    let applied = tokio::task::spawn_blocking(move || schedules.apply(draft, precondition)).await;
    let outcome = match applied {
        Ok(Ok(outcome)) => outcome,
        Ok(Err(error)) => return store_error_response(&error),
        Err(_) => return ApiError::internal().into_response(),
    };
    match outcome {
        ScheduleApplyOutcome::Conflict { current } => conflict_response(current.as_ref()),
        ScheduleApplyOutcome::Created(definition) | ScheduleApplyOutcome::Updated(definition) => {
            state.schedule_wake.notify_one();
            applied_response(&state, definition, true).await
        }
        ScheduleApplyOutcome::Unchanged(definition) => {
            state.schedule_wake.notify_one();
            applied_response(&state, definition, false).await
        }
    }
}

pub(super) async fn get(id: String, state: &AppState) -> Response<ResponseBody> {
    let schedules = Arc::clone(&state.schedules);
    let lookup = id.clone();
    let definition = tokio::task::spawn_blocking(move || schedules.get(&lookup)).await;
    match definition {
        Ok(Ok(Some(definition))) => status_response(state, definition, None).await,
        Ok(Ok(None)) => project_refusal_response(&id, state).await,
        Ok(Err(error)) => store_error_response(&error),
        Err(_) => ApiError::internal().into_response(),
    }
}

/// A project beat the planner refused at load reads as a finding on the
/// same status projection, never as a live schedule that fires nothing.
async fn project_refusal_response(id: &str, state: &AppState) -> Response<ResponseBody> {
    let refused = {
        let projection = state
            .project_refusals
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        projection.get(id).cloned()
    };
    let Some(refused) = refused else {
        return schedule_not_found();
    };
    let definition = refused.definition().clone();
    let revision = definition.revision();
    let schedules = Arc::clone(&state.schedules);
    let lookup = id.to_owned();
    let last = tokio::task::spawn_blocking(move || {
        schedules.last_decision(ScheduleOrigin::Project, &lookup)
    })
    .await;
    let last = match last {
        Ok(Ok(last)) => last,
        Ok(Err(error)) => return store_error_response(&error),
        Err(_) => return ApiError::internal().into_response(),
    };
    let body = json!({
        "definition": definition_json(&definition),
        "origin": "project",
        "revision": revision.as_str(),
        "active": definition.is_active(),
        "pause": pause_json(&definition),
        "finding": planner_finding(refused.error()),
        "next": [],
        "earliestWakeHint": Value::Null,
        "lastDecision": last.as_ref().map(last_decision_json),
    });
    with_etag(json_response(StatusCode::OK, &body), &revision)
}

async fn applied_response(
    state: &AppState,
    definition: ScheduleDefinition,
    changed: bool,
) -> Response<ResponseBody> {
    status_response(state, definition, Some(changed)).await
}

async fn status_response(
    state: &AppState,
    definition: ScheduleDefinition,
    changed: Option<bool>,
) -> Response<ResponseBody> {
    let revision = definition.revision();
    let schedules = Arc::clone(&state.schedules);
    let id = definition.id().to_owned();
    let now = state.clock.now();
    let status = tokio::task::spawn_blocking(move || {
        let prior = schedules.decision_state(ScheduleOrigin::Api, &id)?;
        let last = schedules.last_decision(ScheduleOrigin::Api, &id)?;
        Ok::<_, ScheduleStoreError>(schedule_status(&definition, &now, &prior, last.as_ref()))
    })
    .await;
    let status = match status {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => return store_error_response(&error),
        Err(_) => return ApiError::internal().into_response(),
    };
    let body = changed.map_or_else(
        || status.clone(),
        |changed| json!({"applied": true, "changed": changed, "status": status}),
    );
    with_etag(json_response(StatusCode::OK, &body), &revision)
}

async fn validate_workflow(
    definition: &ScheduleDefinition,
    state: &AppState,
) -> Result<(), Response<ResponseBody>> {
    let project = Arc::clone(&state.project);
    let service = state.service;
    let workflow = definition.workflow().to_owned();
    let admitted =
        tokio::task::spawn_blocking(move || service.admit(&project, Path::new(&workflow))).await;
    match admitted {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => Err(workflow_error_response(&error)),
        Err(_) => Err(ApiError::internal().into_response()),
    }
}

fn workflow_error_response(error: &nika_execution::ExecutionError) -> Response<ResponseBody> {
    match error {
        nika_execution::ExecutionError::UnitDigestMismatch { .. }
        | nika_execution::ExecutionError::SnapshotDigestMismatch
        | nika_execution::ExecutionError::SnapshotStructureMismatch => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "schedule_workflow_corrupt",
            "workflow capture failed integrity validation",
        ),
        _ => json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "schedule.workflow",
            "workflow is missing, invalid, or escapes the configured root",
        ),
    }
}

fn precondition(
    headers: &hyper::HeaderMap,
) -> Result<ScheduleApplyPrecondition, PreconditionError> {
    let none = single_header(headers, IF_NONE_MATCH)?;
    let matched = single_header(headers, IF_MATCH)?;
    match (none, matched) {
        (Some("*"), None) => Ok(ScheduleApplyPrecondition::Create),
        (None, Some(value)) => parse_etag(value)
            .map(ScheduleApplyPrecondition::Revision)
            .ok_or(PreconditionError),
        _ => Err(PreconditionError),
    }
}

fn single_header(
    headers: &hyper::HeaderMap,
    name: hyper::header::HeaderName,
) -> Result<Option<&str>, PreconditionError> {
    let mut values = headers.get_all(name).iter();
    let Some(value) = values.next() else {
        return Ok(None);
    };
    if values.next().is_some() || value.as_bytes().len() > MAX_ETAG_HEADER_BYTES {
        return Err(PreconditionError);
    }
    value.to_str().map(Some).map_err(|_| PreconditionError)
}

struct PreconditionError;

fn parse_etag(value: &str) -> Option<ScheduleRevision> {
    let inner = value.strip_prefix('"')?.strip_suffix('"')?;
    ScheduleRevision::from_wire(inner)
}

fn schedule_status(
    definition: &ScheduleDefinition,
    now: &Zoned,
    prior: &nika_cadence::ScheduleDecisionState,
    last: Option<&ScheduleDecisionRecord>,
) -> Value {
    let definition_body = definition_json(definition);
    match plan_schedule(definition, now, prior, STATUS_SLOT_LIMIT) {
        Ok(plan) => json!({
            "definition": definition_body,
            "origin": "api",
            "revision": definition.revision().as_str(),
            "active": definition.is_active(),
            "pause": pause_json(definition),
            "due": due_json(plan.due()),
            "next": plan.next_slots().map(slot_json).collect::<Vec<_>>(),
            "earliestWakeHint": plan.earliest_wake_hint().map(|at| at.to_string()),
            "lastDecision": last.map(last_decision_json),
        }),
        Err(error) => json!({
            "definition": definition_body,
            "origin": "api",
            "revision": definition.revision().as_str(),
            "active": definition.is_active(),
            "pause": pause_json(definition),
            "finding": planner_finding(&error),
            "next": [],
            "earliestWakeHint": Value::Null,
            "lastDecision": last.map(last_decision_json),
        }),
    }
}

fn definition_json(definition: &ScheduleDefinition) -> Value {
    json!({
        "id": definition.id(),
        "workflow": definition.workflow(),
        "when": when_json(definition.when()),
        "maxCostUsd": definition.max_cost_usd(),
        "missed": missed_word(definition.missed()),
        "maxLatenessSeconds": definition.max_lateness_seconds(),
        "overlap": overlap_word(definition.overlap()),
        "afterSkip": after_skip_word(definition.after_skip()),
        "jitter": definition.jitter().map(|_| "hash"),
        "tolerance": definition.tolerance(),
        "active": definition.is_active(),
        "pauseReason": definition.pause_reason(),
        "pauseUntil": definition.pause_until(),
    })
}

fn when_json(when: &ScheduleWhen) -> Value {
    match when {
        ScheduleWhen::Once { at } => json!({"kind": "once", "at": at.to_string()}),
        ScheduleWhen::Cadence { expression } => {
            json!({"kind": "cadence", "expression": expression})
        }
        ScheduleWhen::Webhook => json!({"kind": "webhook"}),
        _ => json!({"kind": "unknown"}),
    }
}

fn due_json(due: &ScheduleDueVerdict) -> Value {
    match due {
        ScheduleDueVerdict::ScheduledOnTime { slot } => {
            json!({"kind": "scheduled", "slot": slot_json(slot)})
        }
        ScheduleDueVerdict::CatchUp { slot, missed_slots } => json!({
            "kind": "catch_up", "slot": slot_json(slot), "missedSlots": missed_slots
        }),
        ScheduleDueVerdict::SkippedMissed { slot, missed_slots } => json!({
            "kind": "skipped_missed", "slot": slot_json(slot), "missedSlots": missed_slots
        }),
        ScheduleDueVerdict::SkippedTooLate {
            slot,
            lateness_seconds,
            maximum_seconds,
        } => json!({
            "kind": "skipped_too_late", "slot": slot_json(slot),
            "latenessSeconds": lateness_seconds, "maximumSeconds": maximum_seconds
        }),
        ScheduleDueVerdict::PausedInactive {
            reason,
            pause_until,
        } => json!({
            "kind": "paused", "reason": reason, "pauseUntil": pause_until
        }),
        ScheduleDueVerdict::OnceConsumed {
            slot_id,
            scheduled_for,
        } => json!({
            "kind": "once_consumed", "slotId": slot_id.as_str(),
            "scheduledFor": scheduled_for.to_string()
        }),
        ScheduleDueVerdict::NotDue => json!({"kind": "not_due"}),
        _ => json!({"kind": "unknown"}),
    }
}

fn slot_json(slot: &ScheduleSlot) -> Value {
    json!({
        "slotId": slot.id().as_str(),
        "scheduledFor": slot.scheduled_for().to_string(),
        "requestedCivil": slot.requested_civil().map(|civil| civil.to_string()),
        "shift": shift_word(slot.shift()),
    })
}

fn last_decision_json(last: &ScheduleDecisionRecord) -> Value {
    json!({
        "action": match last.action() {
            ScheduleSlotAction::Claimed => "claimed",
            ScheduleSlotAction::Skipped => "skipped",
        },
        "decision": match last.decision() {
            nika_cadence::ScheduleDecision::Scheduled => "scheduled",
            nika_cadence::ScheduleDecision::CatchUp => "catch_up",
            _ => "unknown",
        },
        "revision": last.revision().as_str(),
        "slotId": last.slot().id().as_str(),
        "scheduledFor": last.slot().scheduled_for().to_string(),
        "decidedAt": last.decided_at().to_string(),
        "reason": last.reason(),
        "claim": last.claim().map(|claim| json!({
            "runId": claim.run_id(),
            "executionId": claim.execution_id(),
            "traceId": claim.trace_id(),
            "generation": claim.generation().as_str(),
        })),
    })
}

fn pause_json(definition: &ScheduleDefinition) -> Value {
    if definition.is_active() {
        Value::Null
    } else {
        json!({
            "reason": definition.pause_reason(),
            "until": definition.pause_until(),
        })
    }
}

fn planner_finding(error: &SchedulePlanError) -> Value {
    let code = match error {
        SchedulePlanError::UnsupportedHashJitter => "schedule.jitter",
        SchedulePlanError::UnsupportedOverlapReplace
        | SchedulePlanError::UnsupportedOverlapQueue => "schedule.overlap",
        SchedulePlanError::UnsupportedAfterSkipOnCompletion => "schedule.after-skip",
        SchedulePlanError::InvalidCanonicalCadence(_) => "schedule.cadence",
        _ => "schedule.plan",
    };
    json!({"code": code, "detail": error.to_string()})
}

fn planner_error_response(error: &SchedulePlanError) -> Response<ResponseBody> {
    json_response(
        StatusCode::UNPROCESSABLE_ENTITY,
        &json!({"findings": [planner_finding(error)]}),
    )
}

fn finding_response(finding: &ScheduleFinding) -> Response<ResponseBody> {
    let body = json!({
        "findings": [{"code": finding.kind().code(), "detail": finding.detail()}]
    });
    json_response(StatusCode::UNPROCESSABLE_ENTITY, &body)
}

fn store_error_response(error: &ScheduleStoreError) -> Response<ResponseBody> {
    match error {
        ScheduleStoreError::InvalidSchedule(finding) => finding_response(finding),
        ScheduleStoreError::ScheduleTooLarge { .. }
        | ScheduleStoreError::SnapshotTooLarge { .. } => json_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "schedule_too_large",
            "schedule state exceeds its configured bound",
        ),
        ScheduleStoreError::ScheduleLimit { .. } | ScheduleStoreError::DecisionLimit { .. } => {
            json_error(
                StatusCode::INSUFFICIENT_STORAGE,
                "schedule_capacity",
                "durable schedule capacity is exhausted",
            )
        }
        ScheduleStoreError::Io(_) | ScheduleStoreError::LockPoisoned => json_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "schedule_store_unavailable",
            "durable schedule state is unavailable",
        ),
        ScheduleStoreError::Corrupt(_) | ScheduleStoreError::UnsupportedCanonicalValue => {
            json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "schedule_store_corrupt",
                "durable schedule state failed integrity validation",
            )
        }
    }
}

fn conflict_response(current: Option<&ScheduleRevision>) -> Response<ResponseBody> {
    let mut response = precondition_failed(current);
    if let Some(revision) = current {
        insert_etag(&mut response, revision);
    }
    response
}

fn precondition_failed(current: Option<&ScheduleRevision>) -> Response<ResponseBody> {
    let body = json!({
        "error": {
            "code": "schedule_precondition_failed",
            "message": "create requires If-None-Match: *; update requires the exact current ETag",
            "currentRevision": current.map(ScheduleRevision::as_str),
        }
    });
    json_response(StatusCode::PRECONDITION_FAILED, &body)
}

fn schedule_not_found() -> Response<ResponseBody> {
    json_error(
        StatusCode::NOT_FOUND,
        "schedule_not_found",
        "schedule not found",
    )
}

fn with_etag(
    mut response: Response<ResponseBody>,
    revision: &ScheduleRevision,
) -> Response<ResponseBody> {
    insert_etag(&mut response, revision);
    response
}

fn insert_etag(response: &mut Response<ResponseBody>, revision: &ScheduleRevision) {
    if let Ok(value) = HeaderValue::from_str(&format!("\"{}\"", revision.as_str())) {
        response.headers_mut().insert(ETAG, value);
    }
}

const fn missed_word(value: MissPolicy) -> &'static str {
    match value {
        MissPolicy::Rattraper => "catch-up",
        MissPolicy::RattraperUneFois => "catch-up-once",
        MissPolicy::Sauter => "skip",
        _ => "unknown",
    }
}

const fn overlap_word(value: Overlap) -> &'static str {
    match value {
        Overlap::Sauter => "skip",
        Overlap::File => "queue",
        Overlap::Remplacer => "replace",
        _ => "unknown",
    }
}

const fn after_skip_word(value: AfterSkip) -> &'static str {
    match value {
        AfterSkip::ProchainCreneau => "next_slot",
        AfterSkip::ACompletion => "on_completion",
        _ => "unknown",
    }
}

const fn shift_word(value: Shift) -> &'static str {
    match value {
        Shift::Exact => "exact",
        Shift::AdvancedFirstValid => "advanced_first_valid",
        Shift::FoldedFirst => "folded_first",
        _ => "unknown",
    }
}
