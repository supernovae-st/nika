// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::convert::Infallible;
use std::sync::Arc;

use bytes::Bytes;
use http_body_util::{BodyExt as _, Limited};
use hyper::body::Incoming;
use hyper::header::{CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE};
use hyper::{Method, Request, Response, StatusCode};
use sha2::{Digest as _, Sha256};

use crate::{
    Admission, IdempotencyKey, JobId, MAX_EXECUTION_SNAPSHOT_METADATA_BYTES,
    MAX_EXECUTION_SNAPSHOT_PATH_BYTES, RequestDigest,
};

use super::error::{ApiError, ResponseBody, capture_refused, once_body};
use super::model::{
    HealthResponse, JobResponse, JobStatusResponse, SnapshotValidationAck, WorkflowListResponse,
    WorkflowMetadataResponse,
};
use super::registry::{list_workflows, valid_workflow_name, workflow_exists};
use super::sse;
use super::{AppState, ExecutionTask};

const IDEMPOTENCY_KEY: &str = "idempotency-key";
pub(super) const SNAPSHOT_WIRE_UNIT_CEILING: usize = 256;
const UNIT_COUNT_PROBE_MARKER: &str = "nika snapshot wire unit count exceeded";

#[derive(serde::Deserialize)]
struct SnapshotWireProbe<'a> {
    root: &'a str,
    digest: &'a str,
    #[serde(borrow)]
    units: BoundedWireUnits<'a>,
}

struct BoundedWireUnits<'a>(Vec<SnapshotWireUnit<'a>>);

#[derive(serde::Deserialize)]
struct SnapshotWireUnit<'a> {
    path: &'a str,
    digest: &'a str,
    bytes_hex: &'a str,
}

impl<'de: 'a, 'a> serde::Deserialize<'de> for BoundedWireUnits<'a> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct UnitsVisitor;

        impl<'de> serde::de::Visitor<'de> for UnitsVisitor {
            type Value = BoundedWireUnits<'de>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a bounded execution snapshot unit array")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut units = Vec::new();
                while let Some(unit) = sequence.next_element()? {
                    if units.len() == SNAPSHOT_WIRE_UNIT_CEILING {
                        return Err(serde::de::Error::custom(UNIT_COUNT_PROBE_MARKER));
                    }
                    units.push(unit);
                }
                Ok(BoundedWireUnits(units))
            }
        }

        deserializer.deserialize_seq(UnitsVisitor)
    }
}

pub(crate) async fn handle(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Result<Response<ResponseBody>, Infallible> {
    let response = if request.uri().path() == "/health" && request.method() == Method::GET {
        json_response(StatusCode::OK, &HealthResponse::current())
    } else if request.uri().path().starts_with("/v1/") {
        protected(request, state).await
    } else {
        ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found").into_response()
    };
    Ok(response)
}

async fn protected(request: Request<Incoming>, state: Arc<AppState>) -> Response<ResponseBody> {
    if !state.token.authorizes(request.headers()) {
        return ApiError::unauthorized().into_response();
    }
    if coarse_body_too_large(&request, state.limits.max_body_bytes()) {
        return ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "body_too_large",
            "request body exceeds the configured limit",
        )
        .into_response();
    }
    if request.method() == Method::GET && sse::is_events_path(request.uri().path()) {
        return sse::handle(request, state).await;
    }
    match tokio::time::timeout(
        state.limits.request_timeout(),
        route_authenticated(request, state),
    )
    .await
    {
        Ok(response) => response,
        Err(_) => ApiError::new(
            StatusCode::REQUEST_TIMEOUT,
            "request_timeout",
            "request did not complete before the deadline",
        )
        .into_response(),
    }
}

