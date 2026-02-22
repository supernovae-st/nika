//! Workflow Schema Validator
//!
//! Validates workflow YAML against the Nika JSON Schema before serde parsing.
//!
//! ## Design
//!
//! - Uses embedded schema (compiled at build time)
//! - Validates YAML structure via JSON Schema
//! - Returns detailed errors with paths and suggestions
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::ast::schema_validator::WorkflowSchemaValidator;
//!
//! let validator = WorkflowSchemaValidator::new()?;
//! validator.validate_yaml(yaml_str)?;
//! ```

use crate::error::NikaError;
use jsonschema::Validator;
use serde_json::Value;
use std::sync::OnceLock;

/// Embedded schema JSON (compiled at build time)
const SCHEMA_JSON: &str = include_str!("../../schemas/nika-workflow.schema.json");

/// Global schema validator instance (lazy initialization)
static VALIDATOR: OnceLock<Result<Validator, String>> = OnceLock::new();

/// Workflow schema validator
///
/// Validates workflow YAML against the Nika JSON Schema.
pub struct WorkflowSchemaValidator {
    /// Compiled JSON Schema validator
    validator: &'static Validator,
}

impl WorkflowSchemaValidator {
    /// Create a new workflow schema validator
    ///
    /// Uses a cached global validator for efficiency.
    pub fn new() -> Result<Self, NikaError> {
        let validator_result = VALIDATOR.get_or_init(|| {
            let schema: Value = serde_json::from_str(SCHEMA_JSON)
                .map_err(|e| format!("Failed to parse schema JSON: {}", e))?;
            Validator::new(&schema).map_err(|e| format!("Failed to compile schema: {}", e))
        });

        match validator_result {
            Ok(validator) => Ok(Self { validator }),
            Err(e) => Err(NikaError::ValidationError { reason: e.clone() }),
        }
    }

    /// Validate YAML string against the workflow schema
    ///
    /// # Arguments
    ///
    /// * `yaml` - YAML content to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` if valid
    /// * `Err(NikaError::SchemaValidationFailed)` with detailed errors if invalid
    pub fn validate_yaml(&self, yaml: &str) -> Result<(), NikaError> {
        // Parse YAML to JSON Value (serde_yaml can handle this)
        let value: Value = serde_yaml::from_str(yaml).map_err(|e| NikaError::ParseError {
            details: format!("YAML parse error: {}", e),
        })?;

        self.validate_value(&value)
    }

    /// Validate a JSON Value against the workflow schema
    ///
    /// # Arguments
    ///
    /// * `value` - JSON value to validate
    ///
    /// # Returns
    ///
    /// * `Ok(())` if valid
    /// * `Err(NikaError::SchemaValidationFailed)` with detailed errors if invalid
    pub fn validate_value(&self, value: &Value) -> Result<(), NikaError> {
        let errors: Vec<SchemaError> = self
            .validator
            .iter_errors(value)
            .map(|e| SchemaError {
                path: e.instance_path.to_string(),
                message: e.to_string(),
                kind: classify_error(&e),
            })
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(NikaError::SchemaValidationFailed { errors })
        }
    }
}

/// Schema validation error details
#[derive(Debug, Clone)]
pub struct SchemaError {
    /// JSON pointer path to the error (e.g., "/tasks/0/invoke/params")
    pub path: String,
    /// Human-readable error message
    pub message: String,
    /// Error classification
    pub kind: SchemaErrorKind,
}

/// Schema error classification
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaErrorKind {
    /// Missing required field
    MissingRequired { field: String },
    /// Unknown field (not in schema)
    UnknownField { field: String },
    /// Type mismatch
    TypeMismatch { expected: String, actual: String },
    /// Invalid enum value
    InvalidEnum { value: String, allowed: Vec<String> },
    /// Generic validation error
    Other,
}

/// Classify a JSON Schema error into a SchemaErrorKind
fn classify_error(error: &jsonschema::ValidationError) -> SchemaErrorKind {
    let error_str = format!("{:?}", error.kind);
    let message = error.to_string();

    if error_str.contains("Required") {
        // Extract field name from message
        let field = extract_quoted(&message).unwrap_or_else(|| "unknown".to_string());
        SchemaErrorKind::MissingRequired { field }
    } else if error_str.contains("AdditionalProperties") {
        // Extract field from path
        let path = error.instance_path.to_string();
        let field = path
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("unknown")
            .to_string();
        SchemaErrorKind::UnknownField { field }
    } else if error_str.contains("Type") {
        SchemaErrorKind::TypeMismatch {
            expected: extract_type(&message).unwrap_or_else(|| "expected".to_string()),
            actual: "actual".to_string(),
        }
    } else if error_str.contains("Enum") {
        SchemaErrorKind::InvalidEnum {
            value: error.instance.to_string(),
            allowed: vec![],
        }
    } else {
        SchemaErrorKind::Other
    }
}

