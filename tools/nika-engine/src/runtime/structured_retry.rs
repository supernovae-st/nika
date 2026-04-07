//! Structured output retry logic — schema validation + retry loop.
//!
//! Extracted from runner.rs for modularity. Contains:
//! - `get_retry_config` — checks if a task qualifies for schema validation retry
//! - `execute_with_retry` — retry loop with schema validation feedback
//! - `build_retry_prompt` — builds feedback prompt for retry
//! - `is_retryable` — determines if an error is transient

use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;

use crate::ast::analyzed::{
    AnalyzedTask, AnalyzedTaskAction, OutputFormat as AnalyzedOutputFormat,
};
use crate::ast::output::OutputPolicy;
use crate::ast::{InferParams, TaskAction};
use crate::binding::ResolvedBindings;
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::store::{RunContext, TaskResult};

use super::executor::TaskExecutor;
use super::output::{extract_json, format_validation_errors};

/// Check if a task qualifies for schema validation retry.
///
/// Returns Some((schema, max_retries, infer_params)) if:
/// - Task action is Infer
/// - Output format is JSON
/// - Output has inline schema
/// - structured.max_retries > 0
pub(crate) fn get_retry_config(task: &AnalyzedTask) -> Option<(Value, u8, InferParams)> {
    // Must be an infer action
    let infer_action = match &task.action {
        AnalyzedTaskAction::Infer(infer) => infer,
        _ => return None,
    };

    // Must have output with JSON format and inline schema
    let output = task.output.as_ref()?;
    if output.format != AnalyzedOutputFormat::Json {
        return None;
    }

    // Must have inline schema
    let schema = output.schema.as_ref()?.clone();

    // max_retries comes from structured output spec, NOT from output policy
    let structured = task.structured.as_ref()?;
    let max_retries = structured.max_retries.unwrap_or(0);
    if max_retries == 0 {
        return None;
    }

    // Build InferParams directly from analyzed types
    let infer_params = InferParams {
        prompt: infer_action.prompt.clone(),
        provider: task.provider.clone(),
        model: task.model.clone(),
        // TODO: resolve templates at runtime
        temperature: infer_action.temperature.as_ref().and_then(|t| t.value()),
        max_tokens: infer_action.max_tokens.as_ref().and_then(|t| t.value()),
        system: infer_action.system.clone(),
        response_format: None,
        extended_thinking: None,
        thinking_budget: None,
        content: infer_action
            .content
            .as_ref()
            .map(|parts| parts.iter().cloned().map(Into::into).collect()),
        guardrails: Vec::new(),
        provider_chain: None,
    };

    Some((schema, max_retries, infer_params))
}

