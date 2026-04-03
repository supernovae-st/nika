//! Output Handling - task result processing
//!
//! Extracted from runner.rs for cleaner separation:
//! - `make_task_result`: Convert raw output to TaskResult with format handling
//! - `validate_schema`: Validate JSON output against JSON Schema (with caching)
//! - `validate_schema_ref`: Validate against SchemaRef (inline or file)
//! - `extract_json_from_output`: Extract JSON from markdown code blocks
//! - `format_validation_errors`: Format errors for retry feedback

use std::sync::{Arc, LazyLock};

use dashmap::DashMap;
use jsonschema::Validator;
use serde_json::Value;

use crate::ast::output::SchemaRef;
use crate::ast::OutputFormat;
use crate::error::NikaError;
use crate::store::TaskResult;

/// Maximum number of entries in each cache before eviction.
const CACHE_MAX_ENTRIES: usize = 512;

/// Global schema cache: path → parsed JSON schema.
/// Avoids re-reading and re-parsing schema files on repeated validations.
static SCHEMA_CACHE: LazyLock<DashMap<Arc<str>, Arc<Value>>> = LazyLock::new(DashMap::new);

/// Global compiled validator cache: blake3 hash of schema JSON → compiled validator.
/// Uses blake3 for cryptographic collision resistance (replaces DefaultHasher u64).
/// Avoids recompiling the same JSON Schema on every validation call (10-50ms each).
static VALIDATOR_CACHE: LazyLock<DashMap<String, Arc<Validator>>> = LazyLock::new(DashMap::new);

