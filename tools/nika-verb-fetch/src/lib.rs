// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika `fetch:` verb — HTTP fetch via kernel trait.
//!
//! This crate contains the core HTTP-fetch logic for the `fetch:` verb.
//! It receives pre-validated, pre-resolved inputs from the engine bridge
//! and delegates the HTTP call to `HttpClient` via the kernel trait.
//!
//! ## Scope for S13
//!
//! The S13 implementation handles the minimal fetch path:
//! - Builds an `HttpRequest` from the input
//! - Calls `caps.http.send()` via the kernel trait
//! - Emits `FetchCompleted` events via `EventLog`
//! - Applies post-processing extraction via `nika-extract`
//! - Returns either the raw body or extracted content
//!
//! ## Out of scope for S13 (stays in engine bridge)
//!
//! - SSRF DNS pinning + redirect policy (reqwest-specific custom closure
//!   that can't be expressed through the trait in S13)
//! - Cookie jar + ETag cache + rate limiter + robots.txt (FetchAux types)
//! - response: binary CAS storage (couples to nika-media)
//! - response: full (status+headers+body JSON struct) is trivial but
//!   the engine bridge handles it in its existing code path
//!
//! Session 14 extends this crate with FetchAux + redirect policy when
//! those types move to kernel traits.

use std::sync::Arc;

use nika_core::ast::extract::ExtractMode;
use nika_event::{EventKind, EventLog};
use nika_kernel::caps::FetchCaps;
use nika_kernel::http::{HttpError, HttpMethod, HttpRequest, HttpResponse};

mod error;
pub use error::VerbFetchError;

// S14-β: pure helpers extracted from nika-engine/runtime/executor/fetch.rs.
// The engine bridge re-imports these and calls them verbatim; when the retry
// loop orchestration is extracted in S15 it will move alongside them.
pub mod hreflang;
pub mod retry;

/// Pre-validated input for the fetch verb's simple body-fetch path.
///
/// The engine bridge builds this after template resolution + SSRF
/// validation.
pub struct FetchInput<'a> {
    pub url: &'a str,
    pub method: HttpMethod,
    pub headers: Vec<(String, String)>,
    pub body: Option<bytes::Bytes>,
    pub timeout: Option<std::time::Duration>,
    pub follow_redirects: bool,
    /// Extraction mode (markdown, article, text, jsonpath, etc.). If
    /// `None`, returns the raw body as a string.
    pub extract: Option<ExtractMode>,
    /// CSS selector or JSONPath (required for text/selector/jsonpath modes).
    pub extract_selector: Option<String>,
    /// Task ID for event emission.
    pub task_id: Arc<str>,
}