async fn route_authenticated(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Response<ResponseBody> {
    let path = request.uri().path().to_owned();
    match (request.method(), path.as_str()) {
        (&Method::POST, "/v1/jobs") => create_job(request, state).await,
        (&Method::POST, "/v1/check") => check_snapshot(request, state).await,
        (&Method::GET, "/v1/openapi.json") => {
            json_response(StatusCode::OK, &super::openapi::document())
        }
        (&Method::GET, "/v1/workflows") => list_registry(&state).await,
        (&Method::GET, path) if path.starts_with("/v1/workflows/") => {
            workflow_metadata(path, &state).await
        }
        (&Method::GET, _) => get_job(&path, &state).await,
        _ => ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found").into_response(),
    }
}

async fn create_job(request: Request<Incoming>, state: Arc<AppState>) -> Response<ResponseBody> {
    if let Some(error) = refuse_snapshot_envelope(&request) {
        return error.into_response();
    }
    let key = match idempotency_key(request.headers()) {
        Ok(key) => key,
        Err(error) => return error.into_response(),
    };
    let body = match collect_body(request, state.limits.max_body_bytes()).await {
        Ok(body) => body,
        Err(error) => return error.into_response(),
    };
    let admitted = match readmit_body(&body, &state).await {
        Ok(admitted) => admitted,
        Err(response) => return response,
    };
    let digest = RequestDigest::from_bytes(Sha256::digest(&body).into());
    let workflow = admitted.snapshot().root().to_owned();
    let Ok(world) = String::from_utf8(body.to_vec()) else {
        return malformed_snapshot_encoding().into_response();
    };
    admit_job(state, key, digest, workflow, world).await
}

async fn check_snapshot(
    request: Request<Incoming>,
    state: Arc<AppState>,
) -> Response<ResponseBody> {
    if let Some(error) = refuse_snapshot_envelope(&request) {
        return error.into_response();
    }
    let body = match collect_body(request, state.limits.max_body_bytes()).await {
        Ok(body) => body,
        Err(error) => return error.into_response(),
    };
    match readmit_body(&body, &state).await {
        Ok(admitted) => json_response(
            StatusCode::OK,
            &SnapshotValidationAck::accepted(admitted.snapshot()),
        ),
        Err(response) => response,
    }
}

async fn readmit_body(
    body: &[u8],
    state: &AppState,
) -> Result<nika_execution::AdmittedExecution, Response<ResponseBody>> {
    let encoded = std::str::from_utf8(body)
        .map_err(|_| malformed_snapshot_encoding().into_response())?
        .to_owned();
    validate_wire_envelope(&encoded).map_err(ApiError::into_response)?;
    let service = state.service;
    let limits = state.snapshot_limits;
    let admitted = tokio::task::spawn_blocking(move || {
        let snapshot = nika_execution::ExecutionSnapshot::decode_with_limits(&encoded, limits)?;
        service.readmit_snapshot(snapshot)
    })
    .await
    .map_err(|_| admission_refused().into_response())?;
    admitted.map_err(|error| capture_refused(&error))
}

fn admission_refused() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "admission_refused",
        "workflow world could not be captured",
    )
}

fn refuse_snapshot_envelope(request: &Request<Incoming>) -> Option<ApiError> {
    if !is_json(request.headers()) {
        return Some(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "content-type must be application/json",
        ));
    }
    if request.headers().contains_key(CONTENT_ENCODING) {
        return Some(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_content_encoding",
            "compressed request bodies are not accepted",
        ));
    }
    None
}

fn malformed_snapshot_encoding() -> ApiError {
    ApiError::new(
        StatusCode::UNPROCESSABLE_ENTITY,
        "malformed_snapshot",
        "request body is not a UTF-8 execution snapshot",
    )
}