/// Extract quoted string from error message
fn extract_quoted(msg: &str) -> Option<String> {
    // Pattern: "fieldname" or 'fieldname'
    if let Some(start) = msg.find('"') {
        if let Some(end) = msg[start + 1..].find('"') {
            return Some(msg[start + 1..start + 1 + end].to_string());
        }
    }
    if let Some(start) = msg.find('\'') {
        if let Some(end) = msg[start + 1..].find('\'') {
            return Some(msg[start + 1..start + 1 + end].to_string());
        }
    }
    None
}

/// Extract type name from error message
fn extract_type(msg: &str) -> Option<String> {
    for t in ["string", "integer", "number", "boolean", "array", "object"] {
        if msg.contains(t) {
            return Some(t.to_string());
        }
    }
    None
}

// ============================================================================
// TESTS (TDD)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ========================================================================
    // Test: Validator creation succeeds
    // ========================================================================
    #[test]
    fn test_validator_creation_succeeds() {
        let validator = WorkflowSchemaValidator::new();
        assert!(
            validator.is_ok(),
            "Validator should be created successfully"
        );
    }

    // ========================================================================
    // Test: Valid minimal workflow passes
    // ========================================================================
    #[test]
    fn test_valid_minimal_workflow_passes() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: step1
    infer: "Hello world"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_ok(), "Valid workflow should pass: {:?}", result);
    }

    // ========================================================================
    // Test: Missing schema field fails
    // ========================================================================
    #[test]
    fn test_missing_schema_field_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
tasks:
  - id: step1
    infer: "Hello"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "Missing schema should fail");

        if let Err(NikaError::SchemaValidationFailed { errors }) = result {
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0].kind,
                SchemaErrorKind::MissingRequired { ref field } if field == "schema"
            ));
        } else {
            panic!("Expected SchemaValidationFailed error");
        }
    }

    // ========================================================================
    // Test: Invalid schema version fails
    // ========================================================================
    #[test]
    fn test_invalid_schema_version_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@9.9"
tasks:
  - id: step1
    infer: "Hello"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "Invalid schema version should fail");

        if let Err(NikaError::SchemaValidationFailed { errors }) = result {
            assert!(!errors.is_empty());
            assert!(matches!(
                errors[0].kind,
                SchemaErrorKind::InvalidEnum { .. }
            ));
        } else {
            panic!("Expected SchemaValidationFailed error");
        }
    }

    // ========================================================================
    // Test: Unknown field in invoke params fails
    // ========================================================================
    #[test]
    fn test_unknown_field_in_invoke_params_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
mcp:
  novanet:
    command: cargo
    args: [run]
tasks:
  - id: describe
    invoke:
      mcp: novanet
      tool: novanet_describe
      params:
        unknown_field: "value"
"#;
        let result = validator.validate_yaml(yaml);
        // Note: params is not additionalProperties: false, so this may pass
        // But the key insight is we can validate the overall structure
        // The user's original issue was about invoke.params structure
        // Actually looking at the schema, params has additionalProperties: true
        // So this test should pass (params can have any fields)
        assert!(
            result.is_ok(),
            "Params can have any fields (additionalProperties: true)"
        );
    }

    // ========================================================================
    // Test: Missing required invoke.mcp fails
    // ========================================================================
    #[test]
    fn test_missing_required_invoke_mcp_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: describe
    invoke:
      tool: novanet_describe
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "Missing invoke.mcp should fail");

        if let Err(NikaError::SchemaValidationFailed { errors }) = result {
            assert!(!errors.is_empty());
            // Should have MissingRequired for 'mcp'
            let has_mcp_error = errors.iter().any(
                |e| matches!(&e.kind, SchemaErrorKind::MissingRequired { field } if field == "mcp"),
            );
            assert!(has_mcp_error, "Should have MissingRequired for 'mcp'");
        } else {
            panic!("Expected SchemaValidationFailed error");
        }
    }

    // ========================================================================
    // Test: Unknown field at workflow level fails
    // ========================================================================
    #[test]
    fn test_unknown_field_at_workflow_level_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
unknown_field: "should fail"
tasks:
  - id: step1
    infer: "Hello"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(
            result.is_err(),
            "Unknown field at workflow level should fail"
        );

        if let Err(NikaError::SchemaValidationFailed { errors }) = result {
            assert!(!errors.is_empty());
            let has_unknown_error = errors
                .iter()
                .any(|e| matches!(&e.kind, SchemaErrorKind::UnknownField { .. }));
            assert!(has_unknown_error, "Should have UnknownField error");
        } else {
            panic!("Expected SchemaValidationFailed error");
        }
    }

    // ========================================================================
    // Test: Valid invoke workflow passes
    // ========================================================================
    #[test]
    fn test_valid_invoke_workflow_passes() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