/// Execute an infer task with schema validation and retry loop.
///
/// When LLM output fails schema validation, builds a feedback prompt with:
/// - Original prompt
/// - Schema that must be matched
/// - Previous output
/// - Validation errors
///
/// Retries up to max_retries times before failing.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_with_retry(
    task_id: &Arc<str>,
    original_infer: InferParams,
    schema: &Value,
    max_retries: u8,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    executor: &TaskExecutor,
    event_log: &EventLog,
    start: Instant,
    output_policy: Option<&OutputPolicy>,
    routing: Option<&nika_core::ast::routing::RoutingConfig>,
) -> TaskResult {
    let mut current_infer = original_infer;
    let original_prompt = current_infer.prompt.clone();
    let mut attempts = 0u32;

    // PERF: Compile JSON Schema validator ONCE before the retry loop.
    // SECURITY: Fail-fast if the schema is invalid — don't waste LLM calls.
    let compiled_validator = match jsonschema::validator_for(schema) {
        Ok(v) => v,
        Err(e) => {
            let reason = format!("Invalid JSON Schema: {e}");
            event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(task_id),
                error: reason.clone(),
                error_code: Some("NIKA-300".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            });
            return TaskResult::failed(reason, start.elapsed());
        }
    };

    loop {
        // Check cancellation before each retry attempt (avoids wasting LLM calls)
        if executor.is_cancelled() {
            let reason = "cancelled during structured output retry".to_string();
            event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(task_id),
                error: reason.clone(),
                error_code: Some("NIKA-097".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
            });
            return TaskResult::failed(reason, start.elapsed());
        }
        attempts += 1;

        // Delay between structured output retry attempts to avoid rate limiting
        if attempts > 1 {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }

        // Create action for this attempt
        let action = TaskAction::Infer {
            infer: current_infer.clone(),
        };

        // Execute (with routing fallback if configured)
        let result = executor
            .execute_with_routing(
                task_id,
                &action,
                bindings,
                datastore,
                output_policy,
                routing,
            )
            .await;
        let duration = start.elapsed();

        match result {
            Ok(output) => {
                // Try to extract JSON from output
                let json_value = match extract_json(&output) {
                    Ok(v) => v,
                    Err(e) => {
                        if attempts > u32::from(max_retries) {
                            // Max retries exhausted
                            event_log.emit(EventKind::TaskFailed {
                                task_id: Arc::clone(task_id),
                                error: format!(
                                    "NIKA-060: Invalid JSON after {} attempts: {}",
                                    attempts, e
                                ),
                                duration_ms: duration.as_millis() as u64,
                                error_code: Some("NIKA-060".to_string()),
                            });
                            // Drain orphaned media refs (defense-in-depth)
                            let _ = datastore.take_media(task_id);
                            return TaskResult::failed(
                                format!(
                                    "NIKA-060: Invalid JSON output after {} attempts: {}",
                                    attempts, e
                                ),
                                duration,
                            );
                        }

                        // Build retry prompt with JSON parsing error
                        tracing::debug!(
                            task_id = %task_id,
                            attempt = attempts,
                            "JSON parsing failed, retrying"
                        );
                        current_infer.prompt = build_retry_prompt(
                            &original_prompt,
                            schema,
                            &output,
                            &format!("JSON parsing failed: {}", e),
                        );
                        continue;
                    }
                };

                // Validate against schema (using pre-compiled validator)
                let errors: Vec<_> = compiled_validator.iter_errors(&json_value).collect();
                if errors.is_empty() {
                    // Validation passed — attach media from staging side-channel
                    let media = datastore.take_media(task_id);
                    event_log.emit(EventKind::TaskCompleted {
                        task_id: Arc::clone(task_id),
                        output: Arc::new(json_value.clone()),
                        duration_ms: duration.as_millis() as u64,
                    });
                    return TaskResult::success(json_value, duration).with_media(media);
                }

                // Validation failed
                if attempts > u32::from(max_retries) {
                    let error_feedback = format_validation_errors(&json_value, schema);
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(task_id),
                        error: format!(
                            "Schema validation failed after {} attempts:\n{}",
                            attempts, error_feedback
                        ),
                        duration_ms: duration.as_millis() as u64,
                        error_code: Some("NIKA-061".to_string()),
                    });
                    // Drain orphaned media refs (defense-in-depth)
                    let _ = datastore.take_media(task_id);
                    return TaskResult::failed(
                        format!(
                            "NIKA-061: Schema validation failed after {} attempts:\n{}",
                            attempts, error_feedback
                        ),
                        duration,
                    );
                }

                // Build retry prompt with validation errors
                let error_feedback = format_validation_errors(&json_value, schema);
                tracing::debug!(
                    task_id = %task_id,
                    attempt = attempts,
                    errors = %error_feedback,
                    "Schema validation failed, retrying"
                );
                current_infer.prompt =
                    build_retry_prompt(&original_prompt, schema, &output, &error_feedback);
            }
            Err(e) => {
                // Executor error (not validation error) - don't retry
                // Drain orphaned media refs (defense-in-depth)
                let _ = datastore.take_media(task_id);
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                    error_code: Some(e.code().to_string()),
                });
                return TaskResult::failed(e.to_string(), duration);
            }
        }
    }
}

/// Build a retry prompt with error feedback.
pub(crate) fn build_retry_prompt(
    original_prompt: &str,
    schema: &Value,
    previous_output: &str,
    error_feedback: &str,
) -> String {
    format!(
        r#"{original_prompt}

---
RETRY: Your previous response did not match the required JSON schema.

REQUIRED SCHEMA:
{schema}

YOUR PREVIOUS OUTPUT:
{previous_output}

VALIDATION ERRORS:
{error_feedback}

Please provide a corrected JSON response that strictly matches the schema."#,
        original_prompt = original_prompt,
        schema = serde_json::to_string_pretty(schema).unwrap_or_else(|_| schema.to_string()),
        previous_output = previous_output,
        error_feedback = error_feedback
    )
}

/// Determine if an error is transient and worth retrying.
///
/// Returns true for transient errors (429, 5xx, timeout, connection failures).
/// Returns false for permanent errors (401, 403, 404, validation, DAG, schema,
/// security, command-not-found, permission-denied).
///
/// This is the single source of truth for retryability — used by both the
/// task-level retry loop (runner) and the provider-level retry loop (infer).
pub(crate) fn is_retryable(error: &NikaError) -> bool {
    match error {
        // Provider API errors: only retry transient HTTP failures
        NikaError::ProviderApiError { message } => {
            let m = message.to_lowercase();
            // Permanent: auth failures, invalid model, bad request
            let is_permanent = m.contains("401")
                || m.contains("403")
                || m.contains("404")
                || m.contains("unauthorized")
                || m.contains("forbidden")
                || m.contains("invalid api key")
                || m.contains("invalid_api_key")
                || m.contains("authentication");
            !is_permanent
        }
        // Exec errors: only retry timeouts, not permanent failures
        NikaError::ExecError { reason } => {
            let r = reason.to_lowercase();
            // Permanent: command not found, permission denied, bad cwd
            let is_permanent = r.contains("not found")
                || r.contains("permission denied")
                || r.contains("no such file")
                || r.contains("cannot find")
                || r.contains("unbalanced quotes");
            !is_permanent
        }
        // These are generally transient
        NikaError::FetchError { .. }
        | NikaError::Execution(_)
        | NikaError::McpNotConnected { .. }
        | NikaError::McpToolCallFailed { .. }
        | NikaError::McpTimeout { .. }
        | NikaError::Timeout { .. }
        | NikaError::EndpointConnectionFailed { .. } => true,
        // Everything else is permanent
        _ => false,
    }
}
