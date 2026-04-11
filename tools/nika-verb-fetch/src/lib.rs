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
use std::time::{Duration, Instant};

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

// ─────────────────────────────────────────────────────────────────────────
//  S15-A6: retry loop wrapper
// ─────────────────────────────────────────────────────────────────────────

/// Retry policy for [`run_with_retry`].
///
/// Forward-investment helper for S15+. The engine bridge in
/// `nika-engine/src/runtime/executor/fetch.rs` still owns the production
/// retry loop (it also consumes `fetch_cache`, robots.txt, rate limiter,
/// cookie jar — couplings that require kernel trait extraction to move).
/// New call sites that don't need those features can use this wrapper
/// directly.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct RetryPolicy {
    /// Maximum number of attempts (1 = no retry).
    pub max_attempts: u32,
    /// Base backoff delay in milliseconds.
    pub base_delay_ms: u64,
    /// Exponential backoff multiplier applied per attempt.
    pub multiplier: f64,
    /// Overall wall-clock deadline across all attempts. `None` = only
    /// count-limited.
    pub deadline: Option<Duration>,
}

impl RetryPolicy {
    /// Construct a retry policy with the three mandatory knobs. Leaves
    /// `deadline = None`.
    pub fn new(max_attempts: u32, base_delay_ms: u64, multiplier: f64) -> Self {
        Self {
            max_attempts: max_attempts.max(1),
            base_delay_ms,
            multiplier,
            deadline: None,
        }
    }

    /// Builder: set an overall wall-clock deadline.
    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }
}

/// Classify an error as retryable vs terminal. 429 + 5xx are retryable;
/// everything else is terminal (validation errors, 4xx, extract failures,
/// cancels, etc.).
///
/// `VerbFetchError` is `#[non_exhaustive]` externally, but same-crate
/// matching stays exhaustive. Adding a new variant forces a compile
/// error here, which is the desired triage behavior — future retryable
/// variants must be explicitly added to the first arm-set.
fn is_retryable(err: &VerbFetchError) -> bool {
    match err {
        VerbFetchError::HttpStatus { status, .. } => *status == 429 || (500..600).contains(status),
        VerbFetchError::Http(http_err) => matches!(
            http_err,
            HttpError::Timeout { .. } | HttpError::Connection { .. }
        ),
        VerbFetchError::InvalidBody { .. }
        | VerbFetchError::Extract { .. }
        | VerbFetchError::Cancelled { .. }
        | VerbFetchError::RetryExhausted { .. }
        | VerbFetchError::DeadlineExceeded { .. } => false,
    }
}

/// Run the fetch verb with bounded retries + exponential backoff.
///
/// On success, returns the same body `run()` would. On failure, returns
/// one of:
/// - `VerbFetchError::RetryExhausted` — retryable error on the final attempt
/// - `VerbFetchError::DeadlineExceeded` — wall-clock deadline hit
/// - `VerbFetchError::Cancelled` — cancel token fired mid-attempt or mid-sleep
/// - any terminal `VerbFetchError` — non-retryable error on any attempt
///
/// Between attempts, sleeps for `safe_backoff_delay(base_delay_ms,
/// multiplier, attempt - 1)` and races the sleep against `caps.cancel`.
pub async fn run_with_retry(
    input: &FetchInput<'_>,
    caps: &FetchCaps<'_>,
    event_log: &EventLog,
    policy: &RetryPolicy,
) -> Result<String, VerbFetchError> {
    let start = Instant::now();
    let mut last_status: Option<u16> = None;
    let mut last_reason = String::from("no attempts made");

    for attempt in 1..=policy.max_attempts {
        // Check the overall wall-clock deadline BEFORE each attempt.
        if let Some(deadline) = policy.deadline {
            if start.elapsed() >= deadline {
                return Err(VerbFetchError::DeadlineExceeded {
                    attempts: attempt - 1,
                    max_attempts: policy.max_attempts,
                });
            }
        }

        match run(input, caps, event_log).await {
            Ok(body) => return Ok(body),
            Err(e) => {
                // Capture status for the eventual RetryExhausted error.
                if let VerbFetchError::HttpStatus { status, .. } = &e {
                    last_status = Some(*status);
                }

                // Terminal errors short-circuit the loop.
                if !is_retryable(&e) {
                    return Err(e);
                }

                last_reason = e.to_string();

                // Final attempt exhausted — surface the retry-count failure.
                if attempt == policy.max_attempts {
                    return Err(VerbFetchError::RetryExhausted {
                        attempts: attempt,
                        last_status,
                        reason: last_reason,
                    });
                }

                // Sleep before the next attempt, racing against cancellation.
                let delay_ms =
                    retry::safe_backoff_delay(policy.base_delay_ms, policy.multiplier, attempt - 1);
                tokio::select! {
                    biased;
                    _ = caps.cancel.cancelled() => {
                        return Err(VerbFetchError::Cancelled {
                            task_id: input.task_id.to_string(),
                        });
                    }
                    _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                }
            }
        }
    }

    // Unreachable (`max_attempts >= 1` after `RetryPolicy::new` floor),
    // but satisfy the type checker with a final retry-exhausted error.
    Err(VerbFetchError::RetryExhausted {
        attempts: policy.max_attempts,
        last_status,
        reason: last_reason,
    })
}

