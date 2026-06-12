// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! HTTP client traits — ISP decomposition of async HTTP operations.
//!
//! 2 atomic traits: `HttpGet`, `HttpPost`.
//! 1 super-trait: `HttpClient` (blanket for both).

use std::collections::BTreeMap;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;
use futures_core::Stream;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum HttpMethod {
    /// HTTP GET.
    Get,
    /// HTTP POST.
    Post,
    /// HTTP PUT.
    Put,
    /// HTTP PATCH.
    Patch,
    /// HTTP DELETE.
    Delete,
    /// HTTP HEAD.
    Head,
    /// HTTP OPTIONS.
    Options,
}

impl std::fmt::Display for HttpMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Get => f.write_str("GET"),
            Self::Post => f.write_str("POST"),
            Self::Put => f.write_str("PUT"),
            Self::Patch => f.write_str("PATCH"),
            Self::Delete => f.write_str("DELETE"),
            Self::Head => f.write_str("HEAD"),
            Self::Options => f.write_str("OPTIONS"),
        }
    }
}

/// An HTTP request.
///
/// `Debug` is hand-written: credential-bearing header VALUES are redacted
/// (`authorization` · `x-api-key` · `x-goog-api-key` · `cookie` · …) so a
/// `{:?}` log of a request can never leak an API key — the zeroize
/// guarantee on `Secret` would otherwise end at the header map.
#[derive(Clone)]
#[non_exhaustive]
pub struct HttpRequest {
    /// HTTP method.
    pub method: HttpMethod,
    /// Target URL.
    pub url: String,
    /// Request headers.
    pub headers: BTreeMap<String, String>,
    /// Optional request body.
    pub body: Option<Bytes>,
    /// Request timeout.
    pub timeout: Option<Duration>,
    /// Whether to follow redirects.
    pub follow_redirects: bool,
}

impl HttpRequest {
    /// Create a GET request for the given URL.
    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Get,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            timeout: None,
            follow_redirects: true,
        }
    }

    /// Create a POST request for the given URL.
    #[must_use]
    pub fn post(url: impl Into<String>) -> Self {
        Self {
            method: HttpMethod::Post,
            url: url.into(),
            headers: BTreeMap::new(),
            body: None,
            timeout: None,
            follow_redirects: true,
        }
    }
}

/// An HTTP response.
///
/// `Debug` is hand-written: `set-cookie` (and sibling credential headers)
/// values are redacted — session tokens are credentials too.
#[derive(Clone)]
#[non_exhaustive]
pub struct HttpResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers — the IMPL CONTRACT every producer upholds
    /// (reqwest natively · mocks at enqueue): names are LOWERCASE, and
    /// repeated field lines arrive COMMA-JOINED per RFC 9110 §5.2
    /// (`set-cookie` excepted: comma-unsafe, last wins). Consumers may
    /// exact-match lowercase keys (`headers.get("content-type")`).
    pub headers: BTreeMap<String, String>,
    /// Response body.
    pub body: Bytes,
    /// Final URL after redirects.
    pub final_url: String,
}

impl HttpResponse {
    /// Create a new HTTP response.
    #[must_use]
    pub fn new(
        status: u16,
        headers: BTreeMap<String, String>,
        body: Bytes,
        final_url: impl Into<String>,
    ) -> Self {
        Self {
            status,
            headers,
            body,
            final_url: final_url.into(),
        }
    }
}

/// A streaming HTTP response.
#[non_exhaustive]
pub struct HttpStreamResponse {
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: BTreeMap<String, String>,
    /// Final URL after redirects.
    pub final_url: String,
    /// Content length (if known from header).
    pub content_length: Option<u64>,
    /// Body as an async stream of chunks.
    pub body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
}

impl HttpStreamResponse {
    /// Create a new streaming HTTP response.
    ///
    /// Required by invariant #19 (`#[non_exhaustive]` + `pub fn new()`).
    #[must_use]
    pub fn new(
        status: u16,
        headers: BTreeMap<String, String>,
        final_url: impl Into<String>,
        content_length: Option<u64>,
        body: Pin<Box<dyn Stream<Item = Result<Bytes, HttpError>> + Send>>,
    ) -> Self {
        Self {
            status,
            headers,
            final_url: final_url.into(),
            content_length,
            body,
        }
    }
}

/// Header names whose VALUES are redacted in `Debug` output (compared
/// case-insensitively). Names stay visible — presence is diagnostic signal,
/// values are credentials.
const SENSITIVE_HEADERS: [&str; 8] = [
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "x-goog-api-key",
    "api-key",
    "x-auth-token",
    "cookie",
    "set-cookie",
];

fn redacted_headers(headers: &BTreeMap<String, String>) -> BTreeMap<&str, &str> {
    headers
        .iter()
        .map(|(k, v)| {
            if SENSITIVE_HEADERS.iter().any(|s| k.eq_ignore_ascii_case(s)) {
                (k.as_str(), "***")
            } else {
                (k.as_str(), v.as_str())
            }
        })
        .collect()
}

