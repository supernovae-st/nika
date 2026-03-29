//! Structured Output Engine
//!
//! 5-layer defense system for ~99.99% JSON Schema compliance:
//!
//! - **Layer 0**: Tool Injection (DynamicSubmitTool via `submit_tool` module)
//!   - Handled OUTSIDE the engine, in `executor/verbs.rs` (infer) and
//!     `rig_agent_loop` (agent). Uses `tool_choice: Required` to force
//!     provider-side schema enforcement.
//! - **Layer 1**: rig Extractor (Rust types with JsonSchema via schemars — future)
//! - **Layer 2**: Extract + Validate (extract JSON from output, validate against schema)
//! - **Layer 3**: Retry with Feedback (re-prompt with validation errors)
//! - **Layer 4**: LLM Repair (separate call to fix invalid JSON)
//!
//! Layer 0 is non-blocking: if tool injection fails (native provider, timeout),
//! execution falls through to streaming + Layers 2-4.
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
use std::time::Instant;

use serde_json::Value;
use tracing::debug;

use crate::ast::output::SchemaRef;
use crate::ast::StructuredOutputSpec;
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};

use super::output::{extract_json, format_validation_errors, validate_schema_ref};

/// Callback type for LLM inference during retry/repair (Layers 3 & 4)
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
const LAYER_2_NAME: &str = "extract_validate";
const LAYER_3_NAME: &str = "retry_with_feedback";
const LAYER_4_NAME: &str = "llm_repair";

/// Estimate token count from character length (chars / 4 heuristic)
fn estimate_tokens(char_len: usize) -> u64 {
    char_len.div_ceil(4) as u64
}

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

/// Post-processing structured output validation engine (Layers 2-4)
///
/// Attempts validation through multiple layers until success or exhaustion.
/// All attempts are tracked via events for observability.
///
/// Layer 0 (DynamicSubmitTool injection) is handled externally in `executor/verbs.rs`
/// and `rig_agent_loop/mod.rs` BEFORE this engine is invoked.
///
/// ## Layers (this engine)
///
/// - **Layer 2**: Extract + Validate - extracts JSON from raw output and validates against schema
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
    /// Cached compiled schema (for validation speed, Arc for cheap cloning)
    compiled_schema: Option<Arc<Value>>,
    /// Cached example value when `from_example` is a file.
    ///
    /// Set during `load_schema()` so that `build_json_schema_instruction` can
    /// inject the file-based example into the LLM prompt without async I/O.
    cached_example: Option<Value>,
    /// Callback for LLM inference in Layer 3 (retry) and Layer 4 (repair fallback)
    ///
    /// When set, enables actual LLM retries and repairs instead of just re-validation.
    infer_fn: Option<InferCallback>,
    /// Optional separate callback for Layer 4 repair with a different model.
    ///
    /// When set via `with_repair_callback()`, Layer 4 uses this instead of `infer_fn`.
    /// This supports `repair_model:` which allows a cheaper model for JSON repair.
    repair_fn: Option<InferCallback>,
    /// Original prompt for retry context
    ///
    /// Used by Layer 3 to construct the retry prompt with full context.
    original_prompt: Option<String>,
    /// Provider name for telemetry (e.g., "anthropic")
    provider_name: Option<String>,
    /// Model name for telemetry (e.g., "claude-3-haiku-20240307")
    model_name: Option<String>,
}

