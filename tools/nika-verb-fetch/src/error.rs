// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for the fetch verb.

use nika_kernel::http::HttpError;

/// Errors returned by `nika_verb_fetch::run`.
#[derive(Debug, thiserror::Error)]
pub enum VerbFetchError {
    /// HTTP client error (network, timeout, SSRF block, etc.).
    #[error(transparent)]
    Http(#[from] HttpError),

    /// HTTP response had a non-2xx status code.
    #[error("fetch: HTTP {status} from {url}")]
    HttpStatus { status: u16, url: String },

    /// Response body was not valid UTF-8.
    #[error("fetch: {reason}")]
    InvalidBody { reason: String },

    /// Extraction pipeline failure.
    #[error("fetch: extraction failed: {reason}")]
    Extract { reason: String },

    /// Task was cancelled.
    #[error("fetch: task '{task_id}' cancelled")]
    Cancelled { task_id: String },
}
