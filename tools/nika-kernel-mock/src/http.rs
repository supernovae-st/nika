// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MockHttpClient — programmable HTTP responses for tests.

use std::collections::VecDeque;
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::Mutex;

use nika_kernel::http::{HttpClient, HttpError, HttpRequest, HttpResponse};

/// A mock HTTP client with a programmable response queue.
///
/// Responses are consumed in FIFO order. If the queue is empty,
/// returns a 500 error.
#[derive(Clone, Default)]
pub struct MockHttpClient {
    responses: Arc<Mutex<VecDeque<Result<HttpResponse, HttpError>>>>,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl MockHttpClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a successful response.
    pub fn enqueue_ok(&self, status: u16, body: impl Into<Bytes>) {
        self.responses.lock().push_back(Ok(HttpResponse::new(
            status,
            Default::default(),
            body.into(),
            String::new(),
        )));
    }

    /// Enqueue a JSON response.
    pub fn enqueue_json(&self, status: u16, json: &serde_json::Value) {
        self.enqueue_ok(status, Bytes::from(json.to_string()));
    }

    /// Enqueue an error response.
    pub fn enqueue_err(&self, error: HttpError) {
        self.responses.lock().push_back(Err(error));
    }

    /// Get all requests that were sent (for assertions).
    pub fn sent_requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().clone()
    }

    /// Number of remaining enqueued responses.
    pub fn remaining(&self) -> usize {
        self.responses.lock().len()
    }
}

#[async_trait::async_trait]
impl HttpClient for MockHttpClient {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        self.requests.lock().push(request);
        self.responses
            .lock()
            .pop_front()
            .unwrap_or_else(|| {
                Err(HttpError::Other {
                    reason: "MockHttpClient: no more enqueued responses".to_string(),
                })
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn enqueue_and_send() {
        let client = MockHttpClient::new();
        client.enqueue_ok(200, "OK");
        let resp = client.send(HttpRequest::get("https://example.com")).await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body.as_ref(), b"OK");
    }

    #[tokio::test]
    async fn enqueue_json() {
        let client = MockHttpClient::new();
        client.enqueue_json(200, &json!({"status": "ok"}));
        let resp = client.send(HttpRequest::get("https://api.example.com")).await.unwrap();
        assert_eq!(resp.status, 200);
        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(body["status"], "ok");
    }

    #[tokio::test]
    async fn empty_queue_returns_error() {
        let client = MockHttpClient::new();
        let err = client.send(HttpRequest::get("https://empty.com")).await.unwrap_err();
        assert!(matches!(err, HttpError::Other { .. }));
    }

    #[tokio::test]
    async fn tracks_sent_requests() {
        let client = MockHttpClient::new();
        client.enqueue_ok(200, "");
        client.enqueue_ok(201, "");
        client.send(HttpRequest::get("https://a.com")).await.unwrap();
        client.send(HttpRequest::get("https://b.com")).await.unwrap();
        let sent = client.sent_requests();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].url, "https://a.com");
        assert_eq!(sent[1].url, "https://b.com");
    }

    #[tokio::test]
    async fn enqueue_error() {
        let client = MockHttpClient::new();
        client.enqueue_err(HttpError::Timeout { duration_ms: 5000 });
        let err = client.send(HttpRequest::get("https://slow.com")).await.unwrap_err();
        assert!(matches!(err, HttpError::Timeout { duration_ms: 5000 }));
    }
}
