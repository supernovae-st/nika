// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:prompt - Human-In-The-Loop user input request.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "message": "Please review this output",  // Prompt message for user
//!   "default": "approve"                     // Default value (optional)
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
//! # Note
//!
//! This tool requires runtime integration with a HITL channel.
//! When invoked without a HITL channel (e.g., in headless mode),
//! it returns the default value if provided, or errors.

use super::BuiltinTool;
use crate::error::NikaError;
use crate::runtime::hitl::{HitlHandler, HitlRequest};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Parameters for nika:prompt tool.
#[derive(Debug, Clone, Deserialize)]
pub struct PromptParams {
    /// The prompt message to display to the user.
    pub message: String,
    /// Default value to use if no input is provided (optional).
    #[serde(default)]
    pub default: Option<String>,
}

/// Response from nika:prompt tool.
#[derive(Debug, Clone, Serialize)]
pub struct PromptResponse {
    /// The user's response (or default value).
    pub response: String,
    /// Whether the default value was used.
    pub default_used: bool,
}

/// nika:prompt builtin tool.
///
/// Requests user input during workflow execution (HITL).
/// In headless mode, returns the default value or errors.
///
/// Full HITL integration requires a channel for user interaction,
/// which is set up when the Router is integrated with the TUI.
pub struct PromptTool {
    /// Whether running in headless mode (no TUI).
    headless: bool,
    /// Optional HITL handler for interactive mode.
    handler: Option<Arc<dyn HitlHandler>>,
}

impl PromptTool {
    /// Create a new PromptTool in headless mode.
    /// In headless mode, the tool uses the default value or errors.
    pub fn new_headless() -> Self {
        Self {
            headless: true,
            handler: None,
        }
    }

    /// Create a new PromptTool in interactive mode.
    /// Full HITL support requires runtime integration.
    pub fn new_interactive() -> Self {
        Self {
            headless: false,
            handler: None,
        }
    }

    /// Create a new PromptTool with a custom HITL handler.
    /// The handler will be used for all prompt requests.
    pub fn with_handler(handler: Arc<dyn HitlHandler>) -> Self {
        Self {
            headless: false,
            handler: Some(handler),
        }
    }
}

impl Default for PromptTool {
    fn default() -> Self {
        Self::new_headless()
    }
}

