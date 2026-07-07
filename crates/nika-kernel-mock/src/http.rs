// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockHttp` — canned HTTP responses with call recording.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::Mutex;

use nika_kernel::http::{
    HttpError, HttpGetDyn, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse,
};

/// Programmable HTTP client for tests.
///
/// Enqueue responses with `enqueue_ok` / `enqueue_err`.
/// Inspect recorded requests with `sent_requests`.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct MockHttp {
    responses: Arc<Mutex<VecDeque<Result<HttpResponse, HttpError>>>>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl MockHttp {
    /// Create a new empty mock HTTP client.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a successful response (builder).
    #[must_use]
    pub fn enqueue_ok(self, status: u16, body: impl Into<Bytes>) -> Self {
        let resp = HttpResponse::new(status, BTreeMap::default(), body.into(), String::new());
        self.responses.lock().push_back(Ok(resp));
        self
    }

    /// Enqueue a successful response whose `final_url` differs from the
    /// request (the post-redirect landing URL) — the affordance the
    /// traverse redirect-refilter test needs (every other builder left
    /// `final_url` empty, so the redirect path was untested).
    #[must_use]
    pub fn enqueue_ok_final_url(
        self,
        status: u16,
        body: impl Into<Bytes>,
        final_url: impl Into<String>,
    ) -> Self {
        let resp = HttpResponse::new(status, BTreeMap::default(), body.into(), final_url.into());
        self.responses.lock().push_back(Ok(resp));
        self
    }

    /// Enqueue a successful response carrying headers (builder) — for
    /// consumers that read `Content-Type` & friends (header names
    /// lowercase, mirroring the production client's normalization).
    #[must_use]
    pub fn enqueue_ok_with_headers<K, V>(
        self,
        status: u16,
        headers: impl IntoIterator<Item = (K, V)>,
        body: impl Into<Bytes>,
    ) -> Self
    where
        K: Into<String>,
        V: Into<String>,
    {
        let map: BTreeMap<String, String> = headers
            .into_iter()
            .map(|(k, v)| (k.into().to_ascii_lowercase(), v.into()))
            .collect();
        let resp = HttpResponse::new(status, map, body.into(), String::new());
        self.responses.lock().push_back(Ok(resp));
        self
    }

    /// Enqueue an error response (builder).
    #[must_use]
    pub fn enqueue_err(self, error: HttpError) -> Self {
        self.responses.lock().push_back(Err(error));
        self
    }

    /// Get all recorded requests.
    #[must_use]
    pub fn sent_requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().clone()
    }

    /// Number of remaining queued responses.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.responses.lock().len()
    }

    fn pop_response(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.requests.lock().push(request);
        self.responses.lock().pop_front().unwrap_or_else(|| {
            Err(HttpError::Other {
                reason: "MockHttp: response queue exhausted".into(),
            })
        })
    }
}

impl HttpGetDyn for MockHttp {
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.pop_response(request)
    }
}

impl HttpPostDyn for MockHttp {
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.pop_response(request)
    }

    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        self.requests.lock().push(request);
        Err(HttpError::Unsupported {
            reason: "MockHttp does not support streaming".into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::HttpMethod;

    #[tokio::test]
    async fn enqueue_and_get() {
        let http = MockHttp::new().enqueue_ok(200, "ok");
        let req = HttpRequest::get("https://example.com");
        let resp = http.get(req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"ok");
    }

    #[tokio::test]
    async fn enqueue_with_headers_normalizes_names() {
        let http = MockHttp::new().enqueue_ok_with_headers(
            200,
            [("Content-Type", "text/html; charset=iso-8859-1")],
            "body",
        );
        let resp = http
            .get(HttpRequest::get("https://example.com"))
            .await
            .unwrap();
        assert_eq!(
            resp.headers.get("content-type").map(String::as_str),
            Some("text/html; charset=iso-8859-1"),
            "names lowercased like the production client"
        );
    }

    #[tokio::test]
    async fn fifo_ordering() {
        let http = MockHttp::new()
            .enqueue_ok(200, "first")
            .enqueue_ok(201, "second");
        let r1 = http.get(HttpRequest::get("a")).await.unwrap();
        let r2 = http.get(HttpRequest::get("b")).await.unwrap();
        assert_eq!(r1.status, 200);
        assert_eq!(r2.status, 201);
    }

    #[tokio::test]
    async fn empty_queue_returns_error() {
        let http = MockHttp::new();
        let result = http.get(HttpRequest::get("x")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn records_requests() {
        let http = MockHttp::new().enqueue_ok(200, "");
        let _ = http.get(HttpRequest::get("https://api.example.com")).await;
        let sent = http.sent_requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].url, "https://api.example.com");
        assert_eq!(sent[0].method, HttpMethod::Get);
    }

    #[tokio::test]
    async fn enqueue_error() {
        let http = MockHttp::new().enqueue_err(HttpError::Timeout { duration_ms: 5000 });
        let result = http.get(HttpRequest::get("x")).await;
        assert!(matches!(result, Err(HttpError::Timeout { .. })));
    }

    #[test]
    fn remaining_count() {
        let http = MockHttp::new().enqueue_ok(200, "").enqueue_ok(201, "");
        assert_eq!(http.remaining(), 2);
    }

    #[test]
    fn clone_shares_state() {
        let http1 = MockHttp::new().enqueue_ok(200, "");
        let http2 = http1.clone();
        assert_eq!(http2.remaining(), 1);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_http_is_send_sync() {
        _assert_send_sync::<MockHttp>();
    }

    #[test]
    fn mock_http_satisfies_http_client() {
        fn _accepts<T: nika_kernel::http::HttpGetDyn + nika_kernel::http::HttpPostDyn>(_: &T) {}
        let http = MockHttp::new();
        _accepts(&http);
    }
}
