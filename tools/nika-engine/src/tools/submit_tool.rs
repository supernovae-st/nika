// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Dynamic Submit Tool - Structured Output Enforcement
//!
//! Based on rig's Extractor pattern - injects JSON Schema as tool definition
//! so LLM understands expected output format.
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::tools::DynamicSubmitTool;
//! use serde_json::json;
//!
//! let schema = json!({
//!     "type": "object",
//!     "properties": {
//!         "keywords": { "type": "array", "items": { "type": "string" } }
//!     },
//!     "required": ["keywords"]
//! });
//!
//! let tool = DynamicSubmitTool::new(schema);
//! let definition = tool.to_claude_tool();
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::NikaError;
use crate::runtime::output::validate_inline_schema;

/// Tool definition for provider-agnostic tool calling
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
}

/// Dynamic submit tool that enforces structured output via tool calling
///
/// When added to a chat completion request, the LLM is instructed to "submit"
/// its response using this tool, which includes the expected JSON Schema.
#[derive(Debug, Clone)]
pub struct DynamicSubmitTool {
    schema: Value,
    description: Option<String>,
}

impl DynamicSubmitTool {
    /// Create a new submit tool with the given JSON Schema
    pub fn new(schema: Value) -> Self {
        Self {
            schema,
            description: None,
        }
    }

    /// Set a custom description for the tool
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Get the tool definition for the LLM
    pub fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "submit".to_string(),
            description: self.description.clone().unwrap_or_else(|| {
                "Submit your response in the required structured format. \
                 Use this tool to provide your final answer. The response \
                 MUST match the schema exactly."
                    .to_string()
            }),
            parameters: self.schema.clone(),
        }
    }

    /// Validate and return the submitted data
    pub fn validate(&self, input: &Value) -> Result<(), NikaError> {
        validate_inline_schema(input, &self.schema)
    }

    /// Get the schema for error feedback
    pub fn schema(&self) -> &Value {
        &self.schema
    }

    /// Convert to Claude tool format
    pub fn to_claude_tool(&self) -> Value {
        serde_json::json!({
            "name": "submit",
            "description": self.definition().description,
            "input_schema": self.schema
        })
    }

    /// Convert to OpenAI function format
    pub fn to_openai_tool(&self) -> Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "submit",
                "description": self.definition().description,
                "parameters": self.schema
            }
        })
    }

    /// Convert to generic tool format (for rig-core)
    pub fn to_rig_tool(&self) -> Value {
        serde_json::json!({
            "name": "submit",
            "description": self.definition().description,
            "parameters": self.schema
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_submit_tool_new() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            }
        });

        let tool = DynamicSubmitTool::new(schema.clone());
        assert_eq!(tool.schema(), &schema);
    }

    #[test]
    fn test_submit_tool_definition() {
        let schema = json!({
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["keywords"]
        });

        let tool = DynamicSubmitTool::new(schema.clone());
        let def = tool.definition();

        assert_eq!(def.name, "submit");
        assert_eq!(def.parameters, schema);
        assert!(def.description.contains("structured format"));
    }

    #[test]
    fn test_submit_tool_with_custom_description() {
        let schema = json!({"type": "object"});
        let tool = DynamicSubmitTool::new(schema)
            .with_description("Extract SEO keywords from the content");

        let def = tool.definition();
        assert_eq!(def.description, "Extract SEO keywords from the content");
    }

    #[test]
    fn test_submit_tool_validate_success() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });

        let tool = DynamicSubmitTool::new(schema);
        let input = json!({"name": "test"});
        assert!(tool.validate(&input).is_ok());
    }

    #[test]
    fn test_submit_tool_validate_failure() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });

        let tool = DynamicSubmitTool::new(schema);
        let input = json!({"wrong": "field"});
        let result = tool.validate(&input);
        assert!(result.is_err());
    }

    #[test]
    fn test_to_claude_tool_format() {
        let schema = json!({
            "type": "object",
            "properties": {
                "result": { "type": "string" }
            }
        });

        let tool = DynamicSubmitTool::new(schema.clone());
        let claude_tool = tool.to_claude_tool();

        assert_eq!(claude_tool["name"], "submit");
        assert_eq!(claude_tool["input_schema"], schema);
        assert!(claude_tool["description"].is_string());
    }

    #[test]
    fn test_to_openai_tool_format() {
        let schema = json!({
            "type": "object",
            "properties": {
                "result": { "type": "string" }
            }
        });

        let tool = DynamicSubmitTool::new(schema.clone());
        let openai_tool = tool.to_openai_tool();

        assert_eq!(openai_tool["type"], "function");
        assert_eq!(openai_tool["function"]["name"], "submit");
        assert_eq!(openai_tool["function"]["parameters"], schema);
    }

    #[test]
    fn test_complex_schema_validation() {
        let schema = json!({
            "type": "object",
            "properties": {
                "keywords": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "value": { "type": "string" },
                            "slug_form": { "type": "string", "pattern": "^[a-z0-9-]+$" },
                            "volume": { "type": "integer", "minimum": 0 },
                            "difficulty": { "type": "integer", "minimum": 0, "maximum": 100 }
                        },
                        "required": ["value", "slug_form", "volume", "difficulty"]
                    }
                }
            },
            "required": ["keywords"]
        });

        let tool = DynamicSubmitTool::new(schema);

        // Valid input
        let valid = json!({
            "keywords": [{
                "value": "qr code generator",
                "slug_form": "qr-code-generator",
                "volume": 10000,
                "difficulty": 45
            }]
        });
        assert!(tool.validate(&valid).is_ok());

        // Invalid: missing required field
        let missing_field = json!({
            "keywords": [{
                "value": "qr code",
                "slug_form": "qr-code",
                "volume": 5000
                // missing difficulty
            }]
        });
        assert!(tool.validate(&missing_field).is_err());

        // Invalid: wrong type
        let wrong_type = json!({
            "keywords": [{
                "value": "qr code",
                "slug_form": "qr-code",
                "volume": "not a number",
                "difficulty": 50
            }]
        });
        assert!(tool.validate(&wrong_type).is_err());

        // Invalid: difficulty out of range
        let out_of_range = json!({
            "keywords": [{
                "value": "qr code",
                "slug_form": "qr-code",
                "volume": 5000,
                "difficulty": 150  // max is 100
            }]
        });
        assert!(tool.validate(&out_of_range).is_err());
    }
}
