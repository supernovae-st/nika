//! MCP Validator Module (Layer 2)
//!
//! Pre-call validation of MCP tool parameters against cached schemas.
//!
//! ## Design
//!
//! - Uses cached schemas from ToolSchemaCache
//! - Validates parameters before calling MCP tool
//! - Returns detailed validation errors with suggestions
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::mcp::validation::{McpValidator, ValidationConfig};
//!
//! let validator = McpValidator::new(ValidationConfig::default());
//! validator.cache().populate("novanet", &tools)?;
//!
//! let result = validator.validate("novanet", "novanet_generate", &params);
//! if !result.is_valid {
//!     for error in result.errors {
//!         println!("{}", error.message);
//!     }
//! }
//! ```

use super::schema_cache::{CachedSchema, ToolSchemaCache};
use super::ValidationConfig;

/// Validation result with detailed errors
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether validation passed
    pub is_valid: bool,

    /// List of validation errors (empty if valid)
    pub errors: Vec<ValidationError>,
}

/// Single validation error
#[derive(Debug, Clone)]
pub struct ValidationError {
    /// JSON path to the error (e.g., "/entity", "/locale")
    pub path: String,

    /// Error kind
    pub kind: ValidationErrorKind,

    /// Human-readable message
    pub message: String,
}

/// Validation error kinds
#[derive(Debug, Clone, PartialEq)]
pub enum ValidationErrorKind {
    /// Required field is missing
    MissingRequired { field: String },

    /// Field type is wrong
    TypeMismatch { expected: String, actual: String },

    /// Unknown field (not in schema)
    UnknownField {
        field: String,
        suggestions: Vec<String>,
    },

    /// Value doesn't match pattern/format
    InvalidValue { reason: String },

    /// Enum value not in allowed list
    InvalidEnum { value: String, allowed: Vec<String> },
}

/// MCP parameter validator
pub struct McpValidator {
    cache: ToolSchemaCache,
    config: ValidationConfig,
}

