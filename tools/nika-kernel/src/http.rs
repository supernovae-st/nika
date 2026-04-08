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

/// Async HTTP client trait.
///
/// Production: `ReqwestClient` wrapping `reqwest::Client`.
/// Tests: `MockHttpClient` with programmable response queue.
#[async_trait::async_trait]
pub trait HttpClient: Send + Sync {
    /// Send an HTTP request and return the response.
    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, HttpError>;
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
