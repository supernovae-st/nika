// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HttpClient trait — async HTTP abstraction replacing 16+ raw reqwest sites.
//!
//! Wire points: TaskExecutor::http_client, FetchTool::client,
//! RigProvider::OpenAiCompat, registry, robots, webhook, provider_checker.

use std::collections::HashMap;
use std::time::Duration;

use bytes::Bytes;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
}

/// An HTTP request to send.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Option<Bytes>,
    pub timeout: Option<Duration>,
    pub follow_redirects: bool,
}

/// An HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Bytes,
    pub final_url: String,
}

/// Streaming HTTP response. Body is delivered as an async stream of chunks
/// so callers can enforce size limits / extract partial content without buffering.
pub struct HttpStreamResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub final_url: String,
    pub content_length: Option<u64>,
    pub body: std::pin::Pin<Box<dyn futures_core::Stream<Item = Result<Bytes, HttpError>> + Send>>,
}

impl std::fmt::Debug for HttpStreamResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpStreamResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("final_url", &self.final_url)
            .field("content_length", &self.content_length)
            .field("body", &"<stream>")
            .finish()
    }
}

/// Async HTTP client trait.
///
/// Production: `ReqwestClient` wrapping `reqwest::Client`.
/// Tests: `MockHttpClient` with programmable response queue.
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    /// Send an HTTP request and return the response.
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;

    /// Send a request and return a streaming response. Used by `fetch:` for
    /// early-abort on size limits (50 MB hard cap by default). Default impl
    /// returns `HttpError::Unsupported` so existing mocks compile unchanged.
    async fn send_streaming(
        &self,
        _request: HttpRequest,
    ) -> Result<HttpStreamResponse, HttpError> {
        Err(HttpError::Unsupported {
            feature: "send_streaming".into(),
        })
    }
}

/// HTTP client errors.
#[derive(Debug, thiserror::Error)]
pub enum HttpError {
    #[error("HTTP timeout after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("Connection error: {reason}")]
    Connection { reason: String },

    #[error("SSRF blocked: {url}")]
    SsrfBlocked { url: String },

    #[error("response exceeded size limit: {limit_bytes} bytes")]
    TooLarge { limit_bytes: u64 },

    #[error("feature unsupported by this client: {feature}")]
    Unsupported { feature: String },

    #[error("HTTP error: {reason}")]
    Other { reason: String },
}

impl HttpRequest {
    /// Create a simple GET request.
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: HashMap::new(),
            body: None,
            timeout: None,
            follow_redirects: true,
        }
    }

    /// Create a POST request with a JSON body.
    pub fn post_json(url: impl Into<String>, body: &serde_json::Value) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "application/json".to_string());
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers,
            body: Some(Bytes::from(body.to_string())),
            timeout: None,
            follow_redirects: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct DummyClient;
    #[async_trait::async_trait]
    impl HttpClient for DummyClient {
        async fn send(&self, _: HttpRequest) -> Result<HttpResponse, HttpError> {
            unreachable!()
        }
    }

    #[tokio::test]
    async fn default_send_streaming_errors_unsupported() {
        let err = DummyClient
            .send_streaming(HttpRequest::get("https://x"))
            .await
            .unwrap_err();
        assert!(matches!(err, HttpError::Unsupported { .. }));
    }

    #[test]
    fn too_large_error_displays() {
        let e = HttpError::TooLarge {
            limit_bytes: 52_428_800,
        };
        assert_eq!(
            format!("{e}"),
            "response exceeded size limit: 52428800 bytes"
        );
    }

    #[test]
    fn stream_response_debug() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        // Minimal empty stream impl (no utility crate needed)
        struct EmptyStream;
        impl futures_core::Stream for EmptyStream {
            type Item = Result<Bytes, HttpError>;
            fn poll_next(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
                Poll::Ready(None)
            }
        }

        let resp = HttpStreamResponse {
            status: 200,
            headers: HashMap::new(),
            final_url: "https://x".into(),
            content_length: Some(1024),
            body: Box::pin(EmptyStream),
        };
        let dbg = format!("{resp:?}");
        assert!(dbg.contains("HttpStreamResponse"));
        assert!(dbg.contains("<stream>"));
    }
}
