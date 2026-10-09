// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `POST /v1/compile` — the HTTP transport of the ONE stateless Compile core (#1670).
//!
//! This module bounds and parses a request, calls [`nika_onboard::compile::compile`]
//! and prints the core's own machine document. It holds no authoring semantics: no
//! routing, assembly, policy hole or Check projection lives here. It creates no job,
//! run, approval or trace, writes no file, reads no environment variable and contacts
//! no provider. A candidate is REVIEW material: `POST /v1/jobs` admits it again.
//!
//! Foundation truth, unchanged by this transport: CREATE resolves an exact embedded
//! skeleton, EDIT changes one existing constant, answers are explicit literals.
//! Every other intent is the core's `incomplete`, never a guessed workflow.
//!
//! Generation 2 exists only on a server the operator built with a native authoring seat
//! ([`native`]): a caller then opts in per request (`explicitProvider`) or replays a round
//! the server kept ([`replay`]). Generation 1 keeps this module's path on every server.

mod author;
mod native;
mod replay;
pub(super) mod schema;
mod v2;

use std::sync::Arc;

use bytes::Bytes;
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use nika_onboard::compile::remote::input::{Answers, Object, present};
use nika_onboard::compile::{AuthoringCognition, COMPILE_WIRE_VERSION, CompileRequest};
use serde_json::value::RawValue;

pub(super) use author::settle;
pub(super) use native::Seat;
pub use native::{
    NativeAuthoring, NativeAuthoringArgs, NativeAuthoringError, seat_native_authoring,
    seat_native_authoring_with_calls,
};

use super::AppState;
use super::error::{ApiError, ResponseBody};
use super::route::{
    body_too_large, collect_body, content_length, drain_oversized_body, json_response,
    refuse_snapshot_envelope, request_timeout,
};

/// The only authoring cognition this build implements, in the core's own word.
const DETERMINISTIC_ONLY: &str = AuthoringCognition::DeterministicOnly.word();

/// Generation 1 of the request. Unknown fields, a present `null` and duplicate
/// keys are refused: an authoring value is never chosen silently.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    compile_version: u64,
    mode: String,
    #[serde(default, deserialize_with = "present")]
    intent: Option<String>,
    #[serde(default, deserialize_with = "present")]
    workflow_id: Option<String>,
    /// Inline accepted source. No field of this door names a host path.
    #[serde(default, deserialize_with = "present")]
    source: Option<String>,
    #[serde(default, deserialize_with = "present")]
    change: Option<Object<Change>>,
    #[serde(default)]
    answers: Answers,
    #[serde(default, deserialize_with = "present")]
    cognition: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    #[serde(default, deserialize_with = "present")]
    text: Option<String>,
    #[serde(default, deserialize_with = "present")]
    set_constant: Option<Object<SetConstant>>,
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct SetConstant {
    name: String,
    /// The literal exactly as sent: the core parses it once, as it does for the CLI.
    value: Box<RawValue>,
}

/// Only the generation, so a newer request is named as such instead of "malformed".
#[derive(serde::Deserialize)]
struct VersionProbe {
    compile_version: u64,
}

