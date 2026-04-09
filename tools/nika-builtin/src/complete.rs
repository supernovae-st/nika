// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:complete - Signal agent task completion
//!
//! The completion tool is called by the agent to signal task completion
//! with a structured result. This is part of the "explicit" completion mode.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "result": "The final answer or output",        // Required
//!   "confidence": 0.95,                            // Optional: 0.0-1.0
//!   "reasoning": "Explanation of the approach"     // Optional: For audit
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "completed": true,
//!   "result": "...",
//!   "confidence": 0.95,
//!   "is_final": true
//! }
//! ```

use crate::{BuiltinError, BuiltinTool, __sealed};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Special marker that RigAgentLoop uses to detect completion
pub const COMPLETION_MARKER: &str = "__NIKA_COMPLETE__";

// ═══════════════════════════════════════════════════════════════════════════
// Parameters
// ═══════════════════════════════════════════════════════════════════════════

/// Parameters for nika:complete tool.
#[derive(Debug, Clone, Deserialize)]
pub struct CompleteParams {
    /// The final result of the task (required).
    pub result: Value,

    /// Confidence level in the result (0.0-1.0, optional).
    #[serde(default)]
    pub confidence: Option<f64>,

    /// Reasoning or explanation for the result (optional).
    #[serde(default)]
    pub reasoning: Option<String>,

    /// Additional metadata (optional).
    #[serde(default)]
    pub metadata: Option<Value>,
}

impl CompleteParams {
    /// Validate the parameters.
    pub fn validate(&self) -> Result<(), BuiltinError> {
        if let Some(conf) = self.confidence {
            if !(0.0..=1.0).contains(&conf) {
                return Err(BuiltinError::InvalidArgs {
                    tool: "nika:complete".into(),
                    reason: format!("confidence must be between 0.0 and 1.0, got {}", conf),
                });
            }
        }

        Ok(())
    }

