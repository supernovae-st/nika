// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:prompt — Human-In-The-Loop user input.
//!
//! Requests user input during workflow execution. In headless mode (no
//! [`HitlPrompt`] handler configured), returns the `default` value if
//! provided, or returns an error.
//!
//! In interactive mode the handler is called; the bridge in nika-engine
//! adapts [`crate::runtime::hitl::HitlHandler`] → [`HitlPrompt`].
//!
//! # Parameters
//!
//! ```json
//! {
//!   "message": "Please review this output",
//!   "default": "approve"
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "response": "user input here",
//!   "default_used": false
//! }
//! ```
//!
//! [`HitlPrompt`]: nika_kernel::scope::HitlPrompt

use crate::{BuiltinError, BuiltinTool, __sealed};
use nika_kernel::scope::HitlPrompt;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

// ─────────────────────────────────────────────────────────────────────
// Parameters + response
// ─────────────────────────────────────────────────────────────────────

/// Parameters for `nika:prompt`.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptParams {
    /// Prompt message to display to the user.
    pub message: String,
    /// Default value to use when no handler is available (headless mode).
    #[serde(default)]
    pub default: Option<String>,
}

/// Response from `nika:prompt`.
#[derive(Debug, Clone, Serialize)]
pub struct PromptResponse {
    /// The response (user input or default).
    pub response: String,
    /// Whether the default value was used instead of actual user input.
    pub default_used: bool,
}

// ─────────────────────────────────────────────────────────────────────
// PromptTool
// ─────────────────────────────────────────────────────────────────────

/// `nika:prompt` builtin tool — Human-In-The-Loop input.
///
/// Construct with [`PromptTool::new_headless()`] (uses default value or errors)
/// or [`PromptTool::new()`] (delegates to a [`HitlPrompt`] handler, wired via
/// `BuiltinToolRouter::with_hitl()` in nika-engine).
pub struct PromptTool {
    handler: Option<Arc<dyn HitlPrompt>>,
}

impl PromptTool {
    /// Headless mode — use `default` value or error when no default is provided.
    pub fn new_headless() -> Self {
        Self { handler: None }
    }

    /// Interactive mode — delegate all prompts to the given [`HitlPrompt`] handler.
    pub fn new(handler: Arc<dyn HitlPrompt>) -> Self {
        Self {
            handler: Some(handler),
        }
    }
}

impl __sealed::Sealed for PromptTool {}