pub(super) async fn handle(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Response<ResponseBody> {
    match intake(request, &state).await {
        Ok(body) => foundation(&body, state).await,
        Err(response) => response,
    }
}

/// The door on a server that seats native authoring. A native round outlives the route's
/// request deadline, so the route leaves this door to bound itself: the request deadline
/// still covers intake, generation 1 and replays exactly as before; a fresh native round
/// runs under its seat deadline instead.
pub(super) async fn handle_on_native_server(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Response<ResponseBody> {
    let deadline = tokio::time::Instant::now() + state.limits.request_timeout();
    let body = match tokio::time::timeout_at(deadline, intake(request, &state)).await {
        Ok(Ok(body)) => body,
        Ok(Err(response)) => return response,
        Err(_) => return request_timeout().into_response(),
    };
    match generation(&body) {
        Some(v2::GENERATION) => author::handle(&body, state, deadline).await,
        Some(generation) if generation != u64::from(COMPILE_WIRE_VERSION) => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "compile_version_unsupported",
            "this resident speaks compile_version 1, and 2 through its native authoring seat",
        )
        .into_response(),
        _ => match tokio::time::timeout_at(deadline, foundation(&body, state)).await {
            Ok(response) => response,
            Err(_) => request_timeout().into_response(),
        },
    }
}

/// The bounded body, or the refusal that answers instead of it.
async fn intake(
    request: Request<Incoming>,
    state: &AppState,
) -> Result<Bytes, Response<ResponseBody>> {
    if let Some(error) = refuse_snapshot_envelope(&request) {
        return Err(error.into_response());
    }
    let ceiling = state.limits.max_body_bytes();
    if content_length(&request)
        .ok()
        .flatten()
        .is_some_and(|length| length > ceiling)
    {
        drain_oversized_body(request).await;
        return Err(body_too_large().into_response());
    }
    collect_body(request, ceiling)
        .await
        .map_err(ApiError::into_response)
}

/// The generation a body names, when it names one (a probe: the parse judges the rest).
fn generation(body: &[u8]) -> Option<u64> {
    serde_json::from_slice::<Object<VersionProbe>>(body)
        .ok()
        .map(|Object(probe)| probe.compile_version)
}

/// Generation 1: the deterministic core, whatever the server seats.
async fn foundation(body: &Bytes, state: Arc<AppState>) -> Response<ResponseBody> {
    let compile_request = match parse(body) {
        Ok(compile_request) => compile_request,
        Err(error) => return error.into_response(),
    };
    // Fail fast instead of queueing: a waiter would only turn into a 408.
    let Ok(permit) = Arc::clone(&state.compile_slots).try_acquire_owned() else {
        return busy().into_response();
    };
    #[cfg(test)]
    let before_compile = Arc::clone(&state.before_compile);
    let compiled = tokio::task::spawn_blocking(move || {
        // Blocking work cannot be cancelled. The permit therefore lives exactly as
        // long as the CPU work, not as long as the request: a caller that timed out
        // or disconnected never frees a slot the blocking pool is still using.
        let _permit = permit;
        #[cfg(test)]
        super::test_support::before_compile(&before_compile);
        nika_onboard::compile::compile(&compile_request)
            .map(|outcome| nika_onboard::compile::outcome_document(&outcome))
    })
    .await;
    match compiled {
        // ready, incomplete and refused are all authoring DATA (#1670): 200.
        Ok(Ok(document)) => json_response(StatusCode::OK, &document),
        // Compiler machinery failure or a panicked task. Nothing is echoed.
        Ok(Err(_)) | Err(_) => ApiError::internal().into_response(),
    }
}

fn parse(body: &[u8]) -> Result<CompileRequest, ApiError> {
    let Ok(Object(envelope)) = serde_json::from_slice::<Object<Envelope>>(body) else {
        // A newer generation may carry fields this one refuses: name the
        // generation, so a client never mistakes it for a broken request.
        return Err(match serde_json::from_slice::<Object<VersionProbe>>(body) {
            Ok(Object(probe)) if probe.compile_version != u64::from(COMPILE_WIRE_VERSION) => {
                unsupported_version()
            }
            _ => malformed(),
        });
    };
    // Vocabulary first (generation · mode · cognition), then bounds, then shape.
    if envelope.compile_version != u64::from(COMPILE_WIRE_VERSION) {
        return Err(unsupported_version());
    }
    if !matches!(envelope.mode.as_str(), "create" | "edit") {
        return Err(unsupported_mode());
    }
    if envelope
        .cognition
        .as_deref()
        .is_some_and(|cognition| cognition != DETERMINISTIC_ONLY)
    {
        return Err(unsupported_cognition());
    }
    let Envelope {
        mode,
        intent,
        workflow_id,
        source,
        change,
        answers,
        ..
    } = envelope;
    let mut request = match (mode.as_str(), intent, source, change) {
        ("create", Some(intent), None, None) => CompileRequest::create(intent),
        ("edit", None, Some(source), Some(Object(change))) => {
            match (change.text, change.set_constant) {
                (Some(text), None) => CompileRequest::edit(source, text),
                (None, Some(Object(constant))) => {
                    CompileRequest::set_constant(source, constant.name, constant.value.get())
                }
                // Neither form, or both: one change request names one operation.
                _ => return Err(malformed()),
            }
        }
        // A field that belongs to the other mode, or a missing required one.
        _ => return Err(malformed()),
    };
    // Passed through on EDIT too: refusing to rename an accepted base is the
    // core's judgment, and it answers it as data.
    if let Some(workflow_id) = workflow_id {
        request = request.with_workflow_id(workflow_id);
    }
    for (key, literal) in answers.0 {
        request = request.answer(key, literal.get());
    }
    Ok(request)
}

fn malformed() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "malformed_compile_request",
        "use compile_version 1 with mode create {intent} or edit {source, change}; unknown fields, null values, duplicate keys and wrong types are refused",
    )
}

fn unsupported_version() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "compile_version_unsupported",
        "this resident speaks compile_version 1 only",
    )
}

fn unsupported_mode() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "compile_mode_unsupported",
        "mode must be create or edit",
    )
}

fn unsupported_cognition() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "compile_cognition_unsupported",
        "this resident authors with deterministicOnly cognition; no authoring model is contacted and ambient keys are never consent",
    )
}

fn limit() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "compile_limit",
        "a compile limit must be positive (repairs may be zero), representable and no wider than the configured operator ceiling; GET /v1/openapi.json describes limits",
    )
}

fn busy() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "compile_busy",
        "every compile slot is in use; retry the same request",
    )
}
