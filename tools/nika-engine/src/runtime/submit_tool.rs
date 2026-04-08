// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DynamicSubmitTool — Provider-native structured output via tool injection
//!
//! Implements rig's `ToolDyn` trait with a runtime JSON schema as parameters.
//! When injected into an `AgentBuilder`, the LLM is forced to call
//! `submit_result({...})` matching the schema, giving provider-side enforcement.
//!
//! This is Layer 0 of Nika's structured output defense system.
//!
//! # How it works
//!
//! The "Extractor pattern" from rig creates a synthetic tool whose `parameters`
//! field IS the target JSON schema. Combined with `tool_choice: Required`, the
//! LLM provider enforces schema compliance server-side (~90%+ first-attempt
//! success). The tool's `call()` simply passes through the arguments — the
//! schema enforcement already happened at the provider level.
//!
//! Unlike rig's built-in Extractor (which requires compile-time Rust types via
//! `#[derive(JsonSchema)]`), DynamicSubmitTool accepts runtime `serde_json::Value`
//! schemas from YAML workflow definitions.
//!
//! # Example
//!
//! ```rust,ignore
//! use nika::runtime::DynamicSubmitTool;
//! use rig::tool::ToolDyn;
//!
//! let schema = serde_json::json!({
//!     "type": "object",
//!     "properties": { "name": { "type": "string" } },
//!     "required": ["name"]
//! });
//! let tool = DynamicSubmitTool::new(schema);
//! let agent = AgentBuilder::new(model)
//!     .tools(vec![Box::new(tool) as Box<dyn ToolDyn>])
//!     .tool_choice(ToolChoice::Required)
//!     .build();
//! ```

use std::future::Future;
use std::pin::Pin;

use rig::completion::ToolDefinition;
use rig::tool::{ToolDyn, ToolError};
use serde_json::Value;

/// Type alias for boxed future (matches NikaMcpTool pattern in rig.rs)
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A synthetic tool that forces the LLM to produce structured JSON output.
///
/// Instead of relying on post-processing to extract and validate JSON from
/// free-text LLM output, this tool is injected into the `AgentBuilder` with
/// `tool_choice: Required`. The LLM MUST call `submit_result()` with arguments
/// matching the provided JSON schema.
///
/// This mirrors rig's Extractor pattern but works with runtime schemas
/// (`serde_json::Value`) instead of compile-time Rust types.
#[derive(Debug, Clone)]
pub struct DynamicSubmitTool {
    /// Tool name (default: "submit_result")
    name: String,
    /// JSON Schema for the expected output structure
    schema: Value,
}

impl DynamicSubmitTool {
    /// Create a new DynamicSubmitTool with the default name "submit_result".
    pub fn new(schema: Value) -> Self {
        Self {
            name: "submit_result".to_string(),
            schema,
        }
    }

    /// Create with a custom tool name.
    pub fn with_name(name: impl Into<String>, schema: Value) -> Self {
        Self {
            name: name.into(),
            schema,
        }
    }

    /// Get the schema (for validation after tool call).
    pub fn schema(&self) -> &Value {
        &self.schema
    }
}

impl ToolDyn for DynamicSubmitTool {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn definition(&self, _prompt: String) -> BoxFuture<'_, ToolDefinition> {
        let def = ToolDefinition {
            name: self.name.clone(),
            description: "Submit the structured result. You MUST call this tool with your \
                     response formatted as JSON matching the provided schema."
                .to_string(),
            parameters: self.schema.clone(),
        };
        Box::pin(async move { def })
    }

    fn call(&self, args: String) -> BoxFuture<'_, Result<String, ToolError>> {
        Box::pin(async move {
            // Validate it's valid JSON (the provider should enforce schema,
            // but we double-check parsing)
            let _: Value = serde_json::from_str(&args).map_err(|e| {
                ToolError::ToolCallError(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("submit_result: invalid JSON: {}", e),
                )))
            })?;
            // Return the args as-is — the caller extracts the structured data
            Ok(args)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::tool::ToolDyn;
    use serde_json::json;

    #[test]
    fn submit_tool_has_correct_name() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let tool = DynamicSubmitTool::new(schema);
        assert_eq!(tool.name(), "submit_result");
    }

    #[tokio::test]
    async fn submit_tool_definition_has_schema_as_parameters() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name"]
        });
        let tool = DynamicSubmitTool::new(schema.clone());
        let def = tool.definition("test prompt".to_string()).await;
        assert_eq!(def.name, "submit_result");
        assert_eq!(def.parameters, schema);
        assert!(def.description.contains("Submit"));
    }

    #[tokio::test]
    async fn submit_tool_call_returns_args_as_is() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::new(schema);
        let args = r#"{"name": "Alice", "age": 30}"#;
        let result = tool.call(args.to_string()).await.unwrap();
        assert_eq!(result, args);
    }

    #[tokio::test]
    async fn submit_tool_call_rejects_invalid_json() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::new(schema);
        let result = tool.call("not json".to_string()).await;
        assert!(result.is_err());
    }

    #[test]
    fn submit_tool_custom_name() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::with_name("output_json", schema);
        assert_eq!(tool.name(), "output_json");
    }

    #[test]
    fn submit_tool_schema_accessor() {
        let schema = json!({
            "type": "object",
            "properties": { "x": { "type": "integer" } }
        });
        let tool = DynamicSubmitTool::new(schema.clone());
        assert_eq!(tool.schema(), &schema);
    }

    #[tokio::test]
    async fn submit_tool_with_nested_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "address": {
                            "type": "object",
                            "properties": {
                                "city": { "type": "string" }
                            }
                        }
                    }
                }
            }
        });
        let tool = DynamicSubmitTool::new(schema.clone());
        let def = tool.definition("test".to_string()).await;
        assert_eq!(def.parameters, schema);
    }

    #[tokio::test]
    async fn submit_tool_with_array_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            }
        });
        let tool = DynamicSubmitTool::new(schema);
        let args = r#"{"items": ["a", "b", "c"]}"#;
        let result = tool.call(args.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["items"].as_array().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn submit_tool_preserves_exact_json() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::new(schema);
        // Ensure whitespace and formatting are preserved
        let args = r#"{"key":"value","num":42}"#;
        let result = tool.call(args.to_string()).await.unwrap();
        assert_eq!(result, args);
    }
}
