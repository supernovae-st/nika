// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Runner - workflow execution with tokio
//!
//! Performance optimizations:
//! - Arc for zero-cost task/context sharing
//! - JoinSet for efficient parallel task collection
//! - Tokio handles all concurrency (no artificial limits)

use futures::FutureExt;
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

use crate::ast::analyzed::{AnalyzedTask, AnalyzedWorkflow};
use crate::ast::lower::lower_mcp_servers_with_resolver;
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
use super::output::extract_json;
use super::resolver::{resolve_assets_analyzed, ResolvedAssets};
use super::task_dispatch::{execute_task_iteration, IterationResult};

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
        if crate::secrets::has_provider_key(p) {
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
pub(crate) fn value_to_array(value: &Value) -> Option<Vec<Value>> {
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
/// Extract a human-readable message from a `catch_unwind` panic payload.
fn extract_panic_message(panic_info: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic_info.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic_info.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
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

/// State produced by [`Runner::init_run`] and consumed by the DAG loop and finalize phases.
///
/// Extracted so that `run()` reads as init → loop → finalize glue.
struct InitResult {
    base_path: PathBuf,
    #[allow(dead_code)]
    total_tasks: usize,
    cached_depths: Option<rustc_hash::FxHashMap<String, usize>>,
    pending_indices: Vec<usize>,
}

impl Runner {
    /// Create a Runner without policy enforcement.
    ///
    /// Use `Runner::with_policy()` for production server contexts (nika-serve)
    /// where policy constraints (allowed_hosts, max_token_spend) are required.
    /// This constructor is suitable for CLI usage and tests.
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
        // Default invocation source is Cli — overridable via builder
        // (`with_invocation_source`) before `run()`. nika-serve and nika:run
        // call the builder to set Serve / NestedRun respectively.
        let mut datastore = RunContext::new(nika_core::trust::InvocationSource::Cli);

        // Load vault if available (for $vault.SERVICE.FIELD bindings)
        if let Some(vault) = crate::secrets::vault::try_open_vault() {
            datastore.set_vault(Arc::new(vault));
        }

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
        // Default invocation source is Cli — overridable via builder.
        let mut datastore = RunContext::new(nika_core::trust::InvocationSource::Cli);

        // Load vault if available (for $vault.SERVICE.FIELD bindings)
        if let Some(vault) = crate::secrets::vault::try_open_vault() {
            datastore.set_vault(Arc::new(vault));
        }

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
    #[must_use]
    pub fn quiet(mut self) -> Self {
        self.quiet = true;
        self
    }

    /// Override the workflow's invocation source — controls the trust floor
    /// for `inputs:` bindings (Nika Shield).
    ///
    /// Use `InvocationSource::Cli` for direct CLI runs (default), `Serve`
    /// for `nika serve`, `NestedRun { ceiling }` from inside `nika:run`, and
    /// `Unknown` only when the embedding caller genuinely cannot tell.
    #[must_use]
    pub fn with_invocation_source(mut self, source: nika_core::trust::InvocationSource) -> Self {
        self.datastore.set_invocation_source(source);
        self
    }

    /// Set the CLI detail level for event rendering.
    ///
    /// Automatically selects Live (animated) or Classic (append-only)
    /// renderer based on TTY detection and detail level.
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn with_base_path(mut self, path: std::path::PathBuf) -> Self {
        self.executor = self.executor.with_base_path(path);
        self
    }

    /// Set the project root directory (parent of nika.toml).
    ///
    /// Used by `working_dir_mode = "project"` to set exec task cwd.
    #[must_use]
    pub fn with_project_root(mut self, root: std::path::PathBuf) -> Self {
        self.executor = self.executor.with_project_root(root);
        self
    }

    /// Set the working directory mode from `[tools] working_dir` in nika.toml.
    ///
    /// - `"project"` → exec tasks default cwd to project_root
    /// - `"workflow"` → exec tasks default cwd to workflow_base_dir
    /// - `"none"` → no default cwd, inherit process cwd
    #[must_use]
    pub fn with_working_dir_mode(mut self, mode: String) -> Self {
        self.executor = self.executor.with_working_dir_mode(mode);
        self
    }

    #[must_use]
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

    // execute_task_iteration has been extracted to task_dispatch.rs

    // NOTE: execute_task_iteration (627 lines) extracted to task_dispatch.rs
    // Call it as: execute_task_iteration(...) (free function, not Self::)

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

    /// Phase 1 of `run()`: load context/inputs/skills, resolve agents,
    /// emit WorkflowStarted, compute DAG layers, set up CLI renderer.
    ///
    /// Returns [`InitResult`] carrying local state the DAG loop and
    /// finalize phase need. The RAII lockfile guard stays in `run()`
    /// so it outlives both phases.
    async fn init_run(&mut self) -> Result<InitResult, NikaError> {
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

        // Load inputs BEFORE context files so that {{inputs.*}} templates
        // in context file paths can be resolved (BUG-5 fix).
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

        if !self.workflow.context_files.is_empty() {
            // Resolve {{inputs.*}} templates in context file paths before loading.
            let mut resolved_files = self.workflow.context_files.clone();
            for cf in &mut resolved_files {
                if cf.path.contains("{{") {
                    let empty_with = rustc_hash::FxHashMap::default();
                    cf.path = crate::binding::template_resolve_with(
                        &cf.path,
                        &empty_with,
                        &self.datastore,
                    )?
                    .into_owned();
                }
            }
            let loaded_context = load_context_analyzed(&resolved_files, &base_path).await?;
            self.datastore.set_context(loaded_context);
            debug!("Loaded {} context files", self.workflow.context_files.len());
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
        // Converted to owned strings so InitResult can outlive the borrow of self.workflow.
        let cached_depths: Option<rustc_hash::FxHashMap<String, usize>> = if total_tasks > 1 {
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
            let layers = crate::dag::flow::compute_layers(&nodes, &edges);
            Some(
                layers
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            )
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

        // Wire workflow tasks into executor for on_error: fallback lookups.
        self.executor = self
            .executor
            .clone()
            .with_workflow_tasks(self.workflow.tasks.clone());

        // Pending task indices — shrinks as tasks complete, so get_ready_tasks()
        // only checks remaining tasks instead of rescanning the full task list.
        let pending_indices: Vec<usize> = (0..self.workflow.tasks.len()).collect();

        Ok(InitResult {
            base_path,
            total_tasks,
            cached_depths,
            pending_indices,
        })
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

        // Phase 1: Initialize — context, inputs, skills, agents, DAG layers, renderer
        let mut init = self.init_run().await?;

        // RAII lockfile: auto-removed on all exit paths (normal, error, panic).
        // Must outlive both the DAG loop and finalize, so it stays here in run().
        let _lockfile_guard = LockfileGuard::create(
            init.base_path
                .join(".nika")
                .join("media")
                .join("store")
                .join(".nika-run.lock"),
        );

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

            let ready = self.get_ready_tasks(&mut init.pending_indices);

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
            let artifact_base_path = init.base_path.clone();
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

                        // Unified binding resolution (replaces 4 inline format branches)
                        use super::for_each::{resolve_for_each_binding, ForEachResolution};
                        match resolve_for_each_binding(items_str, &bindings, &self.datastore) {
                            ForEachResolution::Items(items) => Some(items),
                            ForEachResolution::NotABinding => None,
                            ForEachResolution::Failed(err_msg) => {
                                self.emit_scheduling_failure(&task.name, &err_msg, "NIKA-026");
                                self.datastore.insert(
                                    intern(&task.name),
                                    TaskResult::failed(err_msg, std::time::Duration::ZERO),
                                );
                                continue;
                            }
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
                        // Note: bindings aren't available here (they're per-iteration),
                        // but inputs/context are available via empty bindings + datastore.
                        let fe = task.for_each.as_ref();
                        let empty_bindings = crate::binding::ResolvedBindings::new();
                        let concurrency = fe
                            .and_then(|f| {
                                crate::runtime::resolve_typed::resolve_task_u32(
                                    &f.concurrency,
                                    &empty_bindings,
                                    &self.datastore,
                                    "concurrency",
                                )
                                .ok()
                                .flatten()
                            })
                            .or_else(|| {
                                crate::runtime::resolve_typed::resolve_task_u32(
                                    &task.concurrency,
                                    &empty_bindings,
                                    &self.datastore,
                                    "concurrency",
                                )
                                .ok()
                                .flatten()
                            })
                            .unwrap_or(1)
                            .max(1) as usize;
                        let fail_fast = fe
                            .map(|f| {
                                crate::runtime::resolve_typed::resolve_required_bool(
                                    &f.fail_fast,
                                    &empty_bindings,
                                    &self.datastore,
                                    "fail_fast",
                                    true,
                                )
                                .unwrap_or(true)
                            })
                            .or_else(|| {
                                crate::runtime::resolve_typed::resolve_task_bool(
                                    &task.fail_fast,
                                    &empty_bindings,
                                    &self.datastore,
                                    "fail_fast",
                                )
                                .ok()
                                .flatten()
                            })
                            .unwrap_or(true);

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
                                // Wrap in catch_unwind so panics inside the iteration
                                // produce a proper IterationResult with task_id (W6.5).
                                let tid_for_panic = Arc::clone(&task_id);
                                let ptid_for_panic = Arc::clone(&parent_task_id);
                                let result = match std::panic::AssertUnwindSafe(
                                    execute_task_iteration(
                                        task,
                                        Arc::clone(&task_id),
                                        Arc::clone(&parent_task_id),
                                        datastore,
                                        executor,
                                        event_log.clone(),
                                        Some((var_name, item, idx)),
                                        workflow_artifacts,
                                        artifact_base_path,
                                        false, // not a fallback execution
                                    ),
                                )
                                .catch_unwind()
                                .await
                                {
                                    Ok(result) => result,
                                    Err(panic_info) => {
                                        let msg = extract_panic_message(&panic_info);
                                        tracing::error!(
                                            task_id = %tid_for_panic,
                                            panic_message = %msg,
                                            "for_each iteration panicked"
                                        );
                                        let reason = format!(
                                            "task '{}' panicked: {}",
                                            tid_for_panic, msg
                                        );
                                        IterationResult {
                                            store_id: tid_for_panic,
                                            result: TaskResult::failed(
                                                reason,
                                                std::time::Duration::ZERO,
                                            ),
                                            for_each_info: Some((ptid_for_panic, idx)),
                                        }
                                    }
                                };

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
                        // Wrap in catch_unwind so panics inside the task produce a
                        // proper IterationResult with the task_id instead of a bare
                        // JoinError that loses all context (W6.5).
                        let tid_for_panic = Arc::clone(&task_id);
                        match std::panic::AssertUnwindSafe(execute_task_iteration(
                            task,
                            Arc::clone(&task_id),
                            task_id,
                            datastore,
                            executor,
                            event_log,
                            None,
                            workflow_artifacts,
                            artifact_base_path,
                            false, // not a fallback execution
                        ))
                        .catch_unwind()
                        .await
                        {
                            Ok(result) => result,
                            Err(panic_info) => {
                                let msg = extract_panic_message(&panic_info);
                                tracing::error!(
                                    task_id = %tid_for_panic,
                                    panic_message = %msg,
                                    "Task panicked during execution"
                                );
                                let reason = format!("task '{}' panicked: {}", tid_for_panic, msg);
                                IterationResult {
                                    store_id: tid_for_panic,
                                    result: TaskResult::failed(reason, std::time::Duration::ZERO),
                                    for_each_info: None,
                                }
                            }
                        }
                    });
                }
            }

            self.cli_renderer = renderer;

            // Collect for_each results for aggregation: parent_id -> Vec<(index, result)>
            // Use IndexMap to preserve insertion order (deterministic iteration)
            let mut for_each_results: IndexMap<Arc<str>, Vec<(usize, TaskResult)>> =
                IndexMap::new();

            // Clamp max_duration_secs to prevent Instant overflow (max ~292 years)
            // Resolve template using empty bindings (workflow-level, no task context)
            let empty_bindings = crate::binding::ResolvedBindings::new();
            let clamped_duration = crate::runtime::resolve_typed::resolve_required_u64(
                &self.workflow.max_duration_secs,
                &empty_bindings,
                &self.datastore,
                "max_duration_secs",
                3600,
            )
            .unwrap_or(3600)
            .min(604_800); // Cap at 1 week
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

                        let max_dur = clamped_duration;
                        self.event_log.emit(EventKind::WorkflowAborted {
                            reason: format!(
                                "Workflow exceeded max_duration_secs ({} seconds)",
                                max_dur
                            ),
                            duration_ms: duration.as_millis() as u64,
                            running_tasks: running_tasks.clone(),
                        });
                        self.write_trace();
                        return Err(NikaError::WorkflowTimeout {
                            duration_secs: max_dur,
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
                // All usable = all iterations either succeeded or were skipped (no actual failures)
                let all_success = results.iter().all(|(_, r)| r.is_usable());

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
                            trust_level: nika_core::trust::TrustLevel::Trusted,
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

        // Phase 3: Finalize — media integrity, artifacts, trace, summary, MCP shutdown
        self.finalize_run(&init, workflow_start).await
    }

    /// Phase 3 of `run()`: media integrity check, artifact manifest, orchestrator
    /// completion events, CAS GC, record persistence, trace, summary, MCP shutdown.
    async fn finalize_run(
        &mut self,
        init: &InitResult,
        workflow_start: Instant,
    ) -> Result<String, NikaError> {
        // Verify media integrity (warn-only, never fail successful workflows)
        let media_warnings = self.verify_media_integrity();

        // Write artifact manifest if configured
        if let Some(ref artifacts_config) = self.workflow.artifacts {
            write_artifact_manifest(&self.event_log, artifacts_config, &init.base_path);
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
                .and_then(|result| match result.output.as_ref() {
                    Value::String(s) => serde_json::from_str::<serde_json::Value>(s)
                        .ok()
                        .and_then(|v| v["confidence"].as_f64()),
                    v => v.get("confidence").and_then(|c| c.as_f64()),
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
        let cas_store = CasStore::workspace_default(&init.base_path);
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
            let parallel_count = if let Some(ref depths) = init.cached_depths {
                let max_layer: usize = depths.values().copied().max().unwrap_or(0);
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
mod tests;

#[cfg(test)]
mod tests_golden_verbs;
