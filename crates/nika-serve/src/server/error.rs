// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use std::io;

use bytes::Bytes;
use http_body_util::Full;
use hyper::header::{CONTENT_TYPE, WWW_AUTHENTICATE};
use hyper::{Response, StatusCode};
use thiserror::Error;

use crate::JobStoreError;

pub(crate) type ResponseBody = Full<Bytes>;

/// Typed startup and lifecycle failures from the HTTP server.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ServerError {
    /// One explicit configuration ceiling or bind rule is invalid.
    #[error("server configuration refused: {0}")]
    InvalidConfig(&'static str),
    /// The credential source could not be acquired safely.
    #[error("server credential source refused")]
    Credential,
    /// The held workflow registry could not be opened.
    #[error("workflow registry could not be opened: {0}")]
    WorkflowRoot(io::ErrorKind),
    /// Durable job state refused startup or mutation.
    #[error("durable job state refused: {0}")]
    JobStore(#[from] JobStoreError),
    /// The listener could not bind or accept.
    #[error("HTTP listener failed: {0}")]
    Listener(io::ErrorKind),
    /// In-flight executions exceeded the configured graceful-stop budget.
    #[error("HTTP server shutdown exceeded its grace period")]
    ShutdownTimeout,
    /// An internal execution task panicked or was cancelled unexpectedly.
    #[error("HTTP server execution task failed")]
    ExecutionTask,
    /// A blocking filesystem task panicked or was cancelled unexpectedly.
    #[error("HTTP server filesystem task failed")]
    BlockingTask,
    /// The bounded durable-state command queue is full.
    #[error("HTTP server durable-state queue is full")]
    StoreQueueFull,
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

    pub(crate) fn into_response(self) -> Response<ResponseBody> {
        let body = serde_json::json!({
            "error": {"code": self.code, "message": self.message}
        });
        let bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
        let mut response = Response::new(Full::new(Bytes::from(bytes)));
        *response.status_mut() = self.status;
        response.headers_mut().insert(
            CONTENT_TYPE,
            hyper::header::HeaderValue::from_static("application/json"),
        );
        if self.challenge {
            response.headers_mut().insert(
                WWW_AUTHENTICATE,
                hyper::header::HeaderValue::from_static("Bearer realm=\"nika\""),
            );
        }
        response
    }
}
