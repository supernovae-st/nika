// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::convert::Infallible;
use std::fmt;
use std::io;

use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt as _, Full};
use hyper::header::{CONTENT_TYPE, WWW_AUTHENTICATE};
use hyper::{Response, StatusCode};
use thiserror::Error;

use crate::{JobStoreError, ScheduleStoreError};

/// Why a credential source was refused. Never carries a path or secret bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum CredentialRefuse {
    /// The file could not be opened or read.
    Unreadable,
    /// The path is a symlink, FIFO, or other non-regular file.
    FollowRefused,
    /// Group or world bits are set; owner-only mode 0600 is required.
    InsecureMode,
    /// Length is outside 32–512 or a byte is not ASCII graphic.
    InvalidMaterial,
}

impl fmt::Display for CredentialRefuse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Unreadable => "unreadable",
            Self::FollowRefused => "not a regular file",
            Self::InsecureMode => "insecure mode",
            Self::InvalidMaterial => "invalid material",
        })
    }
}

pub(crate) type ResponseBody = UnsyncBoxBody<Bytes, Infallible>;

pub(crate) fn once_body(bytes: impl Into<Bytes>) -> ResponseBody {
    Full::new(bytes.into()).boxed_unsync()
}

/// Typed startup and lifecycle failures from resident execution and HTTP.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerError {
    /// One explicit configuration ceiling or bind rule is invalid.
    #[error("server configuration refused: {0}")]
    InvalidConfig(&'static str),
    /// The credential source could not be acquired safely.
    #[error("server credential source refused: {0}")]
    Credential(CredentialRefuse),
    /// The held workflow registry could not be opened.
    #[error("workflow registry could not be opened: {0}")]
    WorkflowRoot(io::ErrorKind),
    /// Durable job state refused startup or mutation.
    #[error("durable job state refused: {0}")]
    JobStore(#[from] JobStoreError),
    /// Durable schedule state refused startup or mutation.
    #[error("durable schedule state refused: {0}")]
    ScheduleStore(#[from] ScheduleStoreError),
    /// The listener could not bind or accept.
    #[error("HTTP listener failed: {0}")]
    Listener(io::ErrorKind),
    /// In-flight executions exceeded the configured graceful-stop budget.
    #[error("resident execution shutdown exceeded its grace period")]
    ShutdownTimeout,
    /// An internal execution task panicked or was cancelled unexpectedly.
    #[error("resident execution task failed")]
    ExecutionTask,
    /// A blocking filesystem task panicked or was cancelled unexpectedly.
    #[error("resident execution filesystem task failed")]
    BlockingTask,
    /// The bounded durable-state command queue is full.
    #[error("resident durable-state queue is full")]
    StoreQueueFull,
    /// The shared HTTP/ARM execution queue has no admission slot.
    #[error("resident execution queue is full")]
    ExecutionQueueFull,
    /// A scheduled snapshot or provenance binding was not canonical.
    #[error("scheduled execution admission was refused")]
    ScheduledAdmission,
    /// A slot replay resolved to different immutable snapshot bytes.
    #[error("scheduled execution idempotency key conflicts with another snapshot")]
    ScheduledIdempotencyConflict,
    /// A scheduled run did not reach durable terminal observation in bounds.
    #[error("scheduled execution observation timed out")]
    ScheduledObservationTimeout,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: &'static str,
    challenge: bool,
}

impl ApiError {
    pub(crate) const fn new(status: StatusCode, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
            challenge: false,
        }
    }

    pub(crate) const fn unauthorized() -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            message: "authentication required",
            challenge: true,
        }
    }

    pub(crate) const fn job_not_found() -> Self {
        Self::new(StatusCode::NOT_FOUND, "job_not_found", "job not found")
    }

    pub(crate) const fn internal() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "request could not be completed",
        )
    }

    pub(crate) const fn store_busy() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "store_busy",
            "durable job state is temporarily unavailable",
        )
    }

    pub(crate) const fn invalid_cursor() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "invalid_cursor",
            "last-event-id must be a decimal event sequence",
        )
    }

    pub(crate) const fn cursor_beyond_latest() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            "cursor_beyond_latest",
            "last-event-id is beyond the latest persisted event",
        )
    }

    pub(crate) const fn sse_capacity() -> Self {
        Self::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "sse_capacity",
            "event stream capacity is exhausted",
        )
    }

    pub(crate) fn into_response(self) -> Response<ResponseBody> {
        let mut response = json_error(self.status, self.code, self.message);
        if self.challenge {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                hyper::header::HeaderValue::from_static("Bearer realm=\"nika\""),
            );
        }
        response
    }
}

