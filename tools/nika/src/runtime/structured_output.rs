//! Structured Output Engine
//!
//! 4-layer defense system for ~99.99% JSON Schema compliance:
//!
//! - **Layer 1**: rig Extractor (Rust types with JsonSchema via schemars)
//! - **Layer 2**: Provider-Native (tool_use / response_format)
//! - **Layer 3**: Retry with Feedback (re-prompt with validation errors)
//! - **Layer 4**: LLM Repair (separate call to fix invalid JSON)
//!
//! Each layer emits `StructuredOutputAttempt` events for observability.
//! Success emits `StructuredOutputSuccess` with total attempt count.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::runtime::StructuredOutputEngine;
//! use nika::ast::StructuredOutputSpec;
//!
//! let spec = StructuredOutputSpec::with_file_schema("./schema.json");
//! let engine = StructuredOutputEngine::new(spec, event_log.clone());
//!
//! // Validate raw output (Layer 2 only without callback)
//! let result = engine.validate("task-1", raw_output).await?;
//!
//! // With inference callback for full Layer 3 & 4 support
//! let callback: InferCallback = Arc::new(move |prompt: String| {
//!     let provider = provider.clone();
//!     Box::pin(async move {
//!         provider.infer(&prompt, None).await
//!             .map_err(|e| NikaError::ProviderApiError { message: e.to_string() })
//!     })
//! });
//! let engine = engine.with_infer_callback(callback);
//! ```

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use serde_json::Value;
use tracing::debug;

use crate::ast::output::SchemaRef;
use crate::ast::StructuredOutputSpec;
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};

use super::output::{extract_json, format_validation_errors, validate_schema_ref};

/// Callback type for LLM inference during retry/repair (Layer 3 & 4)
///
/// This callback is invoked when the engine needs to re-call the LLM:
/// - Layer 3: Retry with validation error feedback
/// - Layer 4: Repair call to fix invalid JSON
///
/// The callback receives the prompt and returns the LLM response.
pub type InferCallback = Arc<
    dyn Fn(String) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send>> + Send + Sync,
>;

/// Layer names for event tracking
const LAYER_2_NAME: &str = "provider_native";
const LAYER_3_NAME: &str = "retry_with_feedback";
const LAYER_4_NAME: &str = "llm_repair";

/// Result of structured output validation
#[derive(Debug, Clone)]
pub struct StructuredOutputResult {
    /// The validated JSON value
    pub value: Value,
    /// Which layer succeeded (1-4)
    pub layer: u8,
    /// Layer name
    pub layer_name: String,
    /// Total attempts across all layers
    pub total_attempts: u32,
}

/// 4-layer structured output validation engine
///
/// Attempts validation through multiple layers until success or exhaustion.
/// All attempts are tracked via events for observability.
///
/// ## Layers
///
/// - **Layer 1**: rig Extractor (not implemented - requires compile-time types)
/// - **Layer 2**: Provider-Native - validates JSON extraction from raw output
/// - **Layer 3**: Retry with Feedback - re-calls LLM with validation errors (requires `infer_fn`)
/// - **Layer 4**: LLM Repair - calls repair model to fix invalid JSON (requires `infer_fn`)
///
/// Without `infer_fn`, only Layer 2 is functional. Layers 3 & 4 will emit warnings
/// and gracefully skip to the next layer.
pub struct StructuredOutputEngine {
    /// Structured output specification (schema + layer config)
    spec: StructuredOutputSpec,
    /// Event log for observability
    log: Arc<EventLog>,
    /// Cached compiled schema (for validation speed)
    compiled_schema: Option<Value>,
    /// Callback for LLM inference in Layer 3 & 4
    ///
    /// When set, enables actual LLM retries and repairs instead of just re-validation.
    infer_fn: Option<InferCallback>,
    /// Original prompt for retry context
    ///
    /// Used by Layer 3 to construct the retry prompt with full context.
    original_prompt: Option<String>,
}

impl StructuredOutputEngine {
    /// Create a new engine with the given spec and event log
    pub fn new(spec: StructuredOutputSpec, log: Arc<EventLog>) -> Self {
        Self {
            spec,
            log,
            compiled_schema: None,
            infer_fn: None,
            original_prompt: None,
        }
    }