impl std::fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &redacted_headers(&self.headers))
            .field(
                "body",
                &self.body.as_ref().map(|b| format!("<{} bytes>", b.len())),
            )
            .field("timeout", &self.timeout)
            .field("follow_redirects", &self.follow_redirects)
            .finish()
    }
}

impl std::fmt::Debug for HttpResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpResponse")
            .field("status", &self.status)
            .field("headers", &redacted_headers(&self.headers))
            .field("body", &format_args!("<{} bytes>", self.body.len()))
            .field("final_url", &self.final_url)
            .finish()
    }
}

impl std::fmt::Debug for HttpStreamResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpStreamResponse")
            .field("status", &self.status)
            // Same redaction as HttpResponse — a streamed response's
            // set-cookie is a credential too (the raw map here was the
            // one Debug path that leaked · review swarm 2026-06-12).
            .field("headers", &redacted_headers(&self.headers))
            .field("final_url", &self.final_url)
            .field("content_length", &self.content_length)
            .field("body", &"<stream>")
            .finish()
    }
}

/// HTTP errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum HttpError {
    /// Request timed out.
    #[error("HTTP request timed out after {duration_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        duration_ms: u64,
    },

    /// Connection failed.
    #[error("HTTP connection failed: {reason}")]
    Connection {
        /// Connection failure reason.
        reason: String,
    },

    /// SSRF protection blocked the URL.
    #[error("SSRF blocked: {url}")]
    SsrfBlocked {
        /// The blocked URL.
        url: String,
    },

    /// Response body too large.
    #[error("HTTP response too large: {size} bytes (max {max})")]
    TooLarge {
        /// Actual size.
        size: u64,
        /// Maximum allowed size.
        max: u64,
    },

    /// Unsupported operation.
    #[error("HTTP operation unsupported: {reason}")]
    Unsupported {
        /// Description.
        reason: String,
    },

    /// Other HTTP error.
    ///
    /// Retryability contract: consumers treat `Other` as NON-transient
    /// (only `Timeout`/`Connection` earn a retry — the fetch builtin's
    /// table). Impls MUST map genuinely transient transport conditions
    /// to those variants, never park them here.
    #[error("HTTP error: {reason}")]
    Other {
        /// Error description.
        reason: String,
    },
}

/// HTTP GET operations.
#[trait_variant::make(HttpGetDyn: Send)]
pub trait HttpGet: Send + Sync {
    /// Send a GET request.
    ///
    /// CANCEL SAFETY: cancel-safe. Dropping the future aborts the socket
    /// read via `reqwest` / `hyper` drop logic. Response bytes not yet
    /// read are discarded by the transport; the server-side effect of
    /// GET is (by HTTP spec) idempotent and retryable.
    async fn get(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;
}

/// HTTP POST operations (includes streaming responses).
#[trait_variant::make(HttpPostDyn: Send)]
pub trait HttpPost: Send + Sync {
    /// Send a POST request.
    ///
    /// CANCEL SAFETY: NOT cancel-safe at the application layer. A dropped
    /// future may have already delivered the request body to the remote
    /// (causing a mutation) while the response read was cancelled. Callers
    /// on non-idempotent paths MUST pair POST with an idempotency key or
    /// accept that retry-on-cancel may double-commit.
    async fn post(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;

    /// Send a request and receive a streaming response.
    ///
    /// CANCEL SAFETY: cancel-safe from the reader side (drop the stream
    /// and the connection is released). Request-side safety follows
    /// `post` rules above.
    async fn send_streaming(&self, request: HttpRequest) -> Result<HttpStreamResponse, HttpError>;
}

/// Full HTTP client — blanket super-trait.
pub trait HttpClient: HttpGet + HttpPost {}
impl<T: HttpGet + HttpPost> HttpClient for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn http_method_display() {
        assert_eq!(HttpMethod::Get.to_string(), "GET");
        assert_eq!(HttpMethod::Post.to_string(), "POST");
        assert_eq!(HttpMethod::Delete.to_string(), "DELETE");
    }

    #[test]
    fn http_request_get_defaults() {
        let req = HttpRequest::get("https://example.com");
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://example.com");
        assert!(req.headers.is_empty());
        assert!(req.body.is_none());
        assert!(req.timeout.is_none());
        assert!(req.follow_redirects);
    }

    #[test]
    fn http_request_post_defaults() {
        let req = HttpRequest::post("https://api.example.com/data");
        assert_eq!(req.method, HttpMethod::Post);
        assert!(req.follow_redirects);
    }

    #[test]
    fn http_response_new() {
        let resp = HttpResponse::new(
            200,
            BTreeMap::new(),
            Bytes::from_static(b"ok"),
            "https://example.com",
        );
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, Bytes::from_static(b"ok"));
    }