    /// Get the result as a string (for simple text results).
    pub fn result_as_string(&self) -> String {
        match &self.result {
            Value::String(s) => s.clone(),
            Value::Null | Value::Bool(_) | Value::Number(_) | Value::Array(_) | Value::Object(_) => self.result.to_string(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Response
// ═══════════════════════════════════════════════════════════════════════════

/// Response from nika:complete tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompleteResponse {
    /// Whether completion was successful.
    pub completed: bool,

    /// The result value.
    pub result: Value,

    /// Confidence level (if provided).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,

    /// Marker for RigAgentLoop to detect completion.
    #[serde(default = "default_marker")]
    pub marker: String,

    /// Whether this is a final completion (not low-confidence retry).
    #[serde(default)]
    pub is_final: bool,
}

fn default_marker() -> String {
    COMPLETION_MARKER.to_string()
}

impl CompleteResponse {
    /// Create a successful completion response.
    pub fn success(params: &CompleteParams, is_final: bool) -> Self {
        Self {
            completed: true,
            result: params.result.clone(),
            confidence: params.confidence,
            marker: COMPLETION_MARKER.to_string(),
            is_final,
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tool Implementation
// ═══════════════════════════════════════════════════════════════════════════

/// nika:complete builtin tool.
///
/// Called by the agent to signal task completion in "explicit" mode.
/// The RigAgentLoop detects this tool call and triggers the completion flow.
pub struct CompleteTool;

impl __sealed::Sealed for CompleteTool {}

impl BuiltinTool for CompleteTool {
    fn name(&self) -> &'static str {
        "complete"
    }

    fn description(&self) -> &'static str {
        "Signal task completion with a structured result"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "result": {
                    "type": "string",
                    "description": "The final result or answer for the task. Serialize complex values as JSON strings."
                },
                "confidence": {
                    "type": "number",
                    "description": "Confidence level in the result (0.0-1.0)"
                },
                "reasoning": {
                    "type": "string",
                    "description": "Explanation of how you arrived at this result"
                }
            },
            "required": ["result"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: CompleteParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:complete".into(),
                    reason: format!("Invalid JSON parameters: {}", e),
                })?;

            params.validate()?;

            tracing::debug!(
                target: "nika_complete",
                confidence = ?params.confidence,
                has_reasoning = params.reasoning.is_some(),
                "Agent signaling completion"
            );

            let response = CompleteResponse::success(&params, true);

            serde_json::to_string(&response).map_err(|e| BuiltinError::Other {
                tool: "nika:complete".into(),
                reason: format!("Failed to serialize response: {}", e),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Utility Functions
// ═══════════════════════════════════════════════════════════════════════════

/// Check if a tool call response indicates completion.
///
/// Parses the JSON response and checks the `marker` field exactly — substring
/// search would cause false positives if COMPLETION_MARKER appears in
/// user-controlled content processed by the agent.
pub fn is_completion_signal(tool_name: &str, response: &str) -> bool {
    if tool_name != "nika:complete" && tool_name != "complete" {
        return false;
    }

    serde_json::from_str::<CompleteResponse>(response)
        .map(|r| r.marker == COMPLETION_MARKER)
        .unwrap_or(false)
}

/// Parse a completion response from tool output.
pub fn parse_completion_response(response: &str) -> Option<CompleteResponse> {
    serde_json::from_str(response).ok()
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_complete_tool_name() {
        let tool = CompleteTool;
        assert_eq!(tool.name(), "complete");
    }

    #[test]
    fn test_complete_tool_description() {
        let tool = CompleteTool;
        assert!(tool.description().contains("completion"));
    }

    #[test]
    fn test_complete_tool_schema() {
        let tool = CompleteTool;
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["result"].is_object());
        assert!(schema["properties"]["confidence"].is_object());
        assert!(schema["properties"]["reasoning"].is_object());
        assert_eq!(schema["additionalProperties"], false);
    }

    #[tokio::test]
    async fn test_complete_simple_string_result() {
        let tool = CompleteTool;
        let result = tool
            .call(r#"{"result": "Task completed successfully"}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: CompleteResponse = serde_json::from_str(&result.unwrap()).unwrap();
        assert!(response.completed);
        assert_eq!(response.result, "Task completed successfully");
        assert_eq!(response.marker, COMPLETION_MARKER);
        assert!(response.is_final);
    }

    #[tokio::test]
    async fn test_complete_with_confidence() {
        let tool = CompleteTool;
        let result = tool
            .call(r#"{"result": "Answer", "confidence": 0.95}"#.to_string())
            .await;

        assert!(result.is_ok());
        let response: CompleteResponse = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response.confidence, Some(0.95));
    }

    #[tokio::test]
    async fn test_complete_with_reasoning() {
        let tool = CompleteTool;
        let result = tool
            .call(r#"{"result": "42", "reasoning": "Based on the calculation..."}"#.to_string())
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_complete_with_complex_result() {
        let tool = CompleteTool;
        let result = tool
            .call(
                r#"{
                    "result": {"items": [1, 2, 3], "total": 6},
                    "confidence": 0.99
                }"#
                .to_string(),
            )
            .await;

        assert!(result.is_ok());
        let response: CompleteResponse = serde_json::from_str(&result.unwrap()).unwrap();
        assert_eq!(response.result["items"][0], 1);
        assert_eq!(response.result["total"], 6);
    }

    #[tokio::test]
    async fn test_complete_invalid_confidence_too_high() {
        let tool = CompleteTool;
        let result = tool
            .call(r#"{"result": "x", "confidence": 1.5}"#.to_string())
            .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("confidence"));
    }

    #[tokio::test]
    async fn test_complete_invalid_confidence_negative() {
        let tool = CompleteTool;
        let result = tool
            .call(r#"{"result": "x", "confidence": -0.1}"#.to_string())
            .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_missing_result() {
        let tool = CompleteTool;
        let result = tool.call(r#"{"confidence": 0.9}"#.to_string()).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_complete_invalid_json() {
        let tool = CompleteTool;
        let result = tool.call("not json".to_string()).await;

        assert!(result.is_err());
    }

    /// BUG-4: OpenAI rejects schemas where properties lack a "type" field.
    #[test]
    fn test_all_properties_have_type_field() {
        let tool = CompleteTool;
        let schema = tool.parameters_schema();
        let props = schema["properties"]
            .as_object()
            .expect("properties must be an object");
        for (name, prop_schema) in props {
            assert!(
                prop_schema.get("type").is_some(),
                "Property '{}' missing 'type' field — OpenAI will reject this schema",
                name,
            );
        }
    }

    #[test]
    fn test_is_completion_signal_positive() {
        let response = serde_json::to_string(&CompleteResponse {
            completed: true,
            result: Value::String("done".into()),
            confidence: Some(0.9),
            marker: COMPLETION_MARKER.to_string(),
            is_final: true,
        })
        .unwrap();

        assert!(is_completion_signal("nika:complete", &response));
        assert!(is_completion_signal("complete", &response));
    }

    #[test]
    fn test_is_completion_signal_negative_wrong_tool() {
        let response = format!(r#"{{"marker": "{}"}}"#, COMPLETION_MARKER);
        assert!(!is_completion_signal("nika:emit", &response));
    }

    #[test]
    fn test_is_completion_signal_negative_no_marker() {
        let response = r#"{"completed": true}"#;
        assert!(!is_completion_signal("nika:complete", response));
    }

    #[test]
    fn test_is_completion_signal_no_false_positive_from_marker_in_content() {
        // A document that contains the marker string in its text content must NOT
        // trigger completion — only a genuine CompleteResponse.marker field should.
        let poisoned = format!(
            r#"{{"summary": "result: {}", "completed": false}}"#,
            COMPLETION_MARKER
        );
        assert!(!is_completion_signal("nika:complete", &poisoned));
    }

    #[test]
    fn test_schema_required_only_result() {
        // confidence + reasoning must be optional — OpenAI rejects tool calls
        // that don't include all required fields, breaking agents that omit them.
        let tool = CompleteTool;
        let schema = tool.parameters_schema();
        let required = schema["required"].as_array().expect("required must be array");
        assert_eq!(required.len(), 1, "only 'result' should be required");
        assert_eq!(required[0], "result");
    }

    #[test]
    fn test_parse_completion_response() {
        let response = serde_json::to_string(&CompleteResponse {
            completed: true,
            result: Value::String("test".into()),
            confidence: Some(0.8),
            marker: COMPLETION_MARKER.to_string(),
            is_final: true,
        })
        .unwrap();

        let parsed = parse_completion_response(&response).unwrap();
        assert!(parsed.completed);
        assert_eq!(parsed.result, "test");
        assert_eq!(parsed.confidence, Some(0.8));
    }

    #[test]
    fn test_complete_params_validate_valid() {
        let params = CompleteParams {
            result: Value::String("ok".into()),
            confidence: Some(0.5),
            reasoning: None,
            metadata: None,
        };
        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_complete_params_result_as_string() {
        let params = CompleteParams {
            result: Value::String("hello".into()),
            confidence: None,
            reasoning: None,
            metadata: None,
        };
        assert_eq!(params.result_as_string(), "hello");

        let params = CompleteParams {
            result: serde_json::json!(42),
            confidence: None,
            reasoning: None,
            metadata: None,
        };
        assert_eq!(params.result_as_string(), "42");

        let params = CompleteParams {
            result: serde_json::json!({"key": "value"}),
            confidence: None,
            reasoning: None,
            metadata: None,
        };
        assert!(params.result_as_string().contains("key"));
    }

    #[test]
    fn test_complete_response_success() {
        let params = CompleteParams {
            result: Value::String("done".into()),
            confidence: Some(0.99),
            reasoning: Some("explanation".into()),
            metadata: None,
        };

        let response = CompleteResponse::success(&params, true);
        assert!(response.completed);
        assert_eq!(response.result, "done");
        assert_eq!(response.confidence, Some(0.99));
        assert!(response.is_final);
        assert_eq!(response.marker, COMPLETION_MARKER);
    }
}