    /// Set the inference callback for Layer 3 & 4
    ///
    /// This enables actual LLM retries and repairs. Without this callback,
    /// only Layer 2 validation is functional.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let callback: InferCallback = Arc::new(move |prompt: String| {
    ///     let provider = provider.clone();
    ///     Box::pin(async move {
    ///         provider.infer(&prompt, None).await
    ///             .map_err(|e| NikaError::ProviderApiError { message: e.to_string() })
    ///     })
    /// });
    /// let engine = engine.with_infer_callback(callback);
    /// ```
    pub fn with_infer_callback(mut self, callback: InferCallback) -> Self {
        self.infer_fn = Some(callback);
        self
    }

    /// Set the original prompt for retry context
    ///
    /// Used by Layer 3 to construct the retry prompt with full context.
    pub fn with_original_prompt(mut self, prompt: String) -> Self {
        self.original_prompt = Some(prompt);
        self
    }

    /// Load and cache the schema for validation
    pub async fn load_schema(&mut self) -> Result<&Value, NikaError> {
        if self.compiled_schema.is_none() {
            let schema = match &self.spec.schema {
                SchemaRef::Inline(v) => v.clone(),
                SchemaRef::File(path) => {
                    let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                        NikaError::SchemaFailed {
                            details: format!("Failed to read schema '{}': {}", path, e),
                        }
                    })?;
                    serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                        details: format!("Invalid JSON in schema '{}': {}", path, e),
                    })?
                }
            };
            self.compiled_schema = Some(schema);
        }
        self.compiled_schema.as_ref().ok_or_else(|| NikaError::SchemaFailed {
            details: "Schema compilation produced None (internal error)".to_string(),
        })
    }

    /// Get the schema reference
    pub fn schema(&self) -> &SchemaRef {
        &self.spec.schema
    }

    /// Validate raw output through the 4-layer defense system
    ///
    /// Returns the validated JSON value and metadata about which layer succeeded.
    pub async fn validate(
        &mut self,
        task_id: &str,
        raw_output: &str,
    ) -> Result<StructuredOutputResult, NikaError> {
        let task_id: Arc<str> = Arc::from(task_id);
        let mut total_attempts: u32 = 0;

        // Load schema for validation
        let schema = self.load_schema().await?.clone();

        // Layer 1: rig Extractor (skip for now - requires compile-time types)
        // In future: use rig's Extractor with schemars-derived types
        // For now, we rely on Layers 2-4 which work with runtime schemas

        // Layer 2: Provider-Native validation
        // The provider should have already received the schema via tool_use/response_format
        // Here we just validate what the provider returned
        if self.spec.enable_tool_use.unwrap_or(true) {
            total_attempts += 1;
            let layer_result = self
                .try_layer_2(&task_id, raw_output, &schema, total_attempts)
                .await;

            if let Ok(value) = layer_result {
                self.emit_success(&task_id, 2, LAYER_2_NAME, total_attempts);
                return Ok(StructuredOutputResult {
                    value,
                    layer: 2,
                    layer_name: LAYER_2_NAME.to_string(),
                    total_attempts,
                });
            }
        }

        // Layer 3: Retry with Feedback
        if self.spec.enable_retry_or_default() {
            let max_retries = self.spec.max_retries_or_default();
            for retry in 1..=max_retries {
                total_attempts += 1;
                let layer_result = self
                    .try_layer_3(&task_id, raw_output, &schema, retry, total_attempts)
                    .await;

                if let Ok(value) = layer_result {
                    self.emit_success(&task_id, 3, LAYER_3_NAME, total_attempts);
                    return Ok(StructuredOutputResult {
                        value,
                        layer: 3,
                        layer_name: LAYER_3_NAME.to_string(),
                        total_attempts,
                    });
                }
            }
        }

        // Layer 4: LLM Repair
        if self.spec.enable_repair_or_default() {
            total_attempts += 1;
            let layer_result = self
                .try_layer_4(&task_id, raw_output, &schema, total_attempts)
                .await;

            if let Ok(value) = layer_result {
                self.emit_success(&task_id, 4, LAYER_4_NAME, total_attempts);
                return Ok(StructuredOutputResult {
                    value,
                    layer: 4,
                    layer_name: LAYER_4_NAME.to_string(),
                    total_attempts,
                });
            }
        }

        // All layers failed
        let errors = self.collect_validation_errors(raw_output, &schema);
        Err(NikaError::StructuredOutputAllLayersFailed {
            task_id: task_id.to_string(),
            attempts: total_attempts,
            final_errors: errors,
        })
    }

    /// Layer 2: Provider-Native validation
    ///
    /// Extracts JSON from raw output and validates against schema.
    /// The provider should have already been configured with tool_use/response_format.
    async fn try_layer_2(
        &self,
        task_id: &Arc<str>,
        raw_output: &str,
        schema: &Value,
        attempt: u32,
    ) -> Result<Value, NikaError> {
        // Extract JSON from potentially markdown-wrapped output
        let json_value = match extract_json(raw_output) {
            Ok(v) => v,
            Err(e) => {
                self.emit_attempt(task_id, 2, LAYER_2_NAME, attempt, false, Some(e.clone()));
                return Err(NikaError::StructuredOutputExtractionFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_2_NAME.to_string(),
                    reason: e,
                });
            }
        };

        // Validate against schema
        match validate_schema_ref(&json_value, &SchemaRef::Inline(schema.clone())).await {
            Ok(()) => {
                self.emit_attempt(task_id, 2, LAYER_2_NAME, attempt, true, None);
                Ok(json_value)
            }
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    2,
                    LAYER_2_NAME,
                    attempt,
                    false,
                    Some(e.to_string()),
                );
                Err(NikaError::StructuredOutputValidationFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_2_NAME.to_string(),
                    attempt,
                    errors: vec![e.to_string()],
                })
            }
        }
    }

    /// Layer 3: Retry with Feedback
    ///
    /// Re-calls the LLM with validation error feedback to get corrected output.
    /// Requires `infer_fn` callback to be set via `with_infer_callback()`.
    ///
    /// Without `infer_fn`, this layer is skipped with a warning.
    async fn try_layer_3(
        &self,
        task_id: &Arc<str>,
        raw_output: &str,
        schema: &Value,
        retry_num: u8,
        attempt: u32,
    ) -> Result<Value, NikaError> {
        // Check if we have an inference callback
        let infer_fn = match &self.infer_fn {
            Some(f) => f,
            None => {
                // No callback - Layer 3 is disabled
                debug!(
                    task_id = %task_id,
                    retry = retry_num,
                    "Layer 3 skipped: no infer callback configured"
                );
                self.emit_attempt(
                    task_id,
                    3,
                    LAYER_3_NAME,
                    attempt,
                    false,
                    Some(format!(
                        "retry {}: no infer callback - Layer 3 disabled",
                        retry_num
                    )),
                );
                return Err(NikaError::StructuredOutputValidationFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_3_NAME.to_string(),
                    attempt,
                    errors: vec!["Layer 3 requires infer callback".to_string()],
                });
            }
        };

        // Collect validation errors from the raw output
        let validation_errors = self
            .collect_validation_errors(raw_output, schema)
            .join("\n");

        // Generate retry prompt with feedback
        let original_prompt = self.original_prompt.as_deref().unwrap_or("");
        let retry_prompt =
            self.generate_retry_prompt(original_prompt, raw_output, &validation_errors);

        debug!(
            task_id = %task_id,
            retry = retry_num,
            prompt_len = retry_prompt.len(),
            "Layer 3: calling LLM with retry prompt"
        );

        // Actually call the LLM with the retry prompt
        let new_output = match infer_fn(retry_prompt).await {
            Ok(output) => output,
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    3,
                    LAYER_3_NAME,
                    attempt,
                    false,
                    Some(format!("retry {}: LLM call failed: {}", retry_num, e)),
                );
                return Err(e);
            }
        };

        debug!(
            task_id = %task_id,
            retry = retry_num,
            output_len = new_output.len(),
            "Layer 3: received LLM response"
        );

        // Extract JSON from the new output
        let json_value = match extract_json(&new_output) {
            Ok(v) => v,
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    3,
                    LAYER_3_NAME,
                    attempt,
                    false,
                    Some(format!("retry {}: extraction failed: {}", retry_num, e)),
                );
                return Err(NikaError::StructuredOutputExtractionFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_3_NAME.to_string(),
                    reason: e,
                });
            }
        };

        // Validate the new output against schema
        match validate_schema_ref(&json_value, &SchemaRef::Inline(schema.clone())).await {
            Ok(()) => {
                debug!(
                    task_id = %task_id,
                    retry = retry_num,
                    "Layer 3: validation succeeded"
                );
                self.emit_attempt(task_id, 3, LAYER_3_NAME, attempt, true, None);
                Ok(json_value)
            }
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    3,
                    LAYER_3_NAME,
                    attempt,
                    false,
                    Some(format!("retry {}: validation failed: {}", retry_num, e)),
                );
                Err(NikaError::StructuredOutputValidationFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_3_NAME.to_string(),
                    attempt,
                    errors: vec![e.to_string()],
                })
            }
        }
    }

    /// Layer 4: LLM Repair
    ///
    /// Calls a repair LLM to fix invalid JSON.
    /// Requires `infer_fn` callback to be set via `with_infer_callback()`.
    ///
    /// The repair prompt includes the invalid output and schema, asking the LLM
    /// to return only the corrected JSON.
    ///
    /// Without `infer_fn`, this layer is skipped with a warning.
    async fn try_layer_4(
        &self,
        task_id: &Arc<str>,
        raw_output: &str,
        schema: &Value,
        attempt: u32,
    ) -> Result<Value, NikaError> {
        // Check if we have an inference callback
        let infer_fn = match &self.infer_fn {
            Some(f) => f,
            None => {
                // No callback - Layer 4 is disabled
                debug!(
                    task_id = %task_id,
                    "Layer 4 skipped: no infer callback configured"
                );
                self.emit_attempt(
                    task_id,
                    4,
                    LAYER_4_NAME,
                    attempt,
                    false,
                    Some("no infer callback - Layer 4 disabled".to_string()),
                );
                return Err(NikaError::StructuredOutputValidationFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_4_NAME.to_string(),
                    attempt,
                    errors: vec!["Layer 4 requires infer callback".to_string()],
                });
            }
        };

        // Generate repair prompt
        let repair_prompt = self.generate_repair_prompt(raw_output, schema);

        debug!(
            task_id = %task_id,
            prompt_len = repair_prompt.len(),
            "Layer 4: calling repair LLM"
        );

        // Call the LLM to repair the JSON
        let repaired_output = match infer_fn(repair_prompt).await {
            Ok(output) => output,
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    4,
                    LAYER_4_NAME,
                    attempt,
                    false,
                    Some(format!("repair LLM call failed: {}", e)),
                );
                return Err(e);
            }
        };

        debug!(
            task_id = %task_id,
            output_len = repaired_output.len(),
            "Layer 4: received repair LLM response"
        );

        // Extract JSON from the repaired output
        let json_value = match extract_json(&repaired_output) {
            Ok(v) => v,
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    4,
                    LAYER_4_NAME,
                    attempt,
                    false,
                    Some(format!("repair extraction failed: {}", e)),
                );
                return Err(NikaError::StructuredOutputExtractionFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_4_NAME.to_string(),
                    reason: e,
                });
            }
        };

        // Validate the repaired output against schema
        match validate_schema_ref(&json_value, &SchemaRef::Inline(schema.clone())).await {
            Ok(()) => {
                debug!(
                    task_id = %task_id,
                    "Layer 4: repair validation succeeded"
                );
                self.emit_attempt(task_id, 4, LAYER_4_NAME, attempt, true, None);
                Ok(json_value)
            }
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    4,
                    LAYER_4_NAME,
                    attempt,
                    false,
                    Some(format!("repair validation failed: {}", e)),
                );
                Err(NikaError::StructuredOutputValidationFailed {
                    task_id: task_id.to_string(),
                    layer: LAYER_4_NAME.to_string(),
                    attempt,
                    errors: vec![e.to_string()],
                })
            }
        }
    }

    /// Emit a StructuredOutputAttempt event
    fn emit_attempt(
        &self,
        task_id: &Arc<str>,
        layer: u8,
        layer_name: &str,
        attempt: u32,
        success: bool,
        error: Option<String>,
    ) {
        self.log.emit(EventKind::StructuredOutputAttempt {
            task_id: Arc::clone(task_id),
            layer,
            layer_name: layer_name.to_string(),
            attempt,
            success,
            error,
        });
    }

    /// Emit a StructuredOutputSuccess event
    fn emit_success(&self, task_id: &Arc<str>, layer: u8, layer_name: &str, total_attempts: u32) {
        self.log.emit(EventKind::StructuredOutputSuccess {
            task_id: Arc::clone(task_id),
            layer,
            layer_name: layer_name.to_string(),
            total_attempts,
        });
    }

    /// Collect validation errors for the final failure message
    fn collect_validation_errors(&self, raw_output: &str, schema: &Value) -> Vec<String> {
        match extract_json(raw_output) {
            Ok(value) => {
                let errors_str = format_validation_errors(&value, schema);
                errors_str.lines().map(|s| s.to_string()).collect()
            }
            Err(e) => vec![format!("JSON extraction failed: {}", e)],
        }
    }

    /// Generate a retry prompt with validation feedback
    ///
    /// Used by Layer 3 to construct the re-prompt with error context.
    pub fn generate_retry_prompt(
        &self,
        original_prompt: &str,
        invalid_output: &str,
        validation_errors: &str,
    ) -> String {
        format!(
            r#"{original_prompt}

Your previous response was invalid:
```
{invalid_output}
```

Validation errors:
{validation_errors}

Please provide a corrected response that matches the required JSON schema."#
        )
    }

    /// Generate a repair prompt for Layer 4
    ///
    /// Used by the executor to construct the repair LLM call.
    pub fn generate_repair_prompt(&self, invalid_output: &str, schema: &Value) -> String {
        let schema_str =
            serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string());

        format!(
            r#"You are a JSON repair assistant. Fix the following invalid JSON to match the schema.

Invalid JSON:
```
{invalid_output}
```

Required schema:
```json
{schema_str}
```

Respond with ONLY the corrected JSON, no explanation."#
        )
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// STANDALONE VALIDATION FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

