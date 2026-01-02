//! Output Handling - task result processing (v0.1)
//!
//! Extracted from runner.rs for cleaner separation:
//! - `make_task_result`: Convert raw output to TaskResult with format handling
//! - `validate_schema`: Validate JSON output against JSON Schema

use serde_json::Value;

use crate::ast::OutputFormat;
use crate::error::NikaError;
use crate::store::TaskResult;

/// Convert execution output to TaskResult, parsing as JSON if output format is json.
/// Also validates against schema if declared.
pub async fn make_task_result(
    output: String,
    policy: Option<&crate::ast::OutputPolicy>,
    duration: std::time::Duration,
) -> TaskResult {
    if let Some(policy) = policy {
        if policy.format == OutputFormat::Json {
            // Parse as JSON
            let json_value = match serde_json::from_str::<Value>(&output) {
                Ok(v) => v,
                Err(e) => {
                    return TaskResult::failed(
                        format!("NIKA-060: Invalid JSON output: {}", e),
                        duration,
                    );
                }
            };

            // Validate against schema if declared
            if let Some(schema_path) = &policy.schema {
                if let Err(e) = validate_schema(&json_value, schema_path).await {
                    return TaskResult::failed(e.to_string(), duration);
                }
            }

            return TaskResult::success(json_value, duration);
        }
    }
    TaskResult::success_str(output, duration)
}

/// Validate JSON value against a JSON Schema file
pub async fn validate_schema(value: &Value, schema_path: &str) -> Result<(), NikaError> {
    // Read schema file
    let schema_str = tokio::fs::read_to_string(schema_path).await.map_err(|e| {
        NikaError::SchemaFailed {
            details: format!("Failed to read schema '{}': {}", schema_path, e),
        }
    })?;

    // Parse schema
    let schema: Value = serde_json::from_str(&schema_str).map_err(|e| {
        NikaError::SchemaFailed {
            details: format!("Invalid JSON in schema '{}': {}", schema_path, e),
        }
    })?;

    // Compile and validate
    let compiled = jsonschema::validator_for(&schema).map_err(|e| {
        NikaError::SchemaFailed {
            details: format!("Invalid schema '{}': {}", schema_path, e),
        }
    })?;

    // Collect all validation errors
    let errors: Vec<_> = compiled.iter_errors(value).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        let error_msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        Err(NikaError::SchemaFailed {
            details: error_msgs.join("; "),
        })
    }
}