impl StructuredOutputEngine {
    /// Create a new engine with the given spec and event log
    pub fn new(spec: StructuredOutputSpec, log: Arc<EventLog>) -> Self {
        Self {
            spec,
            log,
            compiled_schema: None,
            cached_example: None,
            infer_fn: None,
            repair_fn: None,
            original_prompt: None,
            provider_name: None,
            model_name: None,
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

    /// Set a separate repair callback for Layer 4 (LLM repair).
    ///
    /// When set, Layer 4 uses this callback (with `repair_model`) instead
    /// of the main `infer_fn`. Supports `structured: { repair_model: ... }`.
    pub fn with_repair_callback(mut self, callback: InferCallback) -> Self {
        self.repair_fn = Some(callback);
        self
    }

    /// Set the original prompt for retry context
    ///
    /// Used by Layer 3 to construct the retry prompt with full context.
    pub fn with_original_prompt(mut self, prompt: String) -> Self {
        self.original_prompt = Some(prompt);
        self
    }

    /// Set provider and model names for telemetry on Layer 3/4 LLM calls
    pub fn with_provider_context(mut self, provider: String, model: String) -> Self {
        self.provider_name = Some(provider);
        self.model_name = Some(model);
        self
    }

    /// Estimate cost using provider/model context (returns 0.0 if unknown)
    fn estimate_cost(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let provider = self.provider_name.as_deref().unwrap_or("");
        let model = self.model_name.as_deref().unwrap_or("");
        crate::provider::cost::ProviderKind::parse(provider)
            .map(|pk| crate::provider::cost::calculate_cost(pk, model, input_tokens, output_tokens))
            .unwrap_or(0.0)
    }

    /// Load and cache the schema for validation.
    /// Returns an `Arc<Value>` for cheap cloning across async boundaries.
    pub async fn load_schema(&mut self) -> Result<Arc<Value>, NikaError> {
        if self.compiled_schema.is_none() {
            let schema = if let Some(ref example_ref) = self.spec.from_example {
                // from_example: load example then derive JSON Schema from it
                let example_value = match example_ref {
                    SchemaRef::Inline(v) => v.clone(),
                    SchemaRef::File(path) => {
                        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                            NikaError::SchemaFailed {
                                details: format!("Failed to read example '{}': {}", path, e),
                            }
                        })?;
                        let parsed: Value = serde_json::from_str(&content).map_err(|e| {
                            NikaError::SchemaFailed {
                                details: format!("Invalid JSON in example '{}': {}", path, e),
                            }
                        })?;
                        // Cache example for prompt injection
                        self.cached_example = Some(parsed.clone());
                        parsed
                    }
                };
                if self.spec.strict == Some(true) {
                    crate::ast::structured::json_to_schema_strict(&example_value)
                } else {
                    crate::ast::structured::json_to_schema(&example_value)
                }
            } else {
                // Standard: load schema directly
                match self.spec.schema.as_ref() {
                    Some(SchemaRef::Inline(v)) => v.clone(),
                    Some(SchemaRef::File(path)) => {
                        let content = tokio::fs::read_to_string(path).await.map_err(|e| {
                            NikaError::SchemaFailed {
                                details: format!("Failed to read schema '{}': {}", path, e),
                            }
                        })?;
                        serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                            details: format!("Invalid JSON in schema '{}': {}", path, e),
                        })?
                    }
                    None => {
                        return Err(NikaError::SchemaFailed {
                            details: "No schema or from_example defined".to_string(),
                        });
                    }
                }
            };
            self.compiled_schema = Some(Arc::new(schema));
        }
        self.compiled_schema
            .clone()
            .ok_or_else(|| NikaError::SchemaFailed {
                details: "Schema compilation produced None (internal error)".to_string(),
            })
    }

    /// Get the raw schema reference from the spec.
    ///
    /// Returns `None` when `from_example` is set (schema derived at runtime).
    /// Use `load_schema()` to get the resolved, effective schema.
    pub fn schema(&self) -> Option<&SchemaRef> {
        self.spec.schema.as_ref()
    }

    /// Get the cached example value (set during `load_schema()` for file-based examples).
    ///
    /// Returns `Some` after `load_schema()` has been called when `from_example` is a file.
    /// For inline examples, returns `None` (the value is in `spec.from_example` directly).
    pub fn cached_example(&self) -> Option<&Value> {
        self.cached_example.as_ref()
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

        // Load schema for validation (Arc clone is cheap)
        let schema = self.load_schema().await?;

        // Layer 1: rig Extractor (not yet implemented — requires compile-time types)
        // Log a warning if explicitly enabled so users know it's not active
        if self.spec.enable_extractor == Some(true) {
            tracing::warn!(
                task = %task_id,
                "structured: enable_extractor is not yet implemented (requires compile-time schema types). \
                 Falling through to Layer 2 (runtime JSON validation)."
            );
        }

        // Layer 2: Extract + Validate
        // Extract JSON from the raw output and validate against the schema.
        // This always runs — it's the core post-processing validation step.
        {
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
        // Track the latest LLM output so each retry sees the most recent attempt,
        // not the stale original. try_layer_3 returns (Result, Option<latest_output>).
        let mut current_output = raw_output.to_string();
        if self.spec.enable_retry_or_default() {
            let max_retries = self.spec.max_retries_or_default();
            for retry in 1..=max_retries {
                total_attempts += 1;
                let (layer_result, llm_output) = self
                    .try_layer_3(&task_id, &current_output, &schema, retry, total_attempts)
                    .await;

                // Update current_output with LLM's latest response for next retry
                if let Some(output) = llm_output {
                    current_output = output;
                }

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

        // Layer 4: LLM Repair — uses latest output (from Layer 3 retries if any)
        if self.spec.enable_repair_or_default() {
            total_attempts += 1;
            let layer_result = self
                .try_layer_4(&task_id, &current_output, &schema, total_attempts)
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

        // All layers failed — use latest output for error reporting
        let errors = self.collect_validation_errors(&current_output, &schema);
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
    /// Returns `(Result<Value>, Option<raw_llm_output>)` so the caller can
    /// track the latest LLM response for subsequent retries.
    async fn try_layer_3(
        &self,
        task_id: &Arc<str>,
        raw_output: &str,
        schema: &Value,
        retry_num: u8,
        attempt: u32,
    ) -> (Result<Value, NikaError>, Option<String>) {
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
                return (
                    Err(NikaError::StructuredOutputValidationFailed {
                        task_id: task_id.to_string(),
                        layer: LAYER_3_NAME.to_string(),
                        attempt,
                        errors: vec!["Layer 3 requires infer callback".to_string()],
                    }),
                    None,
                );
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

        let prompt_len = retry_prompt.len();

        debug!(
            task_id = %task_id,
            retry = retry_num,
            prompt_len,
            "Layer 3: calling LLM with retry prompt"
        );

        // EMIT: ProviderCalled before the LLM retry call
        self.log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: self
                .provider_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            model: self
                .model_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            prompt_len,
            endpoint_url: None,
        });

        // Actually call the LLM with the retry prompt
        let infer_start = Instant::now();
        let new_output = match infer_fn(retry_prompt).await {
            Ok(output) => {
                let elapsed = infer_start.elapsed();
                let in_tok = estimate_tokens(prompt_len);
                let out_tok = estimate_tokens(output.len());
                let cost = self.estimate_cost(in_tok, out_tok);
                // EMIT: ProviderResponded after successful LLM retry call
                self.log.emit(EventKind::ProviderResponded {
                    task_id: Arc::clone(task_id),
                    request_id: None,
                    input_tokens: in_tok,
                    output_tokens: out_tok,
                    cache_read_tokens: 0,
                    ttft_ms: Some(elapsed.as_millis() as u64),
                    finish_reason: nika_event::FinishReason::StructuredOutputRetry,
                    cost_usd: cost,
                });
                output
            }
            Err(e) => {
                self.emit_attempt(
                    task_id,
                    3,
                    LAYER_3_NAME,
                    attempt,
                    false,
                    Some(format!("retry {}: LLM call failed: {}", retry_num, e)),
                );
                return (Err(e), None);
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
                return (
                    Err(NikaError::StructuredOutputExtractionFailed {
                        task_id: task_id.to_string(),
                        layer: LAYER_3_NAME.to_string(),
                        reason: e,
                    }),
                    Some(new_output),
                );
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
                (Ok(json_value), Some(new_output))
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
                (
                    Err(NikaError::StructuredOutputValidationFailed {
                        task_id: task_id.to_string(),
                        layer: LAYER_3_NAME.to_string(),
                        attempt,
                        errors: vec![e.to_string()],
                    }),
                    Some(new_output),
                )
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
        // Use repair_fn (repair_model) if set, otherwise fall back to infer_fn
        let infer_fn = match self.repair_fn.as_ref().or(self.infer_fn.as_ref()) {
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
        let prompt_len = repair_prompt.len();

        debug!(
            task_id = %task_id,
            prompt_len,
            "Layer 4: calling repair LLM"
        );

        // EMIT: ProviderCalled before the LLM repair call
        self.log.emit(EventKind::ProviderCalled {
            task_id: Arc::clone(task_id),
            provider: self
                .provider_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            model: self
                .model_name
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            prompt_len,
            endpoint_url: None,
        });

        // Call the LLM to repair the JSON
        let infer_start = Instant::now();
        let repaired_output = match infer_fn(repair_prompt).await {
            Ok(output) => {
                let elapsed = infer_start.elapsed();
                let in_tok = estimate_tokens(prompt_len);
                let out_tok = estimate_tokens(output.len());
                let cost = self.estimate_cost(in_tok, out_tok);
                // EMIT: ProviderResponded after successful LLM repair call
                self.log.emit(EventKind::ProviderResponded {
                    task_id: Arc::clone(task_id),
                    request_id: None,
                    input_tokens: in_tok,
                    output_tokens: out_tok,
                    cache_read_tokens: 0,
                    ttft_ms: Some(elapsed.as_millis() as u64),
                    finish_reason: nika_event::FinishReason::StructuredOutputRepair,
                    cost_usd: cost,
                });
                output
            }
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
    /// Includes the schema so the LLM knows what to produce (especially
    /// important when Layer 0 tool injection was used and the schema
    /// was not in the original prompt text).
    pub fn generate_retry_prompt(
        &self,
        original_prompt: &str,
        invalid_output: &str,
        validation_errors: &str,
    ) -> String {
        let schema_value = self.spec.schema.as_ref().and_then(|s| match s {
            nika_core::ast::SchemaRef::Inline(v) => Some(v),
            nika_core::ast::SchemaRef::File(_) => None,
        });
        let schema_str = schema_value
            .and_then(|v| serde_json::to_string_pretty(v).ok())
            .unwrap_or_default();

        format!(
            r#"{original_prompt}

Your previous response was invalid:
```
{invalid_output}
```

Validation errors:
{validation_errors}

Required JSON schema:
```json
{schema_str}
```

Please provide a corrected JSON response that strictly matches the schema above."#
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
///
/// Correctly handles `from_example`: derives the JSON Schema at call time.
/// For inline examples this is synchronous; for file-based examples it reads the file.
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

    // Resolve the effective schema — honours from_example (derives schema at runtime)
    let effective_schema = if let Some(ref example_ref) = spec.from_example {
        let example_value = match example_ref {
            SchemaRef::Inline(v) => v.clone(),
            SchemaRef::File(path) => {
                let content =
                    tokio::fs::read_to_string(path)
                        .await
                        .map_err(|e| NikaError::SchemaFailed {
                            details: format!("Failed to read example '{}': {}", path, e),
                        })?;
                serde_json::from_str(&content).map_err(|e| NikaError::SchemaFailed {
                    details: format!("Invalid JSON in example '{}': {}", path, e),
                })?
            }
        };
        if spec.strict == Some(true) {
            SchemaRef::Inline(crate::ast::structured::json_to_schema_strict(
                &example_value,
            ))
        } else {
            SchemaRef::Inline(crate::ast::structured::json_to_schema(&example_value))
        }
    } else {
        match spec.schema.clone() {
            Some(schema) => schema,
            None => {
                return Err(NikaError::SchemaFailed {
                    details: "No schema or from_example defined".to_string(),
                });
            }
        }
    };

    // Validate
    validate_schema_ref(&json_value, &effective_schema)
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

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let r = result.unwrap();
        assert_eq!(r.layer, 2);
        assert_eq!(r.layer_name, "extract_validate");
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

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
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

        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
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

        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
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
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // FROM_EXAMPLE TESTS (engine layer)
    // ═══════════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn load_schema_from_example_inline() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_example_inline(serde_json::json!({
            "name": "alice",
            "score": 42
        }));
        let mut engine = StructuredOutputEngine::new(spec, log);
        let schema = engine.load_schema().await.unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["properties"]["score"]["type"], "integer");
    }

    #[tokio::test]
    async fn load_schema_from_example_file() {
        let mut example_file = NamedTempFile::new().unwrap();
        writeln!(example_file, r#"{{"title":"hello","count":1}}"#).unwrap();
        let path = example_file.path().to_string_lossy().to_string();

        let spec = StructuredOutputSpec::with_example_file(&path);
        let mut engine = StructuredOutputEngine::new(spec, create_test_log());
        let schema = engine.load_schema().await.unwrap();
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["title"]["type"], "string");
        assert_eq!(schema["properties"]["count"]["type"], "integer");
    }

    #[tokio::test]
    async fn load_schema_from_example_file_not_found() {
        let spec = StructuredOutputSpec::with_example_file("/nonexistent/example.json");
        let mut engine = StructuredOutputEngine::new(spec, create_test_log());
        let result = engine.load_schema().await;
        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to read example"), "got: {err}");
    }

    #[tokio::test]
    async fn validate_with_example_inline_passes_valid_json() {
        let spec = StructuredOutputSpec::with_example_inline(serde_json::json!({
            "name": "x",
            "score": 0
        }));
        let mut engine = StructuredOutputEngine::new(spec, create_test_log());
        let result = engine
            .validate("t1", r#"{"name": "bob", "score": 99}"#)
            .await;
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
    }

    #[tokio::test]
    async fn validate_with_example_inline_fails_wrong_type() {
        let spec = StructuredOutputSpec::with_example_inline(serde_json::json!({
            "name": "x",
            "score": 0
        }));
        // score should be integer — send string instead
        let mut engine = StructuredOutputEngine::new(spec, create_test_log());
        let result = engine
            .validate("t2", r#"{"name": "bob", "score": "not-a-number"}"#)
            .await;
        assert!(result.is_err(), "expected validation failure on wrong type");
    }

    #[tokio::test]
    async fn validate_structured_output_from_example_inline_passes() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_example_inline(serde_json::json!({
            "name": "x",
            "score": 0
        }));
        let result =
            validate_structured_output("t3", r#"{"name":"alice","score":42}"#, &spec, &log).await;
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
    }

    #[tokio::test]
    async fn validate_structured_output_from_example_inline_rejects_invalid() {
        let log = create_test_log();
        let spec = StructuredOutputSpec::with_example_inline(serde_json::json!({
            "name": "x",
            "score": 0
        }));
        // This used to silently pass with spec.schema ({}) — now it must fail
        let result =
            validate_structured_output("t4", r#"{"anything":"goes","random":true}"#, &spec, &log)
                .await;
        assert!(
            result.is_err(),
            "validate_structured_output must reject missing required fields"
        );
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

        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());

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

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
        let value = result.unwrap();
        assert_eq!(value["name"], "Standalone");
    }

    #[tokio::test]
    async fn standalone_validation_fails_on_invalid() {
        let log = EventLog::new();
        let spec = StructuredOutputSpec::with_inline_schema(create_user_schema());

        let result =
            validate_structured_output("task-5", r#"{"invalid": true}"#, &spec, &log).await;

        assert!(result.is_err(), "Should fail but got: {:?}", result.ok());
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

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
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

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
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

        assert!(result.is_ok(), "Should succeed: {:?}", result.err());
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