impl McpValidator {
    /// Create a new validator with the given config
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            cache: ToolSchemaCache::new(),
            config,
        }
    }

    /// Get reference to schema cache (for populating)
    pub fn cache(&self) -> &ToolSchemaCache {
        &self.cache
    }

    /// Get reference to validation config
    pub fn config(&self) -> &ValidationConfig {
        &self.config
    }

    /// Validate parameters against cached schema
    pub fn validate(
        &self,
        server: &str,
        tool: &str,
        params: &serde_json::Value,
    ) -> ValidationResult {
        // If validation disabled, always pass
        if !self.config.pre_validate {
            return ValidationResult {
                is_valid: true,
                errors: vec![],
            };
        }

        // Get cached schema
        let Some(schema_ref) = self.cache.get(server, tool) else {
            // No schema cached = can't validate, pass through
            tracing::debug!(
                server = %server,
                tool = %tool,
                "No cached schema, skipping validation"
            );
            return ValidationResult {
                is_valid: true,
                errors: vec![],
            };
        };

        let schema = schema_ref.value();
        let mut errors = Vec::new();

        // Run JSON Schema validation
        let validation = schema.validator.iter_errors(params);

        for error in validation {
            let path = error.instance_path.to_string();
            let kind = self.classify_error(&error, schema);
            let message = self.format_error(&error, schema);

            errors.push(ValidationError {
                path,
                kind,
                message,
            });
        }

        ValidationResult {
            is_valid: errors.is_empty(),
            errors,
        }
    }

    /// Classify validation error into a kind
    fn classify_error(
        &self,
        error: &jsonschema::ValidationError,
        schema: &CachedSchema,
    ) -> ValidationErrorKind {
        let error_kind = format!("{:?}", error.kind);
        let error_msg = error.to_string();

        if error_kind.contains("Required") {
            // Extract field name from error message
            let field = self.extract_missing_field(&error_msg);
            ValidationErrorKind::MissingRequired { field }
        } else if error_kind.contains("Type") {
            ValidationErrorKind::TypeMismatch {
                expected: self.extract_expected_type(&error_msg),
                actual: self.extract_actual_type(&error_msg),
            }
        } else if error_kind.contains("AdditionalProperties") {
            // Extract field from path (format: "/field" or "/nested/field")
            let path = error.instance_path.to_string();
            let field = path
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("unknown")
                .to_string();
            let suggestions = self.find_suggestions(&field, &schema.properties);
            ValidationErrorKind::UnknownField { field, suggestions }
        } else if error_kind.contains("Enum") {
            ValidationErrorKind::InvalidEnum {
                value: format!("{}", error.instance),
                allowed: vec![], // Could extract from schema if needed
            }
        } else {
            ValidationErrorKind::InvalidValue { reason: error_msg }
        }
    }

    /// Extract missing field name from error message
    fn extract_missing_field(&self, error_msg: &str) -> String {
        // Pattern: "fieldname" is a required property (double quotes)
        if let Some(start) = error_msg.find('"') {
            if let Some(end) = error_msg[start + 1..].find('"') {
                return error_msg[start + 1..start + 1 + end].to_string();
            }
        }
        // Fallback: try single quotes
        if let Some(start) = error_msg.find('\'') {
            if let Some(end) = error_msg[start + 1..].find('\'') {
                return error_msg[start + 1..start + 1 + end].to_string();
            }
        }
        "unknown".to_string()
    }

    /// Extract expected type from error message
    fn extract_expected_type(&self, error_msg: &str) -> String {
        // Simple extraction - could be improved
        if error_msg.contains("string") {
            "string".to_string()
        } else if error_msg.contains("integer") {
            "integer".to_string()
        } else if error_msg.contains("number") {
            "number".to_string()
        } else if error_msg.contains("boolean") {
            "boolean".to_string()
        } else if error_msg.contains("array") {
            "array".to_string()
        } else if error_msg.contains("object") {
            "object".to_string()
        } else {
            "expected".to_string()
        }
    }

    /// Extract actual type from error message
    fn extract_actual_type(&self, _error_msg: &str) -> String {
        // Would need to inspect the actual value
        "actual".to_string()
    }

    /// Format a human-readable error message
    fn format_error(&self, error: &jsonschema::ValidationError, schema: &CachedSchema) -> String {
        let base = error.to_string();

        // Add suggestions for missing fields
        if !schema.required.is_empty() {
            format!(
                "{}. Required fields: [{}]",
                base,
                schema.required.join(", ")
            )
        } else {
            base
        }
    }

    /// Find similar field names (for "did you mean?")
    pub fn find_suggestions(&self, field: &str, properties: &[String]) -> Vec<String> {
        properties
            .iter()
            .filter(|p| Self::edit_distance(field, p) <= self.config.suggestion_distance)
            .cloned()
            .collect()
    }

    /// Simple Levenshtein distance (case-insensitive)
    pub fn edit_distance(a: &str, b: &str) -> usize {
        let a = a.to_lowercase();
        let b = b.to_lowercase();

        if a.is_empty() {
            return b.len();
        }
        if b.is_empty() {
            return a.len();
        }

        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();

        let mut matrix = vec![vec![0usize; b_chars.len() + 1]; a_chars.len() + 1];

        for (i, row) in matrix.iter_mut().enumerate().take(a_chars.len() + 1) {
            row[0] = i;
        }
        for (j, val) in matrix[0].iter_mut().enumerate() {
            *val = j;
        }

        for i in 1..=a_chars.len() {
            for j in 1..=b_chars.len() {
                let cost = if a_chars[i - 1] == b_chars[j - 1] {
                    0
                } else {
                    1
                };
                matrix[i][j] = std::cmp::min(
                    std::cmp::min(
                        matrix[i - 1][j] + 1, // deletion
                        matrix[i][j - 1] + 1, // insertion
                    ),
                    matrix[i - 1][j - 1] + cost, // substitution
                );
            }
        }

        matrix[a_chars.len()][b_chars.len()]
    }
}