    #[test]
    fn http_error_timeout_display() {
        let err = HttpError::Timeout { duration_ms: 5000 };
        assert_eq!(err.to_string(), "HTTP request timed out after 5000ms");
    }

    #[test]
    fn http_error_ssrf_display() {
        let err = HttpError::SsrfBlocked {
            url: "http://169.254.169.254".into(),
        };
        assert!(err.to_string().contains("SSRF blocked"));
    }

    #[test]
    fn http_error_connection_display() {
        let err = HttpError::Connection {
            reason: "DNS resolution failed".into(),
        };
        assert!(err.to_string().contains("connection failed"));
    }

    #[test]
    fn http_stream_response_new_constructor() {
        use std::task::{Context, Poll};

        struct EmptyStream;
        impl futures_core::Stream for EmptyStream {
            type Item = Result<Bytes, HttpError>;
            fn poll_next(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                Poll::Ready(None)
            }
        }

        let resp = HttpStreamResponse::new(
            200,
            BTreeMap::new(),
            "https://example.com",
            Some(100),
            Box::pin(EmptyStream),
        );
        assert_eq!(resp.status, 200);
        assert_eq!(resp.final_url, "https://example.com");
        assert_eq!(resp.content_length, Some(100));
    }

    #[test]
    fn http_stream_response_debug_format() {
        use std::task::{Context, Poll};

        struct EmptyStream;
        impl futures_core::Stream for EmptyStream {
            type Item = Result<Bytes, HttpError>;
            fn poll_next(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                Poll::Ready(None)
            }
        }

        let resp = HttpStreamResponse::new(
            200,
            BTreeMap::new(),
            "https://example.com",
            Some(100),
            Box::pin(EmptyStream),
        );
        let debug = format!("{resp:?}");
        assert!(debug.contains("HttpStreamResponse"));
        assert!(debug.contains("200"));
        assert!(debug.contains("<stream>"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn http_types_send_sync() {
        _assert_send_sync::<HttpRequest>();
        _assert_send_sync::<HttpResponse>();
    }

    #[test]
    fn request_debug_redacts_credential_headers() {
        let mut req = HttpRequest::post("https://api.example.com/v1");
        req.headers
            .insert("authorization".to_owned(), "Bearer sk-LEAK".to_owned());
        req.headers
            .insert("X-Api-Key".to_owned(), "sk-ant-LEAK".to_owned());
        req.headers
            .insert("x-goog-api-key".to_owned(), "g-LEAK".to_owned());
        req.headers
            .insert("content-type".to_owned(), "application/json".to_owned());
        req.body = Some(Bytes::from_static(b"{\"secret\":\"in-body\"}"));

        let debug = format!("{req:?}");
        assert!(!debug.contains("sk-LEAK"), "bearer redacted: {debug}");
        assert!(
            !debug.contains("sk-ant-LEAK"),
            "x-api-key redacted (case-insensitive)"
        );
        assert!(!debug.contains("g-LEAK"), "goog key redacted");
        assert!(!debug.contains("in-body"), "body never printed");
        assert!(debug.contains("authorization"), "header NAMES stay visible");
        assert!(debug.contains("***"), "redaction marker present");
        assert!(
            debug.contains("application/json"),
            "non-sensitive values stay readable"
        );
        assert!(
            debug.contains("<20 bytes>"),
            "body shows size only: {debug}"
        );
    }

    #[test]
    fn response_debug_redacts_set_cookie() {
        let mut headers = BTreeMap::new();
        headers.insert("set-cookie".to_owned(), "session=TOKEN-LEAK".to_owned());
        headers.insert("content-length".to_owned(), "2".to_owned());
        let resp = HttpResponse::new(200, headers, Bytes::from_static(b"ok"), "https://e.com");
        let debug = format!("{resp:?}");
        assert!(!debug.contains("TOKEN-LEAK"), "{debug}");
        assert!(debug.contains("set-cookie"));
        assert!(debug.contains("content-length"));
    }

    #[test]
    fn stream_response_debug_redacts_set_cookie() {
        use std::task::{Context, Poll};
        struct EmptyStream;
        impl futures_core::Stream for EmptyStream {
            type Item = Result<Bytes, HttpError>;
            fn poll_next(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut Context<'_>,
            ) -> Poll<Option<Self::Item>> {
                Poll::Ready(None)
            }
        }
        let mut headers = BTreeMap::new();
        headers.insert("set-cookie".to_owned(), "session=STREAM-LEAK".to_owned());
        let resp =
            HttpStreamResponse::new(200, headers, "https://e.com", None, Box::pin(EmptyStream));
        let debug = format!("{resp:?}");
        assert!(!debug.contains("STREAM-LEAK"), "{debug}");
        assert!(debug.contains("set-cookie"), "names stay visible");
    }
}
