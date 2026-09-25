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

use std::collections::BTreeMap;
use std::sync::Arc;

use bytes::Bytes;
use hyper::body::Incoming;
use hyper::{Request, Response, StatusCode};
use nika_onboard::compile::{AuthoringCognition, COMPILE_WIRE_VERSION, CompileRequest};
use serde_json::value::RawValue;

pub(super) use author::settle;
pub(super) use native::Seat;
pub use native::{
    NativeAuthoring, NativeAuthoringArgs, NativeAuthoringError, seat_native_authoring,
};

use super::AppState;
use super::error::{ApiError, ResponseBody};
use super::route::{
    body_too_large, collect_body, content_length, drain_oversized_body, json_response,
    refuse_snapshot_envelope, request_timeout,
};

/// Whole-request ceiling. The listener's own body ceiling still applies when lower.
pub(super) const MAX_COMPILE_BODY_BYTES: usize = 1024 * 1024;
/// `intent` and `change.text`, in UTF-8 bytes.
pub(super) const MAX_COMPILE_TEXT_BYTES: usize = 4 * 1024;
/// The accepted EDIT base, in decoded UTF-8 bytes. It bounds uncancellable CPU work.
pub(super) const MAX_COMPILE_SOURCE_BYTES: usize = 512 * 1024;
/// `workflow_id` and `change.set_constant.name`, in UTF-8 bytes.
pub(super) const MAX_COMPILE_NAME_BYTES: usize = 128;
/// Entries in `answers`, counted while the envelope is deserialized.
pub(super) const MAX_COMPILE_ANSWERS: usize = 64;
/// One `answers` key, in UTF-8 bytes.
pub(super) const MAX_COMPILE_ANSWER_KEY_BYTES: usize = 256;
/// One literal (`answers` value or `change.set_constant.value`), as sent.
pub(super) const MAX_COMPILE_LITERAL_BYTES: usize = 64 * 1024;

/// The only authoring cognition this build implements, in the core's own word.
const DETERMINISTIC_ONLY: &str = AuthoringCognition::DeterministicOnly.word();
const ANSWER_COUNT_MARKER: &str = "nika compile answer count exceeded";

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

/// A JSON object and nothing else. A derived struct would also accept a positional
/// array (`[1, "create", "hello"]`), a spelling this contract never publishes.
struct Object<T>(T);

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for Object<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct ObjectVisitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for ObjectVisitor<T> {
            type Value = T;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(self, map: M) -> Result<T, M::Error> {
                // The derived visitor still reads the ORIGINAL map, so unknown
                // fields, duplicates and raw literals are judged exactly as before.
                T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
            }
        }

        deserializer
            .deserialize_map(ObjectVisitor(std::marker::PhantomData))
            .map(Object)
    }
}

/// Literal answers keyed by stable question key, with their sent text preserved.
#[derive(Default)]
struct Answers(BTreeMap<String, Box<RawValue>>);

impl<'de> serde::Deserialize<'de> for Answers {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct AnswersVisitor;

        impl<'de> serde::de::Visitor<'de> for AnswersVisitor {
            type Value = Answers;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a bounded object of literal answers")
            }

            fn visit_map<M: serde::de::MapAccess<'de>>(
                self,
                mut map: M,
            ) -> Result<Self::Value, M::Error> {
                let mut answers = BTreeMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    if answers.len() == MAX_COMPILE_ANSWERS {
                        return Err(serde::de::Error::custom(ANSWER_COUNT_MARKER));
                    }
                    let literal = map.next_value::<Box<RawValue>>()?;
                    // Two values for one question select neither (the core's own law).
                    if answers.insert(key, literal).is_some() {
                        return Err(serde::de::Error::custom("duplicate answer key"));
                    }
                }
                Ok(Answers(answers))
            }
        }

        deserializer.deserialize_map(AnswersVisitor)
    }
}

/// A present value must have its type; JSON `null` never means "absent".
fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
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
    let ceiling = state.limits.max_body_bytes().min(MAX_COMPILE_BODY_BYTES);
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
    let envelope = match serde_json::from_slice::<Object<Envelope>>(body) {
        Ok(Object(envelope)) => envelope,
        Err(error) if error.to_string().contains(ANSWER_COUNT_MARKER) => return Err(limit()),
        Err(_) => {
            // A newer generation may carry fields this one refuses: name the
            // generation, so a client never mistakes it for a broken request.
            return Err(match serde_json::from_slice::<Object<VersionProbe>>(body) {
                Ok(Object(probe)) if probe.compile_version != u64::from(COMPILE_WIRE_VERSION) => {
                    unsupported_version()
                }
                _ => malformed(),
            });
        }
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
    within_limits(&envelope)?;
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

fn within_limits(envelope: &Envelope) -> Result<(), ApiError> {
    let text = |value: &Option<String>| {
        value
            .as_ref()
            .is_none_or(|value| value.len() <= MAX_COMPILE_TEXT_BYTES)
    };
    let name = |value: &str| value.len() <= MAX_COMPILE_NAME_BYTES;
    let bounded = text(&envelope.intent)
        && envelope.workflow_id.as_deref().is_none_or(name)
        && envelope
            .source
            .as_ref()
            .is_none_or(|source| source.len() <= MAX_COMPILE_SOURCE_BYTES)
        && change_within(envelope.change.as_ref())
        && answers_within(&envelope.answers);
    if bounded { Ok(()) } else { Err(limit()) }
}

/// A change within its bounds: its text, the constant's name and literal.
fn change_within(change: Option<&Object<Change>>) -> bool {
    change.is_none_or(|Object(change)| {
        change
            .text
            .as_ref()
            .is_none_or(|text| text.len() <= MAX_COMPILE_TEXT_BYTES)
            && change.set_constant.as_ref().is_none_or(|Object(constant)| {
                constant.name.len() <= MAX_COMPILE_NAME_BYTES
                    && constant.value.get().len() <= MAX_COMPILE_LITERAL_BYTES
            })
    })
}

/// Every answer within its bounds: its key and its literal as sent.
fn answers_within(answers: &Answers) -> bool {
    answers.0.iter().all(|(key, value)| {
        key.len() <= MAX_COMPILE_ANSWER_KEY_BYTES && value.get().len() <= MAX_COMPILE_LITERAL_BYTES
    })
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
        "a compile field exceeds its bound; GET /v1/openapi.json states every bound",
    )
}

fn busy() -> ApiError {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "compile_busy",
        "every compile slot is in use; retry the same request",
    )
}