// ============================================================================
// TESTS (TDD)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::types::ToolDefinition;
    use serde_json::json;

    // ========================================================================
    // Test: Validate missing required field
    // ========================================================================
    #[test]
    fn test_validate_missing_required_field() {
        let validator = McpValidator::new(ValidationConfig::default());
        validator
            .cache()
            .populate(
                "novanet",
                &[
                    ToolDefinition::new("novanet_generate").with_input_schema(json!({
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

        // Missing required "entity" field
        let result = validator.validate(
            "novanet",
            "novanet_generate",
            &json!({
                "locale": "fr-FR"
            }),
        );

        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 1);

        // Check that it's a MissingRequired error
        match &result.errors[0].kind {
            ValidationErrorKind::MissingRequired { field } => {
                assert_eq!(field, "entity");
            }
            other => {
                panic!("Expected MissingRequired, got {:?}", other);
            }
        }
    }

    // ========================================================================
    // Test: Valid params passes
    // ========================================================================
    #[test]
    fn test_validate_valid_params_passes() {
        let validator = McpValidator::new(ValidationConfig::default());
        validator
            .cache()
            .populate(
                "novanet",
                &[
                    ToolDefinition::new("novanet_generate").with_input_schema(json!({
                        "type": "object",
                        "properties": {
                            "entity": { "type": "string" }
                        },
                        "required": ["entity"]
                    })),
                ],
            )
            .unwrap();

        let result = validator.validate(
            "novanet",
            "novanet_generate",
            &json!({
                "entity": "qr-code"
            }),
        );

        assert!(result.is_valid);
        assert!(result.errors.is_empty());
    }

    // ========================================================================
    // Test: Validation disabled always passes
    // ========================================================================
    #[test]
    fn test_validate_disabled_always_passes() {
        let config = ValidationConfig {
            pre_validate: false,
            ..Default::default()
        };
        let validator = McpValidator::new(config);

        // No schema cached, but should pass
        let result = validator.validate("any", "tool", &json!({}));
        assert!(result.is_valid);
    }

    // ========================================================================
    // Test: No cached schema passes
    // ========================================================================
    #[test]
    fn test_validate_no_cached_schema_passes() {
        let validator = McpValidator::new(ValidationConfig::default());

        // No schema cached for this tool
        let result = validator.validate(
            "unknown",
            "tool",
            &json!({
                "anything": "goes"
            }),
        );

        assert!(result.is_valid);
    }

    // ========================================================================
    // Test: Type mismatch
    // ========================================================================
    #[test]
    fn test_validate_type_mismatch() {
        let validator = McpValidator::new(ValidationConfig::default());
        validator
            .cache()
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "count": { "type": "integer" }
                    }
                }))],
            )
            .unwrap();

        let result = validator.validate(
            "s",
            "t",
            &json!({
                "count": "not-an-integer"
            }),
        );

        assert!(!result.is_valid);
        assert!(matches!(
            &result.errors[0].kind,
            ValidationErrorKind::TypeMismatch { .. }
        ));
    }

    // ========================================================================
    // Test: Edit distance exact match
    // ========================================================================
    #[test]
    fn test_edit_distance_exact_match() {
        assert_eq!(McpValidator::edit_distance("entity", "entity"), 0);
    }

    // ========================================================================
    // Test: Edit distance one char diff
    // ========================================================================
    #[test]
    fn test_edit_distance_one_char_diff() {
        assert_eq!(McpValidator::edit_distance("entity", "entityy"), 1);
        assert_eq!(McpValidator::edit_distance("entty", "entity"), 1);
    }

    // ========================================================================
    // Test: Edit distance case insensitive
    // ========================================================================
    #[test]
    fn test_edit_distance_case_insensitive() {
        assert_eq!(McpValidator::edit_distance("Entity", "ENTITY"), 0);
    }

    // ========================================================================
    // Test: Find suggestions within distance
    // ========================================================================
    #[test]
    fn test_find_suggestions_within_distance() {
        let validator = McpValidator::new(ValidationConfig::default());
        validator
            .cache()
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "entity": {},
                        "locale": {},
                        "forms": {}
                    }
                }))],
            )
            .unwrap();

        let schema = validator.cache().get("s", "t").unwrap();
        let suggestions = validator.find_suggestions("entiy", &schema.properties);

        assert!(suggestions.contains(&"entity".to_string()));
    }

    // ========================================================================
    // Test: Edit distance empty strings
    // ========================================================================
    #[test]
    fn test_edit_distance_empty_strings() {
        assert_eq!(McpValidator::edit_distance("", ""), 0);
        assert_eq!(McpValidator::edit_distance("abc", ""), 3);
        assert_eq!(McpValidator::edit_distance("", "xyz"), 3);
    }

    // ========================================================================
    // Test: Edit distance completely different
    // ========================================================================
    #[test]
    fn test_edit_distance_completely_different() {
        assert_eq!(McpValidator::edit_distance("abc", "xyz"), 3);
    }

    // ========================================================================
    // Test: Multiple validation errors
    // ========================================================================
    #[test]
    fn test_multiple_validation_errors() {
        let validator = McpValidator::new(ValidationConfig::default());
        validator
            .cache()
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "a": { "type": "string" },
                        "b": { "type": "integer" }
                    },
                    "required": ["a", "b"]
                }))],
            )
            .unwrap();

        // Missing both required fields
        let result = validator.validate("s", "t", &json!({}));

        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 2);
    }

    // ========================================================================
    // Test: Error message includes required fields
    // ========================================================================
    #[test]
    fn test_error_message_includes_required_fields() {
        let validator = McpValidator::new(ValidationConfig::default());
        validator
            .cache()
            .populate(
                "s",
                &[ToolDefinition::new("t").with_input_schema(json!({
                    "type": "object",
                    "properties": {
                        "entity": { "type": "string" },
                        "locale": { "type": "string" }
                    },
                    "required": ["entity"]
                }))],
            )
            .unwrap();

        let result = validator.validate("s", "t", &json!({}));

        assert!(!result.is_valid);
        // Message should mention required fields
        assert!(result.errors[0].message.contains("Required fields"));
        assert!(result.errors[0].message.contains("entity"));
    }

    // ========================================================================
    // Test: Suggestion distance config respected
    // ========================================================================
    #[test]
    fn test_suggestion_distance_config() {
        let config = ValidationConfig {
            suggestion_distance: 1,
            ..Default::default()
        };
        let validator = McpValidator::new(config);

        // "entiy" is distance 1 from "entity" - should be suggested
        let suggestions = validator.find_suggestions(
            "entiy",
            &["entity".to_string(), "completely_different".to_string()],
        );
        assert!(suggestions.contains(&"entity".to_string()));
        assert!(!suggestions.contains(&"completely_different".to_string()));
    }
}