/// Compute a blake3 hash of a JSON Schema value for use as cache key.
/// Returns a hex-encoded hash string with cryptographic collision resistance.
fn hash_schema(schema: &Value) -> String {
    let canonical = serde_json::to_string(schema).unwrap_or_default();
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

/// Get or compile a JSON Schema validator, using the global cache.
fn get_or_compile_validator(schema: &Value) -> Result<Arc<Validator>, NikaError> {
    let key = hash_schema(schema);
    if let Some(cached) = VALIDATOR_CACHE.get(&key) {
        return Ok(Arc::clone(cached.value()));
    }
    let compiled = jsonschema::validator_for(schema).map_err(|e| NikaError::SchemaFailed {
        details: format!("Invalid schema: {e}"),
    })?;
    let compiled = Arc::new(compiled);
    // Evict all entries if cache exceeds size cap to prevent unbounded growth
    if VALIDATOR_CACHE.len() >= CACHE_MAX_ENTRIES {
        VALIDATOR_CACHE.clear();
    }
    VALIDATOR_CACHE.insert(key, Arc::clone(&compiled));
    Ok(compiled)
}

/// Try to extract JSON from inside a fenced code block starting with `marker`.
/// Returns `Some(Value)` if a valid JSON block was found, `None` otherwise.
fn try_extract_fenced_json(text: &str, marker: &str) -> Option<Value> {
    let start = text.find(marker)?;
    let after_marker = &text[start + marker.len()..];
    let end = after_marker.find("```")?;
    let json_str = after_marker[..end].trim();
    serde_json::from_str::<Value>(json_str).ok()
}

/// Extract JSON from LLM output, handling markdown code blocks.
///
/// LLMs often wrap JSON in markdown code blocks like:
/// ```json
/// {"key": "value"}
/// ```
///
/// This function tries multiple strategies:
/// 1. Direct JSON parsing (fast path)
/// 2. Extract from ```json ... ``` blocks
/// 3. Extract from ``` ... ``` blocks (no language)
/// 4. Find outermost { } or [ ] brackets
fn extract_json_from_output(output: &str) -> Result<Value, String> {
    let trimmed = output.trim();

    // Strategy 1: Direct parse (fast path for well-behaved LLMs)
    if let Ok(v) = serde_json::from_str::<Value>(trimmed) {
        return Ok(v);
    }

    // Strategy 2: Extract from ```json ... ``` blocks (case-insensitive)
    if let Some(result) = try_extract_fenced_json(trimmed, "```json") {
        return Ok(result);
    }
    // Also handle ```JSON (some LLMs use uppercase)
    if let Some(result) = try_extract_fenced_json(trimmed, "```JSON") {
        return Ok(result);
    }

    // Strategy 3: Extract from ``` ... ``` blocks (no language specifier)
    // Handle both Unix (\n) and Windows (\r\n) line endings, plus bare ``` with space
    for fence in &["```\n", "```\r\n", "``` "] {
        if let Some(result) = try_extract_fenced_json(trimmed, fence) {
            return Ok(result);
        }
    }

    // Strategy 4: Find outermost { } or [ ] brackets
    let first_brace = trimmed.find('{');
    let first_bracket = trimmed.find('[');

    let (start_char, end_char, start_pos) = match (first_brace, first_bracket) {
        (Some(b), Some(k)) if b < k => ('{', '}', b),
        (Some(_), Some(k)) => ('[', ']', k),
        (Some(b), None) => ('{', '}', b),
        (None, Some(k)) => ('[', ']', k),
        (None, None) => return Err("No JSON object or array found in output".to_string()),
    };

    // Find matching closing bracket (handle nesting)
    let substr = &trimmed[start_pos..];
    let mut depth = 0;
    let mut end_pos = None;

    for (i, c) in substr.char_indices() {
        if c == start_char {
            depth += 1;
        } else if c == end_char {
            depth -= 1;
            if depth == 0 {
                end_pos = Some(i + 1);
                break;
            }
        }
    }

    if let Some(end) = end_pos {
        let json_str = &substr[..end];
        if let Ok(v) = serde_json::from_str::<Value>(json_str) {
            return Ok(v);
        }
    }

    // Strategy 4b: Greedy fallback — first '{'/[' to last '}'/']'
    // Handles cases where braces inside JSON string values confuse depth tracking.
    if let Some(last) = substr.rfind(end_char) {
        let json_str = &substr[..last + 1];
        if let Ok(v) = serde_json::from_str::<Value>(json_str) {
            return Ok(v);
        }
    }

    // All strategies failed - return original error
    Err(format!(
        "Failed to extract JSON from output. First 200 chars: {}",
        crate::util::truncate_str(trimmed, 200)
    ))
}

/// Convert execution output to TaskResult, parsing as JSON if output format is json.
/// Also validates against schema if declared.
///
/// Empty output with JSON format returns `null` instead of failing.
/// This enables graceful handling of commands that produce no output.
pub async fn make_task_result(
    output: String,
    policy: Option<&crate::ast::OutputPolicy>,
    duration: std::time::Duration,
) -> TaskResult {
    if let Some(policy) = policy {
        if policy.format == OutputFormat::Json {
            // Handle empty output gracefully - return null instead of error
            if output.trim().is_empty() {
                tracing::debug!(
                    target: "nika::output",
                    "Empty output with JSON format, returning null"
                );
                return TaskResult::success(Value::Null, duration);
            }

            // Parse as JSON, handling markdown code blocks
            let json_value = match extract_json_from_output(&output) {
                Ok(v) => v,
                Err(e) => {
                    return TaskResult::failed(
                        format!("NIKA-060: Invalid JSON output: {}", e),
                        duration,
                    );
                }
            };

            // Validate against schema if declared
            if let Some(schema_ref) = &policy.schema {
                if let Err(e) = validate_schema_ref(&json_value, schema_ref).await {
                    return TaskResult::failed(e.to_string(), duration);
                }
            }

            return TaskResult::success(json_value, duration);
        }
    }
    // Auto-parse JSON objects/arrays when no output policy is set.
    // Enables `$task | values` without needing `$task | parse_json | values`.
    // Only when policy is None (no explicit format) — respect explicit Text format.
    if policy.is_none() {
        let trimmed = output.trim_start();
        if trimmed.starts_with('{') || trimmed.starts_with('[') {
            if let Ok(json_value) = serde_json::from_str::<Value>(&output) {
                return TaskResult::success(json_value, duration);
            }
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
        Some(Arc::clone(cached.value()))
    } else {
        None
    };

    let schema = if let Some(s) = schema {
        s
    } else {
        // Cache miss or invalidated: read and parse schema
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

        // Store in cache (evict all if exceeding size cap)
        let schema = Arc::new(schema);
        if SCHEMA_CACHE.len() >= CACHE_MAX_ENTRIES {
            SCHEMA_CACHE.clear();
        }
        SCHEMA_CACHE.insert(Arc::from(schema_path), Arc::clone(&schema));
        schema
    };

    // PERF: Use cached compiled validator (H4 — saves 10-50ms per call)
    let compiled = get_or_compile_validator(&schema).map_err(|_| NikaError::SchemaFailed {
        details: format!("Invalid schema '{}'", schema_path),
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

/// Validate a JSON value against a SchemaRef (inline or file)
pub async fn validate_schema_ref(value: &Value, schema_ref: &SchemaRef) -> Result<(), NikaError> {
    match schema_ref {
        SchemaRef::File(path) => validate_schema(value, path).await,
        SchemaRef::Inline(schema) => validate_inline_schema(value, schema),
    }
}

/// Validate against an inline JSON Schema
pub fn validate_inline_schema(value: &Value, schema: &Value) -> Result<(), NikaError> {
    // PERF: Use cached compiled validator (H4)
    let compiled = get_or_compile_validator(schema)?;

    let errors: Vec<_> = compiled.iter_errors(value).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        let error_msgs: Vec<String> = errors
            .iter()
            .map(|e| format!("- {}: {}", e.instance_path, e))
            .collect();
        Err(NikaError::SchemaFailed {
            details: format!("Output validation failed:\n{}", error_msgs.join("\n")),
        })
    }
}

/// Format validation errors for retry feedback to LLM
pub(crate) fn format_validation_errors(value: &Value, schema: &Value) -> String {
    // PERF: Use cached compiled validator (H4)
    let compiled = match get_or_compile_validator(schema) {
        Ok(c) => c,
        Err(e) => return format!("{e}"),
    };

    let errors: Vec<_> = compiled.iter_errors(value).collect();
    if errors.is_empty() {
        return "No validation errors".to_string();
    }

    errors
        .iter()
        .map(|e| {
            format!(
                "- Path '{}': {} (got: {})",
                e.instance_path,
                e,
                serde_json::to_string(&*e.instance).unwrap_or_default()
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Extract JSON from LLM output - public version for executor retry loop
pub(crate) fn extract_json(output: &str) -> Result<Value, String> {
    extract_json_from_output(output)
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
    async fn make_task_result_validates_json_file_schema() {
        use crate::ast::OutputPolicy;

        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(schema_file, r#"{{"type": "object"}}"#).unwrap();
        let schema_path = schema_file.path().to_string_lossy().to_string();

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: Some(SchemaRef::File(schema_path)),
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
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

    #[tokio::test]
    async fn make_task_result_validates_json_inline_schema() {
        use crate::ast::OutputPolicy;

        let inline_schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: Some(SchemaRef::Inline(inline_schema)),
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
        };

        // Valid JSON with required field
        let result = make_task_result(
            r#"{"name": "test"}"#.to_string(),
            Some(&policy),
            Duration::from_millis(100),
        )
        .await;
        assert!(result.is_success());

        // Invalid JSON missing required field
        let result = make_task_result(
            r#"{"other": "value"}"#.to_string(),
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
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
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
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
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
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
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
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
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
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
        };

        // JSON with various Unicode characters
        let json_str = r#"{"greeting": "你好世界", "emoji": "🚀✨", "japanese": "こんにちは"}"#;

        let result = make_task_result(
            json_str.to_string(),
            Some(&policy),
            Duration::from_millis(50),
        )
        .await;

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
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
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

    // ══════════════════════════════════════════════════════════════
    // EXTRACT_JSON_FROM_OUTPUT TESTS
    // ══════════════════════════════════════════════════════════════

    #[test]
    fn extract_json_direct_parse() {
        let input = r#"{"key": "value"}"#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn extract_json_with_whitespace() {
        let input = r#"
            {"key": "value"}
        "#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn extract_json_from_markdown_json_block() {
        let input = r#"Here's the JSON:

```json
{"name": "Thibaut", "score": 42}
```

Hope this helps!"#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["name"], "Thibaut");
        assert_eq!(result["score"], 42);
    }

    #[test]
    fn extract_json_from_markdown_plain_block() {
        let input = r#"The result:

```
{"items": [1, 2, 3]}
```
"#;
        let result = extract_json_from_output(input).unwrap();
        assert!(result["items"].is_array());
    }

    #[test]
    fn extract_json_from_uppercase_json_fence() {
        let input = "```JSON\n{\"key\": \"value\"}\n```";
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn extract_json_from_windows_line_endings() {
        let input = "```\r\n{\"key\": \"value\"}\r\n```";
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn extract_json_from_fence_with_trailing_space() {
        let input = "``` \n{\"key\": \"value\"}\n```";
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["key"], "value");
    }

    #[test]
    fn extract_json_from_prose_with_braces() {
        let input = r#"I'll generate the fortune for you:

The cosmic reading reveals: {"sign": "scorpio", "lucky_number": 7, "message": "Great things await"}

This is based on ancient wisdom."#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["sign"], "scorpio");
        assert_eq!(result["lucky_number"], 7);
    }

    #[test]
    fn extract_json_array_from_markdown() {
        let input = r#"```json
[{"id": 1}, {"id": 2}, {"id": 3}]
```"#;
        let result = extract_json_from_output(input).unwrap();
        assert!(result.is_array());
        assert_eq!(result.as_array().unwrap().len(), 3);
    }

    #[test]
    fn extract_json_nested_objects() {
        let input = r#"Result: {"outer": {"inner": {"deep": "value"}}}"#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["outer"]["inner"]["deep"], "value");
    }

    #[test]
    fn extract_json_with_escaped_braces_in_strings() {
        let input = r#"{"template": "Use {{variable}} syntax", "count": 1}"#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["template"], "Use {{variable}} syntax");
    }

    #[test]
    fn extract_json_unbalanced_braces_in_string_values() {
        // Edge case: closing brace inside a JSON string value confuses depth tracking.
        // Strategy 4b (greedy fallback) should handle this.
        let input = r#"Here is the result: {"msg": "close brace } here", "ok": true}"#;
        let result = extract_json_from_output(input).unwrap();
        assert_eq!(result["msg"], "close brace } here");
        assert_eq!(result["ok"], true);
    }

    #[test]
    fn extract_json_no_json_found() {
        let input = "This is just plain text without any JSON.";
        let result = extract_json_from_output(input);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("No JSON object or array found"));
    }

    #[tokio::test]
    async fn make_task_result_handles_markdown_wrapped_json() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
        };

        // Simulate LLM output with markdown code block
        let llm_output = r#"Here's your fortune:

```json
{
  "sign": "scorpio",
  "lucky_number": 7,
  "message": "The stars align in your favor"
}
```

Enjoy your reading!"#;

        let result = make_task_result(
            llm_output.to_string(),
            Some(&policy),
            Duration::from_millis(100),
        )
        .await;

        assert!(result.is_success(), "Should parse JSON from markdown block");
        assert_eq!(result.output["sign"], "scorpio");
        assert_eq!(result.output["lucky_number"], 7);
    }

    #[tokio::test]
    async fn make_task_result_empty_output_returns_null() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
        };

        // Empty output with JSON format returns null
        let empty_output = "".to_string();
        let result = make_task_result(empty_output, Some(&policy), std::time::Duration::ZERO).await;

        assert!(result.is_success(), "Empty output should succeed with null");
        assert!(result.output.is_null(), "Empty output should return null");
    }

    #[tokio::test]
    async fn make_task_result_whitespace_output_returns_null() {
        use crate::ast::OutputPolicy;

        let policy = OutputPolicy {
            format: OutputFormat::Json,
            schema: None,
            from_example: None,
            max_retries: None,
            source_structured_spec: None,
        };

        // Whitespace-only output also returns null
        let whitespace_output = "   \n\t  ".to_string();
        let result =
            make_task_result(whitespace_output, Some(&policy), std::time::Duration::ZERO).await;

        assert!(
            result.is_success(),
            "Whitespace-only output should succeed with null"
        );
        assert!(
            result.output.is_null(),
            "Whitespace-only output should return null"
        );
    }

    // ══════════════════════════════════════════════════════════════
    // INLINE SCHEMA VALIDATION TESTS
    // ══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn validate_schema_ref_inline_success() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let value = serde_json::json!({"name": "test"});
        let result = validate_schema_ref(&value, &SchemaRef::Inline(schema)).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn validate_schema_ref_inline_failure() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"]
        });
        let value = serde_json::json!({"other": "field"});
        let result = validate_schema_ref(&value, &SchemaRef::Inline(schema)).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("required") || err.contains("name"),
            "Error should mention missing required field: {}",
            err
        );
    }

    #[test]
    fn format_validation_errors_shows_details() {
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "age": { "type": "integer", "minimum": 0 }
            },
            "required": ["age"]
        });
        let value = serde_json::json!({"age": -5});
        let errors = format_validation_errors(&value, &schema);
        assert!(errors.contains("-5"), "Should show the invalid value");
    }
}