fn validate_wire_envelope(encoded: &str) -> Result<(), ApiError> {
    let probe = match serde_json::from_str::<SnapshotWireProbe<'_>>(encoded) {
        Ok(probe) => probe,
        Err(error) if error.to_string().contains(UNIT_COUNT_PROBE_MARKER) => {
            return Err(ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "snapshot_unit_count_limit",
                "snapshot exceeds the wire unit-count limit",
            ));
        }
        Err(_) => {
            return Err(ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "malformed_snapshot",
                "request body is not a valid execution snapshot",
            ));
        }
    };
    if probe.root.len() > MAX_EXECUTION_SNAPSHOT_PATH_BYTES
        || probe
            .units
            .0
            .iter()
            .any(|unit| unit.path.len() > MAX_EXECUTION_SNAPSHOT_PATH_BYTES)
    {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "snapshot_path_limit",
            "snapshot logical path exceeds the encoded metadata limit",
        ));
    }
    if !canonical_digest(probe.digest)
        || probe
            .units
            .0
            .iter()
            .any(|unit| !canonical_digest(unit.digest))
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "malformed_snapshot_digest",
            "snapshot digests must be canonical lowercase SHA-256",
        ));
    }
    if probe
        .units
        .0
        .iter()
        .any(|unit| malformed_hex(unit.bytes_hex))
    {
        return Err(ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "malformed_snapshot_hex",
            "snapshot unit bytes must be even-length lowercase hexadecimal",
        ));
    }
    let hex_bytes = probe
        .units
        .0
        .iter()
        .try_fold(0usize, |total, unit| {
            total.checked_add(unit.bytes_hex.len())
        })
        .ok_or_else(snapshot_metadata_limit)?;
    if encoded.len().saturating_sub(hex_bytes) > MAX_EXECUTION_SNAPSHOT_METADATA_BYTES {
        return Err(snapshot_metadata_limit());
    }
    Ok(())
}

