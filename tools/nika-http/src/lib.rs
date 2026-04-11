// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production [`HttpClient`] implementation via `reqwest`.
//!
//! This crate sits at L1 in the dependency graph — it implements the
//! [`nika_kernel::http::HttpClient`] trait using `reqwest::Client`.
//!
//! Includes SSRF protection: private IP ranges are blocked by default.

mod ssrf;

use std::time::Duration;

use nika_kernel::http::{HttpClient, HttpError, HttpMethod, HttpRequest, HttpResponse};
use reqwest::Client;

/// Production HTTP client backed by `reqwest::Client`.
///
/// Includes SSRF protection for private IP ranges.
#[derive(Debug, Clone)]
pub struct ReqwestClient {
    inner: Client,
    /// Block requests to private/loopback IP ranges.
    ssrf_protection: bool,
}

impl ReqwestClient {
    /// Create a new client with default settings and SSRF protection enabled.
    pub fn new() -> Self {
        Self {
            inner: Client::builder()
                .user_agent(concat!("nika/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
            ssrf_protection: true,
        }
    }

    /// Create a client without SSRF protection (for testing or internal networks).
    pub fn without_ssrf_protection() -> Self {
        Self {
            inner: Client::builder()
                .user_agent(concat!("nika/", env!("CARGO_PKG_VERSION")))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
            ssrf_protection: false,
        }
    }

    /// Create from an existing reqwest::Client.
    pub fn from_client(client: Client, ssrf_protection: bool) -> Self {
        Self {
            inner: client,
            ssrf_protection,
        }
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl HttpClient for ReqwestClient {
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError> {
        // SSRF check
        if self.ssrf_protection {
            ssrf::check_url(&request.url)?;
        }

        let method = match request.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        let mut builder = self.inner.request(method, &request.url);

        // Headers
        for (key, value) in &request.headers {
            builder = builder.header(key.as_str(), value.as_str());
        }

        // Body
        if let Some(body) = request.body {
            builder = builder.body(body);
        }

        // Timeout override
        if let Some(timeout) = request.timeout {
            builder = builder.timeout(timeout);
        }

        // Note: reqwest doesn't support per-request redirect policy.
        // The redirect policy is set at the Client level.
        // To disable redirects per-request, create a separate client.

        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                HttpError::Timeout {
                    duration_ms: request.timeout.map_or(30_000, |d| d.as_millis() as u64),
                }
            } else if e.is_connect() {
                HttpError::Connection {
                    reason: e.to_string(),
                }
            } else {
                HttpError::Other {
                    reason: e.to_string(),
                }
            }
        })?;

        let final_url = response.url().to_string();
        let status = response.status().as_u16();

        let mut headers = std::collections::HashMap::new();
        for (key, value) in response.headers() {
            if let Ok(v) = value.to_str() {
                headers.insert(key.as_str().to_string(), v.to_string());
            }
        }

        let body = response.bytes().await.map_err(|e| HttpError::Other {
            reason: format!("failed to read response body: {e}"),
        })?;

        Ok(HttpResponse::new(status, headers, body, final_url))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_client() {
        let client = ReqwestClient::new();
        assert!(client.ssrf_protection);
    }

    #[test]
    fn without_ssrf_creates_unprotected() {
        let client = ReqwestClient::without_ssrf_protection();
        assert!(!client.ssrf_protection);
    }

    #[test]
    fn default_has_ssrf_protection() {
        let client = ReqwestClient::default();
        assert!(client.ssrf_protection);
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<ReqwestClient>();
    }

    #[test]
    fn request_builder_get() {
        let req = HttpRequest::get("https://example.com");
        assert_eq!(req.url, "https://example.com");
        assert!(matches!(req.method, HttpMethod::Get));
        assert!(req.body.is_none());
        assert!(req.follow_redirects);
    }

    #[test]
    fn request_builder_post_json() {
        let body = serde_json::json!({"key": "value"});
        let req = HttpRequest::post_json("https://api.example.com", &body);
        assert!(matches!(req.method, HttpMethod::Post));
        assert!(req.body.is_some());
        assert_eq!(
            req.headers.get("Content-Type").unwrap(),
            "application/json"
        );
    }
}
