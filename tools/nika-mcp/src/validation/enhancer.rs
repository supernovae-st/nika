// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Error Enhancer Module (Layer 3)
//!
//! Enhances MCP error messages with better context.
//!
//! ## Design
//!
//! - Parses MCP error messages to identify patterns
//! - Adds "did you mean?" suggestions based on schema
//! - Includes required fields in error messages
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika_mcp::validation::{ErrorEnhancer, ToolSchemaCache};
//!
//! let cache = ToolSchemaCache::new();
//! cache.populate("novanet", &tools)?;
//!
//! let enhancer = ErrorEnhancer::new(&cache);
//! let enhanced = enhancer.enhance("novanet", "novanet_context", original_error);
//! ```

use super::schema_cache::{CachedSchema, ToolSchemaCache};
use crate::error::McpError;

/// Enhances MCP errors with better context
pub struct ErrorEnhancer<'a> {
    cache: &'a ToolSchemaCache,
}

impl<'a> ErrorEnhancer<'a> {
    /// Create a new enhancer with access to schema cache
    pub fn new(cache: &'a ToolSchemaCache) -> Self {
        Self { cache }
    }

    /// Enhance an MCP error with better context
    pub fn enhance(&self, server: &str, tool: &str, error: McpError) -> McpError {
        let McpError::McpToolError {
            tool: tool_name,
            reason,
            error_code,
        } = &error
        else {
            return error; // Only enhance McpToolError
        };

        // Try to parse the error message
        let enhanced_reason = self.enhance_reason(server, tool, reason);

        McpError::McpToolError {
            tool: tool_name.clone(),
            reason: enhanced_reason,
            error_code: *error_code,
        }
    }

    /// Enhance a raw error reason string
    fn enhance_reason(&self, server: &str, tool: &str, reason: &str) -> String {
        let Some(schema_ref) = self.cache.get(server, tool) else {
            return reason.to_string();
        };

        let schema = schema_ref.value();
        let reason_lower = reason.to_lowercase();

        // Missing field pattern
        if reason_lower.contains("missing field") {
            return self.enhance_missing_field(reason, schema);
        }

        // Unknown field pattern
        if reason_lower.contains("unknown field") || reason_lower.contains("unexpected") {
            return self.enhance_unknown_field(reason, schema);
        }

        // Add required fields hint for any error
        if !schema.required.is_empty() {
            format!(
                "{}. Required: [{}]. Available: [{}]",
                reason,
                schema.required.join(", "),
                schema.properties.join(", ")
            )
        } else {
            reason.to_string()
        }
    }

    /// Enhance "missing field" errors
    fn enhance_missing_field(&self, reason: &str, schema: &CachedSchema) -> String {
        format!(
            "{}. Required: [{}]. Available: [{}]",
            reason,
            schema.required.join(", "),
            schema.properties.join(", ")
        )
    }

    /// Enhance "unknown field" errors
    fn enhance_unknown_field(&self, reason: &str, schema: &CachedSchema) -> String {
        format!(
            "{}. Valid fields: [{}]",
            reason,
            schema.properties.join(", ")
        )
    }
}

