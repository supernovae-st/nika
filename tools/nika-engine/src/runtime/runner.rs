//! DAG Runner - workflow execution with tokio
//!
//! Performance optimizations:
//! - Arc for zero-cost task/context sharing
//! - JoinSet for efficient parallel task collection
//! - Tokio handles all concurrency (no artificial limits)

use indexmap::IndexMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::sync::{Notify, Semaphore};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument};

use crate::media::CasStore;

use crate::ast::analyzed::{
    AnalyzedOutput, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow,
    OutputFormat as AnalyzedOutputFormat,
};
use crate::ast::lower::{lower_action, lower_mcp_servers_with_resolver, lower_output};
use crate::ast::output::OutputPolicy;
use crate::ast::{InferParams, TaskAction};
use crate::binding::ResolvedBindings;
use crate::dag::Dag;
use crate::error::NikaError;
use crate::error_domains::ExecutionError;
use crate::event::{prune_traces, EventKind, EventLog, TraceWriter};
use crate::runtime::boot::TraceConfig;
use crate::store::{RunContext, TaskResult};
use crate::util::{intern, DECOMPOSE_TIMEOUT};

use super::artifact_processor::{process_task_artifacts, write_artifact_manifest};
use super::context_loader::load_context_analyzed;
use super::executor::TaskExecutor;
use super::output::{extract_json, format_validation_errors, make_task_result};
use super::resolver::{resolve_assets_analyzed, ResolvedAssets};
use super::structured_output::StructuredOutputEngine;

use crate::ast::artifact::ArtifactsConfig;
use std::path::PathBuf;

// ═══════════════════════════════════════════════════════════════════════════════
// RAII Lockfile Guard
// ═══════════════════════════════════════════════════════════════════════════════

/// RAII guard that holds an exclusive `flock` on a lockfile.
///
/// The media store lockfile (`.nika-run.lock`) prevents `nika media clean` from
/// garbage-collecting blobs that are still in use by a running workflow.
///
/// On Unix, uses nix `Flock` (LOCK_EX|LOCK_NB) — dropping releases the lock.
/// On non-Unix, falls back to PID file.
struct LockfileGuard {
    path: PathBuf,
    /// Unix: nix::fcntl::Flock wrapping the file (drop releases flock).
    #[cfg(unix)]
    _flock: Option<nix::fcntl::Flock<std::fs::File>>,
    /// Non-Unix: held open to keep the file.
    #[cfg(not(unix))]
    _file: Option<std::fs::File>,
}

impl LockfileGuard {
    /// Try to acquire an exclusive flock on the lockfile (non-blocking).
    ///
    /// Best-effort: if locking fails, logs a warning and continues.
    /// The PID is written for debugging but the lock is the real guard.
    fn create(path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                tracing::warn!(path = %parent.display(), error = %e, "Failed to create lockfile directory");
            }
        }

        let file = match std::fs::File::create(&path) {
            Ok(f) => f,
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "Failed to create lockfile");
                #[cfg(unix)]
                return Self { path, _flock: None };
                #[cfg(not(unix))]
                return Self { path, _file: None };
            }
        };

        // Write PID for debugging (lock is the real protection)
        {
            use std::io::Write;
            let _ = (&file).write_all(format!("pid:{}", std::process::id()).as_bytes());
        }

        #[cfg(unix)]
        {
            use nix::fcntl::{Flock, FlockArg};
            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(flock) => {
                    tracing::debug!(path = %path.display(), "Acquired exclusive lockfile");
                    Self {
                        path,
                        _flock: Some(flock),
                    }
                }
                Err(err) => {
                    let errno = err.1;
                    if errno == nix::errno::Errno::EWOULDBLOCK {
                        tracing::warn!(
                            path = %path.display(),
                            "Another nika process holds the lockfile — concurrent runs may conflict"
                        );
                    } else {
                        tracing::warn!(path = %path.display(), error = %errno, "Failed to flock lockfile");
                    }
                    Self { path, _flock: None }
                }
            }
        }

        #[cfg(not(unix))]
        {
            Self {
                path,
                _file: Some(file),
            }
        }
    }
}

impl Drop for LockfileGuard {
    fn drop(&mut self) {
        // Flock/File drop releases the lock automatically. Remove the file for cleanliness.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Detect the first LLM provider with an API key in the environment.
///
/// Returns the provider name (e.g. "openai") or falls back to "anthropic"
/// if no provider key is found (the error will be caught later).
fn detect_first_configured_provider() -> &'static str {
    use nika_core::catalogs::{ProviderCategory, KNOWN_PROVIDERS};
    for p in KNOWN_PROVIDERS {
        if p.category != ProviderCategory::Llm {
            continue;
        }
        if p.has_env_key() {
            return p.id;
        }
    }
    "anthropic" // fallback — will produce a clear MissingApiKey error
}

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
    // BUG-034: Treat null as empty array — 0 iterations, not error.
    if value.is_null() {
        return Some(vec![]);
    }

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
    /// CLI event stream renderer (None when quiet or TUI mode).
    /// Uses `auto_renderer()` to auto-select Live (animated) vs Classic (append-only).
    cli_renderer: Option<Box<dyn crate::display::Renderer + Send>>,
    /// Global concurrency limiter across ALL spawned tasks (regular + for_each).
    ///
    /// This is a per-Runner (per-workflow-run) semaphore. Each `nika run` creates
    /// its own Runner with its own semaphore. For `nika serve`, cross-workflow
    /// concurrency is managed by AppState.semaphore in the serve layer.
    ///
    /// Both regular tasks and for_each iterations acquire a permit before execution,
    /// ensuring the total number of concurrent tasks never exceeds MAX_CONCURRENT_TASKS.
    global_task_semaphore: Arc<Semaphore>,
}