/// Quick validation without the full engine (for simple cases)
///
/// Validates output against a schema without retry or repair.
/// Useful for one-shot validation in exec: or fetch: tasks.
pub async fn validate_structured_output(
    task_id: &str,
    output: &str,
    spec: &StructuredOutputSpec,
    log: &EventLog,
) -> Result<Value, NikaError> {
    let task_id: Arc<str> = Arc::from(task_id);

    // Extract JSON
    let json_value = extract_json(output).map_err(|e| {
        log.emit(EventKind::StructuredOutputAttempt {
            task_id: Arc::clone(&task_id),
            layer: 2,
            layer_name: LAYER_2_NAME.to_string(),
            attempt: 1,
            success: false,
            error: Some(e.clone()),
        });
        NikaError::StructuredOutputExtractionFailed {
            task_id: task_id.to_string(),
            layer: LAYER_2_NAME.to_string(),
            reason: e,
        }
    })?;

    // Validate
    validate_schema_ref(&json_value, &spec.schema)
        .await
        .map_err(|e| {
            log.emit(EventKind::StructuredOutputAttempt {
                task_id: Arc::clone(&task_id),
                layer: 2,
                layer_name: LAYER_2_NAME.to_string(),
                attempt: 1,
                success: false,
                error: Some(e.to_string()),
            });
            NikaError::StructuredOutputValidationFailed {
                task_id: task_id.to_string(),
                layer: LAYER_2_NAME.to_string(),
                attempt: 1,
                errors: vec![e.to_string()],
            }
        })?;

    log.emit(EventKind::StructuredOutputSuccess {
        task_id: Arc::clone(&task_id),
        layer: 2,
        layer_name: LAYER_2_NAME.to_string(),
        total_attempts: 1,
    });

    Ok(json_value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_log() -> Arc<EventLog> {
        Arc::new(EventLog::new())
    }

    fn create_user_schema() -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer", "minimum": 0 }
            },
            "required": ["name", "age"]
        })
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // LAYER 2 TESTS (Provider-Native)
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn layer2_valid_json_passes() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let result = engine
            .validate("test-task", r#"{"name": "Alice", "age": 30}"#)
            .await;

        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.layer, 2);
        assert_eq!(r.layer_name, "provider_native");
        assert_eq!(r.value["name"], "Alice");
    }

    #[tokio::test]
    async fn layer2_markdown_wrapped_json_passes() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let output = r#"Here's the result:
```json
{"name": "Bob", "age": 25}
```
Hope this helps!"#;

        let result = engine.validate("test-task", output).await;

        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.value["name"], "Bob");
        assert_eq!(r.value["age"], 25);
    }

    #[tokio::test]
    async fn layer2_invalid_json_fails() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        // Missing required 'age' field
        let result = engine.validate("test-task", r#"{"name": "Charlie"}"#).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(
            err,
            NikaError::StructuredOutputAllLayersFailed { .. }
        ));
    }

    #[tokio::test]
    async fn layer2_malformed_json_fails() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let result = engine.validate("test-task", "not json at all").await;

        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // SCHEMA LOADING TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn load_schema_from_file() {
        let log = create_test_log();

        let mut schema_file = NamedTempFile::new().unwrap();
        writeln!(
            schema_file,
            r#"{{"type": "object", "properties": {{"x": {{"type": "number"}}}}}}"#
        )
        .unwrap();
        let path = schema_file.path().to_string_lossy().to_string();

        let spec = StructuredOutputSpec::with_file_schema(&path);
        let mut engine = StructuredOutputEngine::new(spec, log);

        let schema = engine.load_schema().await.unwrap();
        assert_eq!(schema["type"], "object");
    }

    #[tokio::test]
    async fn load_schema_file_not_found() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_file_schema("/nonexistent/schema.json");
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine.load_schema().await;
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EVENT EMISSION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn events_emitted_on_success() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let _ = engine
            .validate("task-1", r#"{"name": "Test", "age": 20}"#)
            .await;

        let events = log.events();
        assert!(!events.is_empty());

        // Should have attempt + success events
        let has_attempt = events.iter().any(|e| {
            matches!(
                &e.kind,
                EventKind::StructuredOutputAttempt { success: true, .. }
            )
        });
        let has_success = events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::StructuredOutputSuccess { .. }));

        assert!(has_attempt);
        assert!(has_success);
    }

    #[tokio::test]
    async fn events_emitted_on_failure() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let _ = engine.validate("task-2", "invalid").await;

        let events = log.events();
        assert!(!events.is_empty());

        // Should have failed attempt events
        let has_failed_attempt = events.iter().any(|e| {
            matches!(
                &e.kind,
                EventKind::StructuredOutputAttempt { success: false, .. }
            )
        });
        assert!(has_failed_attempt);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // LAYER TOGGLE TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn layers_can_be_disabled() {
        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.enable_retry = Some(false);
        spec.enable_repair = Some(false);

        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        // Invalid JSON should fail fast with only Layer 2 enabled
        let result = engine
            .validate("task-3", r#"{"name": "Only name, no age"}"#)
            .await;

        assert!(result.is_err());

        // Check attempt count - should be just 1 (Layer 2 only)
        let events = log.events();
        let attempt_count = events
            .iter()
            .filter(|e| matches!(&e.kind, EventKind::StructuredOutputAttempt { .. }))
            .count();
        assert_eq!(attempt_count, 1, "Only Layer 2 should have attempted");
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // RETRY PROMPT GENERATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[test]
    fn generate_retry_prompt_includes_context() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let engine = StructuredOutputEngine::new(spec, log);

        let prompt = engine.generate_retry_prompt(
            "Generate a user object",
            r#"{"name": "Test"}"#,
            "missing required field: age",
        );

        assert!(prompt.contains("Generate a user object"));
        assert!(prompt.contains(r#"{"name": "Test"}"#));
        assert!(prompt.contains("missing required field: age"));
    }

    #[test]
    fn generate_repair_prompt_includes_schema() {
        let log = create_test_log();
        let schema = create_user_schema();
        let spec = StructuredOutputSpec::with_inline_schema(schema.clone());
        let engine = StructuredOutputEngine::new(spec, log);

        let prompt = engine.generate_repair_prompt(r#"{"broken": true}"#, &schema);

        assert!(prompt.contains(r#"{"broken": true}"#));
        assert!(prompt.contains("name"));
        assert!(prompt.contains("age"));
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // STANDALONE VALIDATION TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn standalone_validation_works() {
        let log = EventLog::new();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());

        let result = validate_structured_output(
            "task-4",
            r#"{"name": "Standalone", "age": 42}"#,
            &spec,
            &log,
        )
        .await;

        assert!(result.is_ok());
        let value = result.unwrap();
        assert_eq!(value["name"], "Standalone");
    }

    #[tokio::test]
    async fn standalone_validation_fails_on_invalid() {
        let log = EventLog::new();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());

        let result =
            validate_structured_output("task-5", r#"{"invalid": true}"#, &spec, &log).await;

        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // EDGE CASES
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn handles_unicode_content() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine
            .validate("task-unicode", r#"{"name": "日本語テスト", "age": 25}"#)
            .await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap().value["name"], "日本語テスト");
    }

    #[tokio::test]
    async fn handles_nested_objects() {
        let log = create_test_log();
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" }
                    },
                    "required": ["name"]
                }
            },
            "required": ["user"]
        });
        let spec = StructuredOutputSpec::with_inline_schema(schema);
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine
            .validate("task-nested", r#"{"user": {"name": "Nested User"}}"#)
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn handles_arrays() {
        let log = create_test_log();
        let schema = serde_json::json!({
            "type": "array",
            "items": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer" }
                },
                "required": ["id"]
            }
        });
        let spec = StructuredOutputSpec::with_inline_schema(schema);
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine
            .validate("task-array", r#"[{"id": 1}, {"id": 2}, {"id": 3}]"#)
            .await;

        assert!(result.is_ok());
        let arr = result.unwrap().value;
        assert!(arr.is_array());
        assert_eq!(arr.as_array().unwrap().len(), 3);
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // LAYER 3 TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    use std::sync::atomic::{AtomicU32, Ordering};

    #[tokio::test]
    async fn layer3_actually_retries_llm() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Mock callback that returns valid JSON on second call
        let callback: InferCallback = Arc::new(move |_prompt: String| {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let n = count.fetch_add(1, Ordering::SeqCst);
                if n == 0 {
                    // First call from Layer 3 retry: return valid JSON
                    Ok(r#"{"name": "Alice", "age": 30}"#.to_string())
                } else {
                    // Shouldn't be called more than once if first retry succeeds
                    Ok(r#"{"name": "Bob", "age": 25}"#.to_string())
                }
            })
        });

        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.enable_retry = Some(true);
        spec.max_retries = Some(3);
        spec.enable_repair = Some(false); // Disable Layer 4

        let mut engine = StructuredOutputEngine::new(spec, log.clone())
            .with_infer_callback(callback)
            .with_original_prompt("Generate a user object".to_string());

        // Invalid JSON should trigger Layer 3 retry
        let result = engine.validate("test-task", r#"{"invalid": true}"#).await;

        assert!(result.is_ok(), "Should succeed after Layer 3 retry");
        let r = result.unwrap();
        assert_eq!(r.layer, 3, "Should succeed at Layer 3");
        assert_eq!(r.layer_name, "retry_with_feedback");
        assert_eq!(r.value["name"], "Alice");
        assert!(
            call_count.load(Ordering::SeqCst) >= 1,
            "Should have called LLM at least once"
        );
    }

    #[tokio::test]
    async fn layer3_skipped_without_callback() {
        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.enable_retry = Some(true);
        spec.max_retries = Some(3);
        spec.enable_repair = Some(false);

        // No callback - Layer 3 should be skipped
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let result = engine.validate("test-task", r#"{"invalid": true}"#).await;

        assert!(result.is_err(), "Should fail without callback");

        // Check that Layer 3 attempts were made but failed due to no callback
        let events = log.events();
        let layer3_attempts = events.iter().filter(|e| {
            matches!(
                &e.kind,
                EventKind::StructuredOutputAttempt {
                    layer: 3,
                    success: false,
                    error: Some(err),
                    ..
                } if err.contains("no infer callback")
            )
        });
        assert!(
            layer3_attempts.count() > 0,
            "Should have Layer 3 attempt events showing no callback"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // LAYER 4 TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn layer4_actually_repairs_json() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Mock callback that returns repaired JSON
        let callback: InferCallback = Arc::new(move |prompt: String| {
            let count = call_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                // Verify we received a repair prompt
                assert!(
                    prompt.contains("repair") || prompt.contains("schema"),
                    "Should receive repair prompt"
                );
                // Return valid JSON
                Ok(r#"{"name": "Repaired", "age": 25}"#.to_string())
            })
        });

        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.enable_retry = Some(false); // Skip Layer 3
        spec.enable_repair = Some(true);

        let mut engine =
            StructuredOutputEngine::new(spec, log.clone()).with_infer_callback(callback);

        let result = engine.validate("test-task", "totally broken json").await;

        assert!(result.is_ok(), "Should succeed after Layer 4 repair");
        let r = result.unwrap();
        assert_eq!(r.layer, 4, "Should succeed at Layer 4");
        assert_eq!(r.layer_name, "llm_repair");
        assert_eq!(r.value["name"], "Repaired");
        assert!(
            call_count.load(Ordering::SeqCst) >= 1,
            "Should have called repair LLM"
        );
    }

    #[tokio::test]
    async fn layer4_skipped_without_callback() {
        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.enable_retry = Some(false);
        spec.enable_repair = Some(true);

        // No callback - Layer 4 should be skipped
        let mut engine = StructuredOutputEngine::new(spec, log.clone());

        let result = engine.validate("test-task", "broken json").await;

        assert!(result.is_err(), "Should fail without callback");

        // Check that Layer 4 attempt was made but failed due to no callback
        let events = log.events();
        let layer4_attempts = events.iter().filter(|e| {
            matches!(
                &e.kind,
                EventKind::StructuredOutputAttempt {
                    layer: 4,
                    success: false,
                    error: Some(err),
                    ..
                } if err.contains("no infer callback")
            )
        });
        assert!(
            layer4_attempts.count() > 0,
            "Should have Layer 4 attempt event showing no callback"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MAX_RETRIES TESTS
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn max_retries_is_respected() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Mock callback that always returns invalid JSON
        let callback: InferCallback = Arc::new(move |_prompt: String| {
            let count = call_count_clone.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                // Always return invalid JSON (missing age)
                Ok(r#"{"still_invalid": true}"#.to_string())
            })
        });

        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.max_retries = Some(3);
        spec.enable_retry = Some(true);
        spec.enable_repair = Some(false); // Skip Layer 4

        let mut engine =
            StructuredOutputEngine::new(spec, log.clone()).with_infer_callback(callback);

        let result = engine.validate("test-task", r#"{"invalid": true}"#).await;

        assert!(result.is_err(), "Should fail after max retries");
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            3,
            "Should have retried exactly max_retries times"
        );
    }

    #[tokio::test]
    async fn layer3_layer4_chain_works() {
        let call_count = Arc::new(AtomicU32::new(0));
        let call_count_clone = call_count.clone();

        // Mock callback:
        // - Layer 3 retries all fail (return invalid JSON)
        // - Layer 4 repair succeeds (return valid JSON)
        // Note: Detect Layer 4 by "JSON repair assistant" which is unique to repair prompt
        let callback: InferCallback = Arc::new(move |prompt: String| {
            let count = call_count_clone.clone();
            Box::pin(async move {
                let n = count.fetch_add(1, Ordering::SeqCst);
                if prompt.contains("JSON repair assistant") {
                    // Layer 4 repair call - succeed
                    Ok(r#"{"name": "Repaired", "age": 42}"#.to_string())
                } else {
                    // Layer 3 retry calls - always fail
                    Ok(format!(
                        r#"{{"retry_attempt": {}, "still_invalid": true}}"#,
                        n
                    ))
                }
            })
        });

        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.max_retries = Some(2);
        spec.enable_retry = Some(true);
        spec.enable_repair = Some(true);

        let mut engine = StructuredOutputEngine::new(spec, log.clone())
            .with_infer_callback(callback)
            .with_original_prompt("Generate user".to_string());

        let result = engine.validate("test-task", r#"{"invalid": true}"#).await;

        assert!(result.is_ok(), "Should succeed after Layer 4 repair");
        let r = result.unwrap();
        assert_eq!(r.layer, 4, "Should succeed at Layer 4");
        assert_eq!(r.value["name"], "Repaired");
        // Should have: 2 Layer 3 retries + 1 Layer 4 repair = 3 calls
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            3,
            "Should have made 2 retry calls + 1 repair call"
        );
    }

    #[tokio::test]
    async fn original_prompt_included_in_retry() {
        let captured_prompt = Arc::new(std::sync::Mutex::new(String::new()));
        let captured_prompt_clone = captured_prompt.clone();

        let callback: InferCallback = Arc::new(move |prompt: String| {
            let captured = captured_prompt_clone.clone();
            Box::pin(async move {
                *captured.lock().unwrap() = prompt.clone();
                // Return valid JSON
                Ok(r#"{"name": "Test", "age": 30}"#.to_string())
            })
        });

        let log = create_test_log();
        let mut spec = StructuredOutputSpec::with_inline_schema(create_user_schema());
        spec.enable_retry = Some(true);
        spec.max_retries = Some(1);
        spec.enable_repair = Some(false);

        let mut engine = StructuredOutputEngine::new(spec, log.clone())
            .with_infer_callback(callback)
            .with_original_prompt("Generate a user object for testing".to_string());

        let _ = engine.validate("test-task", r#"{"invalid": true}"#).await;

        let prompt = captured_prompt.lock().unwrap().clone();
        assert!(
            prompt.contains("Generate a user object for testing"),
            "Retry prompt should include original prompt"
        );
        assert!(
            prompt.contains("invalid"),
            "Retry prompt should include the invalid output"
        );
    }
}