/// Execute a fetch verb task via the HttpClient trait.
///
/// Returns the response body (either raw or extracted via nika-extract).
///
/// # S13 scope
///
/// This function handles the minimal fetch path. Complex features
/// (response: full, response: binary CAS storage, SSRF redirect policy)
/// remain in the engine bridge.
pub async fn run(
    input: &FetchInput<'_>,
    caps: &FetchCaps<'_>,
    event_log: &EventLog,
) -> Result<String, VerbFetchError> {
    let start = std::time::Instant::now();

    // Build the HttpRequest from the input.
    let request = HttpRequest {
        method: input.method,
        url: input.url.to_string(),
        headers: input.headers.iter().cloned().collect(),
        body: input.body.clone(),
        timeout: input.timeout,
        follow_redirects: input.follow_redirects,
    };

    // Dispatch via HttpClient trait, racing against cancellation.
    let result: Result<HttpResponse, HttpError> = tokio::select! {
        biased;
        _ = caps.cancel.cancelled() => {
            return Err(VerbFetchError::Cancelled {
                task_id: input.task_id.to_string(),
            });
        }
        r = caps.http.send(request) => r,
    };

    let response = result.map_err(VerbFetchError::from)?;
    let duration_ms = start.elapsed().as_millis() as u64;

    // Emit HttpResponse event (matches engine fetch.rs telemetry).
    event_log.emit(EventKind::HttpResponse {
        task_id: Arc::clone(&input.task_id),
        status_code: response.status,
        content_type: response.headers.get("content-type").cloned(),
        content_length: Some(response.body.len() as u64),
        elapsed_ms: duration_ms,
    });

    // HTTP error status → error.
    if !(200..300).contains(&response.status) {
        return Err(VerbFetchError::HttpStatus {
            status: response.status,
            url: input.url.to_string(),
        });
    }

    // Decode body as UTF-8 (the basic extraction path — binary/CAS
    // storage is handled by the engine bridge).
    let body_str = String::from_utf8(response.body.to_vec()).map_err(|e| {
        VerbFetchError::InvalidBody {
            reason: format!("response body is not valid UTF-8: {e}"),
        }
    })?;

    // Apply extraction if requested.
    let base_url = Some(response.final_url.as_str());
    let selector = input.extract_selector.as_deref();
    nika_extract::extract(&body_str, input.extract, selector, base_url).map_err(|e| {
        VerbFetchError::Extract {
            reason: e.to_string(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::http::HttpMethod;
    use nika_kernel_mock::clock::MockClock;
    use nika_kernel_mock::http::MockHttpClient;
    use nika_kernel_mock::policy::MockPolicyChecker;
    use nika_kernel_mock::store::MemoryBlobStore;

    #[tokio::test]
    async fn fetch_returns_body_from_mock() {
        let http = MockHttpClient::default();
        http.enqueue_ok(200, "hello world");

        let policy = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = FetchCaps::new(&http, &policy, &blobs, &clock, &cancel);
        let event_log = EventLog::new();

        let input = FetchInput {
            url: "https://example.com",
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: None,
            extract_selector: None,
            task_id: Arc::from("fetch_test"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(result.is_ok(), "fetch failed: {result:?}");
        assert_eq!(result.unwrap(), "hello world");

        // W14-A2: assert on concrete HttpResponse fields, not just the
        // variant. A 404 event leaking into a happy-path test would
        // pass a `matches!(.., HttpResponse { .. })` check but fail this.
        let events = event_log.events();
        let http_response = events
            .iter()
            .find_map(|e| match &e.kind {
                EventKind::HttpResponse {
                    status_code,
                    content_length,
                    ..
                } => Some((*status_code, *content_length)),
                _ => None,
            })
            .expect("expected HttpResponse event");
        assert_eq!(http_response.0, 200, "status_code should be 200");
        assert_eq!(
            http_response.1,
            Some(b"hello world".len() as u64),
            "content_length should match body size"
        );
    }

    /// W14-A2: exercise the `extract:` path using the zero-dep jsonpath
    /// mode. Earlier the extract wiring was uncovered — any regression
    /// in `nika_extract::extract()` or the selector/base_url plumbing
    /// would slip past the other tests.
    #[tokio::test]
    async fn fetch_extract_jsonpath_returns_selected_value() {
        let http = MockHttpClient::default();
        http.enqueue_ok(
            200,
            r#"{"user": {"name": "Alice", "age": 30}}"#,
        );

        let policy = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = FetchCaps::new(&http, &policy, &blobs, &clock, &cancel);
        let event_log = EventLog::new();

        let input = FetchInput {
            url: "https://api.example.com/user",
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: Some(ExtractMode::Jsonpath),
            extract_selector: Some("$.user.name".to_string()),
            task_id: Arc::from("fetch_jsonpath"),
        };

        let result = run(&input, &caps, &event_log).await;
        let extracted = result.expect("extract should succeed");
        // The jsonpath extractor returns a JSON value; "Alice" serializes
        // as a JSON string with surrounding quotes.
        assert!(
            extracted.contains("Alice"),
            "expected extracted value to contain 'Alice', got: {extracted}"
        );
    }

    #[tokio::test]
    async fn fetch_http_error_status_returns_error() {
        let http = MockHttpClient::default();
        http.enqueue_ok(404, "Not Found");

        let policy = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = FetchCaps::new(&http, &policy, &blobs, &clock, &cancel);
        let event_log = EventLog::new();

        let input = FetchInput {
            url: "https://example.com/missing",
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: None,
            extract_selector: None,
            task_id: Arc::from("fetch_404"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(
            result,
            Err(VerbFetchError::HttpStatus { status: 404, .. })
        ));
    }

    #[tokio::test]
    async fn fetch_propagates_http_client_error() {
        let http = MockHttpClient::default();
        http.enqueue_err(HttpError::Connection {
            reason: "network unreachable".to_string(),
        });

        let policy = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();

        let caps = FetchCaps::new(&http, &policy, &blobs, &clock, &cancel);
        let event_log = EventLog::new();

        let input = FetchInput {
            url: "https://unreachable.example.com",
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: None,
            extract_selector: None,
            task_id: Arc::from("fetch_netfail"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbFetchError::Http(_))));
    }

    #[tokio::test]
    async fn fetch_cancelled_returns_cancelled() {
        let http = MockHttpClient::default();
        // Enqueue a response but cancel BEFORE the call.
        http.enqueue_ok(200, "late");

        let policy = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        cancel.cancel(); // pre-cancel

        let caps = FetchCaps::new(&http, &policy, &blobs, &clock, &cancel);
        let event_log = EventLog::new();

        let input = FetchInput {
            url: "https://example.com",
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: None,
            extract_selector: None,
            task_id: Arc::from("fetch_cancel"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(matches!(result, Err(VerbFetchError::Cancelled { .. })));
    }
}