// S16-swarm clippy sweep: test closures use `_ => None` catch-alls
// when filtering `EventKind` variants, which tri­ps
// `clippy::wildcard_enum_match_arm`. Expanding to 100+ variant
// listings would defeat the purpose of the filter.
#[cfg(test)]
#[allow(clippy::wildcard_enum_match_arm)]
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

    // ── S15-A6: run_with_retry tests ────────────────────────────────────

    fn retry_input<'a>(url: &'a str) -> FetchInput<'a> {
        FetchInput {
            url,
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout: None,
            follow_redirects: true,
            extract: None,
            extract_selector: None,
            task_id: Arc::from("retry_test"),
        }
    }

    /// Small-backoff policy for fast tests: 2 ms base, no deadline.
    fn fast_policy(max_attempts: u32) -> RetryPolicy {
        RetryPolicy::new(max_attempts, 2, 2.0)
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let http = MockHttpClient::default();
        http.enqueue_ok(200, "body");

        let policy_check = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = FetchCaps::new(&http, &policy_check, &blobs, &clock, &cancel);
        let log = EventLog::new();

        let input = retry_input("https://ex.test");
        let result = run_with_retry(&input, &caps, &log, &fast_policy(3)).await;

        assert_eq!(result.unwrap(), "body");
        assert_eq!(http.remaining(), 0, "used exactly one enqueued response");
    }

    #[tokio::test]
    async fn retry_recovers_after_503() {
        let http = MockHttpClient::default();
        // Two 503s then a 200.
        http.enqueue_ok(503, "down");
        http.enqueue_ok(503, "still down");
        http.enqueue_ok(200, "recovered");

        let policy_check = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = FetchCaps::new(&http, &policy_check, &blobs, &clock, &cancel);
        let log = EventLog::new();

        let input = retry_input("https://ex.test");
        let result = run_with_retry(&input, &caps, &log, &fast_policy(5)).await;

        assert_eq!(result.unwrap(), "recovered");
        assert_eq!(http.remaining(), 0, "consumed all three responses");
        assert_eq!(
            http.sent_requests().len(),
            3,
            "should have issued 3 requests total"
        );
    }

    #[allow(clippy::wildcard_enum_match_arm)]
    #[tokio::test]
    async fn retry_exhausted_on_persistent_5xx() {
        let http = MockHttpClient::default();
        // Three 500s — beyond max_attempts = 3 → RetryExhausted.
        http.enqueue_ok(500, "broken");
        http.enqueue_ok(500, "still broken");
        http.enqueue_ok(500, "very broken");

        let policy_check = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = FetchCaps::new(&http, &policy_check, &blobs, &clock, &cancel);
        let log = EventLog::new();

        let input = retry_input("https://ex.test");
        let result = run_with_retry(&input, &caps, &log, &fast_policy(3)).await;

        match result.unwrap_err() {
            VerbFetchError::RetryExhausted {
                attempts,
                last_status,
                ..
            } => {
                assert_eq!(attempts, 3);
                assert_eq!(last_status, Some(500));
            }
            other => panic!("expected RetryExhausted, got {other:?}"),
        }
    }

    #[allow(clippy::wildcard_enum_match_arm)]
    #[tokio::test]
    async fn retry_terminal_4xx_does_not_retry() {
        let http = MockHttpClient::default();
        // A 404 should NOT trigger retry — short-circuit immediately.
        http.enqueue_ok(404, "not found");
        http.enqueue_ok(200, "would-be-recovery");

        let policy_check = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = FetchCaps::new(&http, &policy_check, &blobs, &clock, &cancel);
        let log = EventLog::new();

        let input = retry_input("https://ex.test");
        let result = run_with_retry(&input, &caps, &log, &fast_policy(3)).await;

        match result.unwrap_err() {
            VerbFetchError::HttpStatus { status, .. } => assert_eq!(status, 404),
            other => panic!("expected HttpStatus 404, got {other:?}"),
        }
        // The second response should still be in the queue (not consumed).
        assert_eq!(http.remaining(), 1, "second response untouched");
    }

    #[tokio::test]
    async fn retry_cancelled_mid_backoff() {
        let http = MockHttpClient::default();
        // Enqueue a 503 to trigger backoff, then a 200 that never runs.
        http.enqueue_ok(503, "down");
        http.enqueue_ok(200, "recovered");

        let policy_check = MockPolicyChecker::allow_all();
        let blobs = MemoryBlobStore::default();
        let clock = MockClock::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let caps = FetchCaps::new(&http, &policy_check, &blobs, &clock, &cancel);
        let log = EventLog::new();

        // Cancel the task before the first attempt so the backoff sleep
        // observes the cancellation immediately. `biased;` in the
        // adapter's select ensures cancel wins the race.
        cancel.cancel();

        let input = retry_input("https://ex.test");
        // Use a policy with a significant backoff so the cancel race is
        // the dominant outcome rather than the response queue.
        let policy = RetryPolicy::new(3, 500, 2.0);
        let result = run_with_retry(&input, &caps, &log, &policy).await;

        // With pre-cancel, the FIRST run() hits the cancel race in its
        // own inner select (line 94) and returns VerbFetchError::Cancelled
        // directly — no retry happens.
        assert!(
            matches!(result, Err(VerbFetchError::Cancelled { .. })),
            "got {result:?}"
        );
    }

    #[tokio::test]
    async fn retry_policy_floors_max_attempts_to_one() {
        let p = RetryPolicy::new(0, 100, 2.0);
        assert_eq!(p.max_attempts, 1);
    }

    #[tokio::test]
    async fn retry_policy_with_deadline_builder() {
        let p = RetryPolicy::new(5, 100, 2.0).with_deadline(Duration::from_secs(30));
        assert_eq!(p.max_attempts, 5);
        assert_eq!(p.deadline, Some(Duration::from_secs(30)));
    }
}
