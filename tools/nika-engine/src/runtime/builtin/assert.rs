// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:assert - Validate condition, fail if false.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "condition": true,       // Boolean condition to assert
//!   "message": "Expected X"  // Error message if assertion fails (optional)
//! }
//! ```
//!
//! # Returns (on success)
//!
//! ```json
//! {
//!   "passed": true,
//!   "condition": true
//! }
//! ```
//!
//! # Error (on failure)
//!
//! Returns NIKA-213 AssertionFailed error with the provided message.

use super::BuiltinTool;
use crate::error::NikaError;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

/// Parameters for nika:assert tool.
#[derive(Debug, Clone, Deserialize)]
struct AssertParams {
    /// The condition to assert (must be true to pass).
    condition: bool,
    /// Error message if assertion fails (optional).
    #[serde(default)]
    message: Option<String>,
}

/// Response from nika:assert tool (on success).
#[derive(Debug, Clone, Serialize)]
struct AssertResponse {
    /// Whether the assertion passed.
    passed: bool,
    /// The condition value that was asserted.
    condition: bool,
}

/// nika:assert builtin tool.
///
/// Validates a condition and fails the workflow if it's false.
/// Useful for workflow invariants and preconditions.
pub struct AssertTool;

impl BuiltinTool for AssertTool {
    fn name(&self) -> &'static str {
        "assert"
    }

    fn description(&self) -> &'static str {
        "Validate condition, fail workflow if false"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        // OpenAI-compatible schema with additionalProperties: false
        serde_json::json!({
            "type": "object",
            "properties": {
                "condition": {
                    "type": "boolean",
                    "description": "Boolean condition to assert (must be true)"
                },
                "message": {
                    "type": "string",
                    "description": "Error message if assertion fails"
                }
            },
            "required": ["condition", "message"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            // Parse parameters
            let params: AssertParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:assert".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            // Check the condition
            if !params.condition {
                let message = params
                    .message
                    .unwrap_or_else(|| "Assertion failed".to_string());
                return Err(NikaError::AssertionFailed {
                    message,
                    condition: "false".to_string(),
                });
            }

            // Return success response
            let response = AssertResponse {
                passed: true,
                condition: params.condition,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:assert".into(),
                reason: format!("Failed to serialize response: {}", e),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assert_tool_name() {
        let tool = AssertTool;
        assert_eq!(tool.name(), "assert");
    }

    #[test]
    fn test_assert_tool_description() {
        let tool = AssertTool;
        assert!(tool.description().contains("condition"));
    }

    #[test]
    fn test_assert_tool_schema() {
        let tool = AssertTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["condition"].is_object());
        assert!(schema["properties"]["message"].is_object());
        assert!(schema["required"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("condition")));
    }

    #[tokio::test]
    async fn test_assert_true_passes() {
        let tool = AssertTool;
        let result = tool.call(r#"{"condition": true}"#.to_string()).await;

        assert!(result.is_ok());
        let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response["passed"], true);
        assert_eq!(response["condition"], true);
    }

    #[tokio::test]
    async fn test_assert_false_fails() {
        let tool = AssertTool;
        let result = tool.call(r#"{"condition": false}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Assertion failed"));
    }

    #[tokio::test]
    async fn test_assert_false_with_message() {
        let tool = AssertTool;
        let result = tool
            .call(r#"{"condition": false, "message": "Expected X to equal Y"}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Expected X to equal Y"));
    }

    #[tokio::test]
    async fn test_assert_true_with_message_still_passes() {
        let tool = AssertTool;
        let result = tool
            .call(r#"{"condition": true, "message": "This should not appear"}"#.to_string())
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_assert_invalid_json() {
        let tool = AssertTool;
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_assert_missing_condition() {
        let tool = AssertTool;
        let result = tool.call(r#"{"message": "test"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_assert_wrong_condition_type() {
        let tool = AssertTool;
        let result = tool.call(r#"{"condition": "true"}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_assert_null_condition() {
        let tool = AssertTool;
        let result = tool.call(r#"{"condition": null}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Invalid JSON parameters"));
    }

    #[tokio::test]
    async fn test_assert_error_code() {
        let tool = AssertTool;
        let result = tool.call(r#"{"condition": false}"#.to_string()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Check it's the AssertionFailed variant (NIKA-213)
        assert!(err.to_string().contains("NIKA-213"));
    }
}
