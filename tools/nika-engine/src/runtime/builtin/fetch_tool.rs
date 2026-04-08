// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:fetch — Agent builtin tool for HTTP requests.
//!
//! Enables agents to make HTTP requests with SSRF protection.
//! Returns `{ status, headers, body, url, elapsed_ms }`.
//!
//! # Security
//!
//! All requests pass through the PolicyEnforcer SSRF check.
//! Private IP ranges, metadata endpoints, and localhost are blocked
//! unless explicitly allowed by policy.

use super::BuiltinTool;
use crate::error::NikaError;
use crate::runtime::policy::{PolicyDecision, PolicyEnforcer};
use nika_event::{EventKind, EventLog};
use serde::Deserialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum response body size for agent fetch (10MB).
const MAX_RESPONSE_SIZE: u64 = 10 * 1024 * 1024;

/// nika:fetch builtin tool for agent HTTP requests.
pub struct FetchTool {
    client: reqwest::Client,
    policy_enforcer: Arc<parking_lot::RwLock<PolicyEnforcer>>,
    event_log: EventLog,
}

impl FetchTool {
    pub fn new(
        policy_enforcer: Arc<parking_lot::RwLock<PolicyEnforcer>>,
        event_log: EventLog,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .redirect(crate::runtime::policy::ssrf_safe_redirect_policy(
                vec![],
                10,
            ))
            .user_agent("nika-agent/1.0")
            .build()
            .expect("reqwest client");
        Self {
            client,
            policy_enforcer,
            event_log,
        }
    }
}

#[derive(Debug, Deserialize)]
struct FetchToolParams {
    url: String,
    #[serde(default = "default_method")]
    method: String,
    #[serde(default)]
    headers: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default)]
    json: Option<serde_json::Value>,
    #[serde(default)]
    timeout: Option<u64>,
}

fn default_method() -> String {
    "GET".into()
}

impl BuiltinTool for FetchTool {
    fn name(&self) -> &'static str {
        "fetch"
    }

    fn description(&self) -> &'static str {
        "Make HTTP requests. Returns { status, headers, body, url, elapsed_ms }."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["url"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch (must be http/https)"
                },
                "method": {
                    "type": "string",
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD"],
                    "default": "GET",
                    "description": "HTTP method"
                },
                "headers": {
                    "type": "object",
                    "description": "HTTP headers as key-value pairs"
                },
                "json": {
                    "description": "JSON body (auto-serialized, sets Content-Type)"
                },
                "timeout": {
                    "type": "integer",
                    "description": "Timeout in seconds (default: 30, max: 120)",
                    "default": 30
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: FetchToolParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // SSRF check (mandatory)
            let decision = self.policy_enforcer.read().check_fetch(&params.url);
            if let PolicyDecision::Block(reason) = decision {
                return Err(NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!("URL blocked by policy: {reason}"),
                });
            }

            // Build request
            let method = params.method.parse::<reqwest::Method>().map_err(|_| {
                NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!("Invalid HTTP method: {}", params.method),
                }
            })?;

            let timeout_secs = params.timeout.unwrap_or(30).min(120);
            let mut req = self
                .client
                .request(method.clone(), &params.url)
                .timeout(Duration::from_secs(timeout_secs));

            if let Some(headers) = &params.headers {
                for (k, v) in headers {
                    if let Some(val) = v.as_str() {
                        if let (Ok(name), Ok(value)) = (
                            reqwest::header::HeaderName::from_bytes(k.as_bytes()),
                            reqwest::header::HeaderValue::from_str(val),
                        ) {
                            req = req.header(name, value);
                        }
                    }
                }
            }

            if let Some(json) = &params.json {
                req = req.json(json);
            }

            // Emit HttpRequest event
            self.event_log.emit(EventKind::HttpRequest {
                task_id: Arc::from("agent"),
                url: params.url.clone(),
                method: method.to_string(),
                has_body: params.json.is_some(),
            });

            // Execute
            let start = Instant::now();
            let response = req.send().await.map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:fetch".into(),
                reason: format!("Request failed: {e}"),
            })?;

            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status = response.status().as_u16();
            let final_url = response.url().to_string();

            // Collect response headers
            let resp_headers: serde_json::Map<String, serde_json::Value> = response
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        serde_json::Value::String(v.to_str().unwrap_or("").to_string()),
                    )
                })
                .collect();

            // Size check
            if let Some(len) = response.content_length() {
                if len > MAX_RESPONSE_SIZE {
                    return Err(NikaError::BuiltinToolError {
                        tool: "nika:fetch".into(),
                        reason: format!(
                            "Response too large ({} bytes, max {} bytes)",
                            len, MAX_RESPONSE_SIZE
                        ),
                    });
                }
            }

            // Read body (lossy UTF-8 for non-text responses)
            let bytes = response
                .bytes()
                .await
                .map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!("Failed to read response body: {e}"),
                })?;
            if bytes.len() as u64 > MAX_RESPONSE_SIZE {
                return Err(NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!(
                        "Response body too large ({} bytes, max {} bytes)",
                        bytes.len(),
                        MAX_RESPONSE_SIZE
                    ),
                });
            }
            let body = String::from_utf8_lossy(&bytes).into_owned();

            // Emit HttpResponse event
            self.event_log.emit(EventKind::HttpResponse {
                task_id: Arc::from("agent"),
                status_code: status,
                content_type: None,
                content_length: Some(bytes.len() as u64),
                elapsed_ms,
            });

            // Return result (even for 4xx/5xx — agent decides what to do)
            Ok(serde_json::json!({
                "status": status,
                "headers": resp_headers,
                "body": body,
                "url": final_url,
                "elapsed_ms": elapsed_ms,
            })
            .to_string())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::boot::PolicyConfig;
    use parking_lot::RwLock;

    fn setup() -> FetchTool {
        let policy = PolicyConfig::default();
        let enforcer = PolicyEnforcer::new(policy);
        FetchTool::new(Arc::new(RwLock::new(enforcer)), EventLog::new())
    }

    #[tokio::test]
    async fn fetch_tool_ssrf_blocked() {
        let tool = setup();
        let result = tool
            .call(r#"{"url": "http://169.254.169.254/latest/meta-data/"}"#.into())
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("blocked") || err.contains("policy"),
            "expected SSRF block, got: {err}"
        );
    }

    #[test]
    fn fetch_tool_method_parsing() {
        // reqwest accepts all standard HTTP methods
        assert!("PATCH".parse::<reqwest::Method>().is_ok());
        assert!("GET".parse::<reqwest::Method>().is_ok());
        // reqwest also accepts custom methods (WebDAV etc.)
        assert!("CUSTOM".parse::<reqwest::Method>().is_ok());
    }

    #[tokio::test]
    async fn fetch_tool_invalid_params() {
        let tool = setup();
        let result = tool.call(r#"{"not_url": "test"}"#.into()).await;
        assert!(result.is_err());
    }

    #[test]
    fn fetch_tool_name() {
        let tool = setup();
        assert_eq!(tool.name(), "fetch");
    }

    #[test]
    fn fetch_tool_schema_has_url() {
        let tool = setup();
        let schema = tool.parameters_schema();
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("url")));
    }
}