impl BuiltinTool for PromptTool {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn description(&self) -> &'static str {
        "Request user input during workflow execution (HITL)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Prompt message to display to the user"
                },
                "default": {
                    "type": "string",
                    "description": "Default value used when running in headless mode"
                }
            },
            "required": ["message"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: PromptParams = serde_json::from_str(&args)
                .map_err(|e| BuiltinError::invalid_params("nika:prompt", e))?;

            if params.message.is_empty() {
                return Err(BuiltinError::invalid_params(
                    "nika:prompt",
                    "Prompt message cannot be empty",
                ));
            }

            match &self.handler {
                None => {
                    // Headless: return default or error.
                    match params.default {
                        Some(default) => {
                            tracing::info!(
                                target: "nika:prompt",
                                message = %params.message,
                                default = %default,
                                "Using default value in headless mode"
                            );
                            serde_json::to_string(&PromptResponse {
                                response: default,
                                default_used: true,
                            })
                            .map_err(|e| BuiltinError::tool_error("nika:prompt", e))
                        }
                        None => Err(BuiltinError::Other {
                            tool: "nika:prompt".into(),
                            reason: format!(
                                "HITL required but running in headless mode. Prompt: '{}'",
                                params.message
                            ),
                        }),
                    }
                }
                Some(handler) => {
                    // Interactive: delegate to handler.
                    // HitlPrompt::ask returns only the response string — the bridge
                    // cannot know whether the default was used on its side.
                    let response = handler
                        .ask(&params.message, params.default.as_deref())
                        .await?;
                    serde_json::to_string(&PromptResponse {
                        response,
                        default_used: false,
                    })
                    .map_err(|e| BuiltinError::tool_error("nika:prompt", e))
                }
            }
        })
    }
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Headless mode ──

    #[tokio::test]
    async fn test_headless_with_default_returns_default() {
        let tool = PromptTool::new_headless();
        let result = tool
            .call(r#"{"message":"Approve?","default":"yes"}"#.to_string())
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["response"], "yes");
        assert_eq!(v["default_used"], true);
    }

    #[tokio::test]
    async fn test_headless_without_default_errors() {
        let tool = PromptTool::new_headless();
        let result = tool.call(r#"{"message":"Approve?"}"#.to_string()).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("headless mode"), "expected 'headless mode' in: {msg}");
    }

    #[tokio::test]
    async fn test_empty_message_errors() {
        let tool = PromptTool::new_headless();
        let result = tool.call(r#"{"message":""}"#.to_string()).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("cannot be empty"), "expected 'cannot be empty' in: {msg}");
    }

    #[tokio::test]
    async fn test_invalid_json_is_nika_212() {
        let tool = PromptTool::new_headless();
        let result = tool.call("not json".to_string()).await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("NIKA-212"), "expected NIKA-212 in: {msg}");
    }

    #[tokio::test]
    async fn test_missing_message_field_errors() {
        let tool = PromptTool::new_headless();
        // message field is required by serde — deserialization must fail
        let result = tool.call(r#"{"default":"fallback"}"#.to_string()).await;
        assert!(result.is_err());
    }

    // ── Handler (interactive) mode ──

    #[tokio::test]
    async fn test_handler_called_and_default_used_false() {
        struct MockPrompt;

        #[async_trait::async_trait]
        impl HitlPrompt for MockPrompt {
            async fn ask(
                &self,
                _message: &str,
                _default: Option<&str>,
            ) -> Result<String, BuiltinError> {
                Ok("user_input".to_string())
            }
        }

        let tool = PromptTool::new(Arc::new(MockPrompt));
        let result = tool
            .call(r#"{"message":"Enter something"}"#.to_string())
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["response"], "user_input");
        // Bridge cannot know if default was used — always false
        assert_eq!(v["default_used"], false);
    }

    #[tokio::test]
    async fn test_handler_ignores_default_field() {
        // When a handler is present, the default field in params is passed to
        // HitlPrompt::ask but the handler decides what to return — PromptTool
        // never short-circuits to default_used: true.
        struct EchoDefault;

        #[async_trait::async_trait]
        impl HitlPrompt for EchoDefault {
            async fn ask(
                &self,
                _message: &str,
                default: Option<&str>,
            ) -> Result<String, BuiltinError> {
                // Simulates handler that echoes the default
                Ok(default.unwrap_or("no-default").to_string())
            }
        }

        let tool = PromptTool::new(Arc::new(EchoDefault));
        let result = tool
            .call(r#"{"message":"Confirm?","default":"ignored_default"}"#.to_string())
            .await;
        assert!(result.is_ok());
        let v: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(v["response"], "ignored_default");
        // Still false — the bridge side decides, not PromptTool
        assert_eq!(v["default_used"], false);
    }

    #[tokio::test]
    async fn test_handler_error_propagates() {
        struct FailPrompt;

        #[async_trait::async_trait]
        impl HitlPrompt for FailPrompt {
            async fn ask(
                &self,
                _message: &str,
                _default: Option<&str>,
            ) -> Result<String, BuiltinError> {
                Err(BuiltinError::Other {
                    tool: "nika:prompt".into(),
                    reason: "HITL cancelled by user".into(),
                })
            }
        }

        let tool = PromptTool::new(Arc::new(FailPrompt));
        let result = tool
            .call(r#"{"message":"Confirm?"}"#.to_string())
            .await;
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("HITL cancelled"), "expected 'HITL cancelled' in: {msg}");
    }

    // ── Schema + metadata ──

    #[test]
    fn test_name_is_prompt() {
        assert_eq!(PromptTool::new_headless().name(), "prompt");
    }

    #[test]
    fn test_description_mentions_hitl() {
        assert!(PromptTool::new_headless().description().contains("HITL"));
    }

    #[test]
    fn test_schema_message_required_default_optional() {
        let schema = PromptTool::new_headless().parameters_schema();
        let required = schema["required"].as_array().unwrap();
        assert!(
            required.contains(&serde_json::json!("message")),
            "message must be in required"
        );
        assert!(
            !required.contains(&serde_json::json!("default")),
            "default must NOT be in required (it is optional)"
        );
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["default"].is_object());
    }
}