// ============================================================================
// TESTS (TDD)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolDefinition;
    use serde_json::json;

    // ========================================================================
    // Test: Enhance missing field error
    // ========================================================================
    #[test]
    fn test_enhance_missing_field_error() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "novanet",
                &[
                    ToolDefinition::new("novanet_context").with_input_schema(json!({
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string" },
                            "locale": { "type": "string" }
                        },
                        "required": ["entity"]
                    })),
                ],
            )
            .unwrap();

        let enhancer = ErrorEnhancer::new(&cache);
        let original = McpError::McpToolError {
            tool: "novanet_context".to_string(),
            reason: "missing field `entity`".to_string(),
            error_code: None,
        };

        let enhanced = enhancer.enhance("novanet", "novanet_context", original);

        let McpError::McpToolError { reason, .. } = enhanced else {
            panic!("Expected McpToolError");
        };

        assert!(reason.contains("Required:"));
        assert!(reason.contains("entity"));
        assert!(reason.contains("Available:"));
    }

    // ========================================================================
    // Test: Enhance unknown field error
    // ========================================================================
    #[test]
    fn test_enhance_unknown_field_error() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "novanet",
                &[ToolDefinition::new("tool").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "entity": {},
                        "locale": {}
                    }
                }))],
            )
            .unwrap();

        let enhancer = ErrorEnhancer::new(&cache);
        let original = McpError::McpToolError {
            tool: "tool".to_string(),
            reason: "unknown field `wrong_name`".to_string(),
            error_code: None,
        };

        let enhanced = enhancer.enhance("novanet", "tool", original);

        let McpError::McpToolError { reason, .. } = enhanced else {
            panic!("Expected McpToolError");
        };

        assert!(reason.contains("Valid fields:"));
        assert!(reason.contains("entity"));
        assert!(reason.contains("locale"));
    }

    // ========================================================================
    // Test: Pass through non-MCP errors
    // ========================================================================
    #[test]
    fn test_enhance_passes_through_non_mcp_errors() {
        let cache = ToolSchemaCache::new();
        let enhancer = ErrorEnhancer::new(&cache);

        let original = McpError::ParseError {
            details: "test".to_string(),
        };
        let enhanced = enhancer.enhance("s", "t", original);

        assert!(matches!(enhanced, McpError::ParseError { .. }));
    }

    // ========================================================================
    // Test: No schema returns original
    // ========================================================================
    #[test]
    fn test_enhance_no_schema_returns_original() {
        let cache = ToolSchemaCache::new();
        let enhancer = ErrorEnhancer::new(&cache);

        let original = McpError::McpToolError {
            tool: "unknown".to_string(),
            reason: "error".to_string(),
            error_code: None,
        };

        let enhanced = enhancer.enhance("s", "unknown", original);

        let McpError::McpToolError { reason, .. } = enhanced else {
            panic!("Expected McpToolError");
        };
        assert_eq!(reason, "error");
    }

    // ========================================================================
    // Test: Generic error gets required fields hint
    // ========================================================================
    #[test]
    fn test_enhance_generic_error_adds_hint() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "a": {},
                        "b": {}
                    },
                    "required": ["a"]
                }))],
            )
            .unwrap();

        let enhancer = ErrorEnhancer::new(&cache);
        let original = McpError::McpToolError {
            tool: "t".to_string(),
            reason: "some generic error".to_string(),
            error_code: None,
        };

        let enhanced = enhancer.enhance("s", "t", original);

        let McpError::McpToolError { reason, .. } = enhanced else {
            panic!("Expected McpToolError");
        };

        assert!(reason.contains("Required:"));
        assert!(reason.contains("a"));
        assert!(reason.contains("Available:"));
    }

    // ========================================================================
    // Test: No required fields, no hint added
    // ========================================================================
    #[test]
    fn test_enhance_no_required_no_hint() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "optional_field": {}
                    }
                    // No "required" array
                }))],
            )
            .unwrap();

        let enhancer = ErrorEnhancer::new(&cache);
        let original = McpError::McpToolError {
            tool: "t".to_string(),
            reason: "some error".to_string(),
            error_code: None,
        };

        let enhanced = enhancer.enhance("s", "t", original);

        let McpError::McpToolError { reason, .. } = enhanced else {
            panic!("Expected McpToolError");
        };

        // Should return original reason since no required fields
        assert_eq!(reason, "some error");
    }

    // ========================================================================
    // Test: Case insensitive pattern matching
    // ========================================================================
    #[test]
    fn test_enhance_case_insensitive() {
        let cache = ToolSchemaCache::new();
        cache
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": { "field": {} },
                    "required": ["field"]
                }))],
            )
            .unwrap();

        let enhancer = ErrorEnhancer::new(&cache);

        // Test with uppercase "Missing Field"
        let original = McpError::McpToolError {
            tool: "t".to_string(),
            reason: "Missing Field `field`".to_string(),
            error_code: None,
        };

        let enhanced = enhancer.enhance("s", "t", original);

        let McpError::McpToolError { reason, .. } = enhanced else {
            panic!("Expected McpToolError");
        };

        assert!(reason.contains("Required:"));
    }
}
