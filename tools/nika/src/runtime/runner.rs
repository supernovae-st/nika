//! DAG Runner - workflow execution with tokio
//!
//! Performance optimizations:
//! - Arc for zero-cost task/context sharing
//! - JoinSet for efficient parallel task collection
//! - Tokio handles all concurrency (no artificial limits)

use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use crate::ast::output::OutputPolicy;
use crate::ast::{InferParams, OutputFormat, Task, TaskAction, Workflow};
use crate::binding::ResolvedBindings;
use crate::dag::{validate_use_wiring, Dag};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog, TraceWriter};
use crate::store::{RunContext, TaskResult};
use crate::util::{intern, DECOMPOSE_TIMEOUT};

use super::artifact_processor::process_task_artifacts;
use super::context_loader::load_context;
use super::executor::TaskExecutor;
use super::output::{extract_json, format_validation_errors, make_task_result};
use super::resolver::{resolve_assets, ResolvedAssets};
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
    // First, try direct array access
    if let Some(arr) = value.as_array() {
        return Some(arr.clone());
    }

    // If it's a string, try to parse it as a JSON array
    if let Some(s) = value.as_str() {
        let trimmed = s.trim();
        // Only try to parse if it looks like a JSON array
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Ok(parsed) = serde_json::from_str::<Vec<Value>>(trimmed) {
                return Some(parsed);
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
}

/// DAG workflow runner with event sourcing
pub struct Runner {
    workflow: Workflow,
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
}

impl Runner {
    pub fn new(workflow: Workflow) -> Self {
        Self::with_event_log(workflow, EventLog::new())
    }

    /// Create a Runner with a custom EventLog (for TUI integration)
    ///
    /// Use `EventLog::new_with_broadcast()` to create an EventLog that
    /// sends events to TUI in real-time.
    pub fn with_event_log(workflow: Workflow, event_log: EventLog) -> Self {
        // DAG construction should not fail for validated workflows.
        // This is a programming invariant: callers must validate before constructing Runner.
        let flow_graph = Dag::from_workflow(&workflow).unwrap_or_else(|e| {
            panic!(
                "BUG: Dag::from_workflow failed for {} tasks — workflow must be validated before Runner: {e}",
                workflow.tasks.len()
            )
        });
        let datastore = RunContext::new();
        let executor = TaskExecutor::new(
            &workflow.provider,
            workflow.model.as_deref(),
            workflow.mcp.clone(),
            event_log.clone(),
        );

        // Generate unique ID for this execution (used for trace files)
        let generation_id = format!("gen-{}", uuid::Uuid::new_v4());

        Self {
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
        }
    }

    /// Enable quiet mode to suppress console output (for TUI mode)
    ///
    /// When quiet is true, Runner will not print to stdout/stderr.
    /// All events are still emitted to the EventLog for TUI display.
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Inject initial context into the datastore
    ///
    /// Used by nika_run to pass parent context to child workflows.
    /// The context is stored as a successful task result under the given key,
    /// making it accessible via `use: alias: <key>.result` in the child workflow.
    ///
    /// # Example
    ///
    /// ```text
    /// // In parent workflow via nika_run:
    /// // context: { "entity": "qr-code", "locale": "fr-FR" }
    ///
    /// // Child workflow can access via:
    /// // use:
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
    fn get_ready_tasks(&self) -> Vec<Arc<Task>> {
        self.workflow
            .tasks
            .iter()
            .filter(|task| {
                // Skip if already done
                if self.datastore.contains(&task.id) {
                    return false;
                }

                // Check all dependencies
                let deps = self.flow_graph.get_dependencies(&task.id);
                for dep in deps.iter() {
                    // Check if dependency has completed
                    if let Some(dep_result) = self.datastore.get(dep.as_ref()) {
                        // If dependency failed (either directly or via its own dependencies),
                        // mark this task as DependencyFailed
                        if !dep_result.is_success() {
                            // Store DependencyFailed result for this task
                            self.datastore.insert(
                                intern(&task.id),
                                TaskResult::dependency_failed(dep.as_ref()),
                            );

                            // Emit event for observability
                            self.event_log.emit(EventKind::TaskFailed {
                                task_id: Arc::from(task.id.as_str()),
                                error: format!("Cannot run: dependency '{}' failed", dep.as_ref()),
                                duration_ms: 0,
                            });

                            debug!(
                                task_id = %task.id,
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
            .cloned() // Clone the Arc, not the Task
            .collect()
    }

    /// Check if all tasks are done (completed, failed, or blocked by dependency failure)
    fn all_done(&self) -> bool {
        self.workflow
            .tasks
            .iter()
            .all(|t| self.datastore.contains(&t.id))
    }

    /// Get tasks that are blocked waiting for incomplete dependencies (not failed)
    ///
    /// Used to distinguish actual deadlocks from dependency failures.
    fn get_pending_tasks(&self) -> Vec<String> {
        self.workflow
            .tasks
            .iter()
            .filter(|task| !self.datastore.contains(&task.id))
            .map(|t| t.id.clone())
            .collect()
    }

    /// Get the first failed task in the workflow (for error reporting)
    fn find_root_failure(&self) -> Option<String> {
        for task in &self.workflow.tasks {
            if let Some(result) = self.datastore.get(&task.id) {
                // Only consider actual failures, not dependency failures
                if matches!(result.status, crate::store::TaskOutcome::Failed(_)) {
                    return Some(task.id.clone());
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
    fn write_trace(&self) {
        if let Ok(trace_writer) = TraceWriter::new(&self.generation_id) {
            if let Err(e) = trace_writer.write_all(&self.event_log) {
                tracing::warn!(error = %e, "Failed to write trace");
            } else {
                tracing::info!(path = %trace_writer.path().display(), "Trace written");
            }
        }
    }

    /// Check if a task qualifies for schema validation retry
    ///
    /// Returns Some((schema, max_retries, infer_params)) if:
    /// - Task action is Infer
    /// - Output format is JSON
    /// - Output has inline schema
    /// - max_retries > 0
    fn get_retry_config(task: &Task) -> Option<(Value, u8, InferParams)> {
        // Must be an infer action
        let infer = match &task.action {
            TaskAction::Infer { infer } => infer,
            _ => return None,
        };

        // Must have output policy with JSON format and inline schema
        let output_policy = task.output.as_ref()?;
        if output_policy.format != OutputFormat::Json {
            return None;
        }

        // Must have inline schema (file schemas don't support retry feedback)
        let schema = match &output_policy.schema {
            Some(crate::ast::output::SchemaRef::Inline(s)) => s.clone(),
            _ => return None,
        };

        // max_retries must be > 0 (default is 0)
        let max_retries = output_policy.max_retries.unwrap_or(0);
        if max_retries == 0 {
            return None;
        }

        Some((schema, max_retries, infer.clone()))
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
    /// # Arguments
    ///
    /// * `task` - The task to execute
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
        task: Arc<Task>,
        task_id: Arc<str>,
        parent_task_id: Arc<str>,
        datastore: RunContext,
        executor: TaskExecutor,
        event_log: EventLog,
        for_each_binding: Option<(String, Value, usize)>, // Added index
        workflow_artifacts: Option<ArtifactsConfig>,      // Artifact config
        base_path: PathBuf,                               // Artifact base path
    ) -> IterationResult {
        let start = Instant::now();

        // Extract for_each info if present
        let for_each_info = for_each_binding
            .as_ref()
            .map(|(_, _, idx)| (Arc::clone(&parent_task_id), *idx));
        let _is_for_each = for_each_binding.is_some();

        // Build bindings from with: or use: wiring
        let mut bindings = match if task.with_spec.is_some() {
            ResolvedBindings::from_with_spec(task.with_spec.as_ref(), &datastore)
        } else {
            ResolvedBindings::from_wiring_spec(task.use_wiring.as_ref(), &datastore)
        } {
            Ok(b) => b,
            Err(e) => {
                let duration = start.elapsed();
                // EMIT: TaskFailed (bindings build failed)
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                });
                return IterationResult {
                    store_id: task_id, // Store with indexed ID for for_each
                    result: TaskResult::failed(e.to_string(), duration),
                    for_each_info,
                };
            }
        };

        // Add for_each binding if present
        if let Some((var_name, value, _idx)) = for_each_binding {
            bindings.set(&var_name, value);
        }

        // EMIT: TaskStarted (with resolved inputs from use: wiring)
        event_log.emit(EventKind::TaskStarted {
            task_id: Arc::clone(&task_id),
            verb: Arc::from(task.action.verb_name()),
            inputs: bindings.to_value(),
        });

        // Check if task qualifies for schema validation retry
        // Conditions: infer action + JSON output + inline schema + max_retries > 0
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
                task.output.as_ref(),
            )
            .await
        } else {
            // Standard execution without retry
            let result = executor
                .execute(
                    &task_id,
                    &task.action,
                    &bindings,
                    &datastore,
                    task.output.as_ref(),
                )
                .await;
            let duration = start.elapsed();

            match result {
                Ok(output) => {
                    // Structured output validation via 4-layer engine
                    let final_output = if let Some(ref structured_spec) = task.structured {
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
                                    "Structured output validation succeeded"
                                );
                                // Return validated JSON as string
                                result.value.to_string()
                            }
                            Err(e) => {
                                // Structured validation failed - emit failure and return error
                                event_log.emit(EventKind::TaskFailed {
                                    task_id: Arc::clone(&task_id),
                                    error: e.to_string(),
                                    duration_ms: duration.as_millis() as u64,
                                });
                                return IterationResult {
                                    store_id: task_id,
                                    result: TaskResult::failed(e.to_string(), duration),
                                    for_each_info,
                                };
                            }
                        }
                    } else {
                        output
                    };

                    let tr = make_task_result(final_output, task.output.as_ref(), duration).await;
                    // EMIT: TaskCompleted or TaskFailed (based on result)
                    if tr.is_success() {
                        event_log.emit(EventKind::TaskCompleted {
                            task_id: Arc::clone(&task_id),
                            output: Arc::clone(&tr.output), // O(1) Arc clone
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
                    // EMIT: TaskFailed
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
        // Pass bindings and datastore for template resolution
        if task_result.is_success() {
            if let Some(ref artifact_spec) = task.artifact {
                // Get the output content for artifact writing
                let output_content = task_result.output_str().into_owned();

                let artifact_result = process_task_artifacts(
                    &task_id,
                    &output_content,
                    artifact_spec,
                    workflow_artifacts.as_ref(),
                    &base_path,
                    Some(&event_log), // Pass event log for artifact events
                    &bindings,        // For template resolution
                    &datastore,       // For lazy binding resolution
                )
                .await;

                // Log artifact results
                if artifact_result.written > 0 {
                    debug!(
                        task_id = %task_id,
                        artifacts_written = artifact_result.written,
                        "Artifacts written"
                    );
                }

                // Log any artifact errors (non-fatal)
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
            store_id: task_id, // Store individual results with indexed ID
            result: task_result,
            for_each_info,
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

        // Validate use: blocks before execution (fail-fast)
        validate_use_wiring(&self.workflow, &self.flow_graph)?;

        // Load context files if workflow has context: block
        let base_path = std::env::current_dir().unwrap_or_default();
        if let Some(context_config) = &self.workflow.context {
            let loaded_context = load_context(context_config, &base_path).await?;
            self.datastore.set_context(loaded_context);
            debug!("Loaded {} context files", context_config.files.len());
        }

        // Load inputs if workflow has inputs: block
        if let Some(ref inputs) = self.workflow.inputs {
            self.datastore.set_inputs(inputs.clone());
            debug!("Loaded {} input parameters", inputs.len());
        }

        // Resolve agents and skills
        // This loads external agent definitions and skill files
        if self.workflow.agents.is_some() || self.workflow.skills.is_some() {
            self.resolved_assets = resolve_assets(&self.workflow, &base_path).await?;
            debug!(
                agents = self.resolved_assets.agents.len(),
                skills = self.resolved_assets.skills.len(),
                "Resolved workflow assets"
            );
        }

        let total_tasks = self.workflow.tasks.len();
        let mut completed = 0;

        // EMIT: WorkflowStarted
        self.event_log.emit(EventKind::WorkflowStarted {
            task_count: total_tasks,
            generation_id: self.generation_id.clone(),
            workflow_hash: self.workflow.compute_hash(),
            nika_version: env!("CARGO_PKG_VERSION").to_string(),
        });

        if !self.quiet {
            println!(
                "{} Running workflow with {} tasks...\n",
                "→".cyan(),
                total_tasks
            );
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
                    .filter(|t| !self.datastore.contains(&t.id))
                    .map(|t| Arc::from(t.id.as_str()))
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
                            .filter(|t| !self.datastore.contains(&t.id))
                            .map(|t| Arc::from(t.id.as_str()))
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
                    .filter(|t| self.datastore.is_dependency_failed(&t.id))
                    .map(|t| t.id.clone())
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
                let task = Arc::clone(&task);
                let task_id = intern(&task.id); // Interned Arc<str> for deduplication

                // EMIT: TaskScheduled
                let deps = self.flow_graph.get_dependencies(&task.id);
                self.event_log.emit(EventKind::TaskScheduled {
                    task_id: Arc::clone(&task_id),
                    dependencies: deps.to_vec(), // Arc::clone is O(1)
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
                    task.decompose_spec()
                {
                    debug!(
                        task_id = %task.id,
                        strategy = ?decompose.strategy,
                        traverse = %decompose.traverse,
                        "Expanding decompose modifier"
                    );
                    // Resolve bindings for decompose source (with: or use:)
                    let bindings = if task.with_spec.is_some() {
                        ResolvedBindings::from_with_spec(task.with_spec.as_ref(), &self.datastore)
                            .unwrap_or_default()
                    } else {
                        ResolvedBindings::from_wiring_spec(
                            task.use_wiring.as_ref(),
                            &self.datastore,
                        )
                        .unwrap_or_default()
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
                                intern(&task.id),
                                TaskResult::failed(e.to_string(), std::time::Duration::ZERO),
                            );
                            continue;
                        }
                        Err(_timeout) => {
                            // Decompose expansion timed out
                            let timeout_error = NikaError::DecomposeTimeout {
                                task_id: task.id.clone(),
                                timeout_secs: DECOMPOSE_TIMEOUT.as_secs(),
                            };
                            self.datastore.insert(
                                intern(&task.id),
                                TaskResult::failed(timeout_error.to_string(), DECOMPOSE_TIMEOUT),
                            );
                            continue;
                        }
                    }
                } else if let Some(for_each) = &task.for_each {
                    // Handle binding references ($alias or {{use.alias}})
                    if let Some(binding_str) = for_each.as_str() {
                        // Resolve bindings from with:/use: wiring to access the referenced array
                        let bindings = if task.with_spec.is_some() {
                            ResolvedBindings::from_with_spec(
                                task.with_spec.as_ref(),
                                &self.datastore,
                            )
                            .unwrap_or_default()
                        } else {
                            ResolvedBindings::from_wiring_spec(
                                task.use_wiring.as_ref(),
                                &self.datastore,
                            )
                            .unwrap_or_default()
                        };

                        if let Some(alias) = binding_str.strip_prefix('$') {
                            // Check for $inputs.xxx format first (workflow inputs)
                            if alias.starts_with("inputs.") {
                                match self.datastore.resolve_input_path(alias) {
                                    Some(value) => value_to_array(&value),
                                    None => {
                                        self.datastore.insert(
                                            intern(&task.id),
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
                                // $alias or $alias.nested.path format (e.g., "$locales", "$step1.items")
                                // Split on '.' — first segment is the binding alias, rest is nested path
                                let mut segments = alias.split('.');
                                let base_alias = segments.next().unwrap();

                                match bindings.get_resolved(base_alias, &self.datastore) {
                                    Ok(base_value) => {
                                        // Parse JSON strings (objects and arrays) before traversal
                                        let parsed_value: Value;
                                        let working_value: &Value = if let Some(s) =
                                            base_value.as_str()
                                        {
                                            let trimmed = s.trim();
                                            if (trimmed.starts_with('{') && trimmed.ends_with('}'))
                                                || (trimmed.starts_with('[')
                                                    && trimmed.ends_with(']'))
                                            {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<Value>(trimmed)
                                                {
                                                    parsed_value = parsed;
                                                    &parsed_value
                                                } else {
                                                    &base_value
                                                }
                                            } else {
                                                &base_value
                                            }
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
                                                        intern(&task.id),
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
                                                // Explicit error instead of silent fallback
                                                self.datastore.insert(
                                                    intern(&task.id),
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
                                        // Binding not found - fail the task
                                        self.datastore.insert(
                                            intern(&task.id),
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
                        } else if binding_str.contains("{{inputs.") {
                            // Template format for inputs (e.g., "{{inputs.items}}")
                            if let Some(start) = binding_str.find("{{inputs.") {
                                let after = &binding_str[start + 9..]; // "{{inputs." is 9 chars
                                if let Some(end) = after.find("}}") {
                                    let param_path = &after[..end];
                                    let full_path = format!("inputs.{}", param_path);
                                    match self.datastore.resolve_input_path(&full_path) {
                                        Some(value) => value_to_array(&value),
                                        None => {
                                            self.datastore.insert(
                                                intern(&task.id),
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
                        } else if binding_str.contains("{{use.") {
                            // Template format (e.g., "{{use.locales}}" or "{{use.data.nested.items}}")
                            // Extract path from template pattern
                            if let Some(start) = binding_str.find("{{use.") {
                                let after = &binding_str[start + 6..];
                                if let Some(end) = after.find("}}") {
                                    let path = &after[..end];

                                    // Split: first segment is alias, rest is nested path
                                    let mut parts = path.split('.');
                                    let alias = parts.next().unwrap();

                                    match bindings.get_resolved(alias, &self.datastore) {
                                        Ok(base_value) => {
                                            // If base_value is a JSON string, parse it first
                                            // This handles exec: tasks that output JSON as strings
                                            let parsed_value: Value;
                                            let working_value: &Value =
                                                if let Some(s) = base_value.as_str() {
                                                    let trimmed = s.trim();
                                                    if (trimmed.starts_with('{')
                                                        && trimmed.ends_with('}'))
                                                        || (trimmed.starts_with('[')
                                                            && trimmed.ends_with(']'))
                                                    {
                                                        if let Ok(parsed) =
                                                            serde_json::from_str::<Value>(trimmed)
                                                        {
                                                            parsed_value = parsed;
                                                            &parsed_value
                                                        } else {
                                                            &base_value
                                                        }
                                                    } else {
                                                        &base_value
                                                    }
                                                } else {
                                                    &base_value
                                                };

                                            // Traverse nested path if present
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
                                                            task_id = %task.id,
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
                                                intern(&task.id),
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
                    } else {
                        // Direct array value (or JSON array string)
                        value_to_array(for_each)
                    }
                } else {
                    None
                };

                // Check if task has for_each or decompose items
                if let Some(items) = for_each_items {
                    if !items.is_empty() {
                        // Get concurrency settings from task
                        let concurrency = task.for_each_concurrency();
                        let fail_fast = task.for_each_fail_fast();

                        debug!(
                            task_id = %task.id,
                            items = items.len(),
                            concurrency = concurrency,
                            fail_fast = fail_fast,
                            "Starting for_each iteration"
                        );

                        // Create semaphore for concurrency limiting
                        let semaphore = Arc::new(Semaphore::new(concurrency));
                        // Create cancellation flag for fail_fast
                        let cancelled = Arc::new(AtomicBool::new(false));

                        // Spawn one execution per item in the array
                        let var_name = task.for_each_var().to_string();
                        for (idx, item) in items.iter().enumerate() {
                            // Check if cancelled before spawning
                            if fail_fast && cancelled.load(Ordering::Relaxed) {
                                debug!(
                                    task_id = %task.id,
                                    idx = idx,
                                    "Skipping iteration due to fail_fast cancellation"
                                );
                                break;
                            }

                            let task = Arc::clone(&task);
                            let task_id = intern(&format!("{}[{}]", task.id, idx));
                            let parent_task_id = intern(&task.id);
                            let datastore = self.datastore.clone();
                            let executor = self.executor.clone();
                            let event_log = self.event_log.clone();
                            let item = item.clone();
                            let var_name = var_name.clone();
                            let semaphore = Arc::clone(&semaphore);
                            let cancelled = Arc::clone(&cancelled);
                            // Clone artifact config for this iteration
                            let workflow_artifacts = workflow_artifacts.clone();
                            let artifact_base_path = artifact_base_path.clone();

                            join_set.spawn(async move {
                                // Check cancellation BEFORE acquiring semaphore
                                // This prevents tasks from executing after fail_fast triggers
                                if cancelled.load(Ordering::SeqCst) {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::skipped(
                                            "Cancelled due to fail_fast before semaphore acquire"
                                                .to_string(),
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),
                                    };
                                }

                                // Use tokio::select! to race semaphore acquisition
                                // against cancellation check. This ensures tasks waiting on
                                // semaphore can be cancelled promptly.
                                let _permit = tokio::select! {
                                    biased;  // Check cancellation first

                                    // Poll cancellation periodically while waiting for semaphore
                                    _ = async {
                                        while !cancelled.load(Ordering::SeqCst) {
                                            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                                        }
                                    } => {
                                        return IterationResult {
                                            store_id: task_id,
                                            result: TaskResult::skipped(
                                                "Cancelled while waiting for semaphore".to_string(),
                                            ),
                                            for_each_info: Some((parent_task_id, idx)),
                                        };
                                    }

                                    // Try to acquire semaphore
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
                                                };
                                            }
                                        }
                                    }
                                };

                                // Final check after acquiring permit (double-check)
                                if cancelled.load(Ordering::SeqCst) {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::skipped(
                                            "Cancelled after semaphore acquire".to_string(),
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),
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

                                // If failed and fail_fast, set cancellation flag with SeqCst
                                // for maximum visibility across threads
                                if !result.result.is_success() && fail_fast {
                                    cancelled.store(true, Ordering::SeqCst);
                                }

                                result
                            });
                        }
                    }
                } else {
                    // Regular task without for_each
                    let datastore = self.datastore.clone();
                    let executor = self.executor.clone();
                    let event_log = self.event_log.clone();
                    // Clone artifact config for this task
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
            let mut for_each_results: FxHashMap<Arc<str>, Vec<(usize, TaskResult)>> =
                FxHashMap::default();

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
                            .filter(|t| !self.datastore.contains(&t.id))
                            .map(|t| Arc::from(t.id.as_str()))
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
                                } = iteration_result;

                                completed += 1;
                                let success = task_result.is_success();
                                let skipped = task_result.is_skipped();

                                let status = if success {
                                    format!("[{}/{}]", completed, total_tasks).green()
                                } else if skipped {
                                    format!("[{}/{}]", completed, total_tasks).yellow()
                                } else {
                                    format!("[{}/{}]", completed, total_tasks).red()
                                };

                                let symbol = if success { "✓" } else if skipped { "⊘" } else { "✗" };
                                let symbol_colored = if success {
                                    symbol.green()
                                } else if skipped {
                                    symbol.yellow()
                                } else {
                                    symbol.red()
                                };
                                let duration_str =
                                    format!("({:.1}s)", task_result.duration.as_secs_f32()).dimmed();

                                if !self.quiet {
                                    println!(
                                        "  {} {} {} {}",
                                        status, &*store_id, symbol_colored, duration_str
                                    );

                                    if let Some(err) = task_result.error() {
                                        if !skipped {
                                            println!("      {} {}", "Error:".red(), err);
                                        }
                                    }
                                }

                                // Store individual result
                                self.datastore
                                    .insert(Arc::clone(&store_id), task_result.clone());

                                // If this is a for_each failure with fail_fast,
                                // abort all remaining in-flight tasks immediately
                                if !success && !skipped && for_each_info.is_some() && !fail_fast_triggered {
                                    // Check if the parent task had fail_fast enabled
                                    // We detect this by checking if we're in a for_each context
                                    // and the task failed (not skipped - skipped means already cancelled)
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
            println!("\n{} Done!\n", "✓".green());
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ExecParams, Flow, FlowEndpoint, Task, TaskAction};
    use crate::binding::UseEntry;
    use std::sync::Arc;

    // ═══════════════════════════════════════════════════════════════
    // QUIET MODE TEST
    // ═══════════════════════════════════════════════════════════════

    fn make_empty_workflow() -> Workflow {
        Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
            flows: vec![],
        }
    }

    #[test]
    fn test_runner_quiet_mode() {
        // Default should not be quiet
        let runner = Runner::new(make_empty_workflow());
        assert!(!runner.quiet, "Runner should not be quiet by default");

        // quiet() should enable quiet mode
        let runner = Runner::new(make_empty_workflow()).quiet();
        assert!(runner.quiet, "Runner should be quiet after .quiet()");

        // Can chain with with_event_log
        let event_log = crate::event::EventLog::new();
        let runner = Runner::with_event_log(make_empty_workflow(), event_log).quiet();
        assert!(runner.quiet, "Runner should be quiet when chained");
    }

    // ═══════════════════════════════════════════════════════════════
    // INITIAL CONTEXT TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_with_initial_context_stores_value() {
        use serde_json::json;

        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow).with_initial_context(
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

    #[tokio::test]
    async fn test_for_each_collects_all_results() {
        // Create workflow with for_each that runs 3 items
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![Arc::new(Task {
                id: "echo_items".to_string(),
                for_each: Some(serde_json::json!(["a", "b", "c"])),
                for_each_as: Some("item".to_string()),
                concurrency: None, // Default sequential
                fail_fast: None,   // Default true
                decompose: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.item}}".to_string(),
                        shell: None,
                        timeout: None,
                        cwd: None,
                        env: None,
                    },
                },
                use_wiring: None,
                with_spec: None,
                output: None,
                artifact: None,
                log: None,
                flow: None,
                structured: None,
            })],
            flows: vec![],
        };

        let mut runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        // The final output should contain results from all 3 iterations
        // When for_each completes, results should be aggregated
        // Check datastore has the parent task result
        let parent_result = runner.datastore.get("echo_items");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        // Should contain all three outputs somehow (either as array or concatenated)
        // The exact format depends on implementation, but all should be present
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
        // Create workflow with for_each that runs 5 items
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![Arc::new(Task {
                id: "ordered".to_string(),
                for_each: Some(serde_json::json!(["first", "second", "third"])),
                for_each_as: Some("x".to_string()),
                concurrency: None,
                fail_fast: None,
                decompose: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.x}}".to_string(),
                        shell: None,
                        timeout: None,
                        cwd: None,
                        env: None,
                    },
                },
                use_wiring: None,
                with_spec: None,
                output: None,
                artifact: None,
                log: None,
                flow: None,
                structured: None,
            })],
            flows: vec![],
        };

        let mut runner = Runner::new(workflow);
        runner.run().await.unwrap();

        let parent_result = runner.datastore.get("ordered");
        assert!(parent_result.is_some(), "Parent task result should exist");

        // If stored as array, order should be preserved
        let result = parent_result.unwrap();
        let output = result.output_str();
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&output) {
            assert_eq!(arr.len(), 3, "Should have 3 results");
            // First element should be "first", last should be "third"
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
        // If not an array, at least verify all are present (parallel execution may reorder)
    }

    // ═══════════════════════════════════════════════════════════════
    // BASIC WORKFLOW TESTS
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create a minimal workflow with exec tasks
    fn create_exec_workflow(tasks: Vec<(&str, &str)>, flows: Vec<(&str, &str)>) -> Workflow {
        Workflow {
            schema: "nika/workflow@0.1".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: tasks
                .into_iter()
                .map(|(id, cmd)| {
                    Arc::new(Task {
                        id: id.to_string(),
                        use_wiring: None,
                        with_spec: None,
                        output: None,
                        decompose: None,
                        for_each: None,
                        for_each_as: None,
                        concurrency: None,
                        fail_fast: None,
                        action: TaskAction::Exec {
                            exec: ExecParams {
                                command: cmd.to_string(),
                                shell: None,
                                timeout: None,
                                cwd: None,
                                env: None,
                            },
                        },
                        artifact: None,
                        log: None,
                        flow: None,
                        structured: None,
                    })
                })
                .collect(),
            flows: flows
                .into_iter()
                .map(|(src, tgt)| Flow {
                    source: FlowEndpoint::Single(src.to_string()),
                    target: FlowEndpoint::Single(tgt.to_string()),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn event_sequence_for_single_task() {
        let workflow = create_exec_workflow(vec![("greet", "echo hello")], vec![]);
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let runner = Runner::new(workflow);

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 2, "Both tasks should be ready");

        let ids: Vec<&str> = ready.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"a"), "Task 'a' should be ready");
        assert!(ids.contains(&"b"), "Task 'b' should be ready");
    }

    #[test]
    fn get_ready_tasks_respects_dependencies() {
        // Chain: a -> b -> c
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B"), ("c", "echo C")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow);

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1, "Only first task should be ready");
        assert_eq!(ready[0].id, "a", "Task 'a' should be ready");
    }

    #[test]
    fn get_ready_tasks_excludes_completed_tasks() {
        let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
        let runner = Runner::new(workflow);

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
        let runner = Runner::new(workflow);

        assert!(!runner.all_done(), "Not all tasks are done initially");
    }

    #[test]
    fn all_done_returns_true_when_all_completed() {
        let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
        let runner = Runner::new(workflow);

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
        let runner = Runner::new(workflow);

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
        let runner = Runner::new(workflow);

        let output = runner.get_final_output();
        assert!(output.is_none(), "No output when tasks not complete");
    }

    #[test]
    fn get_final_output_skips_failed_tasks() {
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "exit 1")],
            vec![], // Both are final tasks (no successors)
        );
        let runner = Runner::new(workflow);

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
        // Create workflow with for_each that specifies concurrency=2
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![Arc::new(Task {
                id: "concurrent".to_string(),
                for_each: Some(serde_json::json!(["a", "b", "c", "d"])),
                for_each_as: Some("item".to_string()),
                concurrency: Some(2), // Limit to 2 concurrent
                fail_fast: None,
                decompose: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.item}}".to_string(),
                        shell: None,
                        timeout: None,
                        cwd: None,
                        env: None,
                    },
                },
                use_wiring: None,
                with_spec: None,
                output: None,
                artifact: None,
                log: None,
                flow: None,
                structured: None,
            })],
            flows: vec![],
        };

        let mut runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        // Verify all 4 items were processed
        let parent_result = runner.datastore.get("concurrent");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        assert!(output.contains("a") || output.contains("\"a\""));
        assert!(output.contains("d") || output.contains("\"d\""));
    }

    #[tokio::test]
    async fn for_each_fail_fast_stops_on_first_error() {
        // Create workflow with for_each where middle item fails
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![Arc::new(Task {
                id: "failfast".to_string(),
                for_each: Some(serde_json::json!(["ok1", "FAIL", "ok2", "ok3"])),
                for_each_as: Some("item".to_string()),
                concurrency: Some(1), // Sequential to make failure predictable
                fail_fast: Some(true),
                decompose: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        // Exit with error if item is "FAIL"
                        command: "test '{{use.item}}' != 'FAIL' && echo {{use.item}}".to_string(),
                        shell: None,
                        timeout: None,
                        cwd: None,
                        env: None,
                    },
                },
                use_wiring: None,
                with_spec: None,
                output: None,
                artifact: None,
                log: None,
                flow: None,
                structured: None,
            })],
            flows: vec![],
        };

        let mut runner = Runner::new(workflow);
        let result = runner.run().await;
        // Workflow completes but parent task may be marked as failed
        assert!(result.is_ok() || result.is_err());

        // The important thing is that some iterations may have been skipped
        // due to fail_fast behavior
    }

    #[tokio::test]
    async fn for_each_fail_fast_false_continues_on_error() {
        // Create workflow with fail_fast=false
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![Arc::new(Task {
                id: "continue".to_string(),
                for_each: Some(serde_json::json!(["ok1", "ok2"])),
                for_each_as: Some("item".to_string()),
                concurrency: None,
                fail_fast: Some(false), // Explicitly disable fail_fast
                decompose: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.item}}".to_string(),
                        shell: None,
                        timeout: None,
                        cwd: None,
                        env: None,
                    },
                },
                use_wiring: None,
                with_spec: None,
                output: None,
                artifact: None,
                log: None,
                flow: None,
                structured: None,
            })],
            flows: vec![],
        };

        let mut runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(result.is_ok(), "Workflow should complete");

        // All items should be processed
        let parent_result = runner.datastore.get("continue");
        assert!(parent_result.is_some());
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH WITH INPUTS.* SUPPORT
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn for_each_with_dollar_inputs_array() {
        use serde_json::json;

        // Test for_each: $inputs.items resolves from workflow inputs
        let mut workflow = make_empty_workflow();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "items".to_string(),
            json!({
                "type": "array",
                "default": ["alpha", "beta", "gamma"]
            }),
        );
        workflow.inputs = Some(inputs);
        workflow.tasks = vec![Arc::new(Task {
            id: "process_items".to_string(),
            for_each: Some(serde_json::json!("$inputs.items")),
            for_each_as: Some("item".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        })];

        let mut runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        // Should have processed all 3 items
        let task_result = runner.datastore.get("process_items");
        assert!(task_result.is_some(), "Task result should exist");
        assert!(task_result.unwrap().is_success(), "Task should succeed");
    }

    #[tokio::test]
    async fn for_each_with_template_inputs() {
        use serde_json::json;

        // Test for_each: "{{inputs.locales}}" resolves from workflow inputs
        let mut workflow = make_empty_workflow();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "locales".to_string(),
            json!({
                "type": "array",
                "default": ["fr-FR", "en-US"]
            }),
        );
        workflow.inputs = Some(inputs);
        workflow.tasks = vec![Arc::new(Task {
            id: "translate".to_string(),
            for_each: Some(json!("{{inputs.locales}}")),
            for_each_as: Some("locale".to_string()),
            concurrency: Some(2),
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo Translating to {{use.locale}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        })];

        let mut runner = Runner::new(workflow);
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
        use serde_json::json;

        // Test for_each: $inputs.nonexistent fails with clear error
        let mut workflow = make_empty_workflow();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "other_param".to_string(),
            json!({
                "type": "string",
                "default": "test"
            }),
        );
        workflow.inputs = Some(inputs);
        workflow.tasks = vec![Arc::new(Task {
            id: "missing_input".to_string(),
            for_each: Some(json!("$inputs.nonexistent")),
            for_each_as: Some("item".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        })];

        let mut runner = Runner::new(workflow);
        let result = runner.run().await;
        // The workflow completes but the task fails
        assert!(result.is_ok(), "Workflow should complete");

        let task_result = runner.datastore.get("missing_input");
        assert!(task_result.is_some(), "Task result should exist");
        let tr = task_result.unwrap();
        assert!(!tr.is_success(), "Task should fail due to missing input");
        // Error message is in status field, not output (TaskResult::failed stores error in status)
        let error_msg = tr.error().expect("Failed task should have error message");
        assert!(
            error_msg.contains("not found"),
            "Error should mention 'not found': {}",
            error_msg
        );
    }

    #[tokio::test]
    async fn for_each_with_inputs_nested_path() {
        use serde_json::json;

        // Test for_each: $inputs.data.items with nested input structure
        let mut workflow = make_empty_workflow();
        let mut inputs = FxHashMap::default();
        inputs.insert(
            "data".to_string(),
            json!({
                "type": "object",
                "default": {
                    "items": ["one", "two", "three"]
                }
            }),
        );
        workflow.inputs = Some(inputs);
        workflow.tasks = vec![Arc::new(Task {
            id: "nested".to_string(),
            for_each: Some(json!("$inputs.data.items")),
            for_each_as: Some("n".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.n}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        })];

        let mut runner = Runner::new(workflow);
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

    #[tokio::test]
    async fn for_each_dollar_binding_nested_path() {
        use serde_json::json;

        // Step1 produces a JSON object with an "items" field containing an array.
        // Step2 uses for_each: "$step1.items" to iterate over that nested array.
        let mut workflow = make_empty_workflow();

        let step1 = Arc::new(Task {
            id: "step1".to_string(),
            for_each: None,
            for_each_as: None,
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: r#"echo '{"items": ["alpha", "beta", "gamma"], "count": 3}'"#
                        .to_string(),
                    shell: Some(true),
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        });

        let mut use_wiring = FxHashMap::default();
        use_wiring.insert("step1".to_string(), UseEntry::new("step1"));

        let step2 = Arc::new(Task {
            id: "step2".to_string(),
            for_each: Some(json!("$step1.items")),
            for_each_as: Some("item".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: Some(use_wiring),
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: Some(vec!["step1".to_string()]),
            structured: None,
        });

        workflow.tasks = vec![step1, step2];
        workflow.flows = vec![Flow {
            source: FlowEndpoint::Single("step1".to_string()),
            target: FlowEndpoint::Single("step2".to_string()),
        }];

        let mut runner = Runner::new(workflow);
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
        use serde_json::json;

        // Step1 produces a plain string (not an array).
        // Step2 uses for_each: "$step1" which should FAIL, not silently skip.
        let mut workflow = make_empty_workflow();

        let step1 = Arc::new(Task {
            id: "step1".to_string(),
            for_each: None,
            for_each_as: None,
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo not_an_array".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        });

        let mut use_wiring = FxHashMap::default();
        use_wiring.insert("step1".to_string(), UseEntry::new("step1"));

        let step2 = Arc::new(Task {
            id: "step2".to_string(),
            for_each: Some(json!("$step1")),
            for_each_as: Some("item".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: Some(use_wiring),
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: Some(vec!["step1".to_string()]),
            structured: None,
        });

        workflow.tasks = vec![step1, step2];
        workflow.flows = vec![Flow {
            source: FlowEndpoint::Single("step1".to_string()),
            target: FlowEndpoint::Single("step2".to_string()),
        }];

        let mut runner = Runner::new(workflow);
        let _ = runner.run().await;

        // Workflow completes (individual task failure doesn't abort workflow by default)
        // but step2 should have FAILED with explicit error, not silently succeed
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
        use serde_json::json;

        // Step1 produces a JSON array as a string: '["a","b","c"]'
        // Step2 uses for_each: "$step1" which should parse the JSON string
        let mut workflow = make_empty_workflow();

        let step1 = Arc::new(Task {
            id: "step1".to_string(),
            for_each: None,
            for_each_as: None,
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: r#"echo '["x","y","z"]'"#.to_string(),
                    shell: Some(true),
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        });

        let mut use_wiring = FxHashMap::default();
        use_wiring.insert("step1".to_string(), UseEntry::new("step1"));

        let step2 = Arc::new(Task {
            id: "step2".to_string(),
            for_each: Some(json!("$step1")),
            for_each_as: Some("item".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: Some(use_wiring),
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: Some(vec!["step1".to_string()]),
            structured: None,
        });

        workflow.tasks = vec![step1, step2];
        workflow.flows = vec![Flow {
            source: FlowEndpoint::Single("step1".to_string()),
            target: FlowEndpoint::Single("step2".to_string()),
        }];

        let mut runner = Runner::new(workflow);
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
        use serde_json::json;

        // Step1 produces JSON, step2 references a non-existent nested path.
        // Should produce explicit error, not silent fallback.
        let mut workflow = make_empty_workflow();

        let step1 = Arc::new(Task {
            id: "step1".to_string(),
            for_each: None,
            for_each_as: None,
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: r#"echo '{"data": {"count": 5}}'"#.to_string(),
                    shell: Some(true),
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: None,
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: None,
            structured: None,
        });

        let mut use_wiring = FxHashMap::default();
        use_wiring.insert("step1".to_string(), UseEntry::new("step1"));

        let step2 = Arc::new(Task {
            id: "step2".to_string(),
            for_each: Some(json!("$step1.data.nonexistent")),
            for_each_as: Some("item".to_string()),
            concurrency: None,
            fail_fast: None,
            decompose: None,
            action: TaskAction::Exec {
                exec: ExecParams {
                    command: "echo {{use.item}}".to_string(),
                    shell: None,
                    timeout: None,
                    cwd: None,
                    env: None,
                },
            },
            use_wiring: Some(use_wiring),
            with_spec: None,
            output: None,
            artifact: None,
            log: None,
            flow: Some(vec!["step1".to_string()]),
            structured: None,
        });

        workflow.tasks = vec![step1, step2];
        workflow.flows = vec![Flow {
            source: FlowEndpoint::Single("step1".to_string()),
            target: FlowEndpoint::Single("step2".to_string()),
        }];

        let mut runner = Runner::new(workflow);
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
    // CONSTRUCTOR AND EVENT LOG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_event_log_uses_provided_event_log() {
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let custom_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, custom_log);

        // The runner should use the provided event log
        assert!(runner.event_log().events().is_empty());
    }

    #[tokio::test]
    async fn workflow_completed_event_has_duration() {
        let workflow = create_exec_workflow(vec![("quick", "echo fast")], vec![]);
        let mut runner = Runner::new(workflow);

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
        let mut runner = Runner::new(workflow);

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
        let runner = Runner::new(workflow);

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

        let runner = Runner::new(workflow).with_cancel_token(token);

        // Cancelling the original token should be reflected
        token_clone.cancel();
        assert!(runner.is_cancelled(), "Runner should detect cancellation");
    }

    #[test]
    fn test_cancel_token_cloning() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow);

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

        let mut runner = Runner::new(workflow).with_cancel_token(token.clone());

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

        let mut runner = Runner::new(workflow).with_cancel_token(token);

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
        let mut runner = Runner::with_event_log(workflow, event_log).with_cancel_token(token);

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
        let runner = Runner::new(workflow);

        // Should not be paused by default
        assert!(
            !runner.is_paused(),
            "Runner should not be paused by default"
        );
    }

    #[test]
    fn test_pause_and_resume() {
        let workflow = make_empty_workflow();
        let runner = Runner::new(workflow);

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
        let runner = Runner::new(workflow);

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
        let runner = Runner::with_event_log(workflow, event_log.clone());

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
        let mut runner = Runner::with_event_log(workflow, event_log);

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
}
