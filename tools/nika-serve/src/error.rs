// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for the Nika HTTP API server.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

/// Errors produced by the serve layer.
#[derive(thiserror::Error, Debug)]
pub enum ServeError {
    #[error("Not found")]
    NotFound,

    #[error("Path traversal rejected")]
    PathTraversal,

    #[error("Queue full ({0} pending jobs)")]
    QueueFull(usize),

    #[error("Invalid workflow: {0}")]
    InvalidWorkflow(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Storage error: {0}")]
    Storage(#[from] nika_storage::StorageError),

    #[error("Internal error")]
    Internal(#[source] Box<dyn std::error::Error + Send + Sync>),
}

// aide: allow ServeError in Result<T, ServeError> returns.
// Error responses are documented manually in per-route doc functions.
impl aide::OperationOutput for ServeError {
    type Inner = ();
}

impl IntoResponse for ServeError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            Self::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            Self::PathTraversal => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Forbidden(_) => (StatusCode::FORBIDDEN, self.to_string()),
            Self::QueueFull(_) => (StatusCode::SERVICE_UNAVAILABLE, self.to_string()),
            Self::InvalidWorkflow(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            Self::Config(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Configuration error".into(),
            ),
            Self::Storage(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Storage error".into()),
            Self::Internal(e) => {
                tracing::error!(error = %e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal error".into())
            }
        };
        (status, axum::Json(serde_json::json!({"error": msg}))).into_response()
    }
}
