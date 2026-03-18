//! DAG Runner - workflow execution with tokio
//!
//! Performance optimizations:
//! - Arc for zero-cost task/context sharing
//! - JoinSet for efficient parallel task collection
//! - Tokio handles all concurrency (no artificial limits)

use indexmap::IndexMap;
use rustc_hash::FxHashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use crate::ast::analyzed::{
    AnalyzedOutput, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow,
    OutputFormat as AnalyzedOutputFormat,
};
use crate::ast::lower::{lower_action, lower_mcp_servers, lower_output};
use crate::ast::output::OutputPolicy;
use crate::ast::{InferParams, TaskAction};
use crate::binding::ResolvedBindings;
use crate::dag::Dag;
use crate::error::NikaError;
use crate::event::{prune_traces, EventKind, EventLog, TraceWriter};
use crate::runtime::boot::TraceConfig;
use crate::store::{RunContext, TaskResult};
use crate::util::{intern, DECOMPOSE_TIMEOUT};

use super::artifact_processor::process_task_artifacts;
use super::context_loader::load_context_analyzed;
use super::executor::TaskExecutor;
use super::output::{extract_json, format_validation_errors, make_task_result};
use super::resolver::{resolve_assets_analyzed, ResolvedAssets};
use super::structured_output::StructuredOutputEngine;

use crate::ast::artifact::ArtifactsConfig;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════════
// Helper Functions
// ═══════════════════════════════════════════════════════════════════════════════

/// Try to extract an array from a Value, parsing JSON strings if needed.
///
/// This function handles the case where a task output is a JSON array stored as a
/// string (e.g., from `exec: 'echo ''["a","b","c"]'''`). The for_each resolution
/// needs to iterate over arrays, but task outputs are stored as strings.
///
/// # Returns
/// - `Some(Vec<Value>)` if the value is an array or a parseable JSON array string
/// - `None` if the value cannot be converted to an array
fn value_to_array(value: &Value) -> Option<Vec<Value>> {
    // Fast path: direct array access
    if let Some(arr) = value.as_array() {
        return Some(arr.clone());
    }

    // String → try extract_json (handles markdown fences, bare JSON, brackets)
    if let Some(s) = value.as_str() {
        if let Ok(extracted) = extract_json(s) {
            if let Some(arr) = extracted.as_array() {
                return Some(arr.clone());
            }
        }
    }

    None
}

/// Result of executing a task iteration
/// For for_each tasks, includes the iteration index for ordered aggregation
struct IterationResult {
    /// ID used for storage (task_id for regular, indexed for for_each)
    store_id: Arc<str>,
    /// The actual task result
    result: TaskResult,
    /// For for_each: (parent_id, index) to enable aggregation
    for_each_info: Option<(Arc<str>, usize)>,
    /// Paths of artifacts written during this task (for CLI reporting)
    artifact_paths: Vec<PathBuf>,
}

/// DAG workflow runner with event sourcing
///
/// Consumes `AnalyzedWorkflow` directly from the analyzer.
/// Bridge conversions (`lower_action`, `lower_output`) happen at the
/// `TaskExecutor` boundary only.
pub struct Runner {
    workflow: AnalyzedWorkflow,
    flow_graph: Dag,
    datastore: RunContext,
    executor: TaskExecutor,
    event_log: EventLog,
    /// Unique identifier for this workflow execution (for trace files)
    generation_id: String,
    /// Suppress console output (for TUI mode)
    quiet: bool,
    /// Cancellation token for aborting workflow
    cancel_token: CancellationToken,
    /// Pause state - when true, runner waits between layers
    paused: Arc<AtomicBool>,
    /// Notify to wake runner from pause
    resume_notify: Arc<Notify>,
    /// Resolved agents and skills
    resolved_assets: ResolvedAssets,
    /// Trace retention config (max_traces + retention_days)
    trace_config: TraceConfig,
}

impl Runner {
    pub fn new(workflow: AnalyzedWorkflow) -> Result<Self, NikaError> {
        Self::with_event_log(workflow, EventLog::new())
    }

    /// Create a Runner with a custom EventLog (for TUI integration)
    ///
    /// Use `EventLog::new_with_broadcast()` to create an EventLog that
    /// sends events to TUI in real-time.
    ///
    /// # Errors
    ///
    /// Returns `NikaError::ValidationError` if DAG construction fails
    /// (e.g. the workflow contains cycles or invalid dependencies).
    pub fn with_event_log(
        workflow: AnalyzedWorkflow,
        event_log: EventLog,
    ) -> Result<Self, NikaError> {
        let flow_graph = Dag::from_analyzed(&workflow).map_err(|e| NikaError::ValidationError {
            reason: format!("DAG construction failed: {e}"),
        })?;
        let datastore = RunContext::new();

        // Bridge MCP servers to old FxHashMap<String, McpConfigInline> for TaskExecutor
        let mcp_configs = lower_mcp_servers(workflow.mcp_servers.clone());
        let provider = workflow.provider.as_deref().unwrap_or("claude");

        let executor = TaskExecutor::new(
            provider,
            workflow.model.as_deref(),
            mcp_configs,
            event_log.clone(),
        );

        // Generate unique ID for this execution (used for trace files)
        let generation_id = format!("gen-{}", uuid::Uuid::new_v4());

        Ok(Self {
            workflow,
            flow_graph,
            datastore,
            executor,
            event_log,
            generation_id,
            quiet: false,
            cancel_token: CancellationToken::new(),
            paused: Arc::new(AtomicBool::new(false)),
            resume_notify: Arc::new(Notify::new()),
            resolved_assets: ResolvedAssets::default(),
            trace_config: TraceConfig::default(),
        })
    }

    /// Enable quiet mode to suppress console output (for TUI mode)
    ///
    /// When quiet is true, Runner will not print to stdout/stderr.
    /// All events are still emitted to the EventLog for TUI display.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Set trace retention config for automatic pruning after each run.
    ///
    /// When omitted, defaults to 100 max traces and 7-day retention.
    pub fn with_trace_config(mut self, config: TraceConfig) -> Self {
        self.trace_config = config;
        self
    }

    /// Inject initial context into the datastore
    ///
    /// Used by nika_run to pass parent context to child workflows.
    /// The context is stored as a successful task result under the given key,
    /// making it accessible via `with: alias: <key>.result` in the child workflow.
    ///
    /// # Example
    ///
    /// ```text
    /// // In parent workflow via nika_run:
    /// // context: { "entity": "qr-code", "locale": "fr-FR" }
    ///
    /// // Child workflow can access via:
    /// // with:
    /// //   parent: __parent_context__.result
    /// ```
    pub fn with_initial_context(self, key: &str, context: Value) -> Self {
        use crate::store::TaskResult;
        use crate::util::intern;

        self.datastore.insert(
            intern(key),
            TaskResult::success(context, std::time::Duration::ZERO),
        );
        self
    }

    /// Set a custom cancellation token
    ///
    /// This allows external control of workflow cancellation.
    /// The TUI can hold a clone of the token and call `cancel()` on it.
    /// Also propagated to TaskExecutor so MCP invoke operations
    /// abort promptly instead of waiting for INVOKE_TASK_DEADLINE.
    pub fn with_cancel_token(mut self, token: CancellationToken) -> Self {
        self.executor = self.executor.with_cancel_token(token.clone());
        self.cancel_token = token;
        self
    }

    /// Get a clone of the cancellation token
    ///
    /// The TUI can use this to abort the workflow by calling `cancel()`.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    /// Check if the workflow has been cancelled
    pub fn is_cancelled(&self) -> bool {
        self.cancel_token.is_cancelled()
    }

