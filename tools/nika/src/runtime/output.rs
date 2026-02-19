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

    // ══════════════════════════════════════════════════════════════
    // make_task_result EDGE CASES
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn make_task_result_no_policy_returns_text() {
        let result = make_task_result(
            "plain text output".to_string(),
            None,
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_success());
        // Without policy, output should be stored as string (success_str)
        assert_eq!(
            result.output.as_ref(),
            &serde_json::Value::String("plain text output".to_string())
        );
    }

    #[tokio::test]
    async fn make_task_result_json_no_schema_parses_json() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None, // No schema validation
        };

        let result = make_task_result(
            r#"{"key": "value", "nested": {"a": 1}}"#.to_string(),
            Some(&policy),
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_success());
        // Should be parsed as JSON object, not string
        assert!(result.output.is_object());
        assert_eq!(result.output["key"], "value");
        assert_eq!(result.output["nested"]["a"], 1);
    }

    #[tokio::test]
    async fn make_task_result_invalid_json_returns_error_with_code() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
        };

        let result = make_task_result(
            "{ invalid json".to_string(),
            Some(&policy),
            Duration::from_millis(50),
        )
        .await;

        assert!(!result.is_success());
        // Error should contain NIKA-060 code
        let error_msg = result.error().expect("Should have error");
        assert!(
            error_msg.contains("NIKA-060"),
            "Error should contain NIKA-060 code: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn make_task_result_text_format_returns_raw_string() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Text,
            schema: None,
        };

        // Even valid JSON should be treated as text
        let result = make_task_result(
            r#"{"key": "value"}"#.to_string(),
            Some(&policy),
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_success());
        // Should be stored as string, not parsed JSON
        assert!(result.output.is_string());
        assert_eq!(
            result.output.as_ref(),
            &serde_json::Value::String(r#"{"key": "value"}"#.to_string())
        );
    }

    // ══════════════════════════════════════════════════════════════
    // validate_schema ERROR PATHS
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn validate_schema_file_not_found_returns_error() {
        let value = serde_json::json!({"name": "test"});
        let result = validate_schema(&value, "/nonexistent/path/schema.json").await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_string = err.to_string();
        assert!(
            err_string.contains("Failed to read schema"),
            "Error should mention file read failure: {}",
            err_string
        );
    }

    #[tokio::test]
    async fn validate_schema_invalid_json_in_schema_file() {
        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, "{{ not valid json").unwrap();
        let schema_path = schema_file.path().to_str().unwrap();

        let value = serde_json::json!({"name": "test"});
        let result = validate_schema(&value, schema_path).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_string = err.to_string();
        assert!(
            err_string.contains("Invalid JSON in schema"),
            "Error should mention invalid JSON: {}",
            err_string
        );
    }

    #[tokio::test]
    async fn validate_schema_invalid_schema_structure() {
        let mut schema_file = NamedTempFile::new().unwrap();
        // Valid JSON but not a valid JSON Schema (type must be a string, not number)
        writeln!(schema_file, r#"{{"type": 123}}"#).unwrap();
        let schema_path = schema_file.path().to_str().unwrap();

        let value = serde_json::json!({"name": "test"});
        let result = validate_schema(&value, schema_path).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_string = err.to_string();
        assert!(
            err_string.contains("Invalid schema"),
            "Error should mention invalid schema: {}",
            err_string
        );
    }

    #[tokio::test]
    async fn validate_schema_multiple_validation_errors() {
        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(
            schema_file,
            r#"{{
                "type": "object",
                "properties": {{
                    "name": {{"type": "string"}},
                    "age": {{"type": "number"}}
                }},
                "required": ["name", "age"]
            }}"#
        )
        .unwrap();
        let schema_path = schema_file.path().to_str().unwrap();

        // Missing both required fields
        let value = serde_json::json!({});
        let result = validate_schema(&value, schema_path).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_string = err.to_string();
        // Should mention both missing fields
        assert!(
            err_string.contains("name") || err_string.contains("required"),
            "Error should mention validation issues: {}",
            err_string
        );
    }

    // ══════════════════════════════════════════════════════════════
    // EDGE CASES
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn make_task_result_large_json_output() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
        };

        // Generate large JSON array
        let large_array: Vec<i32> = (0..10000).collect();
        let json_str = serde_json::to_string(&large_array).unwrap();

        let result = make_task_result(json_str, Some(&policy), Duration::from_millis(100)).await;

        assert!(result.is_success());
        assert!(result.output.is_array());
        assert_eq!(result.output.as_array().unwrap().len(), 10000);
    }

    #[tokio::test]
    async fn make_task_result_unicode_content() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
        };

        // JSON with various Unicode characters
        let json_str = r#"{"greeting": "你好世界", "emoji": "🚀✨", "japanese": "こんにちは"}"#;

        let result =
            make_task_result(json_str.to_string(), Some(&policy), Duration::from_millis(50)).await;

        assert!(result.is_success());
        assert_eq!(result.output["greeting"], "你好世界");
        assert_eq!(result.output["emoji"], "🚀✨");
        assert_eq!(result.output["japanese"], "こんにちは");
    }

    #[tokio::test]
    async fn schema_cache_concurrent_access() {
        // Create a temp schema file
        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, r#"{{"type": "object"}}"#).unwrap();
        let schema_path = schema_file.path().to_str().unwrap().to_string();

        // Spawn multiple concurrent validation tasks
        let handles: Vec<_> = (0..10)
            .map(|i| {
                let path = schema_path.clone();
                tokio::spawn(async move {
                    let value = serde_json::json!({"id": i});
                    validate_schema(&value, &path).await
                })
            })
            .collect();

        // All should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }

    #[tokio::test]
    async fn make_task_result_preserves_duration() {
        let duration = Duration::from_secs(5);
        let result = make_task_result("output".to_string(), None, duration).await;

        assert_eq!(result.duration, duration);
    }

    #[tokio::test]
    async fn make_task_result_json_array() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
        };

        let result = make_task_result(
            r#"[1, 2, 3, "four"]"#.to_string(),
            Some(&policy),
            Duration::from_millis(50),
        )
        .await;

        assert!(result.is_success());
        assert!(result.output.is_array());
        let arr = result.output.as_array().unwrap();
        assert_eq!(arr.len(), 4);
        assert_eq!(arr[3], "four");
    }
}