pub(crate) fn json_error(status: StatusCode, code: &str, message: &str) -> Response<ResponseBody> {
    let body = serde_json::json!({
        "error": {"code": code, "message": message}
    });
    let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    let mut response = Response::new(once_body(bytes));
    *response.status_mut() = status;
    response.headers_mut().insert(
        CONTENT_TYPE,
        hyper::header::HeaderValue::from_static("application/json"),
    );
    response
}

pub(crate) fn capture_refused(error: &nika_execution::ExecutionError) -> Response<ResponseBody> {
    if let Some(api) = snapshot_api_error(error) {
        return api.into_response();
    }
    let (code, message) = diagnose_capture(error);
    json_error(StatusCode::UNPROCESSABLE_ENTITY, &code, &message)
}

fn snapshot_api_error(error: &nika_execution::ExecutionError) -> Option<ApiError> {
    use nika_execution::ExecutionError;

    let error = match error {
        ExecutionError::UnsupportedSnapshotFormat { .. } => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "unsupported_snapshot_version",
            "snapshot format version is not supported",
        ),
        ExecutionError::UnitDigestMismatch { .. } | ExecutionError::SnapshotDigestMismatch => {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "snapshot_tampered",
                "snapshot bytes do not match their immutable digest",
            )
        }
        ExecutionError::SnapshotStructureMismatch => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "malformed_snapshot",
            "request body is not a valid execution snapshot",
        ),
        ExecutionError::UnitCountLimit { .. } => ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "snapshot_unit_count_limit",
            "snapshot exceeds the configured unit-count limit",
        ),
        ExecutionError::UnitSizeLimit { .. } => ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "snapshot_unit_size_limit",
            "snapshot unit exceeds the configured byte limit",
        ),
        ExecutionError::TotalSizeLimit { .. } => ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "snapshot_total_size_limit",
            "snapshot exceeds the configured decoded-byte limit",
        ),
        _ => return None,
    };
    Some(error)
}

pub(crate) fn diagnose_capture(error: &nika_execution::ExecutionError) -> (String, String) {
    let raw = error.to_string();
    let code = first_nika_code(&raw).unwrap_or("admission_refused");
    (bound_token(code, 64), bound_message(&raw))
}

fn first_nika_code(raw: &str) -> Option<&str> {
    let start = raw.find("NIKA-")?;
    let rest = &raw[start..];
    let end = rest
        .find(|ch: char| !(ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-'))
        .unwrap_or(rest.len());
    let token = rest.get(..end).filter(|value| !value.is_empty())?;
    token
        .contains('-')
        .then_some(token)
        .filter(|value| value.bytes().any(|byte| byte.is_ascii_digit()))
}

fn bound_token(raw: &str, max: usize) -> String {
    raw.chars()
        .filter(char::is_ascii_graphic)
        .take(max)
        .collect()
}

fn bound_message(raw: &str) -> String {
    let mut out = String::new();
    for token in raw.split_whitespace() {
        if token.starts_with('/') || token.contains(":\\") {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(token);
        if out.len() >= 240 {
            out.truncate(240);
            break;
        }
    }
    out
}