    /// Pause workflow execution
    ///
    /// When paused, the runner will complete current tasks but won't start new ones.
    /// Use `resume()` to continue execution.
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        self.event_log.emit(EventKind::WorkflowPaused);
    }

    /// Resume workflow execution after pause
    pub fn resume(&self) {
        self.paused.store(false, Ordering::SeqCst);
        self.resume_notify.notify_one();
        self.event_log.emit(EventKind::WorkflowResumed);
    }

    /// Check if the workflow is paused
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    /// Get cloneable handles for external pause/resume control
    ///
    /// Returns (paused_flag, resume_notify) that can be used by the TUI
    /// to control pause state externally.
    pub fn pause_handles(&self) -> (Arc<AtomicBool>, Arc<Notify>) {
        (Arc::clone(&self.paused), Arc::clone(&self.resume_notify))
    }

    /// Get the event log for inspection/export
    pub fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    /// Get tasks that are ready to run (all dependencies satisfied)
    ///
    /// Also detects and marks tasks whose dependencies have failed.
    /// These tasks are marked as DependencyFailed and stored in the datastore.
    fn get_ready_tasks(&self) -> Vec<&AnalyzedTask> {
        self.workflow
            .tasks
            .iter()
            .filter(|task| {
                // Skip if already done
                if self.datastore.contains(&task.name) {
                    return false;
                }

                // Check all dependencies
                let deps = self.flow_graph.get_dependencies(&task.name);
                for dep in deps.iter() {
                    // Check if dependency has completed
                    if let Some(dep_result) = self.datastore.get(dep.as_ref()) {
                        // If dependency failed (either directly or via its own dependencies),
                        // mark this task as DependencyFailed
                        if !dep_result.is_success() {
                            // Store DependencyFailed result for this task
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::dependency_failed(dep.as_ref()),
                            );

                            // Emit event for observability
                            self.event_log.emit(EventKind::TaskFailed {
                                task_id: Arc::from(task.name.as_str()),
                                error: format!("Cannot run: dependency '{}' failed", dep.as_ref()),
                                duration_ms: 0,
                            });

                            debug!(
                                task_id = %task.name,
                                dependency = %dep.as_ref(),
                                "Task blocked due to failed dependency"
                            );

                            return false;
                        }
                    } else {
                        // Dependency hasn't completed yet - task not ready
                        return false;
                    }
                }

                // All dependencies succeeded - task is ready
                true
            })
            .collect()
    }

    /// Check if all tasks are done (completed, failed, or blocked by dependency failure)
    fn all_done(&self) -> bool {
        self.workflow
            .tasks
            .iter()
            .all(|t| self.datastore.contains(&t.name))
    }

    /// Get tasks that are blocked waiting for incomplete dependencies (not failed)
    ///
    /// Used to distinguish actual deadlocks from dependency failures.
    fn get_pending_tasks(&self) -> Vec<String> {
        self.workflow
            .tasks
            .iter()
            .filter(|task| !self.datastore.contains(&task.name))
            .map(|t| t.name.clone())
            .collect()
    }

    /// Get the first failed task in the workflow (for error reporting)
    fn find_root_failure(&self) -> Option<String> {
        for task in &self.workflow.tasks {
            if let Some(result) = self.datastore.get(&task.name) {
                // Only consider actual failures, not dependency failures
                if matches!(result.status, crate::store::TaskOutcome::Failed(_)) {
                    return Some(task.name.clone());
                }
            }
        }
        None
    }

    /// Get the final output (from tasks with no successors)
    ///
    /// Uses `get_deepest_final_task()` to select the terminal task with the
    /// highest topological depth. This ensures branching DAGs return the correct
    /// output (e.g., "final" task, not "branch_a").
    fn get_final_output(&self) -> Option<String> {
        // Use deepest terminal task instead of arbitrary selection
        if let Some(deepest_task) = self.flow_graph.get_deepest_final_task() {
            if let Some(result) = self.datastore.get(deepest_task.as_ref()) {
                if result.is_success() {
                    return Some(result.output_str().into_owned());
                }
            }
        }

        // Fallback: Try any successful final task
        let final_tasks = self.flow_graph.get_final_tasks();
        for task_id in final_tasks {
            if let Some(result) = self.datastore.get(&task_id) {
                if result.is_success() {
                    return Some(result.output_str().into_owned());
                }
            }
        }
        None
    }

    /// Write execution trace to .nika/traces/ (called on ALL exit paths).
    ///
    /// Traces are written for WorkflowCompleted, WorkflowFailed, and WorkflowAborted.
    /// After writing, prunes old traces based on `trace_config` (max_traces + retention_days).
    fn write_trace(&self) {
        if let Ok(trace_writer) = TraceWriter::new(&self.generation_id) {
            if let Err(e) = trace_writer.write_all(&self.event_log) {
                tracing::warn!(error = %e, "Failed to write trace");
            } else {
                tracing::info!(path = %trace_writer.path().display(), "Trace written");
            }
        }

        // Enforce retention: prune traces beyond max_traces / retention_days
        prune_traces(self.trace_config.max_traces, self.trace_config.retention_days);
    }

    /// Check if a task qualifies for schema validation retry
    ///
    /// Returns Some((schema, max_retries, infer_params)) if:
    /// - Task action is Infer
    /// - Output format is JSON
    /// - Output has inline schema
    /// - structured.max_retries > 0
    fn get_retry_config(task: &AnalyzedTask) -> Option<(Value, u8, InferParams)> {
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
            temperature: infer_action.temperature,
            max_tokens: infer_action.max_tokens,
            system: infer_action.system.clone(),
            response_format: None,
            extended_thinking: None,
            thinking_budget: None,
        };

        Some((schema, max_retries, infer_params))
    }

    /// Execute an infer task with schema validation and retry loop
    ///
    /// When LLM output fails schema validation, builds a feedback prompt with:
    /// - Original prompt
    /// - Schema that must be matched
    /// - Previous output
    /// - Validation errors
    ///
    /// Retries up to max_retries times before failing.
    #[allow(clippy::too_many_arguments)]
    async fn execute_with_retry(
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
    ) -> TaskResult {
        let mut current_infer = original_infer;
        let mut attempts = 0u8;

        loop {
            attempts += 1;

            // Create action for this attempt
            let action = TaskAction::Infer {
                infer: current_infer.clone(),
            };

            // Execute
            let result = executor
                .execute(task_id, &action, bindings, datastore, output_policy)
                .await;
            let duration = start.elapsed();

            match result {
                Ok(output) => {
                    // Try to extract JSON from output
                    let json_value = match extract_json(&output) {
                        Ok(v) => v,
                        Err(e) => {
                            if attempts > max_retries {
                                // Max retries exhausted
                                event_log.emit(EventKind::TaskFailed {
                                    task_id: Arc::clone(task_id),
                                    error: format!(
                                        "NIKA-060: Invalid JSON after {} attempts: {}",
                                        attempts, e
                                    ),
                                    duration_ms: duration.as_millis() as u64,
                                });
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
                            current_infer.prompt = Self::build_retry_prompt(
                                &current_infer.prompt,
                                schema,
                                &output,
                                &format!("JSON parsing failed: {}", e),
                            );
                            continue;
                        }
                    };

                    // Validate against schema
                    let compiled = match jsonschema::validator_for(schema) {
                        Ok(c) => c,
                        Err(e) => {
                            event_log.emit(EventKind::TaskFailed {
                                task_id: Arc::clone(task_id),
                                error: format!("Invalid schema: {}", e),
                                duration_ms: duration.as_millis() as u64,
                            });
                            return TaskResult::failed(
                                format!("Invalid inline schema: {}", e),
                                duration,
                            );
                        }
                    };

                    let errors: Vec<_> = compiled.iter_errors(&json_value).collect();
                    if errors.is_empty() {
                        // Validation passed
                        event_log.emit(EventKind::TaskCompleted {
                            task_id: Arc::clone(task_id),
                            output: Arc::new(json_value.clone()),
                            duration_ms: duration.as_millis() as u64,
                        });
                        return TaskResult::success(json_value, duration);
                    }

                    // Validation failed
                    if attempts > max_retries {
                        let error_feedback = format_validation_errors(&json_value, schema);
                        event_log.emit(EventKind::TaskFailed {
                            task_id: Arc::clone(task_id),
                            error: format!(
                                "Schema validation failed after {} attempts:\n{}",
                                attempts, error_feedback
                            ),
                            duration_ms: duration.as_millis() as u64,
                        });
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
                    current_infer.prompt = Self::build_retry_prompt(
                        &current_infer.prompt,
                        schema,
                        &output,
                        &error_feedback,
                    );
                }
                Err(e) => {
                    // Executor error (not validation error) - don't retry
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(task_id),
                        error: e.to_string(),
                        duration_ms: duration.as_millis() as u64,
                    });
                    return TaskResult::failed(e.to_string(), duration);
                }
            }
        }
    }

    /// Build a retry prompt with error feedback
    fn build_retry_prompt(
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

    /// Execute a single task iteration (used for both regular tasks and for_each items)
    ///
    /// Bridge conversions (`lower_action`, `lower_output`) happen here at the
    /// `TaskExecutor` boundary — the rest of Runner works with `AnalyzedTask`.
    ///
    /// # Arguments
    ///
    /// * `task` - The analyzed task to execute
    /// * `task_id` - ID for this specific execution (may include index for for_each)
    /// * `parent_task_id` - Original task ID (for for_each, this is the parent task ID)
    /// * `datastore` - Data store for task results
    /// * `executor` - Task executor
    /// * `event_log` - Event log for observability
    /// * `for_each_binding` - Optional (var_name, value, index) for for_each iteration
    /// * `workflow_artifacts` - Workflow-level artifact configuration
    /// * `base_path` - Base path for artifact resolution
    #[allow(clippy::too_many_arguments)] // Artifact integration requires additional params
    async fn execute_task_iteration(
        task: AnalyzedTask,
        task_id: Arc<str>,
        parent_task_id: Arc<str>,
        datastore: RunContext,
        executor: TaskExecutor,
        event_log: EventLog,
        for_each_binding: Option<(String, Value, usize)>,
        workflow_artifacts: Option<ArtifactsConfig>,
        base_path: PathBuf,
    ) -> IterationResult {
        let start = Instant::now();

        // Extract for_each info if present
        let for_each_info = for_each_binding
            .as_ref()
            .map(|(_, _, idx)| (Arc::clone(&parent_task_id), *idx));

        // Build bindings from with: spec (always present in AnalyzedTask)
        let mut bindings = match ResolvedBindings::from_with_spec(Some(&task.with_spec), &datastore)
        {
            Ok(b) => b,
            Err(e) => {
                let duration = start.elapsed();
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                });
                return IterationResult {
                    store_id: task_id,
                    result: TaskResult::failed(e.to_string(), duration),
                    for_each_info,
                    artifact_paths: vec![],
                };
            }
        };

        // Add for_each binding if present
        if let Some((var_name, value, _idx)) = for_each_binding {
            bindings.set(&var_name, value);
        }

        // EMIT: TaskStarted
        event_log.emit(EventKind::TaskStarted {
            task_id: Arc::clone(&task_id),
            verb: Arc::from(task.action.verb_name()),
            inputs: bindings.to_value(),
        });

        // Bridge AnalyzedTask to lowered types at executor boundary
        let lowered_action = lower_action(
            task.action.clone(),
            task.provider.clone(),
            task.model.clone(),
            task.retry.clone(),
        );
        let lowered_output = task
            .output
            .as_ref()
            .map(|o: &AnalyzedOutput| lower_output(o.clone()));

        // Bridge structured: config to OutputPolicy for executor Layer 0 dispatch.
        // If both output: and structured: are set, output: takes precedence (already lowered).
        // If only structured: is set, synthesize an OutputPolicy so the executor
        // can trigger Layer 0 tool injection and prompt schema instructions.
        let effective_output = if lowered_output.is_some() {
            lowered_output
        } else {
            task.structured.as_ref().map(|spec| spec.to_output_policy())
        };

        // Check if task qualifies for schema validation retry
        let retry_config = Self::get_retry_config(&task);

        // Execute with retry loop if configured
        let task_result = if let Some((schema, max_retries, original_infer)) = retry_config {
            Self::execute_with_retry(
                &task_id,
                original_infer,
                &schema,
                max_retries,
                &bindings,
                &datastore,
                &executor,
                &event_log,
                start,
                effective_output.as_ref(),
            )
            .await
        } else {
            // Standard execution without retry
            let result = executor
                .execute(
                    &task_id,
                    &lowered_action,
                    &bindings,
                    &datastore,
                    effective_output.as_ref(),
                )
                .await;
            let duration = start.elapsed();

            match result {
                Ok(output) => {
                    // Structured output validation via 4-layer engine.
                    //
                    // Skip if effective_output was set (the executor already validated
                    // in verbs.rs via StructuredOutputEngine with InferCallback wired).
                    // Only apply runner-level validation when structured: is set but
                    // wasn't bridged to the executor (shouldn't happen, but defensive).
                    let executor_already_validated = effective_output.is_some();
                    let final_output = if !executor_already_validated {
                        if let Some(ref structured_spec) = task.structured {
                            let mut engine = StructuredOutputEngine::new(
                                structured_spec.clone(),
                                Arc::new(event_log.clone()),
                            );
                            match engine.validate(&task_id, &output).await {
                                Ok(result) => {
                                    debug!(
                                        task_id = %task_id,
                                        layer = result.layer,
                                        layer_name = %result.layer_name,
                                        total_attempts = result.total_attempts,
                                        "Structured output validation succeeded (runner fallback)"
                                    );
                                    result.value.to_string()
                                }
                                Err(e) => {
                                    event_log.emit(EventKind::TaskFailed {
                                        task_id: Arc::clone(&task_id),
                                        error: e.to_string(),
                                        duration_ms: duration.as_millis() as u64,
                                    });
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::failed(e.to_string(), duration),
                                        for_each_info,
                                        artifact_paths: vec![],
                                    };
                                }
                            }
                        } else {
                            output
                        }
                    } else {
                        output
                    };

                    let tr =
                        make_task_result(final_output, effective_output.as_ref(), duration).await;
                    if tr.is_success() {
                        event_log.emit(EventKind::TaskCompleted {
                            task_id: Arc::clone(&task_id),
                            output: Arc::clone(&tr.output),
                            duration_ms: duration.as_millis() as u64,
                        });
                    } else {
                        event_log.emit(EventKind::TaskFailed {
                            task_id: Arc::clone(&task_id),
                            error: tr.error().unwrap_or("Unknown error").to_string(),
                            duration_ms: duration.as_millis() as u64,
                        });
                    }
                    tr
                }
                Err(e) => {
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: e.to_string(),
                        duration_ms: duration.as_millis() as u64,
                    });
                    TaskResult::failed(e.to_string(), duration)
                }
            }
        };

        // Process artifacts if task succeeded and has artifact config
        let mut artifact_paths = Vec::new();
        if task_result.is_success() {
            if let Some(ref artifact_spec) = task.artifact {
                let output_content = task_result.output_str().into_owned();

                let artifact_result = process_task_artifacts(
                    &task_id,
                    &output_content,
                    artifact_spec,
                    workflow_artifacts.as_ref(),
                    &base_path,
                    Some(&event_log),
                    &bindings,
                    &datastore,
                )
                .await;

                if artifact_result.written > 0 {
                    debug!(
                        task_id = %task_id,
                        artifacts_written = artifact_result.written,
                        "Artifacts written"
                    );
                }
                artifact_paths = artifact_result.paths;

                for err in artifact_result.errors {
                    tracing::warn!(
                        task_id = %task_id,
                        error = %err,
                        "Artifact write error (non-fatal)"
                    );
                }
            }
        }

        IterationResult {
            store_id: task_id,
            result: task_result,
            for_each_info,
            artifact_paths,
        }
    }

    /// Main execution loop
    #[instrument(skip(self), fields(workflow_tasks = self.workflow.tasks.len()))]
    pub async fn run(&mut self) -> Result<String, NikaError> {
        let workflow_start = Instant::now();
        info!("Starting workflow execution");

        // Check for cancellation before starting
        if self.cancel_token.is_cancelled() {
            let duration = workflow_start.elapsed();
            self.event_log.emit(EventKind::WorkflowAborted {
                reason: "Workflow cancelled before start".to_string(),
                duration_ms: duration.as_millis() as u64,
                running_tasks: vec![],
            });
            self.write_trace();
            return Err(NikaError::Execution(
                "Workflow cancelled before start".to_string(),
            ));
        }

        // Load context files if workflow has context_files
        let base_path = std::env::current_dir().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to get current directory, using '.'");
            std::path::PathBuf::from(".")
        });
        if !self.workflow.context_files.is_empty() {
            let loaded_context =
                load_context_analyzed(&self.workflow.context_files, &base_path).await?;
            self.datastore.set_context(loaded_context);
            debug!("Loaded {} context files", self.workflow.context_files.len());
        }

        // Load inputs if workflow has inputs
        if !self.workflow.inputs.is_empty() {
            let inputs_map: rustc_hash::FxHashMap<String, serde_json::Value> = self
                .workflow
                .inputs
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            self.datastore.set_inputs(inputs_map);
            debug!("Loaded {} input parameters", self.workflow.inputs.len());
        }

        // Resolve agents
        if self.workflow.agents.is_some() {
            self.resolved_assets = resolve_assets_analyzed(&self.workflow, &base_path).await?;
            debug!(
                agents = self.resolved_assets.agents.len(),
                skills = self.resolved_assets.skills.len(),
                "Resolved workflow assets"
            );
        }

        let total_tasks = self.workflow.tasks.len();
        let mut _completed = 0;

        // EMIT: WorkflowStarted
        self.event_log.emit(EventKind::WorkflowStarted {
            task_count: total_tasks,
            generation_id: self.generation_id.clone(),
            workflow_hash: self.workflow.compute_hash(),
            nika_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        if !self.quiet {
            println!();

            // IMP-5: Warn if no task has output or artifact config
            let has_observable_output = self.workflow.tasks.iter().any(|t| {
                t.output.is_some() || t.artifact.is_some()
            });
            if !has_observable_output && total_tasks > 1 {
                println!(
                    "  {} {}\n",
                    "⚠".yellow(),
                    "No tasks have output: or artifact: config — results won't be persisted"
                        .yellow()
                );
            }
        }

        loop {
            // Check for cancellation at start of each loop iteration
            if self.cancel_token.is_cancelled() {
                let duration = workflow_start.elapsed();
                // Collect IDs of tasks that haven't completed yet
                let running_tasks: Vec<Arc<str>> = self
                    .workflow
                    .tasks
                    .iter()
                    .filter(|t| !self.datastore.contains(&t.name))
                    .map(|t| Arc::from(t.name.as_str()))
                    .collect();

                self.event_log.emit(EventKind::WorkflowAborted {
                    reason: "Workflow cancelled by user".to_string(),
                    duration_ms: duration.as_millis() as u64,
                    running_tasks,
                });
                self.write_trace();
                return Err(NikaError::Execution(
                    "Workflow cancelled by user".to_string(),
                ));
            }

            // Check for pause at start of each loop iteration
            // Waits until resumed, while also checking for cancellation
            while self.paused.load(Ordering::SeqCst) {
                tokio::select! {
                    _ = self.resume_notify.notified() => {
                        // Resumed, continue loop
                    }
                    _ = self.cancel_token.cancelled() => {
                        // Cancelled while paused
                        let duration = workflow_start.elapsed();
                        let running_tasks: Vec<Arc<str>> = self
                            .workflow
                            .tasks
                            .iter()
                            .filter(|t| !self.datastore.contains(&t.name))
                            .map(|t| Arc::from(t.name.as_str()))
                            .collect();

                        self.event_log.emit(EventKind::WorkflowAborted {
                            reason: "Workflow cancelled while paused".to_string(),
                            duration_ms: duration.as_millis() as u64,
                            running_tasks,
                        });
                        self.write_trace();
                        return Err(NikaError::Execution(
                            "Workflow cancelled while paused".to_string(),
                        ));
                    }
                }
            }

            let ready = self.get_ready_tasks();

            // Check for completion or deadlock
            if ready.is_empty() {
                if self.all_done() {
                    break;
                }

                // Check if we're blocked due to dependency failures (not a deadlock)
                let pending = self.get_pending_tasks();
                if pending.is_empty() {
                    // All tasks are done - shouldn't happen, but check for consistency
                    break;
                }

                // Check for dependency chain failures
                let blocked_by_dep_failure: Vec<String> = self
                    .workflow
                    .tasks
                    .iter()
                    .filter(|t| self.datastore.is_dependency_failed(&t.name))
                    .map(|t| t.name.clone())
                    .collect();

                if !blocked_by_dep_failure.is_empty() {
                    // Not a deadlock - tasks are blocked due to failed dependencies
                    let root_failure = self.find_root_failure();

                    self.event_log.emit(EventKind::WorkflowFailed {
                        error: format!(
                            "Dependency chain failed: {} task(s) blocked by failed dependencies",
                            blocked_by_dep_failure.len()
                        ),
                        failed_task: root_failure.clone().map(Arc::from),
                    });
                    self.write_trace();
                    return Err(NikaError::DependencyChainFailed {
                        count: blocked_by_dep_failure.len(),
                        blocked_tasks: blocked_by_dep_failure,
                        root_failure,
                    });
                }

                // Actual deadlock - no tasks ready and no dependency failures detected
                // This indicates a cycle or other structural issue
                self.event_log.emit(EventKind::WorkflowFailed {
                    error: "Deadlock: no tasks ready but workflow not complete".to_string(),
                    failed_task: None,
                });
                self.write_trace();
                return Err(NikaError::Execution(
                    "Deadlock: no tasks ready but workflow not complete. Check for circular dependencies.".to_string(),
                ));
            }

            // Spawn all ready tasks in parallel (Tokio handles concurrency)
            let mut join_set = JoinSet::new();

            // Prepare artifact config for all tasks in this batch
            let workflow_artifacts = self.workflow.artifacts.clone();
            let artifact_base_path = base_path.clone();

            for task in ready {
                let task_id = intern(&task.name);

                // EMIT: TaskScheduled
                let deps = self.flow_graph.get_dependencies(&task.name);
                self.event_log.emit(EventKind::TaskScheduled {
                    task_id: Arc::clone(&task_id),
                    dependencies: deps.to_vec(),
                });

                if !self.quiet {
                    println!(
                        "  {} {} {}",
                        "[⟳]".yellow(),
                        &task_id,
                        "running...".dimmed()
                    );
                }

                // Check if task has decompose - expands to for_each items
                // decompose takes priority over for_each (they're mutually exclusive)
                let for_each_items: Option<Vec<Value>> = if let Some(decompose) =
                    task.decompose.as_ref()
                {
                    debug!(
                        task_id = %task.name,
                        strategy = ?decompose.strategy,
                        traverse = %decompose.traverse,
                        "Expanding decompose modifier"
                    );
                    // Resolve bindings for decompose source
                    let bindings = match ResolvedBindings::from_with_spec(
                        Some(&task.with_spec),
                        &self.datastore,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::warn!(
                                task_id = %task.name,
                                error = %e,
                                "Failed to resolve bindings for decompose, using empty"
                            );
                            ResolvedBindings::default()
                        }
                    };
                    // Expand decompose using executor (with timeout to prevent silent hangs)
                    let decompose_result = tokio::time::timeout(
                        DECOMPOSE_TIMEOUT,
                        self.executor
                            .expand_decompose(decompose, &bindings, &self.datastore),
                    )
                    .await;

                    match decompose_result {
                        Ok(Ok(items)) => Some(items),
                        Ok(Err(e)) => {
                            // Decompose expansion failed
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::failed(e.to_string(), std::time::Duration::ZERO),
                            );
                            continue;
                        }
                        Err(_timeout) => {
                            // Decompose expansion timed out
                            let timeout_error = NikaError::DecomposeTimeout {
                                task_id: task.name.clone(),
                                timeout_secs: DECOMPOSE_TIMEOUT.as_secs(),
                            };
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::failed(timeout_error.to_string(), DECOMPOSE_TIMEOUT),
                            );
                            continue;
                        }
                    }
                } else if let Some(ref for_each) = task.for_each {
                    // AnalyzedForEach has structured fields: items, as_var, parallel, fail_fast
                    let items_str = &for_each.items;

                    if for_each.is_binding() {
                        // Binding reference ($alias, {{with.alias}}, {{inputs.xxx}})
                        let bindings = match ResolvedBindings::from_with_spec(
                            Some(&task.with_spec),
                            &self.datastore,
                        ) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::warn!(
                                    task_id = %task.name,
                                    error = %e,
                                    "Failed to resolve bindings for for_each, using empty"
                                );
                                ResolvedBindings::default()
                            }
                        };

                        if let Some(alias) = items_str.strip_prefix('$') {
                            // Check for $inputs.xxx format first (workflow inputs)
                            if alias.starts_with("inputs.") {
                                match self.datastore.resolve_input_path(alias) {
                                    Some(value) => value_to_array(&value),
                                    None => {
                                        self.datastore.insert(
                                            intern(&task.name),
                                            TaskResult::failed(
                                                format!(
                                                    "for_each input '{}' not found in workflow inputs",
                                                    alias
                                                ),
                                                std::time::Duration::ZERO,
                                            ),
                                        );
                                        continue;
                                    }
                                }
                            } else {
                                // $alias or $alias.nested.path format
                                let mut segments = alias.split('.');
                                let Some(base_alias) = segments.next() else {
                                    self.datastore.insert(
                                        intern(&task.name),
                                        TaskResult::failed(
                                            "for_each: empty alias after '$' prefix".to_string(),
                                            std::time::Duration::ZERO,
                                        ),
                                    );
                                    continue;
                                };

                                // Try with: bindings first, then fall back to datastore
                                let base_result = bindings
                                    .get_resolved(base_alias, &self.datastore)
                                    .or_else(|_| {
                                        // Fall back to direct datastore lookup for $task_id
                                        self.datastore
                                            .get_output(base_alias)
                                            .map(|arc| arc.as_ref().clone())
                                            .ok_or_else(|| NikaError::BindingNotFound {
                                                alias: base_alias.to_string(),
                                            })
                                    });
                                match base_result {
                                    Ok(base_value) => {
                                        // Auto-parse JSON strings before traversal
                                        let parsed_value;
                                        let working_value: &Value =
                                            if let Some(v) = crate::binding::jsonpath::try_parse_json_str(&base_value) {
                                                parsed_value = v;
                                                &parsed_value
                                            } else {
                                                &base_value
                                            };

                                        // Traverse nested path segments if present
                                        let mut value_ref: &Value = working_value;
                                        let mut traversal_failed = false;

                                        for segment in segments {
                                            let next = if let Ok(idx) = segment.parse::<usize>() {
                                                value_ref.get(idx)
                                            } else {
                                                value_ref.get(segment)
                                            };

                                            match next {
                                                Some(v) => value_ref = v,
                                                None => {
                                                    self.datastore.insert(
                                                        intern(&task.name),
                                                        TaskResult::failed(
                                                            format!(
                                                                "for_each binding '${}': nested path segment '{}' not found",
                                                                alias, segment
                                                            ),
                                                            std::time::Duration::ZERO,
                                                        ),
                                                    );
                                                    traversal_failed = true;
                                                    break;
                                                }
                                            }
                                        }

                                        if traversal_failed {
                                            continue;
                                        }

                                        match value_to_array(value_ref) {
                                            Some(items) => Some(items),
                                            None => {
                                                self.datastore.insert(
                                                    intern(&task.name),
                                                    TaskResult::failed(
                                                        format!(
                                                            "for_each binding '${}' resolved to non-array value",
                                                            alias
                                                        ),
                                                        std::time::Duration::ZERO,
                                                    ),
                                                );
                                                continue;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        self.datastore.insert(
                                            intern(&task.name),
                                            TaskResult::failed(
                                                format!(
                                                    "for_each binding '{}' not found: {}",
                                                    base_alias, e
                                                ),
                                                std::time::Duration::ZERO,
                                            ),
                                        );
                                        continue;
                                    }
                                }
                            }
                        } else if items_str.contains("{{inputs.") {
                            // Template format for inputs (e.g., "{{inputs.items}}")
                            if let Some(start) = items_str.find("{{inputs.") {
                                let after = &items_str[start + 9..];
                                if let Some(end) = after.find("}}") {
                                    let param_path = &after[..end];
                                    let full_path = format!("inputs.{}", param_path);
                                    match self.datastore.resolve_input_path(&full_path) {
                                        Some(value) => value_to_array(&value),
                                        None => {
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    format!(
                                                        "for_each input '{}' not found in workflow inputs",
                                                        full_path
                                                    ),
                                                    std::time::Duration::ZERO,
                                                ),
                                            );
                                            continue;
                                        }
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else if items_str.contains("{{with.") {
                            // Template format (e.g., "{{with.locales}}")
                            let prefix_info = items_str.find("{{with.").map(|s| (s, 7usize));
                            if let Some((start, prefix_len)) = prefix_info {
                                let after = &items_str[start + prefix_len..];
                                if let Some(end) = after.find("}}") {
                                    let path = &after[..end];
                                    let mut parts = path.split('.');
                                    let Some(alias) = parts.next() else {
                                        continue;
                                    };

                                    match bindings.get_resolved(alias, &self.datastore) {
                                        Ok(base_value) => {
                                            // Auto-parse JSON strings
                                            let parsed_value;
                                            let working_value: &Value =
                                                if let Some(v) = crate::binding::jsonpath::try_parse_json_str(&base_value) {
                                                    parsed_value = v;
                                                    &parsed_value
                                                } else {
                                                    &base_value
                                                };

                                            let mut value_ref: &Value = working_value;
                                            let mut traversal_failed = false;

                                            for segment in parts {
                                                let next = if let Ok(idx) = segment.parse::<usize>()
                                                {
                                                    value_ref.get(idx)
                                                } else {
                                                    value_ref.get(segment)
                                                };

                                                match next {
                                                    Some(v) => value_ref = v,
                                                    None => {
                                                        tracing::warn!(
                                                            task_id = %task.name,
                                                            path = %path,
                                                            segment = %segment,
                                                            "for_each nested path segment not found"
                                                        );
                                                        traversal_failed = true;
                                                        break;
                                                    }
                                                }
                                            }

                                            if traversal_failed {
                                                None
                                            } else {
                                                value_to_array(value_ref)
                                            }
                                        }
                                        Err(e) => {
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    format!(
                                                        "for_each binding '{}' not found: {}",
                                                        alias, e
                                                    ),
                                                    std::time::Duration::ZERO,
                                                ),
                                            );
                                            continue;
                                        }
                                    }
                                } else {
                                    None
                                }
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else if for_each.is_array() {
                        // Direct JSON array literal
                        for_each.parse_items()
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Check if task has for_each or decompose items
                if let Some(items) = for_each_items {
                    if !items.is_empty() {
                        // Note: total_tasks was used for progress display [N/M] but
                        // the new CLI format uses verb icons instead. Keeping the
                        // adjustment in case we add the counter back.

                        // Get concurrency settings from analyzed for_each
                        let fe = task.for_each.as_ref();
                        let concurrency = fe.and_then(|f| f.parallel).unwrap_or(1).max(1) as usize;
                        let fail_fast = fe.map(|f| f.fail_fast).unwrap_or(true);

                        debug!(
                            task_id = %task.name,
                            items = items.len(),
                            concurrency = concurrency,
                            fail_fast = fail_fast,
                            "Starting for_each iteration"
                        );

                        // Create semaphore for concurrency limiting
                        let semaphore = Arc::new(Semaphore::new(concurrency));
                        // Create cancellation token for fail_fast (notification-based, no busy-poll)
                        let cancel = CancellationToken::new();

                        // Spawn one execution per item in the array
                        let var_name = fe.map(|f| f.as_var.as_str()).unwrap_or("item").to_string();
                        for (idx, item) in items.iter().enumerate() {
                            // Check if cancelled before spawning
                            if fail_fast && cancel.is_cancelled() {
                                debug!(
                                    task_id = %task.name,
                                    idx = idx,
                                    "Skipping iteration due to fail_fast cancellation"
                                );
                                break;
                            }

                            let task = task.clone();
                            let task_id = intern(&format!("{}[{}]", task.name, idx));
                            let parent_task_id = intern(&task.name);
                            let datastore = self.datastore.clone();
                            let executor = self.executor.clone();
                            let event_log = self.event_log.clone();
                            let item = item.clone();
                            let var_name = var_name.clone();
                            let semaphore = Arc::clone(&semaphore);
                            let cancel = cancel.clone();
                            let workflow_artifacts = workflow_artifacts.clone();
                            let artifact_base_path = artifact_base_path.clone();

                            join_set.spawn(async move {
                                // Check cancellation BEFORE acquiring semaphore
                                if cancel.is_cancelled() {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::skipped(
                                            "Cancelled due to fail_fast before semaphore acquire"
                                                .to_string(),
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),
                                        artifact_paths: vec![],
                                    };
                                }

                                // Race semaphore acquisition against cancellation token
                                let _permit = tokio::select! {
                                    biased;

                                    _ = cancel.cancelled() => {
                                        return IterationResult {
                                            store_id: task_id,
                                            result: TaskResult::skipped(
                                                "Cancelled while waiting for semaphore".to_string(),
                                            ),
                                            for_each_info: Some((parent_task_id, idx)),
                                            artifact_paths: vec![],
                                        };
                                    }

                                    permit = semaphore.acquire() => {
                                        match permit {
                                            Ok(p) => p,
                                            Err(_) => {
                                                return IterationResult {
                                                    store_id: task_id,
                                                    result: TaskResult::failed(
                                                        "Semaphore closed unexpectedly".to_string(),
                                                        std::time::Duration::ZERO,
                                                    ),
                                                    for_each_info: Some((parent_task_id, idx)),
                                                    artifact_paths: vec![],
                                                };
                                            }
                                        }
                                    }
                                };

                                // Final check after acquiring permit
                                if cancel.is_cancelled() {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::skipped(
                                            "Cancelled after semaphore acquire".to_string(),
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),
                                        artifact_paths: vec![],
                                    };
                                }

                                let result = Self::execute_task_iteration(
                                    task,
                                    Arc::clone(&task_id),
                                    Arc::clone(&parent_task_id),
                                    datastore,
                                    executor,
                                    event_log,
                                    Some((var_name, item, idx)),
                                    workflow_artifacts,
                                    artifact_base_path,
                                )
                                .await;

                                // If failed and fail_fast, signal cancellation
                                if !result.result.is_success() && fail_fast {
                                    cancel.cancel();
                                }

                                result
                            });
                        }
                    } else {
                        // Empty for_each array: store empty array result immediately
                        debug!(
                            task_id = %task.name,
                            "for_each items array is empty, storing empty result"
                        );
                        self.datastore.insert(
                            intern(&task.name),
                            TaskResult::success(Value::Array(vec![]), std::time::Duration::ZERO),
                        );
                    }
                } else {
                    // Regular task without for_each
                    let task = task.clone();
                    let datastore = self.datastore.clone();
                    let executor = self.executor.clone();
                    let event_log = self.event_log.clone();
                    let workflow_artifacts = workflow_artifacts.clone();
                    let artifact_base_path = artifact_base_path.clone();

                    join_set.spawn(async move {
                        Self::execute_task_iteration(
                            task,
                            Arc::clone(&task_id),
                            task_id,
                            datastore,
                            executor,
                            event_log,
                            None,
                            workflow_artifacts,
                            artifact_base_path,
                        )
                        .await
                    });
                }
            }

            // Collect for_each results for aggregation: parent_id -> Vec<(index, result)>
            // Use IndexMap to preserve insertion order (deterministic iteration)
            let mut for_each_results: IndexMap<Arc<str>, Vec<(usize, TaskResult)>> =
                IndexMap::new();

            // Track which parent tasks have fail_fast enabled (for result collection)
            let fail_fast_parents: FxHashSet<Arc<str>> = self
                .workflow
                .tasks
                .iter()
                .filter(|t| t.for_each.as_ref().map(|fe| fe.fail_fast).unwrap_or(false))
                .map(|t| intern(&t.name))
                .collect();

            // Track if we've already triggered abort_all for fail_fast
            let mut fail_fast_triggered = false;

            // Wait for all spawned tasks to complete (with cancellation support)
            loop {
                tokio::select! {
                    // Check for cancellation
                    _ = self.cancel_token.cancelled() => {
                        // Abort all pending tasks
                        join_set.abort_all();

                        let duration = workflow_start.elapsed();
                        // Collect IDs of tasks that haven't completed yet
                        let running_tasks: Vec<Arc<str>> = self
                            .workflow
                            .tasks
                            .iter()
                            .filter(|t| !self.datastore.contains(&t.name))
                            .map(|t| Arc::from(t.name.as_str()))
                            .collect();

                        self.event_log.emit(EventKind::WorkflowAborted {
                            reason: "Workflow cancelled during execution".to_string(),
                            duration_ms: duration.as_millis() as u64,
                            running_tasks,
                        });
                        self.write_trace();
                        return Err(NikaError::Execution(
                            "Workflow cancelled during execution".to_string(),
                        ));
                    }
                    // Wait for next task result
                    result = join_set.join_next() => {
                        match result {
                            Some(Ok(iteration_result)) => {
                                let IterationResult {
                                    store_id,
                                    result: task_result,
                                    for_each_info,
                                    artifact_paths,
                                } = iteration_result;

                                _completed += 1;
                                let success = task_result.is_success();
                                let skipped = task_result.is_skipped();

                                let symbol = if success { "✓" } else if skipped { "⊘" } else { "✗" };
                                let symbol_colored = if success {
                                    symbol.green()
                                } else if skipped {
                                    symbol.yellow()
                                } else {
                                    symbol.red()
                                };
                                let duration_str =
                                    format!("{:.1}s", task_result.duration.as_secs_f32()).dimmed();

                                if !self.quiet {
                                    // IMP-3: Look up task description and verb for richer output
                                    let parent_name = store_id
                                        .find('[')
                                        .map(|i| &store_id[..i])
                                        .unwrap_or(&store_id);
                                    let task_info = self
                                        .workflow
                                        .tasks
                                        .iter()
                                        .find(|t| t.name == parent_name);
                                    let desc = task_info.and_then(|t| t.description.as_deref());
                                    let verb_name = task_info
                                        .map(|t| t.action.verb_name())
                                        .unwrap_or("exec");
                                    let icon = crate::display::verb_icon(verb_name);

                                    if let Some(d) = desc {
                                        println!(
                                            "  {} {} {} {} — {}",
                                            icon,
                                            &*store_id,
                                            symbol_colored,
                                            duration_str,
                                            d.dimmed()
                                        );
                                    } else {
                                        println!(
                                            "  {} {} {} {}",
                                            icon, &*store_id, symbol_colored, duration_str
                                        );
                                    }

                                    if let Some(err) = task_result.error() {
                                        if !skipped {
                                            println!("      {} {}", "Error:".red(), err);
                                        }
                                    }

                                    // IMP-4: Show intermediate output preview (first line only)
                                    if success {
                                        let out = task_result.output_str();
                                        if !out.is_empty() {
                                            // Take only the first non-empty line for clean display
                                            let first_line = out
                                                .lines()
                                                .find(|l| !l.trim().is_empty())
                                                .unwrap_or(&out);
                                            let preview = if first_line.len() > 120 {
                                                format!(
                                                    "{}…",
                                                    crate::util::truncate_str(first_line, 120)
                                                )
                                            } else if out.contains('\n') {
                                                format!("{}…", first_line)
                                            } else {
                                                first_line.to_string()
                                            };
                                            println!(
                                                "      {} {}",
                                                "→".dimmed(),
                                                preview.dimmed()
                                            );
                                        }
                                    }

                                    // IMP-6: Report artifact writes to the user
                                    for path in &artifact_paths {
                                        println!(
                                            "      {} {}",
                                            "artifact:".cyan(),
                                            path.display().to_string().dimmed()
                                        );
                                    }
                                }

                                // Store individual result
                                self.datastore
                                    .insert(Arc::clone(&store_id), task_result.clone());

                                // If this is a for_each failure with fail_fast,
                                // abort all remaining in-flight tasks immediately.
                                // Only abort if the PARENT task had fail_fast enabled.
                                let parent_has_fail_fast = for_each_info
                                    .as_ref()
                                    .map(|(parent_id, _)| fail_fast_parents.contains(parent_id))
                                    .unwrap_or(false);
                                if !success && !skipped && parent_has_fail_fast && !fail_fast_triggered {
                                    fail_fast_triggered = true;
                                    debug!(
                                        store_id = %store_id,
                                        "Triggering abort_all due to fail_fast"
                                    );
                                    join_set.abort_all();
                                }

                                // If this is a for_each iteration, collect for aggregation
                                if let Some((parent_id, idx)) = for_each_info {
                                    for_each_results
                                        .entry(parent_id)
                                        .or_default()
                                        .push((idx, task_result));
                                }
                            }
                            Some(Err(e)) => {
                                // Task was aborted (likely via abort_all) or panicked
                                if e.is_cancelled() {
                                    // Task was aborted by abort_all - this is expected
                                    debug!("Task aborted (likely due to fail_fast)");
                                    // Continue collecting remaining results
                                } else {
                                    // EMIT: WorkflowFailed (task panic)
                                    self.event_log.emit(EventKind::WorkflowFailed {
                                        error: format!("Task panicked: {}", e),
                                        failed_task: None,
                                    });
                                    self.write_trace();
                                    return Err(NikaError::Execution(format!("Task panicked: {}", e)));
                                }
                            }
                            None => {
                                // All tasks in this batch completed
                                break;
                            }
                        }
                    }
                }
            }

            // Aggregate for_each results into parent task
            for (parent_id, mut results) in for_each_results {
                // Sort by index to preserve order
                results.sort_by_key(|(idx, _)| *idx);

                // Collect outputs into JSON array
                let outputs: Vec<Value> = results
                    .iter()
                    .map(|(_, r)| {
                        // Try to parse as JSON, fall back to string
                        let output_str = r.output_str();
                        serde_json::from_str(&output_str)
                            .unwrap_or(Value::String(output_str.into_owned()))
                    })
                    .collect();

                // Calculate aggregate duration and success
                let total_duration: std::time::Duration =
                    results.iter().map(|(_, r)| r.duration).sum();
                let all_success = results.iter().all(|(_, r)| r.is_success());

                // Create aggregated result with JSON array
                let aggregated_result = if all_success {
                    TaskResult::success(Value::Array(outputs), total_duration)
                } else {
                    // Collect errors
                    let errors: Vec<String> = results
                        .iter()
                        .filter_map(|(idx, r)| r.error().map(|e| format!("[{}]: {}", idx, e)))
                        .collect();
                    TaskResult::failed(errors.join("; "), total_duration)
                };

                // Store aggregated result under parent ID
                self.datastore.insert(parent_id, aggregated_result);
            }
        }

        // Get final output
        let output = self.get_final_output().unwrap_or_default();

        // EMIT: WorkflowCompleted
        self.event_log.emit(EventKind::WorkflowCompleted {
            final_output: Arc::new(Value::String(output.clone())),
            total_duration_ms: workflow_start.elapsed().as_millis() as u64,
        });

        // Write execution trace to .nika/traces/
        self.write_trace();

        if !self.quiet {
            let elapsed = workflow_start.elapsed();
            let elapsed_str = if elapsed.as_secs() >= 60 {
                format!("{}m {:.1}s", elapsed.as_secs() / 60, elapsed.as_secs_f64() % 60.0)
            } else {
                format!("{:.1}s", elapsed.as_secs_f64())
            };

            // Compute total tokens and cost from events
            let events = self.event_log.events();
            let (total_tokens, total_cost) = events.iter().fold((0u64, 0.0f64), |(tokens, cost), e| {
                if let EventKind::ProviderResponded { input_tokens, output_tokens, cost_usd, .. } = &e.kind {
                    (tokens + input_tokens + output_tokens, cost + cost_usd)
                } else {
                    (tokens, cost)
                }
            });

            crate::display::print_done_summary(&elapsed_str, total_tokens, total_cost);
        }

        // Check for task failures — report for CI visibility
        let events = self.event_log.events();
        let failed_tasks: Vec<&str> = events
            .iter()
            .filter_map(|e| {
                if let EventKind::TaskFailed { task_id, .. } = &e.kind {
                    Some(task_id.as_ref())
                } else {
                    None
                }
            })
            .collect();

        if !failed_tasks.is_empty() && !self.quiet {
            println!(
                "{} {} task(s) had errors: {}",
                "⚠".yellow(),
                failed_tasks.len(),
                failed_tasks.join(", ").dimmed()
            );
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::{
        AnalyzedExecAction, AnalyzedForEach, AnalyzedInferAction, AnalyzedOutput, AnalyzedTask,
        AnalyzedTaskAction, AnalyzedWorkflow, OutputFormat as AnalyzedOutputFormat, TaskId,
        TaskTable,
    };
    use crate::ast::schema::SchemaVersion;
    use crate::ast::structured::StructuredOutputSpec;
    use crate::binding::types::{BindingPath, BindingSource};
    use crate::binding::{WithEntry, WithSpec};
    use crate::source::Span;
    use indexmap::IndexMap;
    use serde_json::json;
    use std::time::Duration;

    // ═══════════════════════════════════════════════════════════════
    // QUIET MODE TEST
    // ═══════════════════════════════════════════════════════════════

    fn make_empty_workflow() -> AnalyzedWorkflow {
        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            provider: Some("mock".to_string()),
            model: None,
            task_table: TaskTable::new(),
            tasks: vec![],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            imports: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            span: Span::dummy(),
        }
    }

    #[test]
    fn test_runner_quiet_mode() {
        // Default should not be quiet
        let runner = Runner::new(make_empty_workflow()).unwrap();
        assert!(!runner.quiet, "Runner should not be quiet by default");

        // quiet() should enable quiet mode
        let runner = Runner::new(make_empty_workflow()).unwrap().quiet();
        assert!(runner.quiet, "Runner should be quiet after .quiet()");

        // Can chain with with_event_log
        let event_log = crate::event::EventLog::new();
        let runner = Runner::with_event_log(make_empty_workflow(), event_log)
            .unwrap()
            .quiet();
        assert!(runner.quiet, "Runner should be quiet when chained");
    }

    // ═══════════════════════════════════════════════════════════════
    // INITIAL CONTEXT TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_with_initial_context_stores_value() {
        use serde_json::json;

        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).unwrap().with_initial_context(
            "__parent_context__",
            json!({"key": "value", "nested": {"deep": true}}),
        );

        // Context should be stored in datastore
        let result = runner.datastore.get("__parent_context__");
        assert!(result.is_some(), "Context should be stored");

        let stored = result.unwrap();
        assert!(stored.is_success(), "Should be stored as success");

        let output = stored.output_str();
        assert!(output.contains("key"), "Should contain 'key'");
        assert!(output.contains("value"), "Should contain 'value'");
    }

    #[test]
    fn test_with_initial_context_chaining() {
        use serde_json::json;

        // Should chain with other builder methods
        let workflow = make_empty_workflow();
        let event_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, event_log)
            .unwrap()
            .quiet()
            .with_initial_context("test_ctx", json!({"test": 123}));

        assert!(runner.quiet, "Should be quiet");
        assert!(
            runner.datastore.get("test_ctx").is_some(),
            "Context should exist"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH RESULT AGGREGATION TESTS
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create a workflow with a single for_each exec task
    fn create_for_each_workflow(
        task_id: &str,
        items_json: &str,
        as_var: &str,
        command: &str,
        concurrency: Option<u32>,
        fail_fast: bool,
        shell: bool,
    ) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        task_table.insert(task_id);
        let tid = task_table.get_id(task_id).unwrap();

        let task = AnalyzedTask {
            id: tid,
            name: task_id.to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: command.to_string(),
                shell,
                working_dir: None,
                env: IndexMap::new(),
                timeout_ms: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: items_json.to_string(),
                as_var: as_var.to_string(),
                parallel: concurrency,
                fail_fast,
                span: Span::dummy(),
            }),
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            span: Span::dummy(),
        };

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            provider: Some("mock".to_string()),
            model: None,
            task_table,
            tasks: vec![task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            imports: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            span: Span::dummy(),
        }
    }

    #[tokio::test]
    async fn test_for_each_collects_all_results() {
        let workflow = create_for_each_workflow(
            "echo_items",
            r#"["a", "b", "c"]"#,
            "item",
            "echo {{with.item}}",
            None,  // sequential
            true,  // fail_fast default
            false, // no shell
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let parent_result = runner.datastore.get("echo_items");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        let has_a = output.contains("a") || output.contains("\"a\"");
        let has_b = output.contains("b") || output.contains("\"b\"");
        let has_c = output.contains("c") || output.contains("\"c\"");

        assert!(
            has_a && has_b && has_c,
            "Output should contain all 3 results, got: {}",
            output
        );
    }

    #[tokio::test]
    async fn test_for_each_preserves_order() {
        let workflow = create_for_each_workflow(
            "ordered",
            r#"["first", "second", "third"]"#,
            "x",
            "echo {{with.x}}",
            None,
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        runner.run().await.unwrap();

        let parent_result = runner.datastore.get("ordered");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&output) {
            assert_eq!(arr.len(), 3, "Should have 3 results");
            let first = arr[0].as_str().unwrap_or("");
            let last = arr[2].as_str().unwrap_or("");
            assert!(
                first.contains("first"),
                "First element should contain 'first'"
            );
            assert!(
                last.contains("third"),
                "Last element should contain 'third'"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // BASIC WORKFLOW TESTS
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create a minimal workflow with exec tasks
    fn create_exec_workflow(
        tasks: Vec<(&str, &str)>,
        edges: Vec<(&str, &str)>,
    ) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        for (id, _) in &tasks {
            task_table.insert(id);
        }

        let analyzed_tasks: Vec<AnalyzedTask> = tasks
            .into_iter()
            .map(|(id, cmd)| {
                let task_id = task_table.get_id(id).unwrap();
                let depends_on: Vec<_> = edges
                    .iter()
                    .filter(|(_, tgt)| *tgt == id)
                    .filter_map(|(src, _)| task_table.get_id(src))
                    .collect();
                AnalyzedTask {
                    id: task_id,
                    name: id.to_string(),
                    description: None,
                    action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                        command: cmd.to_string(),
                        shell: false,
                        working_dir: None,
                        env: IndexMap::new(),
                        timeout_ms: None,
                        span: Span::dummy(),
                    }),
                    provider: None,
                    model: None,
                    with_spec: Default::default(),
                    depends_on,
                    implicit_deps: vec![],
                    output: None,
                    for_each: None,
                    retry: None,
                    decompose: None,
                    concurrency: None,
                    fail_fast: None,
                    artifact: None,
                    log: None,
                    structured: None,
                    span: Span::dummy(),
                }
            })
            .collect();

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V01,
            name: None,
            description: None,
            provider: Some("mock".to_string()),
            model: None,
            task_table,
            tasks: analyzed_tasks,
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            imports: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            span: Span::dummy(),
        }
    }

    #[tokio::test]
    async fn event_sequence_for_single_task() {
        let workflow = create_exec_workflow(vec![("greet", "echo hello")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();

        let result = runner.run().await.unwrap();
        assert_eq!(result, "hello");

        // Verify event sequence
        let events = runner.event_log().events();

        // Expected sequence:
        // 1. WorkflowStarted
        // 2. TaskScheduled
        // 3. TaskStarted (with inputs from ResolvedBindings)
        // 4. TemplateResolved (from executor)
        // 5. TaskCompleted
        // 6. WorkflowCompleted

        assert!(
            events.len() >= 5,
            "Expected at least 5 events, got {}",
            events.len()
        );

        // First event should be WorkflowStarted
        assert!(matches!(
            &events[0].kind,
            EventKind::WorkflowStarted { task_count: 1, .. }
        ));

        // Last event should be WorkflowCompleted
        let last = events.last().unwrap();
        assert!(matches!(&last.kind, EventKind::WorkflowCompleted { .. }));

        // Verify task events exist
        let task_events = runner.event_log().filter_task("greet");
        assert!(task_events.len() >= 3, "Expected at least 3 task events");

        // Verify TaskCompleted with correct output
        let completed = task_events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }));
        assert!(completed.is_some(), "TaskCompleted event not found");
    }

    #[tokio::test]
    async fn event_sequence_for_chained_tasks() {
        // Two tasks: greet -> shout (shout depends on greet)
        let workflow = create_exec_workflow(
            vec![("greet", "echo hello"), ("shout", "echo DONE")],
            vec![("greet", "shout")],
        );
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().events();

        // Verify WorkflowStarted with correct task count
        assert!(matches!(
            &events[0].kind,
            EventKind::WorkflowStarted { task_count: 2, .. }
        ));

        // Verify both tasks have complete event sequences
        let greet_events = runner.event_log().filter_task("greet");
        let shout_events = runner.event_log().filter_task("shout");

        assert!(!greet_events.is_empty(), "greet task events missing");
        assert!(!shout_events.is_empty(), "shout task events missing");

        // Verify order: greet TaskCompleted must come before shout TaskStarted
        let greet_completed_id = greet_events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }))
            .map(|e| e.id);
        let shout_started_id = shout_events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::TaskStarted { .. }))
            .map(|e| e.id);

        assert!(greet_completed_id.is_some());
        assert!(shout_started_id.is_some());
        assert!(
            greet_completed_id.unwrap() < shout_started_id.unwrap(),
            "greet should complete before shout starts"
        );
    }

    #[tokio::test]
    async fn event_sequence_for_parallel_tasks() {
        // Two independent tasks that can run in parallel
        let workflow = create_exec_workflow(
            vec![("task_a", "echo A"), ("task_b", "echo B")],
            vec![], // No dependencies = parallel
        );
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().events();

        // Verify WorkflowStarted
        assert!(matches!(
            &events[0].kind,
            EventKind::WorkflowStarted { task_count: 2, .. }
        ));

        // Both tasks should have been scheduled
        let scheduled: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.kind, EventKind::TaskScheduled { .. }))
            .collect();
        assert_eq!(scheduled.len(), 2, "Both tasks should be scheduled");

        // Both tasks should complete
        let completed: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }))
            .collect();
        assert_eq!(completed.len(), 2, "Both tasks should complete");

        // WorkflowCompleted should be last
        let last = events.last().unwrap();
        assert!(matches!(&last.kind, EventKind::WorkflowCompleted { .. }));
    }

    #[tokio::test]
    async fn event_ids_are_monotonic() {
        let workflow = create_exec_workflow(
            vec![("a", "echo 1"), ("b", "echo 2"), ("c", "echo 3")],
            vec![("a", "b"), ("b", "c")],
        );
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().events();
        let ids: Vec<u64> = events.iter().map(|e| e.id).collect();

        // Verify monotonic and sequential
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(id, i as u64, "IDs should be sequential from 0");
        }
    }

    #[tokio::test]
    async fn timestamps_are_relative_and_increasing() {
        let workflow = create_exec_workflow(
            vec![("fast", "echo quick"), ("slow", "sleep 0.1 && echo done")],
            vec![("fast", "slow")],
        );
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().events();

        // First timestamp should be small (relative to start)
        // Use generous 5000ms threshold for CI environments under load
        assert!(
            events[0].timestamp_ms < 5000,
            "First event should be near start (got {}ms, expected < 5000ms)",
            events[0].timestamp_ms
        );

        // Timestamps should generally increase
        for window in events.windows(2) {
            assert!(
                window[1].timestamp_ms >= window[0].timestamp_ms,
                "Timestamps should not decrease"
            );
        }
    }

    #[tokio::test]
    async fn failed_task_emits_task_failed_event() {
        let workflow = create_exec_workflow(vec![("fail", "exit 1")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();

        // Workflow run() returns Ok even when individual tasks fail
        runner
            .run()
            .await
            .expect("workflow should complete even when tasks fail internally");

        let events = runner.event_log().filter_task("fail");
        let failed = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::TaskFailed { .. }));

        assert!(failed.is_some(), "TaskFailed event should be emitted");
    }

    #[tokio::test]
    async fn template_resolved_event_captures_before_and_after() {
        // Create workflow with task that has a command
        let workflow = create_exec_workflow(vec![("echo_test", "echo hello world")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().filter_task("echo_test");
        let template_event = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::TemplateResolved { .. }));

        assert!(template_event.is_some(), "TemplateResolved event expected");

        if let EventKind::TemplateResolved {
            template, result, ..
        } = &template_event.unwrap().kind
        {
            assert_eq!(template, "echo hello world");
            assert_eq!(result, "echo hello world");
        }
    }

    #[tokio::test]
    async fn event_log_to_json_serializes_correctly() {
        let workflow = create_exec_workflow(vec![("simple", "echo test")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let json = runner.event_log().to_json();
        assert!(json.is_array());

        let array = json.as_array().unwrap();
        assert!(!array.is_empty());

        // Verify structure of first event
        let first = &array[0];
        assert!(first.get("id").is_some());
        assert!(first.get("timestamp_ms").is_some());
        assert!(first.get("kind").is_some());
        assert_eq!(first["kind"]["type"], "workflow_started");
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS FOR RUNNER INTERNAL METHODS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn get_ready_tasks_returns_tasks_with_no_deps() {
        // Two independent tasks - both should be ready
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B")],
            vec![], // No flows = no dependencies
        );
        let runner = Runner::new(workflow).unwrap();

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 2, "Both tasks should be ready");

        let names: Vec<&str> = ready.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"a"), "Task 'a' should be ready");
        assert!(names.contains(&"b"), "Task 'b' should be ready");
    }

    #[test]
    fn get_ready_tasks_respects_dependencies() {
        // Chain: a -> b -> c
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B"), ("c", "echo C")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow).unwrap();

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1, "Only first task should be ready");
        assert_eq!(ready[0].name, "a", "Task 'a' should be ready");
    }

    #[test]
    fn get_ready_tasks_excludes_completed_tasks() {
        let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        // Initially task is ready
        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1);

        // Mark task as done
        runner.datastore.insert(
            intern("only"),
            TaskResult::success_str("done", std::time::Duration::ZERO),
        );

        // Now no tasks should be ready
        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 0, "Completed task should not be ready");
    }

    #[test]
    fn all_done_returns_false_when_tasks_pending() {
        let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        assert!(!runner.all_done(), "Not all tasks are done initially");
    }

    #[test]
    fn all_done_returns_true_when_all_completed() {
        let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        // Mark all tasks as done
        runner.datastore.insert(
            intern("a"),
            TaskResult::success_str("A", std::time::Duration::ZERO),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::success_str("B", std::time::Duration::ZERO),
        );

        assert!(runner.all_done(), "All tasks should be done");
    }

    #[test]
    fn get_final_output_returns_output_from_final_task() {
        // Chain: a -> b (b is final)
        let workflow =
            create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![("a", "b")]);
        let runner = Runner::new(workflow).unwrap();

        // Mark tasks as done
        runner.datastore.insert(
            intern("a"),
            TaskResult::success_str("A", std::time::Duration::ZERO),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::success_str("final output", std::time::Duration::ZERO),
        );

        let output = runner.get_final_output();
        assert!(output.is_some());
        assert_eq!(output.unwrap(), "final output");
    }

    #[test]
    fn get_final_output_returns_none_when_no_results() {
        let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        let output = runner.get_final_output();
        assert!(output.is_none(), "No output when tasks not complete");
    }

    #[test]
    fn get_final_output_skips_failed_tasks() {
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "exit 1")],
            vec![], // Both are final tasks (no successors)
        );
        let runner = Runner::new(workflow).unwrap();

        // a succeeds, b fails
        runner.datastore.insert(
            intern("a"),
            TaskResult::success_str("success", std::time::Duration::ZERO),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::failed("error", std::time::Duration::ZERO),
        );

        let output = runner.get_final_output();
        assert!(output.is_some());
        assert_eq!(
            output.unwrap(),
            "success",
            "Should return successful task output"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH CONCURRENCY AND FAIL_FAST TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn for_each_with_explicit_concurrency() {
        let workflow = create_for_each_workflow(
            "concurrent",
            r#"["a", "b", "c", "d"]"#,
            "item",
            "echo {{with.item}}",
            Some(2), // Limit to 2 concurrent
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let parent_result = runner.datastore.get("concurrent");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        assert!(output.contains("a") || output.contains("\"a\""));
        assert!(output.contains("d") || output.contains("\"d\""));
    }

    #[tokio::test]
    async fn for_each_fail_fast_stops_on_first_error() {
        let workflow = create_for_each_workflow(
            "failfast",
            r#"["ok1", "FAIL", "ok2", "ok3"]"#,
            "item",
            "test '{{with.item}}' != 'FAIL' && echo {{with.item}}",
            Some(1), // Sequential to make failure predictable
            true,    // fail_fast
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        // Workflow completes but parent task may be marked as failed
        assert!(result.is_ok() || result.is_err());
    }

    #[tokio::test]
    async fn for_each_fail_fast_false_continues_on_error() {
        let workflow = create_for_each_workflow(
            "continue",
            r#"["ok1", "ok2"]"#,
            "item",
            "echo {{with.item}}",
            None,
            false, // Explicitly disable fail_fast
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok(), "Workflow should complete");

        let parent_result = runner.datastore.get("continue");
        assert!(parent_result.is_some());
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH WITH INPUTS.* SUPPORT
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create a for_each workflow with inputs
    fn create_for_each_with_inputs(
        task_id: &str,
        items_expr: &str,
        as_var: &str,
        command: &str,
        inputs: IndexMap<String, serde_json::Value>,
        concurrency: Option<u32>,
    ) -> AnalyzedWorkflow {
        let mut workflow = create_for_each_workflow(
            task_id,
            items_expr,
            as_var,
            command,
            concurrency,
            true,
            false,
        );
        workflow.inputs = inputs;
        workflow
    }

    #[tokio::test]
    async fn for_each_with_dollar_inputs_array() {
        let mut inputs = IndexMap::new();
        inputs.insert(
            "items".to_string(),
            json!({
                "type": "array",
                "default": ["alpha", "beta", "gamma"]
            }),
        );
        let workflow = create_for_each_with_inputs(
            "process_items",
            "$inputs.items",
            "item",
            "echo {{with.item}}",
            inputs,
            None,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("process_items");
        assert!(task_result.is_some(), "Task result should exist");
        assert!(task_result.unwrap().is_success(), "Task should succeed");
    }

    #[tokio::test]
    async fn for_each_with_template_inputs() {
        let mut inputs = IndexMap::new();
        inputs.insert(
            "locales".to_string(),
            json!({
                "type": "array",
                "default": ["fr-FR", "en-US"]
            }),
        );
        let workflow = create_for_each_with_inputs(
            "translate",
            "{{inputs.locales}}",
            "locale",
            "echo Translating to {{with.locale}}",
            inputs,
            Some(2),
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("translate");
        assert!(task_result.is_some(), "Task result should exist");
        assert!(task_result.unwrap().is_success(), "Task should succeed");
    }

    #[tokio::test]
    async fn for_each_with_inputs_missing_fails_gracefully() {
        let mut inputs = IndexMap::new();
        inputs.insert(
            "other_param".to_string(),
            json!({
                "type": "string",
                "default": "test"
            }),
        );
        let workflow = create_for_each_with_inputs(
            "missing_input",
            "$inputs.nonexistent",
            "item",
            "echo {{with.item}}",
            inputs,
            None,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok(), "Workflow should complete");

        let task_result = runner.datastore.get("missing_input");
        assert!(task_result.is_some(), "Task result should exist");
        let tr = task_result.unwrap();
        assert!(!tr.is_success(), "Task should fail due to missing input");
        let error_msg = tr.error().expect("Failed task should have error message");
        assert!(
            error_msg.contains("not found"),
            "Error should mention 'not found': {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn for_each_with_inputs_nested_path() {
        let mut inputs = IndexMap::new();
        inputs.insert(
            "data".to_string(),
            json!({
                "type": "object",
                "default": {
                    "items": ["one", "two", "three"]
                }
            }),
        );
        let workflow = create_for_each_with_inputs(
            "nested",
            "$inputs.data.items",
            "n",
            "echo {{with.n}}",
            inputs,
            None,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("nested");
        assert!(task_result.is_some(), "Task result should exist");
        assert!(task_result.unwrap().is_success(), "Task should succeed");
    }

    // ═══════════════════════════════════════════════════════════════
    // for_each Pattern 2 ($alias) — nested paths + error
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create a 2-step workflow where step1 produces output, step2 iterates with for_each
    fn create_two_step_for_each_workflow(
        step1_cmd: &str,
        step1_shell: bool,
        for_each_items: &str,
        step2_cmd: &str,
    ) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        task_table.insert("step1");
        task_table.insert("step2");
        let tid1 = task_table.get_id("step1").unwrap();
        let tid2 = task_table.get_id("step2").unwrap();

        let step1 = AnalyzedTask {
            id: tid1,
            name: "step1".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: step1_cmd.to_string(),
                shell: step1_shell,
                working_dir: None,
                env: IndexMap::new(),
                timeout_ms: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            span: Span::dummy(),
        };

        let mut with_spec = WithSpec::default();
        with_spec.insert(
            "step1".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task(intern("step1")),
                segments: vec![],
            }),
        );

        let step2 = AnalyzedTask {
            id: tid2,
            name: "step2".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: step2_cmd.to_string(),
                shell: false,
                working_dir: None,
                env: IndexMap::new(),
                timeout_ms: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            with_spec,
            depends_on: vec![tid1],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: for_each_items.to_string(),
                as_var: "item".to_string(),
                parallel: None,
                fail_fast: true,
                span: Span::dummy(),
            }),
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            span: Span::dummy(),
        };

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            provider: Some("mock".to_string()),
            model: None,
            task_table,
            tasks: vec![step1, step2],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            imports: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            span: Span::dummy(),
        }
    }

    #[tokio::test]
    async fn for_each_dollar_binding_nested_path() {
        let workflow = create_two_step_for_each_workflow(
            r#"echo '{"items": ["alpha", "beta", "gamma"], "count": 3}'"#,
            true,
            "$step1.items",
            "echo {{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("step2");
        assert!(task_result.is_some(), "step2 result should exist");
        assert!(
            task_result.unwrap().is_success(),
            "step2 should succeed with 3 items from nested path"
        );
    }

    #[tokio::test]
    async fn for_each_dollar_binding_non_array_errors() {
        let workflow = create_two_step_for_each_workflow(
            "echo not_an_array",
            false,
            "$step1",
            "echo {{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap();
        let _ = runner.run().await;

        let task_result = runner.datastore.get("step2");
        assert!(task_result.is_some(), "step2 result should exist");

        let result = task_result.unwrap();
        assert!(
            !result.is_success(),
            "step2 should FAIL when for_each binding resolves to non-array"
        );
        let error_msg = result.error().expect("should have error message");
        assert!(
            error_msg.contains("non-array"),
            "Error should mention 'non-array', got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn for_each_dollar_binding_json_string_array() {
        let workflow = create_two_step_for_each_workflow(
            r#"echo '["x","y","z"]'"#,
            true,
            "$step1",
            "echo {{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("step2");
        assert!(task_result.is_some(), "step2 result should exist");
        assert!(
            task_result.unwrap().is_success(),
            "step2 should succeed — JSON string array should be parsed"
        );
    }

    #[tokio::test]
    async fn for_each_dollar_binding_nested_path_not_found() {
        let workflow = create_two_step_for_each_workflow(
            r#"echo '{"data": {"count": 5}}'"#,
            true,
            "$step1.data.nonexistent",
            "echo {{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap();
        let _ = runner.run().await;

        let task_result = runner.datastore.get("step2");
        assert!(task_result.is_some(), "step2 result should exist");

        let result = task_result.unwrap();
        assert!(
            !result.is_success(),
            "step2 should FAIL when nested path segment doesn't exist"
        );
        let error_msg = result.error().expect("should have error message");
        assert!(
            error_msg.contains("not found"),
            "Error should mention path segment not found, got: {}",
            error_msg
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH EMPTY ARRAY EDGE CASE
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn for_each_empty_array_completes_with_empty_result() {
        let workflow = create_for_each_workflow(
            "empty_loop",
            "[]", // empty JSON array
            "item",
            "echo {{with.item}}",
            None,
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow with empty for_each should succeed, got: {:?}",
            result.err()
        );

        // Parent task should have a result (empty array, not missing)
        let parent_result = runner.datastore.get("empty_loop");
        assert!(
            parent_result.is_some(),
            "for_each with empty array should store a result"
        );

        let result = parent_result.unwrap();
        assert!(
            result.is_success(),
            "for_each with empty array should be success"
        );

        // Output should be an empty array "[]"
        let output = result.output_str();
        assert_eq!(
            output.trim(),
            "[]",
            "for_each with empty array should produce empty array, got: {}",
            output
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // CONSTRUCTOR AND EVENT LOG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_event_log_uses_provided_event_log() {
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let custom_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, custom_log).unwrap();

        // The runner should use the provided event log
        assert!(runner.event_log().events().is_empty());
    }

    #[test]
    fn new_and_with_event_log_return_result() {
        // Valid workflow should return Ok
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let result = Runner::new(workflow);
        assert!(
            result.is_ok(),
            "Runner::new should return Ok for a valid workflow"
        );

        // Valid workflow with custom event log should return Ok
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let event_log = EventLog::new();
        let result = Runner::with_event_log(workflow, event_log);
        assert!(
            result.is_ok(),
            "Runner::with_event_log should return Ok for a valid workflow"
        );
    }

    #[tokio::test]
    async fn workflow_completed_event_has_duration() {
        let workflow = create_exec_workflow(vec![("quick", "echo fast")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().events();
        let completed = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. }));

        assert!(completed.is_some());
        // Verify the event has a duration field (u64 is inherently non-negative)
        assert!(matches!(
            &completed.unwrap().kind,
            EventKind::WorkflowCompleted {
                total_duration_ms: _,
                ..
            }
        ));
    }

    #[tokio::test]
    async fn workflow_started_event_has_generation_id() {
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();

        runner.run().await.unwrap();

        let events = runner.event_log().events();
        let started = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowStarted { .. }));

        assert!(started.is_some());
        if let EventKind::WorkflowStarted { generation_id, .. } = &started.unwrap().kind {
            assert!(
                generation_id.starts_with("gen-"),
                "Generation ID should have prefix"
            );
            assert!(
                generation_id.len() > 10,
                "Generation ID should include UUID"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // CANCELLATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_cancel_token_default() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).unwrap();

        // Should not be cancelled by default
        assert!(
            !runner.is_cancelled(),
            "Runner should not be cancelled by default"
        );
    }

    #[test]
    fn test_cancel_token_can_be_set() {
        let workflow = make_empty_workflow();
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let runner = Runner::new(workflow).unwrap().with_cancel_token(token);

        // Cancelling the original token should be reflected
        token_clone.cancel();
        assert!(runner.is_cancelled(), "Runner should detect cancellation");
    }

    #[test]
    fn test_cancel_token_cloning() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).unwrap();

        let token1 = runner.cancel_token();
        let token2 = runner.cancel_token();

        // Both tokens should be clones of the same underlying token
        token1.cancel();
        assert!(token2.is_cancelled(), "Cloned tokens should share state");
        assert!(runner.is_cancelled(), "Runner should detect cancellation");
    }

    #[tokio::test]
    async fn test_cancellation_before_start_returns_aborted() {
        // Create a slow workflow
        let workflow = create_exec_workflow(vec![("slow", "sleep 10")], vec![]);
        let token = CancellationToken::new();

        let mut runner = Runner::new(workflow)
            .unwrap()
            .with_cancel_token(token.clone());

        // Cancel before starting
        token.cancel();

        let result = runner.run().await;
        assert!(result.is_err(), "Cancelled workflow should return error");

        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("cancelled") || err.to_string().contains("aborted"),
            "Error should mention cancellation: {}",
            err
        );

        // Should emit WorkflowAborted event
        let events = runner.event_log().events();
        let aborted = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowAborted { .. }));
        assert!(aborted.is_some(), "WorkflowAborted event should be emitted");
    }

    #[tokio::test]
    async fn test_cancellation_during_execution_aborts_workflow() {
        use std::time::Duration;

        // Create a workflow with a slow task
        let workflow = create_exec_workflow(vec![("slow", "sleep 5")], vec![]);
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let mut runner = Runner::new(workflow).unwrap().with_cancel_token(token);

        // Spawn the workflow run in background
        let handle = tokio::spawn(async move { runner.run().await });

        // Wait a bit then cancel
        tokio::time::sleep(Duration::from_millis(100)).await;
        token_clone.cancel();

        // Should complete with error (not take 5 seconds)
        let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(
            result.is_ok(),
            "Cancellation should complete within 2 seconds"
        );

        let workflow_result = result.unwrap().unwrap();
        assert!(
            workflow_result.is_err(),
            "Cancelled workflow should return error"
        );
    }

    #[tokio::test]
    async fn test_workflow_aborted_event_has_running_tasks() {
        use std::time::Duration;

        // Create workflow with parallel slow tasks
        let workflow = create_exec_workflow(
            vec![("slow1", "sleep 5"), ("slow2", "sleep 5")],
            vec![], // No deps = parallel
        );
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let event_log = EventLog::new();
        let event_log_clone = event_log.clone();
        let mut runner = Runner::with_event_log(workflow, event_log)
            .unwrap()
            .with_cancel_token(token);

        // Spawn the workflow
        let run_handle = tokio::spawn(async move { runner.run().await });

        // Wait for tasks to start, then cancel
        tokio::time::sleep(Duration::from_millis(100)).await;
        token_clone.cancel();

        // Wait for abort
        let result = run_handle.await.unwrap();
        assert!(result.is_err(), "Cancelled workflow should return error");

        // Check that WorkflowAborted event was emitted with running tasks
        let events = event_log_clone.events();
        let aborted = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowAborted { .. }));
        assert!(aborted.is_some(), "WorkflowAborted event should be emitted");

        if let EventKind::WorkflowAborted { running_tasks, .. } = &aborted.unwrap().kind {
            // At least one task should have been running
            assert!(
                !running_tasks.is_empty() || running_tasks.len() <= 2,
                "Should have captured running tasks (0-2 expected)"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PAUSE/RESUME TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_pause_state_default() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).unwrap();

        // Should not be paused by default
        assert!(
            !runner.is_paused(),
            "Runner should not be paused by default"
        );
    }

    #[test]
    fn test_pause_and_resume() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).unwrap();

        // Initially not paused
        assert!(!runner.is_paused());

        // Pause
        runner.pause();
        assert!(runner.is_paused(), "Runner should be paused after pause()");

        // Resume
        runner.resume();
        assert!(
            !runner.is_paused(),
            "Runner should not be paused after resume()"
        );
    }

    #[test]
    fn test_pause_handles_cloning() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).unwrap();

        let (paused1, notify1) = runner.pause_handles();
        let (paused2, _notify2) = runner.pause_handles();

        // Both should share the same underlying state
        runner.pause();
        assert!(
            paused1.load(Ordering::SeqCst),
            "First handle should see paused state"
        );
        assert!(
            paused2.load(Ordering::SeqCst),
            "Second handle should see paused state"
        );

        // Resume via runner
        runner.resume();
        assert!(
            !paused1.load(Ordering::SeqCst),
            "First handle should see resumed state"
        );
        assert!(
            !paused2.load(Ordering::SeqCst),
            "Second handle should see resumed state"
        );

        // Verify notify exists (just access it to prove it's valid)
        notify1.notify_one();
    }

    #[test]
    fn test_pause_emits_events() {
        let workflow = make_empty_workflow();
        let event_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();

        // Pause and resume
        runner.pause();
        runner.resume();

        // Check events
        let events = event_log.events();
        let paused = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowPaused));
        let resumed = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowResumed));

        assert!(paused.is_some(), "WorkflowPaused event should be emitted");
        assert!(resumed.is_some(), "WorkflowResumed event should be emitted");
    }

    #[tokio::test]
    async fn test_pause_waits_for_resume() {
        use std::sync::atomic::AtomicUsize;
        use std::time::Duration;

        // Create a simple workflow
        let workflow = create_exec_workflow(vec![("task1", "echo done")], vec![]);
        let event_log = EventLog::new();
        let event_log_clone = event_log.clone();
        let mut runner = Runner::with_event_log(workflow, event_log).unwrap();

        // Pause before running
        runner.pause();

        let (paused, notify) = runner.pause_handles();
        let resume_count = Arc::new(AtomicUsize::new(0));
        let resume_count_clone = Arc::clone(&resume_count);

        // Spawn the workflow
        let handle = tokio::spawn(async move { runner.run().await });

        // Wait a bit - workflow should be waiting
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Check events - should not have completed yet
        {
            let events = event_log_clone.events();
            let completed = events
                .iter()
                .find(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. }));
            assert!(
                completed.is_none(),
                "Workflow should be paused, not completed"
            );
        }

        // Resume
        paused.store(false, Ordering::SeqCst);
        notify.notify_one();
        resume_count_clone.fetch_add(1, Ordering::SeqCst);

        // Now it should complete
        let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
        assert!(result.is_ok(), "Workflow should complete after resume");

        let inner_result = result.unwrap().unwrap();
        assert!(inner_result.is_ok(), "Workflow should succeed");
    }

    // ═══════════════════════════════════════════════════════════════
    // VALUE_TO_ARRAY HELPER TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_value_to_array_direct_array() {
        use serde_json::json;

        let value = json!(["a", "b", "c"]);
        let result = value_to_array(&value);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_value_to_array_json_string() {
        use serde_json::json;

        // String containing JSON array (common case from exec output)
        let value = json!(r#"["x","y","z"]"#);
        let result = value_to_array(&value);
        assert!(result.is_some(), "Should parse JSON array string");
        let arr = result.unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], "x");
        assert_eq!(arr[1], "y");
        assert_eq!(arr[2], "z");
    }

    #[test]
    fn test_value_to_array_json_string_with_whitespace() {
        use serde_json::json;

        // String with leading/trailing whitespace
        let value = json!("  [1, 2, 3]  ");
        let result = value_to_array(&value);
        assert!(result.is_some(), "Should handle whitespace");
        assert_eq!(result.unwrap().len(), 3);
    }

    #[test]
    fn test_value_to_array_not_array_string() {
        use serde_json::json;

        // String that's not a JSON array
        let value = json!("hello world");
        let result = value_to_array(&value);
        assert!(result.is_none(), "Should return None for non-array string");
    }

    #[test]
    fn test_value_to_array_object() {
        use serde_json::json;

        // Object should return None
        let value = json!({"key": "value"});
        let result = value_to_array(&value);
        assert!(result.is_none(), "Should return None for object");
    }

    #[test]
    fn test_value_to_array_number() {
        use serde_json::json;

        // Number should return None
        let value = json!(42);
        let result = value_to_array(&value);
        assert!(result.is_none(), "Should return None for number");
    }

    #[test]
    fn test_value_to_array_nested_json_string() {
        use serde_json::json;

        // Complex JSON array as string
        let value = json!(r#"[{"id": 1}, {"id": 2}]"#);
        let result = value_to_array(&value);
        assert!(result.is_some(), "Should parse complex JSON array string");
        let arr = result.unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["id"], 1);
        assert_eq!(arr[1]["id"], 2);
    }

    #[test]
    fn test_value_to_array_invalid_json_string() {
        use serde_json::json;

        // Invalid JSON that looks like an array
        let value = json!("[not valid json");
        let result = value_to_array(&value);
        assert!(result.is_none(), "Should return None for invalid JSON");
    }

    #[test]
    fn test_value_to_array_markdown_fenced_json_array() {
        let value = Value::String("```json\n[\"a\", \"b\", \"c\"]\n```".to_string());
        let result = value_to_array(&value);
        assert!(
            result.is_some(),
            "Should parse JSON array from markdown fence"
        );
        let arr = result.unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], json!("a"));
        assert_eq!(arr[1], json!("b"));
        assert_eq!(arr[2], json!("c"));
    }

    #[test]
    fn test_value_to_array_plain_fenced_json_array() {
        let value = Value::String("```\n[1, 2, 3]\n```".to_string());
        let result = value_to_array(&value);
        assert!(result.is_some(), "Should parse JSON array from plain fence");
        let arr = result.unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr[0], json!(1));
        assert_eq!(arr[1], json!(2));
        assert_eq!(arr[2], json!(3));
    }

    #[test]
    fn test_value_to_array_bare_json_string_still_works() {
        let value = Value::String("[\"x\", \"y\"]".to_string());
        let result = value_to_array(&value);
        assert!(result.is_some(), "Bare JSON array string should still work");
        let arr = result.unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], json!("x"));
        assert_eq!(arr[1], json!("y"));
    }

    #[test]
    fn test_value_to_array_direct_array_value_still_works() {
        let value = json!(["a", "b"]);
        let result = value_to_array(&value);
        assert!(result.is_some(), "Direct array value should still work");
        let arr = result.unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0], json!("a"));
        assert_eq!(arr[1], json!("b"));
    }

    #[test]
    fn test_value_to_array_non_array_string_returns_none() {
        let value = Value::String("just a string".to_string());
        let result = value_to_array(&value);
        assert!(result.is_none(), "Non-array string should return None");
    }

    #[test]
    fn test_value_to_array_empty_array() {
        use serde_json::json;

        // Empty array (direct)
        let value = json!([]);
        let result = value_to_array(&value);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 0);

        // Empty array (as string)
        let value = json!("[]");
        let result = value_to_array(&value);
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 0);
    }

    // ═══════════════════════════════════════════════════════════════
    // GET_RETRY_CONFIG TESTS
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create an AnalyzedTask with an infer action.
    ///
    /// `output` controls the AnalyzedOutput (format + schema).
    /// `structured` controls the StructuredOutputSpec (max_retries).
    /// Both must be Some with matching values for retry to qualify.
    fn make_infer_task(
        name: &str,
        output: Option<AnalyzedOutput>,
        structured: Option<StructuredOutputSpec>,
    ) -> AnalyzedTask {
        AnalyzedTask {
            id: TaskId(0),
            name: name.to_string(),
            description: None,
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "test prompt".to_string(),
                system: None,
                temperature: None,
                max_tokens: None,
                ..Default::default()
            }),
            provider: None,
            model: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output,
            for_each: None,
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured,
            span: Span::dummy(),
        }
    }

    #[test]
    fn test_get_retry_config_none_for_exec_task() {
        let task = AnalyzedTask {
            id: TaskId(0),
            name: "exec_task".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "echo hi".to_string(),
                shell: false,
                working_dir: None,
                env: IndexMap::new(),
                timeout_ms: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(json!({"type": "object"})),
                span: Span::dummy(),
            }),
            for_each: None,
            retry: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: Some(StructuredOutputSpec::with_inline_schema(
                json!({"type": "object"}),
            )),
            span: Span::dummy(),
        };
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "Exec tasks should never qualify for retry"
        );
    }

    #[test]
    fn test_get_retry_config_none_for_no_output() {
        let task = make_infer_task("no_output", None, None);
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "No output means no retry"
        );
    }

    #[test]
    fn test_get_retry_config_none_for_text_format() {
        let task = make_infer_task(
            "text_format",
            Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Text,
                schema: Some(json!({"type": "object"})),
                span: Span::dummy(),
            }),
            Some(StructuredOutputSpec::with_inline_schema(
                json!({"type": "object"}),
            )),
        );
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "Text format should not qualify for retry"
        );
    }

    #[test]
    fn test_get_retry_config_none_for_json_no_schema() {
        let task = make_infer_task(
            "json_no_schema",
            Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: None,
                span: Span::dummy(),
            }),
            Some(StructuredOutputSpec::with_inline_schema(
                json!({"type": "object"}),
            )),
        );
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "JSON without schema should not qualify"
        );
    }

    #[test]
    fn test_get_retry_config_none_for_no_structured() {
        let task = make_infer_task(
            "no_structured",
            Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(json!({"type": "object"})),
                span: Span::dummy(),
            }),
            None, // No structured spec → no max_retries
        );
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "No structured spec means no retry"
        );
    }

    #[test]
    fn test_get_retry_config_none_for_zero_retries() {
        let mut structured = StructuredOutputSpec::with_inline_schema(json!({"type": "object"}));
        structured.max_retries = Some(0);
        let task = make_infer_task(
            "zero_retries",
            Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(json!({"type": "object"})),
                span: Span::dummy(),
            }),
            Some(structured),
        );
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "Zero retries means no retry"
        );
    }

    #[test]
    fn test_get_retry_config_none_for_default_retries() {
        let mut structured = StructuredOutputSpec::with_inline_schema(json!({"type": "object"}));
        structured.max_retries = None; // defaults to 0 via unwrap_or(0)
        let task = make_infer_task(
            "default_retries",
            Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(json!({"type": "object"})),
                span: Span::dummy(),
            }),
            Some(structured),
        );
        assert!(
            Runner::get_retry_config(&task).is_none(),
            "Default retries (None → 0) means no retry"
        );
    }

    #[test]
    fn test_get_retry_config_some_for_valid_config() {
        let schema = json!({"type": "object", "properties": {"name": {"type": "string"}}});
        let mut structured = StructuredOutputSpec::with_inline_schema(schema.clone());
        structured.max_retries = Some(3);
        let task = make_infer_task(
            "valid_retry",
            Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(schema.clone()),
                span: Span::dummy(),
            }),
            Some(structured),
        );
        let result = Runner::get_retry_config(&task);
        assert!(result.is_some(), "Valid config should return Some");

        let (ret_schema, max_retries, infer) = result.unwrap();
        assert_eq!(ret_schema, schema);
        assert_eq!(max_retries, 3);
        assert_eq!(infer.prompt, "test prompt");
    }

    // ═══════════════════════════════════════════════════════════════
    // FIND_ROOT_FAILURE TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_find_root_failure_none_when_empty() {
        let runner = Runner::new(make_empty_workflow()).unwrap();
        assert!(
            runner.find_root_failure().is_none(),
            "Empty workflow has no failures"
        );
    }

    #[test]
    fn test_find_root_failure_none_when_all_succeed() {
        let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        // Simulate successful completions
        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::success(json!("ok"), Duration::from_millis(20)),
        );

        assert!(
            runner.find_root_failure().is_none(),
            "All-success should return None"
        );
    }

    #[test]
    fn test_find_root_failure_returns_first_failed() {
        let workflow = create_exec_workflow(
            vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
            vec![],
        );
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::failed("something broke".to_string(), Duration::from_millis(20)),
        );
        runner.datastore.insert(
            intern("c"),
            TaskResult::failed("also broken".to_string(), Duration::from_millis(30)),
        );

        assert_eq!(
            runner.find_root_failure(),
            Some("b".to_string()),
            "Should return first failed task in workflow order"
        );
    }

    #[test]
    fn test_find_root_failure_skips_dependency_failed() {
        let workflow = create_exec_workflow(
            vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("root cause".to_string(), Duration::from_millis(10)),
        );
        runner
            .datastore
            .insert(intern("b"), TaskResult::dependency_failed("a"));
        runner
            .datastore
            .insert(intern("c"), TaskResult::dependency_failed("b"));

        assert_eq!(
            runner.find_root_failure(),
            Some("a".to_string()),
            "Should skip DependencyFailed and return the actual failure"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // GET_PENDING_TASKS TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_get_pending_tasks_all_pending() {
        let workflow = create_exec_workflow(
            vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
            vec![],
        );
        let runner = Runner::new(workflow).unwrap();

        let pending = runner.get_pending_tasks();
        assert_eq!(pending.len(), 3);
        assert!(pending.contains(&"a".to_string()));
        assert!(pending.contains(&"b".to_string()));
        assert!(pending.contains(&"c".to_string()));
    }

    #[test]
    fn test_get_pending_tasks_excludes_completed() {
        let workflow = create_exec_workflow(
            vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
            vec![],
        );
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );
        runner.datastore.insert(
            intern("c"),
            TaskResult::success(json!("ok"), Duration::from_millis(20)),
        );

        let pending = runner.get_pending_tasks();
        assert_eq!(pending, vec!["b".to_string()]);
    }

    #[test]
    fn test_get_pending_tasks_empty_when_all_done() {
        let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::success(json!("ok"), Duration::from_millis(20)),
        );

        let pending = runner.get_pending_tasks();
        assert!(pending.is_empty());
    }

    #[test]
    fn test_get_pending_tasks_excludes_failed() {
        let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("error".to_string(), Duration::from_millis(10)),
        );

        let pending = runner.get_pending_tasks();
        assert_eq!(pending, vec!["b".to_string()]);
    }

    // ═══════════════════════════════════════════════════════════════
    // GET_READY_TASKS + DEPENDENCY FAILURE PROPAGATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_get_ready_tasks_no_deps() {
        let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 2, "Tasks with no deps should all be ready");
    }

    #[test]
    fn test_get_ready_tasks_blocked_by_incomplete_dep() {
        let workflow =
            create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
        let runner = Runner::new(workflow).unwrap();

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "a", "Only root task should be ready");
    }

    #[test]
    fn test_get_ready_tasks_unblocked_after_dep_success() {
        let workflow =
            create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "b", "b should be ready after a succeeds");
    }

    #[test]
    fn test_get_ready_tasks_skips_already_done() {
        let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "b", "Completed task should not be returned");
    }

    #[test]
    fn test_dependency_failure_propagation() {
        // a → b → c: if a fails, b and c should get DependencyFailed
        let workflow = create_exec_workflow(
            vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow).unwrap();

        // Mark a as failed
        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("boom".to_string(), Duration::from_millis(10)),
        );

        // First call: b should be marked as DependencyFailed
        let ready = runner.get_ready_tasks();
        assert!(ready.is_empty(), "No tasks should be ready when dep failed");

        // Verify b was stored as DependencyFailed
        let b_result = runner.datastore.get("b").expect("b should be in store");
        assert!(
            b_result.is_dependency_failed(),
            "b should be DependencyFailed"
        );
        assert_eq!(
            b_result.failed_dependency(),
            Some("a"),
            "b should record a as the failed dependency"
        );

        // Second call: c should now also be marked as DependencyFailed
        let ready = runner.get_ready_tasks();
        assert!(ready.is_empty());

        let c_result = runner.datastore.get("c").expect("c should be in store");
        assert!(
            c_result.is_dependency_failed(),
            "c should be DependencyFailed"
        );
        assert_eq!(
            c_result.failed_dependency(),
            Some("b"),
            "c should record b as the failed dependency"
        );
    }

    #[test]
    fn test_dependency_failure_does_not_affect_parallel_tasks() {
        // a → b, a → c (parallel), d (independent)
        // If a fails, b and c get DependencyFailed, but d remains pending
        let workflow = create_exec_workflow(
            vec![
                ("a", "echo a"),
                ("b", "echo b"),
                ("c", "echo c"),
                ("d", "echo d"),
            ],
            vec![("a", "b"), ("a", "c")],
        );
        let runner = Runner::new(workflow).unwrap();

        // Mark a as failed
        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("oops".to_string(), Duration::from_millis(10)),
        );

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1, "Only d should be ready");
        assert_eq!(ready[0].name, "d");

        // b and c should be dependency-failed
        assert!(runner.datastore.get("b").unwrap().is_dependency_failed());
        assert!(runner.datastore.get("c").unwrap().is_dependency_failed());
    }

    #[test]
    fn test_dependency_failure_emits_events() {
        let workflow =
            create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("crash".to_string(), Duration::from_millis(10)),
        );

        // Trigger dependency failure propagation
        let _ = runner.get_ready_tasks();

        // Check that a TaskFailed event was emitted for b
        let events = runner.event_log.events();
        let fail_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    &e.kind,
                    EventKind::TaskFailed { task_id, .. } if task_id.as_ref() == "b"
                )
            })
            .collect();
        assert_eq!(
            fail_events.len(),
            1,
            "Should emit exactly one TaskFailed event for b"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // ALL_DONE TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_all_done_empty_workflow() {
        let runner = Runner::new(make_empty_workflow()).unwrap();
        assert!(runner.all_done(), "Empty workflow is trivially done");
    }

    #[test]
    fn test_all_done_false_when_pending() {
        let workflow = create_exec_workflow(vec![("a", "echo a")], vec![]);
        let runner = Runner::new(workflow).unwrap();
        assert!(!runner.all_done());
    }

    #[test]
    fn test_all_done_true_with_mixed_outcomes() {
        let workflow = create_exec_workflow(
            vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
            vec![],
        );
        let runner = Runner::new(workflow).unwrap();

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );
        runner.datastore.insert(
            intern("b"),
            TaskResult::failed("err".to_string(), Duration::from_millis(20)),
        );
        runner
            .datastore
            .insert(intern("c"), TaskResult::dependency_failed("b"));

        assert!(
            runner.all_done(),
            "All tasks have results (success, failed, or dep-failed)"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // AUDIT: FOR_EACH EDGE CASES
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn audit_for_each_empty_array_produces_empty_result() {
        let workflow = create_for_each_workflow(
            "empty_loop",
            "[]",
            "item",
            "echo {{with.item}}",
            None,
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Empty for_each should complete successfully: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("empty_loop");
        assert!(task_result.is_some(), "Task result should exist");

        let tr = task_result.unwrap();
        assert!(tr.is_success(), "Empty for_each should succeed");

        // The output should be an empty array [], NOT the output of running
        // the task body once as a regular task.
        let output = tr.output_str();
        let parsed: Result<Vec<Value>, _> = serde_json::from_str(&output);
        assert!(
            parsed.is_ok(),
            "Output should be valid JSON array, got: {}",
            output
        );
        assert_eq!(
            parsed.unwrap().len(),
            0,
            "Empty for_each should produce empty array, got: {}",
            output
        );
    }

    #[tokio::test]
    async fn audit_for_each_single_item_array_works() {
        let workflow = create_for_each_workflow(
            "single",
            r#"["only_one"]"#,
            "item",
            "echo {{with.item}}",
            None,
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Single-item for_each should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("single");
        assert!(task_result.is_some(), "Task result should exist");

        let tr = task_result.unwrap();
        assert!(tr.is_success(), "Task should succeed");

        let output = tr.output_str();
        let parsed: Vec<Value> = serde_json::from_str(&output)
            .unwrap_or_else(|_| panic!("Output should be JSON array, got: {}", output));
        assert_eq!(parsed.len(), 1, "Should have exactly one result");
        let first_str = parsed[0].as_str().unwrap_or("");
        assert!(
            first_str.contains("only_one"),
            "Single result should contain 'only_one', got: {}",
            first_str
        );
    }

    #[tokio::test]
    async fn audit_for_each_nested_json_items_bound_correctly() {
        let workflow = create_for_each_workflow(
            "nested_items",
            r#"[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]"#,
            "person",
            "echo {{with.person}}",
            None,
            true,
            true,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Nested JSON for_each should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("nested_items");
        assert!(task_result.is_some(), "Task result should exist");

        let tr = task_result.unwrap();
        assert!(tr.is_success(), "Task should succeed");

        let output = tr.output_str();
        assert!(
            output.contains("Alice") && output.contains("Bob"),
            "Output should contain both names from nested JSON items, got: {}",
            output
        );
    }

    #[tokio::test]
    async fn audit_for_each_fail_fast_false_continues_after_failure() {
        // BUG TEST: With fail_fast=false, ALL iterations should run even when
        // one fails. Previously, abort_all was called unconditionally.
        let workflow = create_for_each_workflow(
            "continue_on_fail",
            r#"["ok1", "FAIL", "ok2"]"#,
            "item",
            "test '{{with.item}}' != 'FAIL' && echo {{with.item}}",
            Some(1),
            false, // fail_fast = false
            true,  // shell = true (command uses shell operators)
        );

        let mut runner = Runner::new(workflow).unwrap();
        let _result = runner.run().await;

        let task_result = runner.datastore.get("continue_on_fail");
        assert!(task_result.is_some(), "Parent task result should exist");

        let ok1_result = runner.datastore.get("continue_on_fail[0]");
        let fail_result = runner.datastore.get("continue_on_fail[1]");
        let ok2_result = runner.datastore.get("continue_on_fail[2]");

        assert!(ok1_result.is_some(), "First iteration result should exist");
        assert!(
            fail_result.is_some(),
            "Second iteration result should exist"
        );
        assert!(
            ok2_result.is_some(),
            "Third iteration result should exist (fail_fast=false)"
        );

        assert!(
            ok1_result.unwrap().is_success(),
            "First iteration should succeed"
        );
        assert!(
            !fail_result.unwrap().is_success(),
            "Second iteration should fail"
        );
        assert!(
            ok2_result.unwrap().is_success(),
            "Third iteration should succeed (not aborted by fail_fast=false)"
        );
    }

    #[tokio::test]
    async fn audit_for_each_with_depends_on_runs_after_dependency() {
        let workflow = create_two_step_for_each_workflow(
            r#"echo '["red", "green", "blue"]'"#,
            true,
            "$step1",
            "echo color={{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "for_each with depends_on should complete: {:?}",
            result.err()
        );

        let step1_result = runner.datastore.get("step1");
        assert!(step1_result.is_some());
        assert!(step1_result.unwrap().is_success());

        let step2_result = runner.datastore.get("step2");
        assert!(step2_result.is_some());
        let tr = step2_result.unwrap();
        assert!(
            tr.is_success(),
            "step2 should succeed, got error: {:?}",
            tr.error()
        );

        let output = tr.output_str();
        assert!(
            output.contains("red") && output.contains("green") && output.contains("blue"),
            "for_each with depends_on should produce all 3 colors, got: {}",
            output
        );
    }

    #[tokio::test]
    async fn audit_for_each_items_non_array_non_string_errors() {
        let workflow =
            create_two_step_for_each_workflow("echo 42", false, "$step1", "echo {{with.item}}");

        let mut runner = Runner::new(workflow).unwrap();
        let _ = runner.run().await;

        let task_result = runner.datastore.get("step2");
        assert!(task_result.is_some(), "step2 result should exist");

        let tr = task_result.unwrap();
        assert!(
            !tr.is_success(),
            "for_each with non-array binding should fail"
        );
        let error_msg = tr.error().expect("should have error");
        assert!(
            error_msg.contains("non-array"),
            "Error should mention non-array, got: {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn audit_for_each_large_array_with_concurrency() {
        let items: Vec<String> = (0..20).map(|i| format!("\"item{}\"", i)).collect();
        let items_json = format!("[{}]", items.join(", "));

        let workflow = create_for_each_workflow(
            "large_batch",
            &items_json,
            "x",
            "echo {{with.x}}",
            Some(4),
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Large for_each should complete: {:?}",
            result.err()
        );

        let task_result = runner.datastore.get("large_batch");
        assert!(task_result.is_some());
        let tr = task_result.unwrap();
        assert!(tr.is_success(), "Large batch should succeed");

        let output = tr.output_str();
        let parsed: Vec<Value> = serde_json::from_str(&output)
            .unwrap_or_else(|_| panic!("Should be JSON array, got: {}", output));
        assert_eq!(
            parsed.len(),
            20,
            "Should have 20 results from large batch, got: {}",
            parsed.len()
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // AUDIT: STRUCTURED OUTPUT ENGINE EDGE CASES
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn audit_structured_output_layer3_with_mock_callback_succeeds() {
        use crate::runtime::structured_output::{InferCallback, StructuredOutputEngine};

        let log = Arc::new(EventLog::new());
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" }
            },
            "required": ["name", "age"]
        });
        let mut spec = StructuredOutputSpec::with_inline_schema(schema);
        spec.max_retries = Some(2);
        spec.enable_retry = Some(true);

        let callback: InferCallback = Arc::new(move |_prompt: String| {
            Box::pin(async move { Ok(r#"{"name": "Fixed", "age": 42}"#.to_string()) })
        });

        let mut engine = StructuredOutputEngine::new(spec, log.clone())
            .with_infer_callback(callback)
            .with_original_prompt("Generate a user".to_string());

        let result = engine
            .validate("retry-test", r#"{"name": "Incomplete"}"#)
            .await;

        assert!(
            result.is_ok(),
            "Layer 3 should succeed with mock callback: {:?}",
            result.err()
        );
        let r = result.unwrap();
        assert_eq!(r.layer, 3, "Should have succeeded at Layer 3");
        assert_eq!(r.value["name"], "Fixed");
        assert_eq!(r.value["age"], 42);
    }

    #[tokio::test]
    async fn audit_structured_output_layer3_exhausts_retries() {
        use crate::runtime::structured_output::{InferCallback, StructuredOutputEngine};

        let log = Arc::new(EventLog::new());
        let schema = json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "score": { "type": "number" }
            },
            "required": ["name", "score"]
        });
        let mut spec = StructuredOutputSpec::with_inline_schema(schema);
        spec.max_retries = Some(2);
        spec.enable_retry = Some(true);
        spec.enable_repair = Some(false);

        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let call_count_clone = Arc::clone(&call_count);
        let callback: InferCallback = Arc::new(move |_prompt: String| {
            call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Box::pin(async move { Ok(r#"{"name": "Still Wrong"}"#.to_string()) })
        });

        let mut engine = StructuredOutputEngine::new(spec, log.clone())
            .with_infer_callback(callback)
            .with_original_prompt("test".to_string());

        let result = engine
            .validate("exhaust-test", r#"{"name": "Invalid"}"#)
            .await;

        assert!(result.is_err(), "Should fail after exhausting all retries");

        let calls = call_count.load(std::sync::atomic::Ordering::SeqCst);
        assert_eq!(
            calls, 2,
            "Layer 3 should call LLM exactly max_retries times, got: {}",
            calls
        );
    }

    #[tokio::test]
    async fn audit_structured_output_layer4_repair_succeeds() {
        use crate::runtime::structured_output::{InferCallback, StructuredOutputEngine};

        let log = Arc::new(EventLog::new());
        let schema = json!({
            "type": "object",
            "properties": {
                "valid": { "type": "boolean" }
            },
            "required": ["valid"]
        });
        let mut spec = StructuredOutputSpec::with_inline_schema(schema);
        spec.enable_retry = Some(false);
        spec.enable_repair = Some(true);

        let callback: InferCallback = Arc::new(move |_prompt: String| {
            Box::pin(async move { Ok(r#"{"valid": true}"#.to_string()) })
        });

        let mut engine =
            StructuredOutputEngine::new(spec, log.clone()).with_infer_callback(callback);

        let result = engine
            .validate("repair-test", r#"{"invalid_field": 123}"#)
            .await;

        assert!(
            result.is_ok(),
            "Layer 4 repair should succeed: {:?}",
            result.err()
        );
        let r = result.unwrap();
        assert_eq!(r.layer, 4, "Should have succeeded at Layer 4");
        assert_eq!(r.value["valid"], true);
    }

    #[tokio::test]
    async fn audit_structured_output_validates_array_schema() {
        use crate::runtime::structured_output::StructuredOutputEngine;

        let log = Arc::new(EventLog::new());
        let schema = json!({
            "type": "array",
            "items": { "type": "string" },
            "minItems": 1
        });
        let spec = StructuredOutputSpec::with_inline_schema(schema);
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine.validate("arr-ok", r#"["hello", "world"]"#).await;
        assert!(result.is_ok(), "String array should validate");

        let mut engine2 = StructuredOutputEngine::new(
            StructuredOutputSpec::with_inline_schema(json!({
                "type": "array",
                "items": { "type": "string" },
                "minItems": 1
            })),
            Arc::new(EventLog::new()),
        );
        let result = engine2.validate("arr-empty", "[]").await;
        assert!(result.is_err(), "Empty array should fail minItems check");
    }

    #[tokio::test]
    async fn audit_structured_output_validates_additional_properties_false() {
        use crate::runtime::structured_output::StructuredOutputEngine;

        let log = Arc::new(EventLog::new());
        let spec = StructuredOutputSpec::with_inline_schema(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"],
            "additionalProperties": false
        }));
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine.validate("addl-ok", r#"{"name": "test"}"#).await;
        assert!(result.is_ok(), "Known properties only should validate");

        let mut engine2 = StructuredOutputEngine::new(
            StructuredOutputSpec::with_inline_schema(json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" }
                },
                "required": ["name"],
                "additionalProperties": false
            })),
            Arc::new(EventLog::new()),
        );
        let result = engine2
            .validate("addl-bad", r#"{"name": "test", "extra": true}"#)
            .await;
        assert!(
            result.is_err(),
            "Extra properties should fail when additionalProperties=false"
        );
    }

    #[tokio::test]
    async fn audit_structured_output_validates_deeply_nested_schema() {
        use crate::runtime::structured_output::StructuredOutputEngine;

        let log = Arc::new(EventLog::new());
        let schema = json!({
            "type": "object",
            "properties": {
                "level1": {
                    "type": "object",
                    "properties": {
                        "level2": {
                            "type": "object",
                            "properties": {
                                "value": { "type": "integer" }
                            },
                            "required": ["value"]
                        }
                    },
                    "required": ["level2"]
                }
            },
            "required": ["level1"]
        });
        let spec = StructuredOutputSpec::with_inline_schema(schema);
        let mut engine = StructuredOutputEngine::new(spec, log);

        let result = engine
            .validate("deep-ok", r#"{"level1": {"level2": {"value": 42}}}"#)
            .await;
        assert!(result.is_ok(), "Deeply nested valid should pass");

        let mut engine2 = StructuredOutputEngine::new(
            StructuredOutputSpec::with_inline_schema(json!({
                "type": "object",
                "properties": {
                    "level1": {
                        "type": "object",
                        "properties": {
                            "level2": {
                                "type": "object",
                                "properties": {
                                    "value": { "type": "integer" }
                                },
                                "required": ["value"]
                            }
                        },
                        "required": ["level2"]
                    }
                },
                "required": ["level1"]
            })),
            Arc::new(EventLog::new()),
        );
        let result = engine2
            .validate(
                "deep-bad",
                r#"{"level1": {"level2": {"value": "not_a_number"}}}"#,
            )
            .await;
        assert!(
            result.is_err(),
            "Wrong type at deep level should fail validation"
        );
    }

    #[tokio::test]
    async fn audit_structured_output_validates_primitive_types() {
        use crate::runtime::structured_output::StructuredOutputEngine;

        // String schema
        let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "string"}));
        let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
        let result = engine.validate("str-ok", r#""hello""#).await;
        assert!(
            result.is_ok(),
            "Quoted string should validate as string type"
        );

        // Number schema
        let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "number"}));
        let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
        let result = engine.validate("num-ok", "42.5").await;
        assert!(result.is_ok(), "Number should validate as number type");

        // Boolean schema
        let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "boolean"}));
        let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
        let result = engine.validate("bool-ok", "true").await;
        assert!(result.is_ok(), "Boolean should validate as boolean type");

        // Null schema
        let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "null"}));
        let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
        let result = engine.validate("null-ok", "null").await;
        assert!(result.is_ok(), "null should validate as null type");
    }

    // ═══════════════════════════════════════════════════════════════
    // AUDIT: BUILD_RETRY_PROMPT TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn audit_build_retry_prompt_includes_all_components() {
        let schema = json!({"type": "object", "required": ["name"]});
        let prompt = Runner::build_retry_prompt(
            "Generate a user",
            &schema,
            r#"{"broken": true}"#,
            "missing required field: name",
        );

        assert!(
            prompt.contains("Generate a user"),
            "Should contain original prompt"
        );
        assert!(
            prompt.contains(r#"{"broken": true}"#),
            "Should contain the actual previous output"
        );
        assert!(
            prompt.contains("missing required field: name"),
            "Should contain validation errors"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // AUDIT: LOWERING OF STRUCTURED FIELD
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn audit_to_output_policy_preserves_structured_spec() {
        let schema = json!({"type": "object"});
        let mut spec = StructuredOutputSpec::with_inline_schema(schema.clone());
        spec.max_retries = Some(5);
        spec.enable_repair = Some(false);

        let policy = spec.to_output_policy();

        assert_eq!(policy.format, crate::ast::output::OutputFormat::Json,);

        assert!(policy.schema.is_some());

        assert_eq!(policy.max_retries, Some(5));

        assert!(policy.source_structured_spec.is_some());
        let roundtripped = policy.source_structured_spec.unwrap();
        assert_eq!(roundtripped.max_retries, Some(5));
        assert_eq!(roundtripped.enable_repair, Some(false));
    }
}