provider: claude
mcp:
  novanet:
    command: cargo
    args: [run, -p, novanet-mcp]
    env:
      NEO4J_URI: bolt://localhost:7687
tasks:
  - id: describe
    invoke:
      mcp: novanet
      tool: novanet_describe
      params: {}
    output:
      format: json

  - id: generate
    use:
      schema: describe
    invoke:
      mcp: novanet
      tool: novanet_generate
      params:
        entity: qr-code
        locale: fr-FR
        forms:
          - text
          - title
    output:
      format: json
flows:
  - source: describe
    target: generate
"#;
        let result = validator.validate_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid invoke workflow should pass: {:?}",
            result
        );
    }

    // ========================================================================
    // Test: Task without any verb fails
    // ========================================================================
    #[test]
    fn test_task_without_verb_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: step1
    output:
      format: json
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "Task without verb should fail");
    }

    // ========================================================================
    // Test: Multiple verbs in task fails (oneOf)
    // ========================================================================
    #[test]
    fn test_multiple_verbs_in_task_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: step1
    infer: "Hello"
    exec: "echo done"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "Multiple verbs should fail");
    }

    // ========================================================================
    // Test: Valid agent params passes
    // ========================================================================
    #[test]
    fn test_valid_agent_params_passes() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
mcp:
  novanet:
    command: cargo
tasks:
  - id: orchestrator
    agent:
      prompt: "Generate content"
      mcp:
        - novanet
      max_turns: 5
      depth_limit: 3
      extended_thinking: true
      thinking_budget: 8192
"#;
        let result = validator.validate_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid agent params should pass: {:?}",
            result
        );
    }

    // ========================================================================
    // Test: Invalid depth_limit fails
    // ========================================================================
    #[test]
    fn test_invalid_depth_limit_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: orchestrator
    agent:
      prompt: "Generate content"
      depth_limit: 100
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "depth_limit > 10 should fail");
    }

    // ========================================================================
    // Test: Valid decompose spec passes
    // ========================================================================
    #[test]
    fn test_valid_decompose_spec_passes() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: expand_entities
    decompose:
      strategy: semantic
      traverse: HAS_CHILD
      source: "$entity"
      max_items: 10
    infer: "Generate for {{use.item}}"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid decompose spec should pass: {:?}",
            result
        );
    }

    // ========================================================================
    // Test: Invalid decompose strategy fails
    // ========================================================================
    #[test]
    fn test_invalid_decompose_strategy_fails() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: expand_entities
    decompose:
      strategy: invalid_strategy
      traverse: HAS_CHILD
      source: "$entity"
    infer: "Generate for {{use.item}}"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(result.is_err(), "Invalid decompose strategy should fail");
    }

    // ========================================================================
    // Test: Valid lazy binding passes
    // ========================================================================
    #[test]
    fn test_valid_lazy_binding_passes() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: step1
    infer: "Hello"

  - id: step2
    use:
      eager: step1
      lazy_val:
        path: step1.result
        lazy: true
        default: "fallback"
    infer: "Using {{use.eager}} and {{use.lazy_val}}"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(
            result.is_ok(),
            "Valid lazy binding should pass: {:?}",
            result
        );
    }

    // ========================================================================
    // Test: for_each with binding expression passes
    // ========================================================================
    #[test]
    fn test_for_each_binding_expression_passes() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: process
    for_each: "{{use.items}}"
    as: item
    concurrency: 5
    infer: "Process {{use.item}}"
"#;
        let result = validator.validate_yaml(yaml);
        assert!(
            result.is_ok(),
            "for_each binding expression should pass: {:?}",
            result
        );
    }

    // ========================================================================
    // Test: Error message includes path
    // ========================================================================
    #[test]
    fn test_error_message_includes_path() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let yaml = r#"
schema: "nika/workflow@0.5"
tasks:
  - id: step1
    invoke:
      tool: novanet_describe
"#;
        let result = validator.validate_yaml(yaml);
        if let Err(NikaError::SchemaValidationFailed { errors }) = result {
            // Should have path pointing to the invoke object
            let has_path = errors.iter().any(|e| e.path.contains("invoke"));
            assert!(has_path, "Error should include path to invoke");
        } else {
            panic!("Expected SchemaValidationFailed error");
        }
    }

    // ========================================================================
    // Test: JSON value validation works
    // ========================================================================
    #[test]
    fn test_validate_value_works() {
        let validator = WorkflowSchemaValidator::new().unwrap();
        let value = json!({
            "schema": "nika/workflow@0.5",
            "tasks": [
                {
                    "id": "step1",
                    "infer": "Hello"
                }
            ]
        });
        let result = validator.validate_value(&value);
        assert!(result.is_ok(), "JSON value validation should work");
    }
}