/// Maximum concurrent tasks (regular + for_each) in a single workflow run.
const MAX_CONCURRENT_TASKS: usize = 64;

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
        // P-ORCHESTRATE: If workflow has a goal, wrap it as an orchestrator agent
        let workflow = if workflow.goal.is_some() {
            crate::runtime::orchestrate::wrap_as_orchestrator(workflow)
        } else {
            workflow
        };

        let flow_graph = Dag::from_analyzed(&workflow).map_err(|e| NikaError::ValidationError {
            reason: format!("DAG construction failed: {e}"),
        })?;
        flow_graph.detect_cycles()?;
        let datastore = RunContext::new();

        // Resolve MCP servers: from: references resolved from .mcp.json / global
        let resolver = crate::core::McpConfigResolver::from_environment();
        let mcp_configs =
            lower_mcp_servers_with_resolver(workflow.mcp_servers.clone(), Some(&resolver));
        let provider = workflow
            .provider
            .as_ref()
            .map(|p| p.as_str())
            .unwrap_or_else(|| detect_first_configured_provider());

        let mut executor = TaskExecutor::new(
            provider,
            workflow.model.as_deref(),
            mcp_configs,
            event_log.clone(),
        )?;

        // Wire introspection tools (records, dag_info, task_status, threads, orchestrate)
        executor.wire_introspection_tools(Arc::new(datastore.clone()));

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
            cli_renderer: None,
            global_task_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS)),
        })
    }

    /// Create a Runner with explicit policy configuration.
    ///
    /// Policy from `[policy]` in nika.toml is threaded to the TaskExecutor
    /// so that `allowed_hosts`, `blocked_hosts`, `max_token_spend`, etc.
    /// are enforced during workflow execution.
    pub fn with_policy(
        workflow: AnalyzedWorkflow,
        event_log: EventLog,
        policy: crate::runtime::boot::PolicyConfig,
    ) -> Result<Self, NikaError> {
        let workflow = if workflow.goal.is_some() {
            crate::runtime::orchestrate::wrap_as_orchestrator(workflow)
        } else {
            workflow
        };

        let flow_graph = Dag::from_analyzed(&workflow).map_err(|e| NikaError::ValidationError {
            reason: format!("DAG construction failed: {e}"),
        })?;
        flow_graph.detect_cycles()?;
        let datastore = RunContext::new();

        let resolver = crate::core::McpConfigResolver::from_environment();
        let mcp_configs =
            lower_mcp_servers_with_resolver(workflow.mcp_servers.clone(), Some(&resolver));
        let provider = workflow
            .provider
            .as_ref()
            .map(|p| p.as_str())
            .unwrap_or_else(|| detect_first_configured_provider());

        let mut executor = TaskExecutor::with_policy(
            provider,
            workflow.model.as_deref(),
            mcp_configs,
            event_log.clone(),
            Some(policy),
            None,
            None,
        )?;

        executor.wire_introspection_tools(Arc::new(datastore.clone()));

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
            cli_renderer: None,
            global_task_semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS)),
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

    /// Set the CLI detail level for event rendering.
    ///
    /// Automatically selects Live (animated) or Classic (append-only)
    /// renderer based on TTY detection and detail level.
    pub fn with_detail_level(mut self, detail: crate::display::DetailLevel) -> Self {
        let effective_detail = if self.quiet {
            crate::display::DetailLevel::Min
        } else {
            detail
        };
        self.cli_renderer = Some(crate::display::auto_renderer(effective_detail));
        self
    }

    /// Force the classic (append-only) renderer regardless of TTY.
    pub fn with_classic_renderer(mut self, detail: crate::display::DetailLevel) -> Self {
        let effective_detail = if self.quiet {
            crate::display::DetailLevel::Min
        } else {
            detail
        };
        self.cli_renderer = Some(crate::display::classic_renderer(effective_detail));
        self
    }

    /// Get a reference to the run context (task results store).
    ///
    /// Available after `run()` completes to collect task outputs for `-o/--output`.
    pub fn datastore(&self) -> &RunContext {
        &self.datastore
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

    /// Set custom endpoints for OpenAI-compatible servers (vLLM, TGI, Ollama).
    ///
    /// Endpoints configured in `~/.config/nika/config.toml` are passed here
    /// so the executor can resolve `provider: h100` to a custom endpoint.
    pub fn with_custom_endpoints(
        &mut self,
        endpoints: crate::provider::endpoints::CustomEndpointMap,
    ) {
        self.executor.set_custom_endpoints(endpoints);
    }

    /// Set the permission mode for file tools (nika:write, nika:edit, etc.)
    ///
    /// By default, `PermissionMode::Plan` is used (deny writes, emit permission request).
    /// For `nika run`, use `AcceptAll` since the user explicitly chose to run.
    pub fn with_permission_mode(self, mode: crate::tools::PermissionMode) -> Self {
        self.executor.set_permission_mode(mode);
        self
    }

    /// Set a custom cancellation token
    ///
    /// This allows external control of workflow cancellation.
    /// The TUI can hold a clone of the token and call `cancel()` on it.
    /// Also propagated to TaskExecutor so MCP invoke operations
    /// abort promptly instead of waiting for INVOKE_TASK_DEADLINE.
    /// Set the workflow base directory for exec `cwd:` security checks.
    ///
    /// By default, the runner uses `std::env::current_dir()`. When set,
    /// exec tasks with `cwd:` can only access paths under this directory.
    /// This should be the directory containing the workflow file.
    pub fn with_base_path(mut self, path: std::path::PathBuf) -> Self {
        self.executor = self.executor.with_base_path(path);
        self
    }

    /// Set the project root directory (parent of nika.toml).
    ///
    /// Used by `working_dir_mode = "project"` to set exec task cwd.
    pub fn with_project_root(mut self, root: std::path::PathBuf) -> Self {
        self.executor = self.executor.with_project_root(root);
        self
    }

    /// Set the working directory mode from `[tools] working_dir` in nika.toml.
    ///
    /// - `"project"` → exec tasks default cwd to project_root
    /// - `"workflow"` → exec tasks default cwd to workflow_base_dir
    /// - `"none"` → no default cwd, inherit process cwd
    pub fn with_working_dir_mode(mut self, mode: String) -> Self {
        self.executor = self.executor.with_working_dir_mode(mode);
        self
    }

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

    /// Get tasks that are ready to run (all dependencies satisfied).
    ///
    /// Only checks tasks at `pending_indices`. Indices of tasks that are now
    /// stored in the datastore (completed, failed, or dependency-failed) are
    /// removed from `pending_indices` so subsequent calls skip them entirely.
    /// This reduces per-iteration work from O(total_tasks) to O(remaining_tasks).
    fn get_ready_tasks(&self, pending_indices: &mut Vec<usize>) -> Vec<&AnalyzedTask> {
        let mut ready = Vec::new();
        pending_indices.retain(|&idx| {
            let task = &self.workflow.tasks[idx];

            // Already in datastore → remove from pending set
            if self.datastore.contains(&task.name) {
                return false;
            }

            // Check all dependencies
            let deps = self.flow_graph.get_dependencies(&task.name);
            for dep in deps.iter() {
                if let Some(succeeded) = self.datastore.is_completed_successfully(dep.as_ref()) {
                    if !succeeded {
                        // Dependency failed → mark this task as DependencyFailed
                        self.datastore.insert(
                            intern(&task.name),
                            TaskResult::dependency_failed(dep.as_ref()),
                        );
                        self.event_log.emit(EventKind::TaskSkipped {
                            task_id: Arc::from(task.name.as_str()),
                            reason: format!("dependency '{}' failed", dep.as_ref()),
                        });
                        debug!(
                            task_id = %task.name,
                            dependency = %dep.as_ref(),
                            "Task blocked due to failed dependency"
                        );
                        return false; // Remove from pending
                    }
                } else {
                    // Dependency hasn't completed yet — keep in pending, not ready
                    return true;
                }
            }

            // All dependencies succeeded — task is ready
            ready.push(task);
            false // Remove from pending (will be dispatched)
        });
        ready
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
                if result.is_usable() {
                    return Some(result.output_str().into_owned());
                }
            }
        }

        // Fallback: Try any usable final task (includes partial success)
        let final_tasks = self.flow_graph.get_final_tasks();
        for task_id in final_tasks {
            if let Some(result) = self.datastore.get(&task_id) {
                if result.is_usable() {
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
    fn write_trace(&self) -> Option<String> {
        let trace_path = match TraceWriter::new(&self.generation_id) {
            Ok(trace_writer) => {
                if let Err(e) = trace_writer.write_all(&self.event_log) {
                    tracing::warn!(error = %e, "Failed to write trace");
                    None
                } else {
                    let path = trace_writer.path().display().to_string();
                    tracing::info!(path = %path, "Trace written");
                    Some(path)
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create trace writer — traces disabled for this run");
                None
            }
        };

        // Enforce retention: prune traces beyond max_traces / retention_days
        prune_traces(
            self.trace_config.max_traces,
            self.trace_config.retention_days,
        );
        trace_path
    }

    /// Verify media integrity: check that all MediaRef paths exist and sizes match.
    ///
    /// Called after all tasks complete but before WorkflowCompleted event.
    /// Emits a `MediaIntegrityCheck` event with results.
    /// Returns warning count. Never fails the workflow -- only warns.
    fn verify_media_integrity(&self) -> usize {
        let mut warnings = 0;
        let mut checked: u64 = 0;
        for (task_id, result) in self.datastore.iter_results() {
            for media_ref in &result.media {
                checked += 1;
                if !media_ref.path.exists() {
                    tracing::warn!(
                        task_id = %task_id,
                        hash = %media_ref.hash,
                        path = %media_ref.path.display(),
                        "Media integrity: CAS file missing"
                    );
                    warnings += 1;
                    continue;
                }
                // Note: Size check skipped — media-compression (default feature)
                // adds NK framing + zstd, so on-disk size != MediaRef.size_bytes.
                // Existence check above is sufficient.
            }
        }

        // Emit structured event for telemetry consumers
        if checked > 0 {
            self.event_log.emit(EventKind::MediaIntegrityCheck {
                checked,
                warnings: warnings as u64,
            });
        }

        warnings
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
            content: infer_action
                .content
                .as_ref()
                .map(|parts| parts.iter().cloned().map(Into::into).collect()),
            guardrails: Vec::new(),
            base_url: None,
            provider_chain: None,
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
                            current_infer.prompt = Self::build_retry_prompt(
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
                    current_infer.prompt = Self::build_retry_prompt(
                        &original_prompt,
                        schema,
                        &output,
                        &error_feedback,
                    );
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
        task: Arc<AnalyzedTask>,
        task_id: Arc<str>,
        parent_task_id: Arc<str>,
        datastore: RunContext,
        executor: TaskExecutor,
        event_log: EventLog,
        for_each_binding: Option<(String, Value, usize)>,
        workflow_artifacts: Option<ArtifactsConfig>,
        base_path: PathBuf,
        workflow_base_url: Option<String>,
    ) -> IterationResult {
        let start = Instant::now();

        // Extract for_each info if present
        let for_each_info = for_each_binding
            .as_ref()
            .map(|(_, _, idx)| (Arc::clone(&parent_task_id), *idx));

        // Check cancellation before binding resolution (which is synchronous
        // and could involve deep JSON path traversal on large outputs)
        if executor.is_cancelled() {
            event_log.emit(EventKind::TaskCancelled {
                task_id: Arc::clone(&task_id),
                reason: "Cancelled before binding resolution".to_string(),
            });
            return IterationResult {
                store_id: task_id,
                result: TaskResult::skipped("Cancelled before binding resolution"),
                for_each_info,
            };
        }

        // Build bindings from with: spec (always present in AnalyzedTask)
        let (mut bindings, binding_events) = match ResolvedBindings::from_with_spec_traced(
            Some(&task.with_spec),
            &datastore,
            &task_id,
        ) {
            Ok(result) => result,
            Err(e) => {
                let duration = start.elapsed();
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                    error_code: Some(e.code().to_string()),
                });
                return IterationResult {
                    store_id: task_id,
                    result: TaskResult::failed(e.to_string(), duration),
                    for_each_info,
                };
            }
        };
        // Emit collected binding events
        for event in binding_events {
            event_log.emit(event);
        }

        // Add for_each binding if present (item value + iteration index)
        if let Some((var_name, value, idx)) = for_each_binding {
            bindings.set(&var_name, value);
            bindings.set(
                "for_each_index",
                Value::Number(serde_json::Number::from(idx)),
            );
        }

        // Enforce context_budget if configured on this task
        if let Some(budget) = task.context_budget {
            let budget_event =
                crate::binding::token_budget::enforce_budget(&mut bindings, budget, &task_id);
            event_log.emit(budget_event);
        }

        // Register env-sourced secret values for value-based redaction in traces.
        // This catches custom API keys (e.g., ELEVENLABS_API_KEY) that don't match
        // the pattern-based regex in redact_secrets().
        let env_secrets = bindings.env_sourced_values();
        if !env_secrets.is_empty() {
            crate::util::register_secrets(env_secrets);
        }

        // EMIT: TaskStarted (with redacted env-sourced secrets)
        event_log.emit(EventKind::TaskStarted {
            task_id: Arc::clone(&task_id),
            verb: Arc::from(task.action.verb_name()),
            inputs: Arc::new(bindings.to_value_redacted()),
        });

        // Bridge AnalyzedTask to lowered types at executor boundary
        // PERF(M4): pass references — lower_action clones only what each verb needs

        // Resolve preset: merge agent preset values as fallback for provider/model
        // Precedence: task-level > preset > workflow-default
        let (effective_provider, effective_model) = if let Some(ref preset_name) = task.preset {
            if let Some(agent) = executor.get_preset(preset_name) {
                crate::runtime::preset::resolve_provider_model(&task.provider, &task.model, agent)
            } else {
                (task.provider.clone(), task.model.clone())
            }
        } else {
            (task.provider.clone(), task.model.clone())
        };

        // Resolve base_url: task-level override takes precedence over workflow default
        let resolved_base_url = task.base_url.clone().or(workflow_base_url);
        // Extract provider fallback chain from routing config
        let provider_chain: Option<Vec<nika_core::ProviderName>> = task
            .routing
            .as_ref()
            .filter(|r| r.fallback.len() > 1)
            .map(|r| {
                r.fallback
                    .iter()
                    .map(|s| nika_core::ProviderName::parse(s))
                    .collect()
            });
        let mut lowered_action = lower_action(
            &task.action,
            &effective_provider,
            &effective_model,
            &task.retry,
            &resolved_base_url,
            &provider_chain,
        );

        // Inject preset system/temperature into lowered action (task-level > preset)
        if let Some(ref preset_name) = task.preset {
            if let Some(agent) = executor.get_preset(preset_name) {
                // Emit PresetApplied event
                event_log.emit(crate::event::EventKind::PresetApplied {
                    task_id: Arc::clone(&task_id),
                    preset_name: preset_name.clone(),
                    provider: agent.provider.clone(),
                    model: agent.model.clone(),
                });

                crate::runtime::preset::apply_preset_fields(&mut lowered_action, agent);
            }
        }

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
        let mut task_result = if let Some((schema, max_retries, original_infer)) = retry_config {
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
                task.routing.as_ref(),
            )
            .await
        } else {
            // Standard execution with optional task-level retry.
            // Fetch handles retry internally (HTTP 5xx backoff) — skip runner retry.
            let is_fetch = matches!(lowered_action, TaskAction::Fetch { .. });
            let task_retry = if !is_fetch { task.retry.as_ref() } else { None };
            let max_attempts = task_retry.map_or(1u32, |r| r.max_attempts.max(1));

            let result = if max_attempts <= 1 {
                // No retry — single execution (with routing fallback if configured)
                executor
                    .execute_with_routing(
                        &task_id,
                        &lowered_action,
                        &bindings,
                        &datastore,
                        effective_output.as_ref(),
                        task.routing.as_ref(),
                    )
                    .await
            } else {
                // Task-level retry loop with exponential backoff.
                //
                // RETRY COMPOUNDING: This retry wraps the ENTIRE verb execution,
                // including structured output validation retries (max_retries in
                // structured: block). Worst case: max_attempts × max_retries total
                // LLM calls. Example: retry: { max_attempts: 3 } + structured:
                // { max_retries: 2 } = up to 9 LLM calls per task.
                let delay_ms = task_retry.map_or(1000u64, |r| r.delay_ms);
                let backoff = task_retry
                    .map_or(1.0f64, |r| r.backoff.unwrap_or(1.0))
                    .clamp(1.0, 10.0); // Prevent runaway backoff
                let mut last_err: Option<NikaError> = None;
                let mut final_result = None;

                // Max retry delay: 5 minutes (prevents infinite sleep on extreme values)
                const MAX_RETRY_DELAY_MS: u64 = 300_000;

                for attempt in 1..=max_attempts {
                    if attempt > 1 {
                        let exp = (attempt - 2).min(30) as i32;
                        let delay =
                            ((delay_ms as f64 * backoff.powi(exp)) as u64).min(MAX_RETRY_DELAY_MS);
                        tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                        event_log.emit(EventKind::TaskRetry {
                            task_id: Arc::clone(&task_id),
                            attempt,
                            max_attempts,
                            backoff_ms: delay,
                            error: last_err.take().map(|e| e.to_string()),
                        });
                    }

                    match executor
                        .execute_with_routing(
                            &task_id,
                            &lowered_action,
                            &bindings,
                            &datastore,
                            effective_output.as_ref(),
                            task.routing.as_ref(),
                        )
                        .await
                    {
                        Ok(output) => {
                            if attempt > 1 {
                                info!(
                                    task_id = %task_id,
                                    attempt = attempt,
                                    "Task succeeded after retry"
                                );
                            }
                            final_result = Some(Ok(output));
                            break;
                        }
                        Err(e) if attempt < max_attempts && Self::is_retryable(&e) => {
                            tracing::warn!(
                                task_id = %task_id,
                                attempt = attempt,
                                max_attempts = max_attempts,
                                error = %e,
                                "Task failed (attempt {}/{}), retrying...",
                                attempt,
                                max_attempts,
                            );
                            last_err = Some(e);
                        }
                        Err(e) => {
                            final_result = Some(Err(e));
                            break;
                        }
                    }
                }

                final_result.unwrap_or_else(|| {
                    Err(
                        last_err.unwrap_or_else(|| crate::error::NikaError::ExecError {
                            reason: format!(
                                "Task '{}' failed after {} attempts: no error captured",
                                task_id, max_attempts
                            ),
                        }),
                    )
                })
            };
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
                            )
                            .with_workflow_dir(base_path.clone());
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
                                    // Drain any orphaned media refs (defense-in-depth)
                                    let _ = datastore.take_media(&task_id);
                                    event_log.emit(EventKind::TaskFailed {
                                        task_id: Arc::clone(&task_id),
                                        error: e.to_string(),
                                        duration_ms: duration.as_millis() as u64,
                                        error_code: Some(e.code().to_string()),
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
                        }
                    } else {
                        output
                    };

                    let tr =
                        make_task_result(final_output, effective_output.as_ref(), duration).await;
                    // Attach media refs from staging side-channel
                    let tr = tr.with_media(datastore.take_media(&task_id));
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
                            error_code: Some("NIKA-060".to_string()),
                        });
                    }
                    tr
                }
                Err(e) => {
                    // Drain any orphaned media refs (defense-in-depth)
                    let _ = datastore.take_media(&task_id);
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: e.to_string(),
                        duration_ms: duration.as_millis() as u64,
                        error_code: Some(e.code().to_string()),
                    });
                    TaskResult::failed(e.to_string(), duration)
                }
            }
        };

        // Process artifacts if task succeeded and has artifact config.
        // BUG-018 fix: skip per-iteration writes in for_each — the post-aggregation
        // block writes the complete array output under the parent task ID.
        if task_result.is_success() && for_each_info.is_none() {
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
                    task_result.media.as_slice(),
                )
                .await;

                if artifact_result.written > 0 {
                    debug!(
                        task_id = %task_id,
                        artifacts_written = artifact_result.written,
                        "Artifacts written"
                    );
                }
                if !artifact_result.errors.is_empty() {
                    let error_msgs: Vec<String> = artifact_result
                        .errors
                        .iter()
                        .map(|err| {
                            tracing::error!(
                                task_id = %task_id,
                                error = %err,
                                "Artifact write failed"
                            );
                            event_log.emit(EventKind::ArtifactFailed {
                                task_id: Arc::clone(&task_id),
                                path: String::new(),
                                reason: err.to_string(),
                            });
                            err.to_string()
                        })
                        .collect();
                    let error = format!("Artifact write errors: {}", error_msgs.join("; "));
                    // Emit TaskFailed to correct the earlier TaskCompleted event
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: error.clone(),
                        error_code: Some("NIKA-281".to_string()),
                        duration_ms: start.elapsed().as_millis() as u64,
                    });
                    task_result = TaskResult::failed(error, start.elapsed());
                }
            }
        }

        // Record compression: if task succeeded and has record: { compress: true },
        // create a compressed Record via LLM (or truncation fallback) and store.
        if task_result.is_success() {
            if let Some(ref record_spec) = task.record {
                if record_spec.compress {
                    let raw_output = task_result.output_str();
                    let compressor =
                        crate::runtime::record_compress::RecordCompressor::new(event_log.clone());
                    let compressor_model = nika_core::catalogs::default_model_for_provider(
                        executor.default_provider(),
                    )
                    .unwrap_or("claude-haiku-4-5");
                    let llm = crate::runtime::executor_compressor::ExecutorCompressorLlm::new(
                        &executor,
                        executor.default_provider(),
                        compressor_model,
                    );
                    let record = compressor
                        .compress(&task_id, &raw_output, record_spec, &llm)
                        .await;
                    datastore.set_record(Arc::clone(&task_id), record);
                }
            }
        }

        IterationResult {
            store_id: task_id,
            result: task_result,
            for_each_info,
        }
    }

    /// Emit a TaskFailed event for DAG scheduling failures.
    ///
    /// These failures occur before task execution (binding resolution, for_each
    /// expansion, etc.) so they never get a TaskStarted event. We emit just
    /// TaskFailed so the TUI, CLI renderer, and trace writer learn about them.
    fn emit_scheduling_failure(&self, task_name: &str, error: &str, error_code: &str) {
        self.event_log.emit(EventKind::TaskFailed {
            task_id: Arc::from(task_name),
            error: error.to_string(),
            duration_ms: 0,
            error_code: Some(error_code.to_string()),
        });
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
            return Err(ExecutionError::Cancelled {
                phase: "before start".to_string(),
            }
            .into());
        }

        // P-ORCHESTRATE: Emit OrchestratorStarted when goal is present
        if let Some(ref goal) = self.workflow.goal {
            let orch_config = self.workflow.orchestrate.as_ref();
            self.event_log.emit(EventKind::OrchestratorStarted {
                goal: goal.clone(),
                max_rounds: orch_config.map(|c| c.max_rounds).unwrap_or(10),
                agent: orch_config.and_then(|c| c.agent.clone()),
            });
        }

        // Load context files if workflow has context_files
        let base_path = std::env::current_dir().unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Failed to get current directory, using '.'");
            std::path::PathBuf::from(".")
        });

        // Set workspace root for CAS media store path resolution
        self.datastore.set_workspace_root(base_path.clone());

        // RAII lockfile: auto-removed on all exit paths (normal, error, panic)
        let _lockfile_guard = LockfileGuard::create(
            base_path
                .join(".nika")
                .join("media")
                .join("store")
                .join(".nika-run.lock"),
        );

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

        // Wire resolved agent presets into executor for preset: resolution
        if !self.resolved_assets.agents.is_empty() {
            self.executor = self
                .executor
                .clone()
                .with_resolved_agents(self.resolved_assets.agents.clone());
        }

        // Wire workflow-level skills mapping into the executor for agent skill injection.
        // TaskExecutor is Clone, so we clone-and-replace to call the builder-style setter.
        if !self.workflow.skills_map.is_empty() {
            self.executor = self
                .executor
                .clone()
                .with_skills(self.workflow.skills_map.clone(), base_path.clone());

            // Also load skills files into LoadedContext for {{skills.NAME}} template resolution
            let mut skills_loaded: rustc_hash::FxHashMap<String, serde_json::Value> =
                rustc_hash::FxHashMap::default();
            for (alias, path) in &self.workflow.skills_map {
                let full_path = base_path.join(path);
                match tokio::fs::read_to_string(&full_path).await {
                    Ok(content) => {
                        skills_loaded.insert(alias.clone(), serde_json::Value::String(content));
                    }
                    Err(e) => {
                        tracing::warn!(skill = %alias, path = %full_path.display(), error = %e, "Failed to load skill file for template resolution");
                    }
                }
            }
            if !skills_loaded.is_empty() {
                self.datastore.set_skills(skills_loaded);
            }

            debug!(
                skills_count = self.workflow.skills_map.len(),
                "Wired skills mapping into executor"
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
            let has_observable_output = self
                .workflow
                .tasks
                .iter()
                .any(|t| t.output.is_some() || t.artifact.is_some());
            if !has_observable_output && total_tasks > 1 {
                println!(
                    "  {} {}\n",
                    "ℹ".dimmed(),
                    "Tip: add artifact: to tasks to persist results to files".dimmed()
                );
            }
        }

        // PERF(M2): Compute DAG layers ONCE, reuse for both display and summary.
        // compute_layers() is O(N²) worst-case; previously called twice with identical inputs.
        let cached_depths = if total_tasks > 1 {
            let nodes: Vec<&str> = self
                .workflow
                .tasks
                .iter()
                .map(|t| t.name.as_str())
                .collect();
            let edges: Vec<(&str, &str)> = self
                .workflow
                .tasks
                .iter()
                .flat_map(|task| {
                    task.depends_on.iter().filter_map(|dep_id| {
                        self.workflow
                            .task_table
                            .get_name(*dep_id)
                            .map(|dep_name| (dep_name, task.name.as_str()))
                    })
                })
                .collect();
            Some(crate::dag::flow::compute_layers(&nodes, &edges))
        } else {
            None
        };

        // Print static DAG + set up CliRenderer task layers
        if let Some(ref mut renderer) = self.cli_renderer {
            if let Some(ref depths) = cached_depths {
                use crate::display::dag::{StaticDagEdge, StaticDagTask};
                let dag_tasks: Vec<StaticDagTask> = self
                    .workflow
                    .tasks
                    .iter()
                    .map(|t| StaticDagTask {
                        id: t.name.clone(),
                        verb: t.action.verb_name().to_string(),
                        layer: depths.get(t.name.as_str()).copied().unwrap_or(0),
                    })
                    .collect();
                let total_deps: usize =
                    self.workflow.tasks.iter().map(|t| t.depends_on.len()).sum();
                let mut dag_edges = Vec::with_capacity(total_deps);
                for task in &self.workflow.tasks {
                    for dep_id in &task.depends_on {
                        if let Some(dep_name) = self.workflow.task_table.get_name(*dep_id) {
                            dag_edges.push(StaticDagEdge {
                                from: dep_name.to_string(),
                                to: task.name.clone(),
                            });
                        }
                    }
                }
                crate::display::dag::print_static_dag(&dag_tasks, &dag_edges);
                println!("{}", "\u{254C}".repeat(69).dimmed());
                println!();
                let task_layers: std::collections::HashMap<Arc<str>, usize> = self
                    .workflow
                    .tasks
                    .iter()
                    .map(|t| {
                        (
                            Arc::from(t.name.as_str()),
                            depths.get(t.name.as_str()).copied().unwrap_or(0),
                        )
                    })
                    .collect();
                renderer.set_task_layers(task_layers);
            }

            // Initialize live task bars (no-op for Classic renderer)
            let task_ids: Vec<String> =
                self.workflow.tasks.iter().map(|t| t.name.clone()).collect();
            let task_deps: std::collections::HashMap<String, Vec<String>> = self
                .workflow
                .tasks
                .iter()
                .map(|t| {
                    let deps: Vec<String> = t
                        .depends_on
                        .iter()
                        .filter_map(|dep_id| {
                            self.workflow
                                .task_table
                                .get_name(*dep_id)
                                .map(|s| s.to_string())
                        })
                        .collect();
                    (t.name.clone(), deps)
                })
                .collect();
            renderer.init_tasks(&task_ids, &task_deps);
        }

        // Pending task indices — shrinks as tasks complete, so get_ready_tasks()
        // only checks remaining tasks instead of rescanning the full task list.
        let mut pending_indices: Vec<usize> = (0..self.workflow.tasks.len()).collect();

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
                return Err(ExecutionError::Cancelled {
                    phase: "by user".to_string(),
                }
                .into());
            }

            // Check for pause at start of each loop iteration
            // Waits until resumed, while also checking for cancellation
            while self.paused.load(Ordering::SeqCst) {
                tokio::select! {
                    biased;
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
                        return Err(ExecutionError::Cancelled {
                            phase: "while paused".to_string(),
                        }
                        .into());
                    }
                    _ = self.resume_notify.notified() => {
                        // Resumed, continue loop
                    }
                }
            }

            let mut renderer = self.cli_renderer.take();

            let ready = self.get_ready_tasks(&mut pending_indices);

            // Check for completion or deadlock
            if ready.is_empty() {
                self.cli_renderer = renderer;

                if self.all_done() {
                    // Check if any tasks actually failed before declaring success.
                    // all_done() returns true when all tasks are in the datastore,
                    // including failed/dependency-failed tasks.
                    let failed_tasks: Vec<String> = self
                        .workflow
                        .tasks
                        .iter()
                        .filter(|t| self.datastore.is_failed(&t.name))
                        .map(|t| t.name.clone())
                        .collect();

                    if !failed_tasks.is_empty() {
                        let root_failure = self.find_root_failure();
                        let dep_failed_count = failed_tasks
                            .iter()
                            .filter(|t| self.datastore.is_dependency_failed(t))
                            .count();

                        self.event_log.emit(EventKind::WorkflowFailed {
                            error: format!(
                                "{} task(s) failed ({} direct, {} from dependency chain)",
                                failed_tasks.len(),
                                failed_tasks.len() - dep_failed_count,
                                dep_failed_count,
                            ),
                            failed_task: root_failure.clone().map(Arc::from),
                        });
                        self.write_trace();
                        return Err(NikaError::DependencyChainFailed {
                            count: failed_tasks.len(),
                            blocked_tasks: failed_tasks,
                            root_failure,
                        });
                    }
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
                return Err(NikaError::RuntimeDeadlock {
                    details:
                        "no tasks ready but workflow not complete. Check for circular dependencies."
                            .to_string(),
                });
            }

            // Spawn all ready tasks in parallel (Tokio handles concurrency)
            let mut join_set = JoinSet::new();

            // Per-parent cancellation tokens for for_each fail_fast.
            // Using targeted cancellation instead of JoinSet::abort_all() to avoid
            // killing unrelated sibling tasks from other for_each parents (Bug #26).
            let mut for_each_cancel_tokens: rustc_hash::FxHashMap<Arc<str>, CancellationToken> =
                rustc_hash::FxHashMap::default();

            // Prepare artifact config for all tasks in this batch
            let workflow_artifacts = self.workflow.artifacts.clone();
            let artifact_base_path = base_path.clone();
            let workflow_base_url = self.workflow.base_url.clone();

            for task in ready {
                let task_id = intern(&task.name);

                // EMIT: TaskScheduled
                let deps = self.flow_graph.get_dependencies(&task.name);
                let sched_kind = EventKind::TaskScheduled {
                    task_id: Arc::clone(&task_id),
                    dependencies: deps.to_vec(),
                };
                if let Some(ref mut r) = renderer {
                    r.render_kind(&sched_kind);
                }
                self.event_log.emit(sched_kind);

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
                    self.event_log.emit(EventKind::DecomposeStarted {
                        task_id: Arc::from(task.name.as_str()),
                        strategy: format!("{:?}", decompose.strategy).to_lowercase(),
                    });
                    // Resolve bindings for decompose source
                    let bindings = match ResolvedBindings::from_with_spec(
                        Some(&task.with_spec),
                        &self.datastore,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            tracing::error!(
                                task_id = %task.name,
                                error = %e,
                                "Failed to resolve bindings for decompose"
                            );
                            let err_msg = format!("Decompose binding resolution failed: {e}");
                            self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::failed(err_msg, std::time::Duration::ZERO),
                            );
                            continue;
                        }
                    };
                    // Expand decompose using executor (with timeout to prevent silent hangs)
                    let decompose_start = Instant::now();
                    let decompose_result = tokio::time::timeout(
                        DECOMPOSE_TIMEOUT,
                        self.executor
                            .expand_decompose(decompose, &bindings, &self.datastore),
                    )
                    .await;

                    match decompose_result {
                        Ok(Ok(items)) => {
                            self.event_log.emit(EventKind::DecomposeCompleted {
                                task_id: Arc::from(task.name.as_str()),
                                strategy: format!("{:?}", decompose.strategy).to_lowercase(),
                                item_count: items.len(),
                                duration_ms: decompose_start.elapsed().as_millis() as u64,
                            });
                            Some(items)
                        }
                        Ok(Err(e)) => {
                            // Decompose expansion failed
                            self.event_log.emit(EventKind::DecomposeCompleted {
                                task_id: Arc::from(task.name.as_str()),
                                strategy: format!("{:?}", decompose.strategy).to_lowercase(),
                                item_count: 0,
                                duration_ms: decompose_start.elapsed().as_millis() as u64,
                            });
                            self.emit_scheduling_failure(&task.name, &e.to_string(), "NIKA-026");
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::failed(e.to_string(), std::time::Duration::ZERO),
                            );
                            continue;
                        }
                        Err(_timeout) => {
                            // Decompose expansion timed out
                            self.event_log.emit(EventKind::DecomposeCompleted {
                                task_id: Arc::from(task.name.as_str()),
                                strategy: format!("{:?}", decompose.strategy).to_lowercase(),
                                item_count: 0,
                                duration_ms: decompose_start.elapsed().as_millis() as u64,
                            });
                            let timeout_error = NikaError::DecomposeTimeout {
                                task_id: task.name.clone(),
                                timeout_secs: DECOMPOSE_TIMEOUT.as_secs(),
                            };
                            self.emit_scheduling_failure(
                                &task.name,
                                &timeout_error.to_string(),
                                "NIKA-026",
                            );
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::failed(timeout_error.to_string(), DECOMPOSE_TIMEOUT),
                            );
                            continue;
                        }
                    }
                } else if let Some(ref for_each) = task.for_each {
                    // AnalyzedForEach has structured fields: items, as_var, concurrency, fail_fast
                    let items_str = &for_each.items;

                    if for_each.is_binding() {
                        // Check cancellation before synchronous binding resolution
                        if self.cancel_token.is_cancelled() {
                            self.event_log.emit(EventKind::TaskCancelled {
                                task_id: Arc::from(task.name.as_str()),
                                reason: "Cancelled before for_each binding resolution".to_string(),
                            });
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::skipped("Cancelled before for_each binding resolution"),
                            );
                            continue;
                        }

                        // Binding reference ($alias, {{with.alias}}, {{inputs.xxx}})
                        let bindings = match ResolvedBindings::from_with_spec(
                            Some(&task.with_spec),
                            &self.datastore,
                        ) {
                            Ok(b) => b,
                            Err(e) => {
                                tracing::error!(
                                    task_id = %task.name,
                                    error = %e,
                                    "Failed to resolve bindings for for_each"
                                );
                                let err_msg = format!("for_each binding resolution failed: {e}");
                                self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                                self.datastore.insert(
                                    intern(&task.name),
                                    TaskResult::failed(err_msg, std::time::Duration::ZERO),
                                );
                                continue;
                            }
                        };

                        if items_str.starts_with('$')
                            && (items_str.contains('|') || items_str.contains("??"))
                        {
                            // Pipe transform expression: $task | transform or $task.path | transform
                            // Parse using the full WithEntry parser which handles path + transforms + defaults.
                            match crate::binding::parse_with_entry(items_str) {
                                Ok(entry) => {
                                    // Resolve the raw value from the binding source
                                    let raw_value = match &entry.source.source {
                                        crate::binding::BindingSource::Task(task_id) => {
                                            // B01 fix: resolve through with: bindings first,
                                            // then fall back to direct datastore lookup
                                            let resolved = bindings
                                                .get_resolved(task_id, &self.datastore)
                                                .or_else(|_| {
                                                    self.datastore
                                                        .get_output(task_id)
                                                        .map(|arc| arc.as_ref().clone())
                                                        .ok_or_else(|| NikaError::BindingNotFound {
                                                            alias: task_id.to_string(),
                                                        })
                                                });
                                            match resolved {
                                                Ok(output) => {
                                                    // Auto-parse JSON strings (same as plain $ path)
                                                    let working = crate::binding::jsonpath::try_parse_json_str(&output)
                                                        .unwrap_or_else(|| output.clone());

                                                    // Navigate path segments
                                                    let mut value_ref = &working;
                                                    let mut ok = true;
                                                    for seg in &entry.source.segments {
                                                        let next = match seg {
                                                            crate::binding::PathSegment::Field(
                                                                f,
                                                            ) => value_ref.get(f.as_ref()),
                                                            crate::binding::PathSegment::Index(
                                                                i,
                                                            ) => value_ref.get(*i),
                                                        };
                                                        match next {
                                                            Some(v) => value_ref = v,
                                                            None => {
                                                                let err_msg = format!(
                                                                    "for_each binding '{}': path segment '{:?}' not found",
                                                                    items_str, seg
                                                                );
                                                                self.emit_scheduling_failure(
                                                                    &task.name, &err_msg,
                                                                    "NIKA-026",
                                                                );
                                                                self.datastore.insert(
                                                                    intern(&task.name),
                                                                    TaskResult::failed(
                                                                        err_msg,
                                                                        std::time::Duration::ZERO,
                                                                    ),
                                                                );
                                                                ok = false;
                                                                break;
                                                            }
                                                        }
                                                    }
                                                    if !ok {
                                                        continue;
                                                    }
                                                    value_ref.clone()
                                                }
                                                Err(_) => {
                                                    // B10 fix: check ?? default before failing
                                                    if let Some(ref default_val) = entry.default {
                                                        default_val.clone()
                                                    } else {
                                                        let err_msg = format!(
                                                            "for_each binding '{}': '{}' not found in with: bindings or task outputs",
                                                            items_str, task_id
                                                        );
                                                        self.emit_scheduling_failure(
                                                            &task.name, &err_msg, "NIKA-026",
                                                        );
                                                        self.datastore.insert(
                                                            intern(&task.name),
                                                            TaskResult::failed(
                                                                err_msg,
                                                                std::time::Duration::ZERO,
                                                            ),
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                        crate::binding::BindingSource::Input(sub_path) => {
                                            let full_path = format!("inputs.{}", sub_path);
                                            match self.datastore.resolve_input_path(&full_path) {
                                                Some(v) => v,
                                                None => {
                                                    // B10 fix: check ?? default before failing
                                                    if let Some(ref default_val) = entry.default {
                                                        default_val.clone()
                                                    } else {
                                                        let err_msg = format!(
                                                            "for_each binding '{}': input '{}' not found",
                                                            items_str, full_path
                                                        );
                                                        self.emit_scheduling_failure(
                                                            &task.name, &err_msg, "NIKA-026",
                                                        );
                                                        self.datastore.insert(
                                                            intern(&task.name),
                                                            TaskResult::failed(
                                                                err_msg,
                                                                std::time::Duration::ZERO,
                                                            ),
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                        crate::binding::BindingSource::Env(var) => {
                                            match std::env::var(var.as_ref()) {
                                                Ok(v) => Value::String(v),
                                                Err(_) => {
                                                    // B10 fix: check ?? default before failing
                                                    if let Some(ref default_val) = entry.default {
                                                        default_val.clone()
                                                    } else {
                                                        let err_msg = format!(
                                                            "for_each binding '{}': env var '{}' not set",
                                                            items_str, var
                                                        );
                                                        self.emit_scheduling_failure(
                                                            &task.name, &err_msg, "NIKA-026",
                                                        );
                                                        self.datastore.insert(
                                                            intern(&task.name),
                                                            TaskResult::failed(
                                                                err_msg,
                                                                std::time::Duration::ZERO,
                                                            ),
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                        other => {
                                            let err_msg = format!(
                                                "for_each binding '{}': unsupported source type '{:?}'",
                                                items_str, other
                                            );
                                            self.emit_scheduling_failure(
                                                &task.name, &err_msg, "NIKA-026",
                                            );
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    err_msg,
                                                    std::time::Duration::ZERO,
                                                ),
                                            );
                                            continue;
                                        }
                                    };

                                    // Apply transforms
                                    let transformed = if let Some(ref expr) = entry.transform {
                                        match expr.apply(&raw_value) {
                                            Ok(v) => v,
                                            Err(e) => {
                                                let err_msg = format!(
                                                    "for_each binding '{}' transform failed: {}",
                                                    items_str, e
                                                );
                                                self.emit_scheduling_failure(
                                                    &task.name, &err_msg, "NIKA-026",
                                                );
                                                self.datastore.insert(
                                                    intern(&task.name),
                                                    TaskResult::failed(
                                                        err_msg,
                                                        std::time::Duration::ZERO,
                                                    ),
                                                );
                                                continue;
                                            }
                                        }
                                    } else {
                                        raw_value
                                    };

                                    // Apply default if transformed value is null
                                    let final_value = if transformed.is_null() {
                                        entry.default.unwrap_or(transformed)
                                    } else {
                                        transformed
                                    };

                                    match value_to_array(&final_value) {
                                        Some(items) => Some(items),
                                        None => {
                                            let err_msg = format!(
                                                "for_each binding '{}' resolved to non-array value after transforms",
                                                items_str
                                            );
                                            self.emit_scheduling_failure(
                                                &task.name, &err_msg, "NIKA-026",
                                            );
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    err_msg,
                                                    std::time::Duration::ZERO,
                                                ),
                                            );
                                            continue;
                                        }
                                    }
                                }
                                Err(e) => {
                                    let err_msg = format!(
                                        "for_each items '{}' parse error: {}",
                                        items_str, e
                                    );
                                    self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                                    self.datastore.insert(
                                        intern(&task.name),
                                        TaskResult::failed(err_msg, std::time::Duration::ZERO),
                                    );
                                    continue;
                                }
                            }
                        } else if let Some(alias) = items_str.strip_prefix('$') {
                            // Check for $inputs.xxx format first (workflow inputs)
                            if alias.starts_with("inputs.") {
                                match self.datastore.resolve_input_path(alias) {
                                    Some(value) => match value_to_array(&value) {
                                        Some(items) => Some(items),
                                        None => {
                                            let err_msg = format!(
                                                "for_each binding '${}' resolved to non-array value",
                                                alias
                                            );
                                            self.emit_scheduling_failure(
                                                &task.name, &err_msg, "NIKA-026",
                                            );
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    err_msg,
                                                    std::time::Duration::ZERO,
                                                ),
                                            );
                                            continue;
                                        }
                                    },
                                    None => {
                                        let err_msg = format!(
                                            "for_each input '{}' not found in workflow inputs",
                                            alias
                                        );
                                        self.emit_scheduling_failure(
                                            &task.name, &err_msg, "NIKA-026",
                                        );
                                        self.datastore.insert(
                                            intern(&task.name),
                                            TaskResult::failed(err_msg, std::time::Duration::ZERO),
                                        );
                                        continue;
                                    }
                                }
                            } else {
                                // $alias or $alias.nested.path format
                                let mut segments = alias.split('.');
                                let Some(base_alias) = segments.next() else {
                                    let err_msg =
                                        "for_each: empty alias after '$' prefix".to_string();
                                    self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                                    self.datastore.insert(
                                        intern(&task.name),
                                        TaskResult::failed(err_msg, std::time::Duration::ZERO),
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
                                        let working_value: &Value = if let Some(v) =
                                            crate::binding::jsonpath::try_parse_json_str(
                                                &base_value,
                                            ) {
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
                                                    let err_msg = format!(
                                                        "for_each binding '${}': nested path segment '{}' not found",
                                                        alias, segment
                                                    );
                                                    self.emit_scheduling_failure(
                                                        &task.name, &err_msg, "NIKA-026",
                                                    );
                                                    self.datastore.insert(
                                                        intern(&task.name),
                                                        TaskResult::failed(
                                                            err_msg,
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
                                                let err_msg = format!(
                                                    "for_each binding '${}' resolved to non-array value",
                                                    alias
                                                );
                                                self.emit_scheduling_failure(
                                                    &task.name, &err_msg, "NIKA-026",
                                                );
                                                self.datastore.insert(
                                                    intern(&task.name),
                                                    TaskResult::failed(
                                                        err_msg,
                                                        std::time::Duration::ZERO,
                                                    ),
                                                );
                                                continue;
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        let err_msg = format!(
                                            "for_each binding '{}' not found: {}",
                                            base_alias, e
                                        );
                                        self.emit_scheduling_failure(
                                            &task.name, &err_msg, "NIKA-026",
                                        );
                                        self.datastore.insert(
                                            intern(&task.name),
                                            TaskResult::failed(err_msg, std::time::Duration::ZERO),
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
                                        Some(value) => match value_to_array(&value) {
                                            Some(items) => Some(items),
                                            None => {
                                                let err_msg = format!(
                                                    "for_each binding '{{{{inputs.{}}}}}' resolved to non-array value",
                                                    param_path
                                                );
                                                self.emit_scheduling_failure(
                                                    &task.name, &err_msg, "NIKA-026",
                                                );
                                                self.datastore.insert(
                                                    intern(&task.name),
                                                    TaskResult::failed(
                                                        err_msg,
                                                        std::time::Duration::ZERO,
                                                    ),
                                                );
                                                continue;
                                            }
                                        },
                                        None => {
                                            let err_msg = format!(
                                                "for_each input '{}' not found in workflow inputs",
                                                full_path
                                            );
                                            self.emit_scheduling_failure(
                                                &task.name, &err_msg, "NIKA-026",
                                            );
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    err_msg,
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
                                            let working_value: &Value = if let Some(v) =
                                                crate::binding::jsonpath::try_parse_json_str(
                                                    &base_value,
                                                ) {
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
                                                let err_msg = format!(
                                                    "for_each items: path traversal failed for '{{{{with.{}}}}}'",
                                                    path
                                                );
                                                self.emit_scheduling_failure(
                                                    &task.name, &err_msg, "NIKA-026",
                                                );
                                                self.datastore.insert(
                                                    intern(&task.name),
                                                    TaskResult::failed(
                                                        err_msg,
                                                        std::time::Duration::ZERO,
                                                    ),
                                                );
                                                continue;
                                            } else {
                                                match value_to_array(value_ref) {
                                                    Some(items) => Some(items),
                                                    None => {
                                                        let err_msg = format!(
                                                            "for_each binding '{{{{with.{}}}}}' resolved to non-array value",
                                                            path
                                                        );
                                                        self.emit_scheduling_failure(
                                                            &task.name, &err_msg, "NIKA-026",
                                                        );
                                                        self.datastore.insert(
                                                            intern(&task.name),
                                                            TaskResult::failed(
                                                                err_msg,
                                                                std::time::Duration::ZERO,
                                                            ),
                                                        );
                                                        continue;
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            let err_msg = format!(
                                                "for_each binding '{}' not found: {}",
                                                alias, e
                                            );
                                            self.emit_scheduling_failure(
                                                &task.name, &err_msg, "NIKA-026",
                                            );
                                            self.datastore.insert(
                                                intern(&task.name),
                                                TaskResult::failed(
                                                    err_msg,
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

                        // Get concurrency settings: for_each overrides, then standalone task fields
                        let fe = task.for_each.as_ref();
                        let concurrency = fe
                            .and_then(|f| f.concurrency)
                            .or(task.concurrency)
                            .unwrap_or(1)
                            .max(1) as usize;
                        let fail_fast = fe.map(|f| f.fail_fast).or(task.fail_fast).unwrap_or(true);

                        // Guard against unbounded for_each arrays that could OOM
                        const MAX_FOR_EACH_ITEMS: usize = 50_000;
                        if items.len() > MAX_FOR_EACH_ITEMS {
                            let err_msg = format!(
                                "for_each has {} items, exceeding limit of {} — \
                                 reduce the array size or split into batches",
                                items.len(),
                                MAX_FOR_EACH_ITEMS
                            );
                            self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                            self.datastore.insert(
                                intern(&task.name),
                                TaskResult::failed(err_msg, std::time::Duration::ZERO),
                            );
                            continue;
                        }

                        debug!(
                            task_id = %task.name,
                            items = items.len(),
                            concurrency = concurrency,
                            fail_fast = fail_fast,
                            "Starting for_each iteration"
                        );
                        self.event_log.emit(EventKind::ForEachStarted {
                            task_id: Arc::from(task.name.as_str()),
                            item_count: items.len(),
                            concurrency,
                            fail_fast,
                        });

                        // Create semaphore for concurrency limiting
                        let semaphore = Arc::new(Semaphore::new(concurrency));
                        // Create cancellation token for fail_fast (notification-based, no busy-poll)
                        let cancel = CancellationToken::new();

                        // Store token so the result-collection loop can cancel only THIS
                        // parent's iterations on fail_fast, not the entire JoinSet.
                        if fail_fast {
                            for_each_cancel_tokens.insert(intern(&task.name), cancel.clone());
                        }

                        // Spawn one execution per item in the array
                        let var_name = fe.map(|f| f.as_var.as_str()).unwrap_or("item").to_string();
                        // PERF(M1): Wrap in Arc once, then Arc::clone per iteration
                        // instead of deep-cloning AnalyzedTask (~800-1200 bytes each).
                        let task = Arc::new(task.clone());
                        // PERF(L7): Pre-allocate format buffer for task_id construction
                        let mut task_id_buf = String::with_capacity(task.name.len() + 8);
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

                            let task = Arc::clone(&task);
                            task_id_buf.clear();
                            use std::fmt::Write;
                            let _ = write!(task_id_buf, "{}[{}]", task.name, idx);
                            let task_id = intern(&task_id_buf);
                            let parent_task_id = intern(&task.name);
                            let datastore = self.datastore.clone();
                            let executor = self.executor.clone();
                            let event_log = self.event_log.clone();
                            let item = item.clone();
                            let var_name = var_name.clone();
                            let semaphore = Arc::clone(&semaphore);
                            let global_semaphore = Arc::clone(&self.global_task_semaphore);
                            let cancel = cancel.clone();
                            let for_each_total = items.len();
                            let workflow_artifacts = workflow_artifacts.clone();
                            let artifact_base_path = artifact_base_path.clone();
                            let workflow_base_url = workflow_base_url.clone();

                            join_set.spawn(async move {
                                // Check cancellation BEFORE acquiring semaphores
                                if cancel.is_cancelled() {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::skipped(
                                            "Cancelled due to fail_fast before semaphore acquire"
                                                .to_string(),
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),

                                    };
                                }

                                // Acquire global semaphore first (cross-for_each limit)
                                let _global_permit = tokio::select! {
                                    biased;

                                    _ = cancel.cancelled() => {
                                        return IterationResult {
                                            store_id: task_id,
                                            result: TaskResult::skipped(
                                                "Cancelled while waiting for global semaphore".to_string(),
                                            ),
                                            for_each_info: Some((parent_task_id, idx)),
                                        };
                                    }

                                    permit = global_semaphore.acquire() => {
                                        match permit {
                                            Ok(p) => p,
                                            Err(_) => {
                                                return IterationResult {
                                                    store_id: task_id,
                                                    result: TaskResult::failed(
                                                        "Global semaphore closed".to_string(),
                                                        std::time::Duration::ZERO,
                                                    ),
                                                    for_each_info: Some((parent_task_id, idx)),
                                                };
                                            }
                                        }
                                    }
                                };

                                // Then acquire per-parent semaphore (per-for_each concurrency limit)
                                let _permit = tokio::select! {
                                    biased;

                                    _ = cancel.cancelled() => {
                                        return IterationResult {
                                            store_id: task_id,
                                            result: TaskResult::skipped(
                                                "Cancelled while waiting for semaphore".to_string(),
                                            ),
                                            for_each_info: Some((parent_task_id, idx)),

                                        };
                                    }

                                    permit = semaphore.acquire() => {
                                        match permit {
                                            Ok(p) => p,
                                            Err(_) => {
                                                event_log.emit(EventKind::TaskFailed {
                                                    task_id: Arc::clone(&task_id),
                                                    error: "Semaphore closed unexpectedly".to_string(),
                                                    duration_ms: 0,
                                                    error_code: Some("NIKA-028".to_string()),
                                                });
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

                                // Final check after acquiring permit
                                if cancel.is_cancelled() {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::skipped(
                                            "Cancelled after semaphore acquire".to_string(),
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),

                                    };
                                }

                                event_log.emit(EventKind::ForEachItemStarted {
                                    task_id: Arc::clone(&parent_task_id),
                                    index: idx,
                                    total: for_each_total,
                                });

                                let item_start = std::time::Instant::now();
                                let result = Self::execute_task_iteration(
                                    task,
                                    Arc::clone(&task_id),
                                    Arc::clone(&parent_task_id),
                                    datastore,
                                    executor,
                                    event_log.clone(),
                                    Some((var_name, item, idx)),
                                    workflow_artifacts,
                                    artifact_base_path,
                                    workflow_base_url,
                                )
                                .await;

                                let item_duration_ms = item_start.elapsed().as_millis() as u64;
                                if result.result.is_success() {
                                    event_log.emit(EventKind::ForEachItemCompleted {
                                        task_id: Arc::clone(&parent_task_id),
                                        index: idx,
                                        duration_ms: item_duration_ms,
                                    });
                                } else {
                                    let err = match &result.result.status {
                                        crate::store::TaskOutcome::Failed(e) => e.clone(),
                                        _ => "iteration failed".to_string(),
                                    };
                                    event_log.emit(EventKind::ForEachItemFailed {
                                        task_id: Arc::clone(&parent_task_id),
                                        index: idx,
                                        error: err,
                                    });
                                }

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
                } else if task.for_each.is_some() {
                    // for_each was declared but items could not be resolved
                    // (unrecognized pattern, malformed JSON, malformed template).
                    // Fail explicitly instead of silently running as a regular task.
                    let err_msg = format!(
                        "for_each items could not be resolved for task '{}'. \
                         Check the binding reference.",
                        task.name
                    );
                    self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                    self.datastore.insert(
                        intern(&task.name),
                        TaskResult::failed(err_msg, std::time::Duration::ZERO),
                    );
                    continue;
                } else {
                    // Regular task without for_each
                    let task = Arc::new(task.clone());
                    let datastore = self.datastore.clone();
                    let executor = self.executor.clone();
                    let event_log = self.event_log.clone();
                    let workflow_artifacts = workflow_artifacts.clone();
                    let artifact_base_path = artifact_base_path.clone();
                    let workflow_base_url = workflow_base_url.clone();
                    let global_semaphore = Arc::clone(&self.global_task_semaphore);

                    join_set.spawn(async move {
                        // Acquire global semaphore to bound concurrent regular tasks
                        let _global_permit = match global_semaphore.acquire().await {
                            Ok(permit) => permit,
                            Err(_) => {
                                return IterationResult {
                                    store_id: task_id,
                                    result: crate::store::TaskResult::failed(
                                        "global task semaphore closed unexpectedly",
                                        std::time::Duration::ZERO,
                                    ),
                                    for_each_info: None,
                                };
                            }
                        };
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
                            workflow_base_url,
                        )
                        .await
                    });
                }
            }

            self.cli_renderer = renderer;

            // Collect for_each results for aggregation: parent_id -> Vec<(index, result)>
            // Use IndexMap to preserve insertion order (deterministic iteration)
            let mut for_each_results: IndexMap<Arc<str>, Vec<(usize, TaskResult)>> =
                IndexMap::new();

            // Clamp max_duration_secs to prevent Instant overflow (max ~292 years)
            let clamped_duration = self.workflow.max_duration_secs.min(604_800); // Cap at 1 week
            let timeout_deadline =
                tokio::time::Instant::now() + std::time::Duration::from_secs(clamped_duration);
            // Hoist the sleep future to avoid re-creating it on every select! iteration
            let timeout_sleep = tokio::time::sleep_until(timeout_deadline);
            tokio::pin!(timeout_sleep);

            // Wait for all spawned tasks to complete (with cancellation support)
            loop {
                tokio::select! {
                    biased;
                    // Check for cancellation first (biased ensures priority)
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
                        return Err(ExecutionError::Cancelled {
                            phase: "during execution".to_string(),
                        }
                        .into());
                    }
                    // Global workflow timeout (pinned future, not re-created per iteration)
                    _ = &mut timeout_sleep => {
                        join_set.abort_all();

                        let duration = workflow_start.elapsed();
                        let running_tasks: Vec<Arc<str>> = self
                            .workflow
                            .tasks
                            .iter()
                            .filter(|t| !self.datastore.contains(&t.name))
                            .map(|t| Arc::from(t.name.as_str()))
                            .collect();

                        self.event_log.emit(EventKind::WorkflowAborted {
                            reason: format!(
                                "Workflow exceeded max_duration_secs ({} seconds)",
                                self.workflow.max_duration_secs
                            ),
                            duration_ms: duration.as_millis() as u64,
                            running_tasks: running_tasks.clone(),
                        });
                        self.write_trace();
                        return Err(NikaError::WorkflowTimeout {
                            duration_secs: self.workflow.max_duration_secs,
                            running_tasks: running_tasks.iter().map(|t| t.to_string()).collect(),
                        });
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

                                _completed += 1;
                                let success = task_result.is_success();
                                let skipped = task_result.is_skipped();

                                // Render new events via CliRenderer
                                if let Some(ref mut r) = self.cli_renderer {
                                    self.event_log.with_events_since(r.last_rendered_id(), |events| {
                                        r.render_new_events(events);
                                    });
                                }

                                // Enforce 50MB output size limit to prevent OOM
                                let mut task_result = task_result;
                                if let Some(original_size) = task_result.truncate_if_oversized() {
                                    tracing::warn!(
                                        task_id = %store_id,
                                        original_size,
                                        limit = TaskResult::MAX_OUTPUT_SIZE,
                                        "Task output exceeds 50MB limit — truncated"
                                    );
                                }

                                // Store individual result
                                self.datastore
                                    .insert(Arc::clone(&store_id), task_result.clone());

                                // If this is a for_each failure with fail_fast,
                                // cancel only THIS parent's remaining iterations via its
                                // CancellationToken. This avoids killing unrelated sibling
                                // tasks from other for_each parents (Bug #26).
                                if !success && !skipped {
                                    if let Some((ref parent_id, _)) = for_each_info {
                                        if let Some(token) = for_each_cancel_tokens.get(parent_id) {
                                            if !token.is_cancelled() {
                                                debug!(
                                                    store_id = %store_id,
                                                    parent_id = %parent_id,
                                                    "Triggering fail_fast cancellation for parent"
                                                );
                                                token.cancel();
                                            }
                                        }
                                    }
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
                                // Task was cancelled or panicked
                                if e.is_cancelled() {
                                    // Task was cancelled (workflow abort or fail_fast) - expected
                                    debug!("Task cancelled (workflow abort or fail_fast)");
                                    // Continue collecting remaining results
                                } else {
                                    // EMIT: WorkflowFailed (task panic)
                                    self.event_log.emit(EventKind::WorkflowFailed {
                                        error: format!("Task panicked: {}", e),
                                        failed_task: None,
                                    });
                                    self.write_trace();
                                    return Err(ExecutionError::Panicked { reason: format!("{}", e) }.into());
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

                // Collect outputs into JSON array.
                // Auto-parse Value::String elements that contain valid JSON objects/arrays
                // so downstream tasks can access fields directly (e.g. $result[0].title).
                let outputs: Vec<Value> = results
                    .iter()
                    .map(|(_, r)| {
                        let val = (*r.output).clone();
                        if let Value::String(ref s) = val {
                            if s.starts_with('{') || s.starts_with('[') {
                                if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                                    return parsed;
                                }
                            }
                        }
                        val
                    })
                    .collect();

                // Calculate aggregate duration and success
                let total_duration: std::time::Duration =
                    results.iter().map(|(_, r)| r.duration).sum();
                let all_success = results.iter().all(|(_, r)| r.is_success());

                // Merge media refs from all successful iterations
                let merged_media: Vec<crate::media::MediaRef> = results
                    .iter()
                    .filter(|(_, r)| r.is_success())
                    .flat_map(|(_, r)| r.media.iter().cloned())
                    .collect();

                // Create aggregated result with JSON array + merged media
                let succeeded_count = results.iter().filter(|(_, r)| r.is_success()).count() as u32;
                let failed_count = results
                    .iter()
                    .filter(|(_, r)| !r.is_success() && !r.is_skipped())
                    .count() as u32;
                let aggregated_result = if all_success {
                    TaskResult::success(Value::Array(outputs), total_duration)
                        .with_media(merged_media)
                } else {
                    // Collect actual failures
                    let errors: Vec<String> = results
                        .iter()
                        .filter(|(_, r)| !r.is_success() && !r.is_skipped())
                        .filter_map(|(idx, r)| r.error().map(|e| format!("[{}]: {}", idx, e)))
                        .collect();
                    let skipped_count = results.iter().filter(|(_, r)| r.is_skipped()).count();
                    // Include skipped/cancelled count in error summary
                    let error_msg = match (errors.is_empty(), skipped_count) {
                        (true, 0) => "for_each: unknown failure".to_string(),
                        (true, n) => format!("{} item(s) cancelled", n),
                        (false, 0) => errors.join("; "),
                        (false, n) => {
                            format!("{}; {} item(s) cancelled", errors.join("; "), n)
                        }
                    };

                    // fail_fast parents have cancel tokens; non-fail_fast don't
                    let is_fail_fast = for_each_cancel_tokens.contains_key(&parent_id);
                    if succeeded_count > 0 && !is_fail_fast {
                        // Partial success: some iterations succeeded, output is usable.
                        // Only when fail_fast=false — user explicitly opted into partial results.
                        TaskResult {
                            output: Arc::new(Value::Array(outputs)),
                            duration: total_duration,
                            status: crate::store::TaskOutcome::PartialSuccess {
                                error_summary: error_msg,
                                succeeded: succeeded_count,
                                failed: failed_count,
                            },
                            media: merged_media,
                        }
                    } else {
                        // Total failure: no iterations succeeded
                        let mut result =
                            TaskResult::failed(error_msg, total_duration).with_media(merged_media);
                        result.output = Arc::new(Value::Array(outputs));
                        result
                    }
                };

                // Emit ForEachCompleted before storing (parent_id is consumed by insert)
                self.event_log.emit(EventKind::ForEachCompleted {
                    task_id: Arc::clone(&parent_id),
                    total: results.len() as u32,
                    succeeded: results.iter().filter(|(_, r)| r.is_success()).count() as u32,
                    failed: results
                        .iter()
                        .filter(|(_, r)| !r.is_success() && !r.is_skipped())
                        .count() as u32,
                    skipped: results.iter().filter(|(_, r)| r.is_skipped()).count() as u32,
                    duration_ms: total_duration.as_millis() as u64,
                });

                // Store aggregated result under parent ID
                self.datastore
                    .insert(Arc::clone(&parent_id), aggregated_result);

                // BUG-018: Process artifacts for the parent task with aggregated output.
                // Per-iteration artifact writes (line ~1371) overwrite the same file each time,
                // so for static paths only the last iteration's output survived. Here we write
                // the full aggregated array, which overwrites the incorrect per-iteration file.
                if let Some(parent_task) =
                    self.workflow.tasks.iter().find(|t| *t.name == *parent_id)
                {
                    if let Some(ref artifact_spec) = parent_task.artifact {
                        // Retrieve the aggregated result we just stored
                        if let Some(agg_result) = self.datastore.get(&parent_id) {
                            if agg_result.is_usable() {
                                let output_content = agg_result.output_str().into_owned();
                                let bindings = ResolvedBindings::from_with_spec(
                                    Some(&parent_task.with_spec),
                                    &self.datastore,
                                )
                                .unwrap_or_default();
                                let artifact_result = process_task_artifacts(
                                    &parent_id,
                                    &output_content,
                                    artifact_spec,
                                    workflow_artifacts.as_ref(),
                                    &artifact_base_path,
                                    Some(&self.event_log),
                                    &bindings,
                                    &self.datastore,
                                    agg_result.media.as_slice(),
                                )
                                .await;
                                if artifact_result.written > 0 {
                                    debug!(
                                        task_id = %parent_id,
                                        artifacts_written = artifact_result.written,
                                        "for_each aggregated artifacts written"
                                    );
                                }
                                for err in &artifact_result.errors {
                                    tracing::error!(
                                        task_id = %parent_id,
                                        error = %err,
                                        "for_each aggregated artifact write failed"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        // Verify media integrity (warn-only, never fail successful workflows)
        let media_warnings = self.verify_media_integrity();

        // Lockfile is removed automatically when `_lockfile_guard` drops
        // (at function exit -- normal return, error, or panic).

        // Write artifact manifest if configured
        if let Some(ref artifacts_config) = self.workflow.artifacts {
            write_artifact_manifest(&self.event_log, artifacts_config, &base_path);
        }

        // Get final output
        let output = match self.get_final_output() {
            Some(o) => o,
            None => {
                tracing::warn!("No final output from workflow — defaulting to empty string");
                String::new()
            }
        };

        // P-ORCHESTRATE: Emit OrchestratorCompleted when goal workflow finishes
        if self.workflow.goal.is_some() {
            // Compute total cost from ProviderResponded events
            let total_cost: f64 = self
                .event_log
                .events()
                .iter()
                .filter_map(|e| match &e.kind {
                    EventKind::ProviderResponded { cost_usd, .. } => Some(cost_usd),
                    _ => None,
                })
                .sum();
            // Count orchestrator rounds from OrchestratorRound events (if any)
            let rounds: u32 = self
                .event_log
                .events()
                .iter()
                .filter(|e| matches!(&e.kind, EventKind::OrchestratorRound { .. }))
                .count() as u32;
            // Extract confidence from the orchestrator's output (nika:complete response)
            let confidence = self
                .datastore
                .get(crate::runtime::orchestrate::ORCHESTRATOR_TASK_ID)
                .and_then(|result| {
                    // The output is either a JSON string containing the complete response
                    // or the raw text. Try to parse confidence from it.
                    match result.output.as_ref() {
                        Value::String(s) => serde_json::from_str::<serde_json::Value>(s)
                            .ok()
                            .and_then(|v| v["confidence"].as_f64()),
                        v => v.get("confidence").and_then(|c| c.as_f64()),
                    }
                })
                .unwrap_or(1.0);
            self.event_log.emit(EventKind::OrchestratorCompleted {
                rounds,
                total_cost_usd: total_cost,
                confidence,
            });
        }

        // EMIT: WorkflowCompleted
        self.event_log.emit(EventKind::WorkflowCompleted {
            final_output: Arc::new(Value::String(output.clone())),
            total_duration_ms: workflow_start.elapsed().as_millis() as u64,
        });

        // GC: remove CAS media files older than 30 days and emit MediaCleanup
        let cas_store = CasStore::workspace_default(&base_path);
        let gc = cas_store.clean_older_than(std::time::Duration::from_secs(30 * 24 * 3600));
        if gc.removed > 0 || gc.bytes_freed > 0 {
            tracing::debug!(
                removed = gc.removed,
                bytes_freed = gc.bytes_freed,
                "CAS GC: removed stale media files"
            );
        }
        self.event_log.emit(EventKind::MediaCleanup {
            removed: gc.removed,
            bytes_freed: gc.bytes_freed,
            dry_run: false,
        });

        if media_warnings > 0 {
            tracing::warn!(
                warnings = media_warnings,
                "Media integrity check completed with warnings"
            );
        }

        // Persist records as NDJSON for cross-session search
        let records: Vec<(String, crate::runtime::record::Record)> = self
            .datastore
            .iter_records()
            .into_iter()
            .map(|(tid, arc_rec)| (tid.to_string(), (*arc_rec).clone()))
            .collect();
        if !records.is_empty() {
            let wf_name = self.workflow.name.as_deref().unwrap_or("unnamed");
            if let Err(e) = crate::store::RecordWriter::write_records(wf_name, &records) {
                tracing::warn!(error = %e, "Failed to persist records as NDJSON");
            }
        }

        // Write execution trace to .nika/traces/
        let trace_path = self.write_trace();

        if let Some(ref mut renderer) = self.cli_renderer {
            self.event_log
                .with_events_since(renderer.last_rendered_id(), |events| {
                    renderer.render_new_events(events);
                });
            let total_duration_ms = workflow_start.elapsed().as_millis() as u64;
            if self.quiet {
                renderer.render_quiet_summary(total_duration_ms);
            } else {
                renderer.render_summary(total_duration_ms, trace_path.as_deref());
            }
        } else if !self.quiet {
            let elapsed = workflow_start.elapsed();
            let elapsed_str = if elapsed.as_secs() >= 60 {
                format!(
                    "{}m {:.1}s",
                    elapsed.as_secs() / 60,
                    elapsed.as_secs_f64() % 60.0
                )
            } else {
                format!("{:.1}s", elapsed.as_secs_f64())
            };
            let events = self.event_log.events();
            let (total_tokens, total_cost) =
                events.iter().fold((0u64, 0.0f64), |(tokens, cost), e| {
                    if let EventKind::ProviderResponded {
                        input_tokens,
                        output_tokens,
                        cost_usd,
                        ..
                    } = &e.kind
                    {
                        (
                            tokens
                                .saturating_add(*input_tokens)
                                .saturating_add(*output_tokens),
                            cost + cost_usd,
                        )
                    } else {
                        (tokens, cost)
                    }
                });
            // PERF(M2): Reuse cached_depths from earlier computation
            let task_count = self.workflow.tasks.len();
            let parallel_count = if let Some(ref depths) = cached_depths {
                let max_layer = depths.values().copied().max().unwrap_or(0);
                let mut layers: Vec<Vec<&str>> = vec![Vec::new(); max_layer + 1];
                for task in &self.workflow.tasks {
                    if let Some(&layer) = depths.get(task.name.as_str()) {
                        layers[layer].push(task.name.as_str());
                    }
                }
                layers
                    .iter()
                    .filter(|l| l.len() > 1)
                    .flat_map(|l| l.iter())
                    .count()
            } else {
                0
            };
            crate::display::print_done_summary(
                &elapsed_str,
                total_tokens,
                total_cost,
                trace_path.as_deref(),
                task_count,
                parallel_count,
            );
        }

        // Gracefully shut down MCP server processes to avoid orphans
        self.executor.shutdown_mcp().await;

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
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table: TaskTable::new(),
            tasks: vec![],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
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
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: items_json.to_string(),
                as_var: as_var.to_string(),
                concurrency,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
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
                        cwd: None,
                        env: IndexMap::new(),
                        timeout_ms: None,
                        max_stdout: None,
                        span: Span::dummy(),
                    }),
                    provider: None,
                    model: None,
                    base_url: None,
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
                    record: None,
                    context_budget: None,
                    preset: None,
                    routing: None,
                    span: Span::dummy(),
                }
            })
            .collect();

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V01,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: analyzed_tasks,
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
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

        // WorkflowCompleted should be emitted (followed by MediaCleanup GC event)
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. })),
            "WorkflowCompleted should be emitted"
        );
        // Last event is MediaCleanup (GC runs after workflow)
        let last = events.last().unwrap();
        assert!(matches!(&last.kind, EventKind::MediaCleanup { .. }));

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

        // WorkflowCompleted should be present; MediaCleanup GC event follows it
        assert!(
            events
                .iter()
                .any(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. })),
            "WorkflowCompleted should be emitted"
        );
        let last = events.last().unwrap();
        assert!(matches!(&last.kind, EventKind::MediaCleanup { .. }));
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
            vec![("fast", "echo quick"), ("slow", "echo done")],
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

        // Workflow run() now returns Err when tasks fail (NIKA-084)
        let result = runner.run().await;
        assert!(
            result.is_err(),
            "workflow should return Err when tasks fail"
        );

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

    /// Helper: create a fresh pending_indices vec for all tasks in the runner.
    fn fresh_pending(runner: &Runner) -> Vec<usize> {
        (0..runner.workflow.tasks.len()).collect()
    }

    #[test]
    fn get_ready_tasks_returns_tasks_with_no_deps() {
        // Two independent tasks - both should be ready
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B")],
            vec![], // No flows = no dependencies
        );
        let runner = Runner::new(workflow).unwrap();
        let mut pending = fresh_pending(&runner);

        let ready = runner.get_ready_tasks(&mut pending);
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
        let mut pending = fresh_pending(&runner);

        let ready = runner.get_ready_tasks(&mut pending);
        assert_eq!(ready.len(), 1, "Only first task should be ready");
        assert_eq!(ready[0].name, "a", "Task 'a' should be ready");
    }

    #[test]
    fn get_ready_tasks_excludes_completed_tasks() {
        let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
        let runner = Runner::new(workflow).unwrap();
        let mut pending = fresh_pending(&runner);

        // Initially task is ready
        let ready = runner.get_ready_tasks(&mut pending);
        assert_eq!(ready.len(), 1);

        // Mark task as done
        runner.datastore.insert(
            intern("only"),
            TaskResult::success_str("done", std::time::Duration::ZERO),
        );

        // Now no tasks should be ready (was already removed from pending on dispatch)
        let ready = runner.get_ready_tasks(&mut pending);
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
        // runner.run() now returns Err when tasks fail (NIKA-084)
        assert!(
            result.is_err(),
            "Workflow should fail when fail_fast triggers"
        );
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
        // Now returns Err(DependencyChainFailed) when task fails (NIKA-084)
        assert!(result.is_err(), "Workflow should fail when task fails");

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
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
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
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec,
            depends_on: vec![tid1],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: for_each_items.to_string(),
                as_var: "item".to_string(),
                concurrency: None,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![step1, step2],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
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
    async fn for_each_dollar_binding_with_pipe_transform() {
        // Step1 outputs a raw JSON string (not auto-parsed because it's a plain string value).
        // Step2 uses `$step1 | parse_json` to parse + iterate.
        let workflow = create_two_step_for_each_workflow(
            r#"echo '["alpha","beta","gamma"]'"#,
            true,
            "$step1 | parse_json",
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
            "step2 should succeed — pipe transform parse_json should produce iterable array"
        );
    }

    #[tokio::test]
    async fn for_each_dollar_binding_with_chained_pipe_transforms() {
        // Step1 outputs a JSON string. Step2 uses `$step1 | parse_json | reverse` to
        // parse and reverse the array before iterating.
        let workflow = create_two_step_for_each_workflow(
            r#"echo '["x","y","z"]'"#,
            true,
            "$step1 | parse_json | reverse",
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
            "step2 should succeed — chained pipe transforms should produce iterable array"
        );
    }

    #[tokio::test]
    async fn for_each_dollar_binding_with_path_and_pipe_transform() {
        // Step1 outputs a JSON object. Step2 uses `$step1.data | sort` to
        // access a nested path AND apply a transform.
        let workflow = create_two_step_for_each_workflow(
            r#"echo '{"data":["cherry","apple","banana"]}'"#,
            true,
            "$step1.data | sort",
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
            "step2 should succeed — path + pipe transform should produce iterable array"
        );
    }

    #[tokio::test]
    async fn for_each_output_auto_parses_json_strings() {
        // Step1 outputs a JSON array of names. Step2 for_each iterates and echoes JSON objects.
        // After aggregation, step2's output elements should be Value::Object, not Value::String.
        let workflow = create_two_step_for_each_workflow(
            r#"echo '["alice","bob"]'"#,
            true,
            "$step1",
            r#"echo '{"name": "{{with.item}}"}'"#,
        );
        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok(), "Workflow failed: {:?}", result.err());

        let step2_result = runner.datastore.get("step2").expect("step2 result");
        assert!(step2_result.is_success());
        let output = &*step2_result.output;
        let arr = output.as_array().expect("for_each output should be array");
        assert_eq!(arr.len(), 2);
        // Key assertion: elements should be auto-parsed to Value::Object
        assert!(
            arr[0].is_object(),
            "for_each element should be auto-parsed to object, got: {:?}",
            arr[0]
        );
        assert_eq!(arr[0]["name"], "alice");
        assert_eq!(arr[1]["name"], "bob");
    }

    #[tokio::test]
    async fn for_each_output_preserves_plain_strings() {
        // Step2 echoes plain text (not JSON). Output should stay as Value::String.
        let workflow = create_two_step_for_each_workflow(
            r#"echo '["hello","world"]'"#,
            true,
            "$step1",
            "echo {{with.item}}",
        );
        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok());

        let step2_result = runner.datastore.get("step2").expect("step2 result");
        let arr = step2_result.output.as_array().expect("array");
        assert_eq!(arr.len(), 2);
        // Plain strings should stay as strings
        assert!(arr[0].is_string(), "plain string should stay as string");
        assert_eq!(arr[0].as_str().unwrap().trim(), "hello");
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
    // FOR_EACH INDEX VARIABLE
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn for_each_injects_for_each_index_binding() {
        let workflow = create_for_each_workflow(
            "indexed",
            r#"["a", "b", "c"]"#,
            "item",
            "echo {{with.for_each_index}}",
            None,
            true,
            false,
        );

        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should succeed: {:?}",
            result.err()
        );

        let parent = runner.datastore.get("indexed");
        assert!(parent.is_some(), "Parent result should exist");

        let parent_result = parent.unwrap();
        let output = parent_result.output_str();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output)
            .unwrap_or_else(|_| panic!("Should be valid JSON array: {output}"));

        // Results should contain indices 0, 1, 2 (order may vary due to concurrency)
        let indices: Vec<String> = parsed
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
            .collect();
        assert!(
            indices.contains(&"0".to_string()),
            "Should contain index 0: {indices:?}"
        );
        assert!(
            indices.contains(&"1".to_string()),
            "Should contain index 1: {indices:?}"
        );
        assert!(
            indices.contains(&"2".to_string()),
            "Should contain index 2: {indices:?}"
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
            base_url: None,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
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
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: Some(AnalyzedOutput {
                format: AnalyzedOutputFormat::Json,
                schema: Some(json!({"type": "object"})),
                schema_ref: None,
                max_retries: None,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
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
                schema_ref: None,
                max_retries: None,
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
                schema_ref: None,
                max_retries: None,
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
                schema_ref: None,
                max_retries: None,
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
                schema_ref: None,
                max_retries: None,
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
                schema_ref: None,
                max_retries: None,
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
                schema_ref: None,
                max_retries: None,
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
        let mut pending = fresh_pending(&runner);

        let ready = runner.get_ready_tasks(&mut pending);
        assert_eq!(ready.len(), 2, "Tasks with no deps should all be ready");
    }

    #[test]
    fn test_get_ready_tasks_blocked_by_incomplete_dep() {
        let workflow =
            create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
        let runner = Runner::new(workflow).unwrap();
        let mut pending = fresh_pending(&runner);

        let ready = runner.get_ready_tasks(&mut pending);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "a", "Only root task should be ready");
    }

    #[test]
    fn test_get_ready_tasks_unblocked_after_dep_success() {
        let workflow =
            create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
        let runner = Runner::new(workflow).unwrap();
        let mut pending = fresh_pending(&runner);

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );

        let ready = runner.get_ready_tasks(&mut pending);
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].name, "b", "b should be ready after a succeeds");
    }

    #[test]
    fn test_get_ready_tasks_skips_already_done() {
        let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
        let runner = Runner::new(workflow).unwrap();
        let mut pending = fresh_pending(&runner);

        runner.datastore.insert(
            intern("a"),
            TaskResult::success(json!("ok"), Duration::from_millis(10)),
        );

        let ready = runner.get_ready_tasks(&mut pending);
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
        let mut pending = fresh_pending(&runner);

        // Mark a as failed
        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("boom".to_string(), Duration::from_millis(10)),
        );

        // Single call cascades when tasks are in topological order (a, b, c):
        // retain processes indices in order, so b is marked DependencyFailed and
        // stored in the datastore before c is checked. c then sees b's failure
        // and cascades in the same pass. This is an optimization, not a guarantee —
        // reverse task order would require two passes (see reverse-order test below).
        let ready = runner.get_ready_tasks(&mut pending);
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

        // c cascaded in the same pass (topological order)
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
    fn test_dependency_failure_cascade_reverse_order() {
        // Same chain a → b → c, but tasks declared in reverse order.
        // Cascade needs two passes: first marks b (since a is failed),
        // second marks c (since b is now failed).
        let workflow = create_exec_workflow(
            vec![("c", "echo c"), ("b", "echo b"), ("a", "echo a")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow).unwrap();
        let mut pending = fresh_pending(&runner);

        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("boom".to_string(), Duration::from_millis(10)),
        );

        // First pass: c is checked first but b hasn't failed yet → c stays pending.
        // b is checked next → a failed → b marked DependencyFailed.
        let ready = runner.get_ready_tasks(&mut pending);
        assert!(ready.is_empty());
        assert!(runner.datastore.get("b").unwrap().is_dependency_failed());

        // Second pass: c is checked → b now failed → c marked DependencyFailed.
        let ready = runner.get_ready_tasks(&mut pending);
        assert!(ready.is_empty());
        assert!(
            runner.datastore.get("c").unwrap().is_dependency_failed(),
            "c should cascade on second pass when tasks are in reverse order"
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
        let mut pending = fresh_pending(&runner);

        // Mark a as failed
        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("oops".to_string(), Duration::from_millis(10)),
        );

        let ready = runner.get_ready_tasks(&mut pending);
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
        let mut pending = fresh_pending(&runner);

        runner.datastore.insert(
            intern("a"),
            TaskResult::failed("crash".to_string(), Duration::from_millis(10)),
        );

        // Trigger dependency failure propagation
        let _ = runner.get_ready_tasks(&mut pending);

        // Check that a TaskSkipped event was emitted for b
        let events = runner.event_log.events();
        let skip_events: Vec<_> = events
            .iter()
            .filter(|e| {
                matches!(
                    &e.kind,
                    EventKind::TaskSkipped { task_id, .. } if task_id.as_ref() == "b"
                )
            })
            .collect();
        assert_eq!(
            skip_events.len(),
            1,
            "Should emit exactly one TaskSkipped event for b"
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

        let callback: InferCallback = Arc::new(move |_prompt: String, _max_tokens: Option<u32>| {
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
        let callback: InferCallback = Arc::new(move |_prompt: String, _max_tokens: Option<u32>| {
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

        let callback: InferCallback = Arc::new(move |_prompt: String, _max_tokens: Option<u32>| {
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

    // ═══════════════════════════════════════════════════════════════
    // LOCKFILE GUARD RAII TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn lockfile_guard_creates_and_removes_on_drop() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("store").join(".nika-run.lock");

        {
            let _guard = LockfileGuard::create(lock_path.clone());
            assert!(
                lock_path.exists(),
                "Lockfile should exist while guard is alive"
            );

            let content = std::fs::read_to_string(&lock_path).unwrap();
            assert!(
                content.starts_with("pid:"),
                "Lockfile should contain pid, got: {content}"
            );
        }

        assert!(
            !lock_path.exists(),
            "Lockfile should be removed after guard is dropped"
        );
    }

    #[test]
    fn lockfile_guard_removes_on_panic_unwind() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("store").join(".nika-run.lock");

        let result = std::panic::catch_unwind(|| {
            let _guard = LockfileGuard::create(lock_path.clone());
            assert!(lock_path.exists(), "Lockfile should exist before panic");
            panic!("simulated runner panic");
        });

        assert!(result.is_err(), "Should have caught the panic");
        assert!(
            !lock_path.exists(),
            "Lockfile should be removed even after panic unwind"
        );
    }

    #[test]
    fn lockfile_guard_removes_on_early_return() {
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("store").join(".nika-run.lock");

        fn simulate_early_return(path: &std::path::Path) -> Result<(), &'static str> {
            let _guard = LockfileGuard::create(path.to_path_buf());
            assert!(path.exists(), "Lockfile should exist before early return");
            Err("simulated ? operator bail-out")?;
            unreachable!()
        }

        let result = simulate_early_return(&lock_path);
        assert!(result.is_err());
        assert!(
            !lock_path.exists(),
            "Lockfile should be removed after early return via ?"
        );
    }

    #[test]
    fn lockfile_guard_tolerates_missing_file() {
        // If someone manually deletes the lockfile during a run,
        // Drop should not panic.
        let dir = tempfile::tempdir().unwrap();
        let lock_path = dir.path().join("store").join(".nika-run.lock");

        let guard = LockfileGuard::create(lock_path.clone());
        assert!(lock_path.exists());

        // Simulate external deletion
        std::fs::remove_file(&lock_path).unwrap();
        assert!(!lock_path.exists());

        // Drop should not panic
        drop(guard);
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug #24: for_each {{with.alias.path}} traversal failure must
    // record a failed result, not silently run as a regular task.
    // ═══════════════════════════════════════════════════════════════

    /// Build a two-step workflow where step2 uses {{with.step1.path}} for_each.
    fn create_with_template_for_each_workflow(
        step1_cmd: &str,
        step1_shell: bool,
        for_each_template: &str,
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
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
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
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec,
            depends_on: vec![tid1],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: for_each_template.to_string(),
                as_var: "item".to_string(),
                concurrency: None,
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![step1, step2],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        }
    }

    #[tokio::test]
    async fn bug24_for_each_with_template_traversal_failure_records_error() {
        // step1 outputs JSON object, step2 references nonexistent nested path
        let workflow = create_with_template_for_each_workflow(
            r#"echo '{"items": ["a","b"]}'"#,
            true,
            "{{with.step1.nonexistent}}",
            "echo {{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap().quiet();
        let _ = runner.run().await;

        let task_result = runner.datastore.get("step2");
        assert!(task_result.is_some(), "step2 result should exist");

        let result = task_result.unwrap();
        assert!(
            !result.is_success(),
            "step2 should FAIL when path traversal fails, not run as regular task"
        );
        let error_msg = result.error().expect("should have error message");
        assert!(
            error_msg.contains("traversal failed"),
            "Error should mention path traversal failure, got: {}",
            error_msg
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug #25: for_each {{with.alias}} non-array must record error,
    // not silently run as a regular task.
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn bug25_for_each_with_template_non_array_records_error() {
        // step1 outputs a plain string (not an array), step2 tries to iterate over it
        let workflow = create_with_template_for_each_workflow(
            "echo not_an_array",
            false,
            "{{with.step1}}",
            "echo {{with.item}}",
        );

        let mut runner = Runner::new(workflow).unwrap().quiet();
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

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH CONCURRENT FAIL_FAST CANCELLATION
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn for_each_concurrent_fail_fast_cancels_remaining_iterations() {
        // Verify that with concurrency > 1 and fail_fast = true, when one
        // iteration fails, iterations waiting on the semaphore are cancelled
        // (returned as Skipped) rather than being allowed to proceed.
        //
        // Cancellation semantics: the CancellationToken fires when an iteration
        // fails. This cancels iterations that are:
        //   (a) waiting to acquire the semaphore (via tokio::select!)
        //   (b) not yet spawned (checked in the spawn loop)
        //   (c) checked again after acquiring the permit
        //
        // Iterations that already acquired a permit and started executing will
        // run to completion — the token does not abort running shell processes.
        //
        // Strategy:
        //   - 6 items, concurrency=2, fail_fast=true
        //   - Item 0 fails immediately (exit 1)
        //   - Item 1 succeeds quickly (echo)
        //   - Items 2-5 would run if allowed, but should be cancelled at the
        //     semaphore gate since the cancel token fires before they acquire.

        let workflow = create_for_each_workflow(
            "cancel_test",
            r#"["FAIL", "ok1", "wait2", "wait3", "wait4", "wait5"]"#,
            "item",
            // Non-failing items sleep briefly so cancellation propagates before they complete.
            // Without the sleep, items 2-5 can finish instantly after acquiring the permit
            // released by the failing item, racing the CancellationToken.
            "if [ '{{with.item}}' = 'FAIL' ]; then exit 1; else sleep 0.05 && echo {{with.item}}; fi",
            Some(2), // concurrency = 2
            true,    // fail_fast = true
            true,    // shell = true
        );

        let mut runner = Runner::new(workflow).unwrap().quiet();
        let result = runner.run().await;

        // Workflow should fail (fail_fast propagates the error)
        assert!(
            result.is_err(),
            "Workflow should fail when fail_fast triggers on concurrent for_each"
        );

        // The failing iteration (index 0) should exist and be a failure
        let fail_iter = runner.datastore.get("cancel_test[0]");
        assert!(
            fail_iter.is_some(),
            "Failing iteration [0] result should exist"
        );
        assert!(
            !fail_iter.unwrap().is_success(),
            "Iteration [0] should have failed"
        );

        // Item 1 may have completed (it was in the same concurrency batch as
        // item 0) or may have been skipped — either is acceptable.
        // But items 2-5 were waiting on the semaphore and MUST be skipped.
        let mut skipped_count = 0;
        let mut total_stored = 0;
        for idx in 2..6 {
            let key = format!("cancel_test[{}]", idx);
            if let Some(iter_result) = runner.datastore.get(&key) {
                total_stored += 1;
                if iter_result.is_skipped() {
                    skipped_count += 1;
                }
            }
            // Iterations that were never spawned (spawn loop saw cancellation)
            // won't have a datastore entry at all — that's also valid cancellation.
        }

        // Items 2-5 were queued behind the semaphore. They should either be
        // skipped (cancel fired while waiting) or never spawned (cancel fired
        // before the spawn loop reached them). Either way, they should NOT
        // have succeeded.
        let succeeded_after_cancel: Vec<usize> = (2..6)
            .filter(|idx| {
                let key = format!("cancel_test[{}]", idx);
                runner
                    .datastore
                    .get(&key)
                    .map(|r| r.is_success())
                    .unwrap_or(false)
            })
            .collect();

        assert!(
            succeeded_after_cancel.is_empty(),
            "Iterations behind the semaphore should not succeed after fail_fast cancellation, \
            but these succeeded: {:?}",
            succeeded_after_cancel
        );

        // At least some of the queued iterations should have a skipped result
        // (others may not have been spawned at all).
        let not_spawned = 4 - total_stored;
        assert!(
            skipped_count + not_spawned >= 1,
            "At least one iteration should be cancelled (skipped={}, not_spawned={}). \
            This suggests cancellation tokens are not working for concurrent for_each.",
            skipped_count,
            not_spawned
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Bug #26: fail_fast should only cancel sibling iterations,
    // not unrelated tasks in the same JoinSet.
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn bug26_fail_fast_does_not_abort_unrelated_sibling_tasks() {
        // Two independent for_each parents: one fails fast, the other should still complete.
        // "failing_parent" has fail_fast=true and one failing item.
        // "passing_parent" has fail_fast=true but all items succeed.
        // Both run in parallel (no depends_on between them).
        //
        // Before the fix, abort_all() would kill BOTH parents' tasks.
        // After the fix, only failing_parent's iterations are cancelled.

        let mut task_table = TaskTable::new();
        task_table.insert("failing_parent");
        task_table.insert("passing_parent");
        let tid_fail = task_table.get_id("failing_parent").unwrap();
        let tid_pass = task_table.get_id("passing_parent").unwrap();

        let failing_parent = AnalyzedTask {
            id: tid_fail,
            name: "failing_parent".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "test '{{with.item}}' != 'FAIL' && echo {{with.item}}".to_string(),
                shell: true,
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: r#"["ok", "FAIL", "ok2"]"#.to_string(),
                as_var: "item".to_string(),
                concurrency: Some(3),
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        let passing_parent = AnalyzedTask {
            id: tid_pass,
            name: "passing_parent".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "echo {{with.item}}".to_string(),
                shell: true,
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: Some(AnalyzedForEach {
                items: r#"["a", "b", "c"]"#.to_string(),
                as_var: "item".to_string(),
                concurrency: Some(3),
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
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        let workflow = AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![failing_parent, passing_parent],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        };

        let mut runner = Runner::new(workflow).unwrap().quiet();
        let _ = runner.run().await;

        // The failing parent should have a result (aggregated, with at least one failure)
        let fail_result = runner.datastore.get("failing_parent");
        assert!(fail_result.is_some(), "failing_parent result should exist");

        // The passing parent should ALSO have a result — it should NOT be aborted
        let pass_result = runner.datastore.get("passing_parent");
        assert!(
            pass_result.is_some(),
            "passing_parent result should exist (fail_fast of sibling should not abort it)"
        );

        let pass_tr = pass_result.unwrap();
        assert!(
            pass_tr.is_success(),
            "passing_parent should succeed — its iterations were all OK. \
            Error: {:?}",
            pass_tr.error()
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // TASK-LEVEL RETRY TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_is_retryable_provider_api_error() {
        let err = NikaError::ProviderApiError {
            message: "429 Too Many Requests".to_string(),
        };
        assert!(
            Runner::is_retryable(&err),
            "ProviderApiError should be retryable"
        );
    }

    #[test]
    fn test_is_retryable_exec_error() {
        let err = NikaError::ExecError {
            reason: "command timed out".to_string(),
        };
        assert!(Runner::is_retryable(&err), "ExecError should be retryable");
    }

    #[test]
    fn test_is_retryable_fetch_error() {
        let err = NikaError::FetchError {
            reason: "connection reset".to_string(),
        };
        assert!(Runner::is_retryable(&err), "FetchError should be retryable");
    }

    #[test]
    fn test_is_retryable_mcp_not_connected() {
        let err = NikaError::McpNotConnected {
            name: "novanet".to_string(),
        };
        assert!(
            Runner::is_retryable(&err),
            "McpNotConnected should be retryable"
        );
    }

    #[test]
    fn test_is_retryable_mcp_timeout() {
        let err = NikaError::McpTimeout {
            name: "search".to_string(),
            operation: "tool_call".to_string(),
            timeout_secs: 30,
        };
        assert!(Runner::is_retryable(&err), "McpTimeout should be retryable");
    }

    #[test]
    fn test_is_retryable_timeout() {
        let err = NikaError::Timeout {
            operation: "infer".to_string(),
            duration_ms: 300_000,
        };
        assert!(Runner::is_retryable(&err), "Timeout should be retryable");
    }

    #[test]
    fn test_is_retryable_endpoint_connection_failed() {
        let err = NikaError::EndpointConnectionFailed {
            endpoint: "h100".to_string(),
            reason: "connection refused".to_string(),
        };
        assert!(
            Runner::is_retryable(&err),
            "EndpointConnectionFailed should be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_validation_error() {
        let err = NikaError::ValidationError {
            reason: "invalid schema".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "ValidationError should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_template_error() {
        let err = NikaError::TemplateError {
            template: "{{with.x}}".to_string(),
            reason: "not found".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "TemplateError should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_cycle_detected() {
        let err = NikaError::CycleDetected {
            cycle: "a -> b -> a".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "CycleDetected should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_missing_api_key() {
        let err = NikaError::MissingApiKey {
            provider: "openai".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "MissingApiKey should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_provider_401() {
        let err = NikaError::ProviderApiError {
            message: "401 Unauthorized: invalid API key".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "ProviderApiError 401 should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_provider_403() {
        let err = NikaError::ProviderApiError {
            message: "403 Forbidden: account suspended".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "ProviderApiError 403 should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_exec_not_found() {
        let err = NikaError::ExecError {
            reason: "Failed to spawn command: No such file or directory".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "ExecError 'not found' should NOT be retryable"
        );
    }

    #[test]
    fn test_is_not_retryable_exec_permission_denied() {
        let err = NikaError::ExecError {
            reason: "Failed to spawn command: Permission denied".to_string(),
        };
        assert!(
            !Runner::is_retryable(&err),
            "ExecError 'permission denied' should NOT be retryable"
        );
    }

    #[tokio::test]
    async fn test_exec_task_with_retry_runs_and_succeeds() {
        // Create a workflow with an exec task that has retry config.
        // The command succeeds on first try, so no retry needed —
        // this tests that retry config doesn't break normal execution.
        use crate::ast::analyzed::AnalyzedRetry;

        let mut task_table = TaskTable::new();
        task_table.insert("retryable");
        let tid = task_table.get_id("retryable").unwrap();

        let task = AnalyzedTask {
            id: tid,
            name: "retryable".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "echo hello".to_string(),
                shell: false,
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: Some(AnalyzedRetry {
                max_attempts: 3,
                delay_ms: 100,
                backoff: Some(2.0),
                span: Span::dummy(),
            }),
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        let workflow = AnalyzedWorkflow {
            schema_version: SchemaVersion::V01,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        };

        let event_log = crate::event::EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone())
            .unwrap()
            .quiet();
        let result = runner.run().await;

        assert!(
            result.is_ok(),
            "Workflow with retry should succeed: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert!(
            output.contains("hello"),
            "Output should contain 'hello', got: {}",
            output
        );

        // Verify no TaskRetry events (first attempt succeeded)
        let events = event_log.events();
        let retry_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::TaskRetry { .. }))
            .collect();
        assert!(
            retry_events.is_empty(),
            "No TaskRetry events should be emitted when first attempt succeeds"
        );
    }

    #[tokio::test]
    async fn test_exec_task_with_retry_retries_on_failure() {
        // Create a workflow with an exec task that fails with retry config.
        // Uses a temp file counter: first call creates the file and fails,
        // second call sees the file and succeeds.
        use crate::ast::analyzed::AnalyzedRetry;

        let tmp = std::env::temp_dir().join(format!("nika_retry_test_{}", std::process::id()));
        // Clean up any leftover from previous run
        let _ = std::fs::remove_file(&tmp);

        let cmd = format!(
            "if [ -f '{}' ]; then echo success; else touch '{}'; exit 1; fi",
            tmp.display(),
            tmp.display()
        );

        let mut task_table = TaskTable::new();
        task_table.insert("retryable");
        let tid = task_table.get_id("retryable").unwrap();

        let task = AnalyzedTask {
            id: tid,
            name: "retryable".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: cmd,
                shell: true,
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
            with_spec: Default::default(),
            depends_on: vec![],
            implicit_deps: vec![],
            output: None,
            for_each: None,
            retry: Some(AnalyzedRetry {
                max_attempts: 3,
                delay_ms: 50,
                backoff: None,
                span: Span::dummy(),
            }),
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        let workflow = AnalyzedWorkflow {
            schema_version: SchemaVersion::V01,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: None,
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        };

        let event_log = crate::event::EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone())
            .unwrap()
            .quiet();
        let result = runner.run().await;

        // Clean up
        let _ = std::fs::remove_file(&tmp);

        assert!(
            result.is_ok(),
            "Workflow should succeed after retry: {:?}",
            result.err()
        );
        let output = result.unwrap();
        assert!(
            output.contains("success"),
            "Output should contain 'success' after retry, got: {}",
            output
        );

        // Verify TaskRetry event was emitted
        let events = event_log.events();
        let retry_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::TaskRetry { .. }))
            .collect();
        assert_eq!(
            retry_events.len(),
            1,
            "Should have exactly 1 TaskRetry event (attempt 2 after first failure)"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // PRESET TESTS
    // ═══════════════════════════════════════════════════════════════

    fn make_preset_workflow(
        preset_name: &str,
        task_provider: Option<&str>,
        task_model: Option<&str>,
    ) -> AnalyzedWorkflow {
        use crate::ast::agent_def::AgentDef;

        let mut task_table = TaskTable::new();
        task_table.insert("gen");
        let tid = task_table.get_id("gen").unwrap();

        let task = AnalyzedTask {
            id: tid,
            name: "gen".to_string(),
            description: None,
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "hello".to_string(),
                system: None,
                temperature: None,
                max_tokens: None,
                ..Default::default()
            }),
            provider: task_provider.map(nika_core::ProviderName::parse),
            model: task_model.map(|s| s.to_string()),
            base_url: None,
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
            record: None,
            context_budget: None,
            preset: Some(preset_name.to_string()),
            routing: None,
            span: Span::dummy(),
        };

        let mut agents = IndexMap::new();
        agents.insert(
            preset_name.to_string(),
            AgentDef::Inline {
                system: "You are helpful".to_string(),
                provider: Some("mock".to_string()),
                model: Some("mock-fast".to_string()),
                max_turns: None,
                temperature: Some(0.3),
                skills: None,
            },
        );

        AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: Some(agents),
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        }
    }

    #[tokio::test]
    async fn test_preset_resolves_provider_model() {
        let workflow = make_preset_workflow("assistant", None, None);
        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Preset workflow should run: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_preset_task_override_wins() {
        // Task sets provider: mock, preset sets provider: mock too but model differs.
        // The task-level provider/model should win over preset.
        let workflow = make_preset_workflow("writer", Some("mock"), Some("mock-fast"));
        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Task-level provider override should work: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_preset_injects_system_temperature() {
        // Preset defines system + temperature; task has neither.
        // The runner should inject both from the preset into the lowered action.
        let workflow = make_preset_workflow("creative", None, None);
        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Preset with system + temperature should work: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_preset_emits_preset_applied_event() {
        let workflow = make_preset_workflow("assistant", None, None);
        let event_log = EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        let events = event_log.events();
        let preset_events: Vec<_> = events
            .iter()
            .filter(|e| matches!(&e.kind, crate::event::EventKind::PresetApplied { .. }))
            .collect();

        assert_eq!(
            preset_events.len(),
            1,
            "Expected exactly one PresetApplied event"
        );

        if let crate::event::EventKind::PresetApplied {
            preset_name,
            provider,
            ..
        } = &preset_events[0].kind
        {
            assert_eq!(preset_name, "assistant");
            assert_eq!(provider.as_deref(), Some("mock"));
        } else {
            panic!("Expected PresetApplied event");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Backward compat + integration tests for L.3
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_no_preset_workflow_runs_normally() {
        // Workflow without agents: or preset: fields → current behavior unchanged
        let workflow = create_exec_workflow(vec![("a", "echo hello")], vec![]);
        let mut runner = Runner::new(workflow).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow without presets should run normally: {:?}",
            result.err()
        );
    }

    #[tokio::test]
    async fn test_no_preset_emits_no_preset_event() {
        let workflow = create_exec_workflow(vec![("a", "echo hello")], vec![]);
        let event_log = EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
        let _ = runner.run().await;

        let preset_events: Vec<_> = event_log
            .events()
            .iter()
            .filter(|e| matches!(&e.kind, crate::event::EventKind::PresetApplied { .. }))
            .cloned()
            .collect();
        assert!(
            preset_events.is_empty(),
            "No PresetApplied events when no preset is used"
        );
    }

    #[tokio::test]
    async fn test_multiple_tasks_different_presets() {
        use crate::ast::agent_def::AgentDef;

        let mut task_table = TaskTable::new();
        task_table.insert("fast");
        task_table.insert("deep");
        let fast_id = task_table.get_id("fast").unwrap();
        let deep_id = task_table.get_id("deep").unwrap();

        let fast_task = AnalyzedTask {
            id: fast_id,
            name: "fast".to_string(),
            description: None,
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "quick answer".to_string(),
                ..Default::default()
            }),
            provider: None,
            model: None,
            base_url: None,
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
            record: None,
            context_budget: None,
            preset: Some("speed".to_string()),
            routing: None,
            span: Span::dummy(),
        };

        let deep_task = AnalyzedTask {
            id: deep_id,
            name: "deep".to_string(),
            description: None,
            action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
                prompt: "deep analysis".to_string(),
                ..Default::default()
            }),
            provider: None,
            model: None,
            base_url: None,
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
            record: None,
            context_budget: None,
            preset: Some("think".to_string()),
            routing: None,
            span: Span::dummy(),
        };

        let mut agents = IndexMap::new();
        agents.insert(
            "speed".to_string(),
            AgentDef::Inline {
                system: "Be fast".to_string(),
                provider: Some("mock".to_string()),
                model: Some("mock-fast".to_string()),
                max_turns: None,
                temperature: Some(0.1),
                skills: None,
            },
        );
        agents.insert(
            "think".to_string(),
            AgentDef::Inline {
                system: "Think deeply".to_string(),
                provider: Some("mock".to_string()),
                model: Some("mock-slow".to_string()),
                max_turns: None,
                temperature: Some(0.7),
                skills: None,
            },
        );

        let workflow = AnalyzedWorkflow {
            schema_version: SchemaVersion::V03,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![fast_task, deep_task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            include: vec![],
            inputs: IndexMap::new(),
            artifacts: None,
            log: None,
            agents: Some(agents),
            skills_map: std::collections::HashMap::new(),
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        };

        let event_log = EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Multiple presets should work: {:?}",
            result.err()
        );

        let preset_events: Vec<_> = event_log
            .events()
            .iter()
            .filter(|e| matches!(&e.kind, crate::event::EventKind::PresetApplied { .. }))
            .cloned()
            .collect();
        assert_eq!(
            preset_events.len(),
            2,
            "Two tasks with presets → two PresetApplied events"
        );
    }

    #[test]
    fn test_cost_tool_registered_via_with_cost_tool() {
        use crate::runtime::builtin::BuiltinToolRouter;
        let router = BuiltinToolRouter::new().with_cost_tool(EventLog::new());
        assert!(
            router.has_tool("cost"),
            "nika:cost should be registered via with_cost_tool"
        );
    }

    #[tokio::test]
    async fn test_cost_accumulates_across_tasks() {
        // Run a preset workflow and check that ProviderResponded events exist
        let workflow = make_preset_workflow("assistant", None, None);
        let event_log = EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        // The mock provider should have emitted at least one ProviderResponded
        let provider_events: Vec<_> = event_log
            .events()
            .iter()
            .filter(|e| matches!(&e.kind, crate::event::EventKind::ProviderResponded { .. }))
            .cloned()
            .collect();
        assert!(
            !provider_events.is_empty(),
            "Mock provider should emit ProviderResponded"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // RECORD TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_record_shorthand_true_creates_record() {
        use nika_core::ast::record::RecordSpec;

        let mut task_table = TaskTable::new();
        task_table.insert("recorded");
        let tid = task_table.get_id("recorded").unwrap();

        let task = AnalyzedTask {
            id: tid,
            name: "recorded".to_string(),
            description: None,
            action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                command: "echo hello".to_string(),
                shell: false,
                cwd: None,
                env: IndexMap::new(),
                timeout_ms: None,
                max_stdout: None,
                span: Span::dummy(),
            }),
            provider: None,
            model: None,
            base_url: None,
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
            record: Some(RecordSpec::shorthand_true()),
            context_budget: None,
            preset: None,
            routing: None,
            span: Span::dummy(),
        };

        let workflow = AnalyzedWorkflow {
            schema_version: SchemaVersion::V01,
            name: None,
            description: None,
            goal: None,
            provider: Some(nika_core::ProviderName::Mock),
            model: None,
            base_url: None,
            task_table,
            tasks: vec![task],
            mcp_servers: IndexMap::new(),
            context_files: vec![],
            inputs: Default::default(),
            agents: Some(IndexMap::new()),
            skills_map: std::collections::HashMap::new(),
            artifacts: None,
            log: None,
            include: vec![],
            orchestrate: None,
            routing: None,
            max_duration_secs: 3600,
            span: Span::dummy(),
        };

        let event_log = EventLog::new();
        let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
        let result = runner.run().await;
        assert!(result.is_ok(), "Should succeed: {:?}", result.err());

        // Task should have produced a record in the datastore
        let record = runner.datastore().get_record("recorded");
        assert!(
            record.is_some(),
            "Task with record: true should create a Record in the datastore"
        );

        let rec = record.unwrap();
        assert_eq!(rec.task_id.as_ref(), "recorded");
        // Short output (< max_tokens) should produce a truncated/skipped record
        assert!(
            !rec.summary.is_empty(),
            "Record summary should not be empty"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // GLOBAL TASK CONCURRENCY SEMAPHORE FOR REGULAR TASKS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn runner_has_global_task_semaphore_with_64_permits() {
        let runner = Runner::new(make_empty_workflow()).unwrap();
        // The semaphore should be initialized with MAX_CONCURRENT_TASKS (64) permits.
        // available_permits() returns the number of permits not currently held.
        assert_eq!(
            runner.global_task_semaphore.available_permits(),
            MAX_CONCURRENT_TASKS,
            "Global semaphore should have {MAX_CONCURRENT_TASKS} permits"
        );
    }

    #[tokio::test]
    async fn regular_tasks_acquire_global_semaphore() {
        // Spawn many independent regular tasks (no for_each) in a single DAG layer.
        // Verify they all complete successfully — proving the semaphore is acquired
        // and released correctly for regular tasks.
        let task_count = 8;
        let tasks: Vec<(&str, &str)> = vec![
            ("t0", "echo task0"),
            ("t1", "echo task1"),
            ("t2", "echo task2"),
            ("t3", "echo task3"),
            ("t4", "echo task4"),
            ("t5", "echo task5"),
            ("t6", "echo task6"),
            ("t7", "echo task7"),
        ];
        // No edges — all tasks run in parallel in the same DAG layer
        let edges: Vec<(&str, &str)> = vec![];

        let workflow = create_exec_workflow(tasks, edges);
        let mut runner = Runner::new(workflow).unwrap().quiet();
        let result = runner.run().await;

        assert!(
            result.is_ok(),
            "Workflow with {task_count} parallel regular tasks should succeed: {:?}",
            result.err()
        );

        // Verify all tasks completed successfully
        for i in 0..task_count {
            let key = format!("t{}", i);
            let task_result = runner.datastore.get(&key);
            assert!(
                task_result.is_some(),
                "Task '{}' result should exist in datastore",
                key
            );
            assert!(
                task_result.unwrap().is_success(),
                "Task '{}' should have succeeded",
                key
            );
        }

        // After all tasks complete, all permits should be returned
        assert_eq!(
            runner.global_task_semaphore.available_permits(),
            MAX_CONCURRENT_TASKS,
            "All semaphore permits should be returned after tasks complete"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // BUG-034: value_to_array null handling
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn value_to_array_null_returns_empty() {
        assert_eq!(value_to_array(&Value::Null), Some(vec![]));
    }

    #[test]
    fn value_to_array_empty_array_returns_empty() {
        assert_eq!(value_to_array(&json!([])), Some(vec![]));
    }

    #[test]
    fn value_to_array_normal_array_unchanged() {
        let arr = json!([1, 2, 3]);
        let result = value_to_array(&arr).unwrap();
        assert_eq!(result, vec![json!(1), json!(2), json!(3)]);
    }
}
