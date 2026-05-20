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
    #[serde(default)]
    method: Option<String>,
    /// Schema-typed as a JSON-object string (OpenAI strict mode cannot
    /// express a free-form map). A lenient provider may still send a raw
    /// object — `normalize_json_arg` accepts either.
    #[serde(default)]
    headers: Option<serde_json::Value>,
    /// Schema-typed as a JSON string for the same reason.
    #[serde(default)]
    json: Option<serde_json::Value>,
    #[serde(default)]
    timeout: Option<u64>,
}

/// Normalize a tool argument that may arrive as a JSON-encoded string
/// (OpenAI strict mode) or as a raw JSON value (lenient providers).
///
/// `Ok(None)` for absent / null input. A `String` argument is parsed as
/// JSON; a malformed string is surfaced as `Err` so the caller can fail
/// loudly instead of silently dropping a body / headers.
fn normalize_json_arg(
    arg: Option<&serde_json::Value>,
) -> Result<Option<serde_json::Value>, serde_json::Error> {
    match arg {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => serde_json::from_str(s).map(Some),
        Some(other) => Ok(Some(other.clone())),
    }
}

impl BuiltinTool for FetchTool {
    fn name(&self) -> &'static str {
        "fetch"
    }

    fn description(&self) -> &'static str {
        "Make HTTP requests. Returns { status, headers, body, url, elapsed_ms }."
    }

    /// Tool parameter schema — kept **OpenAI-strict-mode compatible**.
    ///
    /// rig-core routes agent streaming through the OpenAI Responses API,
    /// which enables strict mode by default and runs `sanitize_schema`:
    /// it forces every property into `required` and adds
    /// `additionalProperties: false` to every object. A free-form
    /// `{"type": "object"}` (no `properties`) is rejected by OpenAI strict
    /// mode. So `headers` / `json` are typed here as **JSON strings** (the
    /// agent passes a JSON-encoded value); all optionals are nullable
    /// unions and no `default` keyword is used (strict mode forbids it).
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["url", "method", "headers", "json", "timeout"],
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL to fetch (must be http/https)"
                },
                "method": {
                    "type": ["string", "null"],
                    "enum": ["GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", null],
                    "description": "HTTP method — null means GET"
                },
                "headers": {
                    "type": ["string", "null"],
                    "description": "Optional HTTP headers as a JSON object string, \
                                    e.g. {\"Authorization\":\"Bearer x\"} — null for none"
                },
                "json": {
                    "type": ["string", "null"],
                    "description": "Optional JSON request body as a JSON string \
                                    (auto-serialized, sets Content-Type) — null for none"
                },
                "timeout": {
                    "type": ["integer", "null"],
                    "description": "Timeout in seconds (max 120) — null means 30"
                }
            }
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

            // Build request — method defaults to GET when the agent omits it.
            let method_str = params.method.as_deref().unwrap_or("GET");
            let method = method_str.parse::<reqwest::Method>().map_err(|_| {
                NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!("Invalid HTTP method: {method_str}"),
                }
            })?;

            let timeout_secs = params.timeout.unwrap_or(30).min(120);
            let mut req = self
                .client
                .request(method.clone(), &params.url)
                .timeout(Duration::from_secs(timeout_secs));

            // `headers` arrives as a JSON-object string under strict mode;
            // `normalize_json_arg` also tolerates a raw object from lenient
            // providers. A malformed string fails loudly.
            let header_arg = normalize_json_arg(params.headers.as_ref()).map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!(
                        "malformed `headers` argument (expected a JSON object string): {e}"
                    ),
                }
            })?;
            if let Some(serde_json::Value::Object(headers)) = header_arg {
                for (k, v) in &headers {
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

            // `json` arrives as a JSON string under strict mode. Normalize
            // once: the result drives both the request body and the
            // `has_body` telemetry flag below.
            let body = normalize_json_arg(params.json.as_ref()).map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "nika:fetch".into(),
                    reason: format!(
                        "malformed `json` argument (expected a JSON string): {e}"
                    ),
                }
            })?;
            if let Some(ref json) = body {
                req = req.json(json);
            }

            // Emit HttpRequest event
            self.event_log.emit(EventKind::HttpRequest {
                task_id: Arc::from("agent"),
                url: params.url.clone(),
                method: method.to_string(),
                has_body: body.is_some(),
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

    #[test]
    fn fetch_tool_schema_is_strict_clean() {
        // OpenAI strict mode: every property must be in `required`, and no
        // free-form `{"type":"object"}` (must be string or scalar).
        let schema = setup().parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        let required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        for key in props.keys() {
            assert!(required.contains(&key.as_str()), "{key} missing from required");
        }
        // `headers` / `json` must be string-typed, never bare object.
        for key in ["headers", "json"] {
            let ty = &props[key]["type"];
            assert!(
                ty.as_array().is_some_and(|a| a.iter().any(|t| t == "string")),
                "{key} must be a (nullable) string, got {ty}"
            );
        }
    }

    #[test]
    fn normalize_json_arg_variants() {
        use serde_json::json;
        // absent / null → Ok(None)
        assert_eq!(normalize_json_arg(None).unwrap(), None);
        assert_eq!(normalize_json_arg(Some(&json!(null))).unwrap(), None);
        // JSON-encoded string → parsed
        let encoded = json!("{\"a\":1}");
        assert_eq!(
            normalize_json_arg(Some(&encoded)).unwrap(),
            Some(json!({"a": 1}))
        );
        // raw object passes through unchanged (lenient providers)
        let raw = json!({"b": 2});
        assert_eq!(normalize_json_arg(Some(&raw)).unwrap(), Some(json!({"b": 2})));
        // malformed string → Err (no longer silently swallowed)
        assert!(normalize_json_arg(Some(&json!("{not json"))).is_err());
    }
}