fn canonical_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn malformed_hex(value: &str) -> bool {
    !value.len().is_multiple_of(2)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn snapshot_metadata_limit() -> ApiError {
    ApiError::new(
        StatusCode::PAYLOAD_TOO_LARGE,
        "snapshot_metadata_limit",
        "snapshot exceeds the encoded JSON metadata limit",
    )
}

async fn admit_job(
    state: Arc<AppState>,
    key: IdempotencyKey,
    digest: RequestDigest,
    workflow: String,
    world: String,
) -> Response<ResponseBody> {
    let Ok(permit) = state.jobs.try_reserve() else {
        return queue_full();
    };
    let admission = match state
        .store
        .create_or_replay(
            key,
            digest,
            state.limits.max_jobs(),
            workflow.clone(),
            world,
        )
        .await
    {
        Ok(admission) => admission,
        Err(super::ServerError::JobStore(crate::JobStoreError::CapacityExceeded)) => {
            return job_capacity();
        }
        Err(
            super::ServerError::JobStore(crate::JobStoreError::Busy)
            | super::ServerError::StoreQueueFull,
        ) => return store_busy(),
        Err(_) => return internal_error(),
    };
    enqueue_admission(permit, admission)
}

fn enqueue_admission(
    permit: tokio::sync::mpsc::Permit<'_, ExecutionTask>,
    admission: Admission,
) -> Response<ResponseBody> {
    match admission {
        Admission::Conflict(_) => ApiError::new(
            StatusCode::CONFLICT,
            "idempotency_conflict",
            "idempotency key is already bound to another request",
        )
        .into_response(),
        Admission::Created(record) => {
            permit.send(ExecutionTask::new(record.id().clone()));
            json_response(StatusCode::ACCEPTED, &JobResponse::from(&record))
        }
        Admission::Existing(record) => replay_existing(permit, &record),
    }
}

fn replay_existing(
    permit: tokio::sync::mpsc::Permit<'_, ExecutionTask>,
    record: &crate::JobRecord,
) -> Response<ResponseBody> {
    if record.status() == crate::JobStatus::Queued {
        permit.send(ExecutionTask::new(record.id().clone()));
    }
    json_response(StatusCode::OK, &JobResponse::from(record))
}

async fn list_registry(state: &AppState) -> Response<ResponseBody> {
    let project = Arc::clone(&state.project);
    let limits = state.snapshot_limits;
    let listed = tokio::task::spawn_blocking(move || list_workflows(&project, limits)).await;
    match listed {
        Ok(Ok(workflows)) => json_response(StatusCode::OK, &WorkflowListResponse::new(workflows)),
        Ok(Err(
            super::ServerError::JobStore(crate::JobStoreError::Busy)
            | super::ServerError::StoreQueueFull,
        )) => store_busy(),
        _ => internal_error(),
    }
}

async fn workflow_metadata(path: &str, state: &AppState) -> Response<ResponseBody> {
    let Some(name) = path.strip_prefix("/v1/workflows/") else {
        return ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found")
            .into_response();
    };
    if name.is_empty() || !valid_workflow_name(name) {
        return ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found")
            .into_response();
    }
    let project = Arc::clone(&state.project);
    let lookup = name.to_owned();
    let exists = tokio::task::spawn_blocking(move || workflow_exists(&project, &lookup)).await;
    match exists {
        Ok(true) => json_response(StatusCode::OK, &WorkflowMetadataResponse::new(name)),
        Ok(false) => {
            ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found").into_response()
        }
        Err(_) => internal_error(),
    }
}

async fn get_job(path: &str, state: &AppState) -> Response<ResponseBody> {
    let Some((id, status_only)) = job_route(path) else {
        return ApiError::new(StatusCode::NOT_FOUND, "not_found", "route not found")
            .into_response();
    };
    let Ok(id) = JobId::parse(id) else {
        return job_not_found();
    };
    let record = match state.store.get(id).await {
        Ok(Some(record)) => record,
        Ok(None) => return job_not_found(),
        Err(
            super::ServerError::JobStore(crate::JobStoreError::Busy)
            | super::ServerError::StoreQueueFull,
        ) => return store_busy(),
        Err(_) => return internal_error(),
    };
    if status_only {
        json_response(StatusCode::OK, &JobStatusResponse::from(&record))
    } else {
        json_response(StatusCode::OK, &JobResponse::from(&record))
    }
}

fn job_route(path: &str) -> Option<(&str, bool)> {
    let tail = path.strip_prefix("/v1/jobs/")?;
    if tail.is_empty() || tail.contains('/') && !tail.ends_with("/status") {
        return None;
    }
    if let Some(id) = tail.strip_suffix("/status") {
        (!id.is_empty() && !id.contains('/')).then_some((id, true))
    } else {
        Some((tail, false))
    }
}

async fn collect_body(request: Request<Incoming>, limit: usize) -> Result<Bytes, ApiError> {
    Limited::new(request.into_body(), limit)
        .collect()
        .await
        .map(http_body_util::Collected::to_bytes)
        .map_err(|_| {
            ApiError::new(
                StatusCode::PAYLOAD_TOO_LARGE,
                "body_too_large",
                "request body exceeds the configured limit",
            )
        })
}

fn idempotency_key(headers: &hyper::HeaderMap) -> Result<IdempotencyKey, ApiError> {
    let mut values = headers.get_all(IDEMPOTENCY_KEY).iter();
    let value = values.next().ok_or_else(invalid_idempotency_key)?;
    if values.next().is_some() {
        return Err(invalid_idempotency_key());
    }
    let value = value.to_str().map_err(|_| invalid_idempotency_key())?;
    IdempotencyKey::new(value).map_err(|_| invalid_idempotency_key())
}

fn invalid_idempotency_key() -> ApiError {
    ApiError::new(
        StatusCode::BAD_REQUEST,
        "invalid_idempotency_key",
        "exactly one bounded idempotency key is required",
    )
}

fn coarse_body_too_large(request: &Request<Incoming>, limit: usize) -> bool {
    let mut values = request.headers().get_all(CONTENT_LENGTH).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return true;
    }
    value
        .to_str()
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .is_none_or(|length| length > limit)
}

fn is_json(headers: &hyper::HeaderMap) -> bool {
    let mut values = headers.get_all(CONTENT_TYPE).iter();
    let Some(value) = values.next() else {
        return false;
    };
    if values.next().is_some() {
        return false;
    }
    value.to_str().is_ok_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
    })
}

fn json_response<T: serde::Serialize>(status: StatusCode, value: &T) -> Response<ResponseBody> {
    let bytes = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = Response::new(once_body(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

fn job_not_found() -> Response<ResponseBody> {
    ApiError::job_not_found().into_response()
}

fn internal_error() -> Response<ResponseBody> {
    ApiError::internal().into_response()
}

fn queue_full() -> Response<ResponseBody> {
    ApiError::new(
        StatusCode::SERVICE_UNAVAILABLE,
        "queue_full",
        "execution queue is at capacity",
    )
    .into_response()
}

fn job_capacity() -> Response<ResponseBody> {
    ApiError::new(
        StatusCode::INSUFFICIENT_STORAGE,
        "job_capacity",
        "durable job capacity is exhausted",
    )
    .into_response()
}

fn store_busy() -> Response<ResponseBody> {
    ApiError::store_busy().into_response()
}