impl BuiltinTool for PromptTool {
    fn name(&self) -> &'static str {
        "prompt"
    }

    fn description(&self) -> &'static str {
        "Request user input during workflow execution (HITL)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // OpenAI-compatible schema with additionalProperties: false
        serde_json::json!({
            "type": "object",
            "properties": {
                "message": {
                    "type": "string",
                    "description": "Prompt message to display to the user"
                },
                "default": {
                    "type": "string",
                    "description": "Default value if no input provided"
                }
            },
            "required": ["message", "default"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse parameters
            let params: PromptParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:prompt".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Validate message is not empty
            if params.message.is_empty() {
                return Err(NikaError::BuiltinInvalidParams {
                    tool: "nika:prompt".into(),
                    reason: "Prompt message cannot be empty".into(),
                });
            }

            // In headless mode, use default value or error
            if self.headless {
                match params.default {
                    Some(default) => {
                        tracing::info!(
                            target: "nika:prompt",
                            message = %params.message,
                            default = %default,
                            "Using default value in headless mode"
                        );
                        let response = PromptResponse {
                            response: default,
                            default_used: true,
                        };
                        return serde_json::to_string(&response).map_err(|e| {
                            NikaError::BuiltinToolError {
                                tool: "nika:prompt".into(),
                                reason: format!("Failed to serialize response: {}", e),
                            }
                        });
                    }
                    None => {
                        return Err(NikaError::BuiltinToolError {
                            tool: "nika:prompt".into(),
                            reason: format!(
                                "HITL required but running in headless mode. Prompt: '{}'",
                                params.message
                            ),
                        });
                    }
                }
            }

            // Use HITL handler if provided
            if let Some(handler) = &self.handler {
                let request = HitlRequest::new(&params.message);
                let request = if let Some(default) = params.default.clone() {
                    request.with_default(default)
                } else {
                    request
                };

                let hitl_response =
                    handler
                        .prompt(request)
                        .await
                        .map_err(|e| NikaError::BuiltinToolError {
                            tool: "nika:prompt".into(),
                            reason: format!("HITL handler error: {}", e),
                        })?;

                let response = PromptResponse {
                    response: hitl_response.response,
                    default_used: hitl_response.default_used,
                };

                return serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:prompt".into(),
                    reason: format!("Failed to serialize response: {}", e),
                });
            }

            // Fallback: interactive mode without handler uses default or errors
            match params.default {
                Some(default) => {
                    tracing::warn!(
                        target: "nika:prompt",
                        message = %params.message,
                        default = %default,
                        "HITL handler not configured, using default value"
                    );
                    let response = PromptResponse {
                        response: default,
                        default_used: true,
                    };
                    serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                        tool: "nika:prompt".into(),
                        reason: format!("Failed to serialize response: {}", e),
                    })
                }
                None => Err(NikaError::BuiltinToolError {
                    tool: "nika:prompt".into(),
                    reason: format!(
                        "HITL handler not configured and no default provided. Prompt: '{}'",
                        params.message
                    ),
                }),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_tool_name() {
        let tool = PromptTool::default();
        assert_eq!(tool.name(), "prompt");
    }

    #[test]
    fn test_prompt_tool_description() {
        let tool = PromptTool::default();
        assert!(tool.description().contains("HITL"));
    }

    #[test]
    fn test_prompt_tool_schema() {
        let tool = PromptTool::default();
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["properties"]["default"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("message")));
    }

    #[tokio::test]
    async fn test_prompt_headless_with_default() {
        let tool = PromptTool::new_headless();
        let result = tool
            .call(r#"{"message": "Approve?", "default": "yes"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["response"], "yes");
        assert_eq!(response["default_used"], true);
    }

    #[tokio::test]
    async fn test_prompt_headless_without_default_errors() {
        let tool = PromptTool::new_headless();
        let result = tool.call(r#"{"message": "Approve?"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("headless mode"));
    }

    #[tokio::test]
    async fn test_prompt_interactive_with_default() {
        let tool = PromptTool::new_interactive();
        let result = tool
            .call(r#"{"message": "Confirm?", "default": "confirmed"}"#.to_string())
            .await;

        // Currently falls back to default since HITL channel not configured
        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["response"], "confirmed");
        assert_eq!(response["default_used"], true);
    }

    #[tokio::test]
    async fn test_prompt_interactive_without_default_errors() {
        let tool = PromptTool::new_interactive();
        let result = tool
            .call(r#"{"message": "User input needed"}"#.to_string())
            .await;

        // Currently errors since HITL handler not configured
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("HITL handler not configured"));
    }

    #[tokio::test]
    async fn test_prompt_empty_message_errors() {
        let tool = PromptTool::default();
        let result = tool.call(r#"{"message": ""}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot be empty"));
    }

    #[tokio::test]
    async fn test_prompt_invalid_json() {
        let tool = PromptTool::default();
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_prompt_missing_message() {
        let tool = PromptTool::default();
        let result = tool.call(r#"{"default": "test"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_prompt_params_deserialization() {
        let json = r#"{"message": "Test prompt", "default": "default_value"}"#;
        let params: PromptParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.message, "Test prompt");
        assert_eq!(params.default, Some("default_value".to_string()));
    }

    #[tokio::test]
    async fn test_prompt_params_without_default() {
        let json = r#"{"message": "Test prompt"}"#;
        let params: PromptParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.message, "Test prompt");
        assert_eq!(params.default, None);
    }

    // Tests for HitlHandler integration
    #[tokio::test]
    async fn test_prompt_with_hitl_handler_calls_handler() {
        use crate::runtime::hitl::{HitlError, HitlResponse};
        use async_trait::async_trait;

        struct MockHandler {
            response: String,
        }

        #[async_trait]
        impl HitlHandler for MockHandler {
            async fn prompt(&self, _request: HitlRequest) -> Result<HitlResponse, HitlError> {
                Ok(HitlResponse::new(&self.response))
            }
        }

        let handler = Arc::new(MockHandler {
            response: "user_input".to_string(),
        });
        let tool = PromptTool::with_handler(handler);
        let result = tool
            .call(r#"{"message": "Enter something"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["response"], "user_input");
        assert_eq!(response["default_used"], false);
    }

    #[tokio::test]
    async fn test_prompt_with_hitl_handler_ignores_default() {
        use crate::runtime::hitl::{HitlError, HitlResponse};
        use async_trait::async_trait;

        struct MockHandler;

        #[async_trait]
        impl HitlHandler for MockHandler {
            async fn prompt(&self, _request: HitlRequest) -> Result<HitlResponse, HitlError> {
                Ok(HitlResponse::new("handler_response"))
            }
        }

        let tool = PromptTool::with_handler(Arc::new(MockHandler));
        let result = tool
            .call(r#"{"message": "Confirm?", "default": "ignored_default"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        // Handler response takes precedence over default
        assert_eq!(response["response"], "handler_response");
        assert_eq!(response["default_used"], false);
    }

    #[tokio::test]
    async fn test_prompt_with_hitl_handler_error_propagates() {
        use crate::runtime::hitl::{HitlError, HitlResponse};
        use async_trait::async_trait;

        struct ErrorHandler;

        #[async_trait]
        impl HitlHandler for ErrorHandler {
            async fn prompt(&self, _request: HitlRequest) -> Result<HitlResponse, HitlError> {
                Err(HitlError::Cancelled)
            }
        }

        let tool = PromptTool::with_handler(Arc::new(ErrorHandler));
        let result = tool.call(r#"{"message": "Confirm?"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("HITL handler error"));
    }
}
