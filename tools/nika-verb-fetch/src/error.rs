// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error types for the fetch verb.

use nika_kernel::http::HttpError;

/// Errors returned by `nika_verb_fetch::run`.
///
/// `#[non_exhaustive]` so new retry-loop variants can be added in S15
/// without breaking downstream `From<VerbFetchError> for NikaError`
/// mappings.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    // ── S14-γ: retry-loop variants (S15 prep) ──────────────────────────
    /// Retry loop exhausted all attempts without a successful response.
    ///
    /// Emitted when the engine bridge's retry loop hits `max_attempts`.
    /// When the retry orchestration moves into this crate in S15, this
    /// variant becomes the primary failure mode of `run_with_retry`.
    #[error(
        "fetch: retry exhausted after {attempts} attempt(s){}: {reason}",
        .last_status.map(|s| format!(" (last status: {s})")).unwrap_or_default()
    )]
    RetryExhausted {
        attempts: u32,
        last_status: Option<u16>,
        reason: String,
    },

    /// Retry loop hit the overall deadline before `max_attempts`.
    ///
    /// Distinct from `RetryExhausted`: `DeadlineExceeded` means the
    /// wall-clock budget ran out, `RetryExhausted` means the retry
    /// count ran out. Surfacing the distinction lets callers apply
    /// different recovery strategies (e.g. retry with a longer
    /// timeout vs. give up entirely).
    #[error("fetch: deadline exceeded after {attempts}/{max_attempts} attempt(s)")]
    DeadlineExceeded { attempts: u32, max_attempts: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_exhausted_display_with_status() {
        let err = VerbFetchError::RetryExhausted {
            attempts: 3,
            last_status: Some(503),
            reason: "upstream flapping".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("3 attempt(s)"));
        assert!(msg.contains("last status: 503"));
        assert!(msg.contains("upstream flapping"));
    }

    #[test]
    fn retry_exhausted_display_without_status() {
        let err = VerbFetchError::RetryExhausted {
            attempts: 5,
            last_status: None,
            reason: "network error".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("5 attempt(s)"));
        assert!(!msg.contains("last status"));
    }

    #[test]
    fn deadline_exceeded_display() {
        let err = VerbFetchError::DeadlineExceeded {
            attempts: 2,
            max_attempts: 5,
        };
        assert_eq!(
            err.to_string(),
            "fetch: deadline exceeded after 2/5 attempt(s)"
        );
    }
}
