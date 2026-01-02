//! Output Handling - task result processing (v0.1)
//!
//! Extracted from runner.rs for cleaner separation:
//! - `make_task_result`: Convert raw output to TaskResult with format handling
//! - `validate_schema`: Validate JSON output against JSON Schema (with caching)

use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use serde_json::Value;

use crate::ast::OutputFormat;
use crate::error::NikaError;
use crate::store::TaskResult;

/// Global schema cache: path → parsed JSON schema
/// Avoids re-reading and re-parsing schema files on repeated validations.
static SCHEMA_CACHE: LazyLock<DashMap<Arc<str>, Arc<Value>>> = LazyLock::new(DashMap::new);

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

/// Validate JSON value against a JSON Schema file (with caching)
///
/// Schema files are cached after first load to avoid repeated file I/O.
pub async fn validate_schema(value: &Value, schema_path: &str) -> Result<(), NikaError> {
    // Try cache first (fast path)
    let schema = if let Some(cached) = SCHEMA_CACHE.get(schema_path) {
        Arc::clone(cached.value())
    } else {
        // Cache miss: read and parse schema
        let schema_str =
            tokio::fs::read_to_string(schema_path)
                .await
                .map_err(|e| NikaError::SchemaFailed {
                    details: format!("Failed to read schema '{}': {}", schema_path, e),
                })?;

        let schema: Value =
            serde_json::from_str(&schema_str).map_err(|e| NikaError::SchemaFailed {
                details: format!("Invalid JSON in schema '{}': {}", schema_path, e),
            })?;

        // Store in cache
        let schema = Arc::new(schema);
        SCHEMA_CACHE.insert(Arc::from(schema_path), Arc::clone(&schema));
        schema
    };

    // Compile and validate (compilation is fast, validation needs fresh instance)
    let compiled = jsonschema::validator_for(&schema).map_err(|e| NikaError::SchemaFailed {
        details: format!("Invalid schema '{}': {}", schema_path, e),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::time::Duration;
    use tempfile::NamedTempFile;

    #[tokio::test]
    async fn schema_cache_works() {
        // Create a temp schema file
        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(
            schema_file,
            r#"{{"type": "object", "properties": {{"name": {{"type": "string"}}}}}}"#
        )
        .unwrap();
        let schema_path = schema_file.path().to_str().unwrap();

        // First validation - cache miss
        let value = serde_json::json!({"name": "test"});
        assert!(validate_schema(&value, schema_path).await.is_ok());

        // Second validation - cache hit (same path)
        assert!(validate_schema(&value, schema_path).await.is_ok());

        // Cache should have the entry
        assert!(SCHEMA_CACHE.contains_key(schema_path));
    }

    #[tokio::test]
    async fn schema_validation_rejects_invalid() {
        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, r#"{{"type": "object", "properties": {{"age": {{"type": "number"}}}}, "required": ["age"]}}"#).unwrap();
        let schema_path = schema_file.path().to_str().unwrap();

        // Missing required field
        let value = serde_json::json!({"name": "test"});
        assert!(validate_schema(&value, schema_path).await.is_err());

        // Correct value
        let value = serde_json::json!({"age": 25});
        assert!(validate_schema(&value, schema_path).await.is_ok());
    }

    #[tokio::test]
    async fn make_task_result_validates_json() {
        use crate::ast::OutputPolicy;

        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, r#"{{"type": "object"}}"#).unwrap();
        let schema_path = schema_file.path().to_string_lossy().to_string();

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: Some(schema_path),
        };

        // Valid JSON object
        let result = make_task_result(
            r#"{"key": "value"}"#.to_string(),
            Some(&policy),
            Duration::from_millis(100),
        )
        .await;
        assert!(result.is_success());

        // Invalid JSON
        let result = make_task_result(
            "not json".to_string(),
            Some(&policy),
            Duration::from_millis(100),
        )
        .await;
        assert!(!result.is_success());
    }
}
