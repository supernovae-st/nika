// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! RunContext - task output storage with DashMap
//!
//! Single HashMap design with lock-free concurrent access.
//! Path resolution unified with jsonpath module.
//!
//! Added context storage for workflow `context:` block.
//! Added inputs storage for workflow `inputs:` block.

use std::borrow::Cow;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use dashmap::DashMap;
use parking_lot::RwLock;
use rustc_hash::{FxBuildHasher, FxHashMap};
use serde_json::Value;

use super::context::LoadedContext;
use crate::binding::jsonpath;

/// Task execution status
#[derive(Debug, Clone)]
pub enum TaskOutcome {
    Success,
    /// Partial success: some for_each iterations succeeded, some failed.
    /// Downstream tasks can still use the output (nulls for failed iterations).
    PartialSuccess {
        error_summary: String,
        succeeded: u32,
        failed: u32,
    },
    Failed(String),
    /// Task cannot run because a dependency failed
    DependencyFailed {
        /// ID of the failed dependency
        dependency: String,
    },
    /// Task was skipped (not executed)
    Skipped {
        /// Reason for skipping
        reason: String,
    },
}

/// Task execution result (unified storage)
#[derive(Debug, Clone)]
pub struct TaskResult {
    /// Output as JSON Value (Arc for O(1) cloning of large JSON structures)
    pub output: Arc<Value>,
    /// Execution duration
    pub duration: Duration,
    /// Success or failure status
    pub status: TaskOutcome,
    /// Media files produced by this task (empty for non-media tasks)
    pub media: Vec<crate::media::MediaRef>,
    /// Trust level of this task's output (Nika Shield).
    /// Defaults to Trusted for backward compatibility.
    pub trust_level: nika_core::trust::TrustLevel,
}

impl TaskResult {
    /// Create a successful result
    pub fn success(output: impl Into<Value>, duration: Duration) -> Self {
        Self {
            output: Arc::new(output.into()),
            duration,
            status: TaskOutcome::Success,
            media: Vec::new(),
            trust_level: nika_core::trust::TrustLevel::Trusted,
        }
    }

    /// Create a successful result from string (converts to Value::String)
    pub fn success_str(output: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::String(output.into())),
            duration,
            status: TaskOutcome::Success,
            media: Vec::new(),
            trust_level: nika_core::trust::TrustLevel::Trusted,
        }
    }

    /// Create a failed result
    pub fn failed(error: impl Into<String>, duration: Duration) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration,
            status: TaskOutcome::Failed(error.into()),
            media: Vec::new(),
            trust_level: nika_core::trust::TrustLevel::Trusted,
        }
    }

    /// Create a result for a task that cannot run because its dependency failed
    ///
    /// This is distinct from `failed()` because the task itself didn't fail -
    /// it simply cannot run because an upstream dependency failed.
    pub fn dependency_failed(dependency: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskOutcome::DependencyFailed {
                dependency: dependency.into(),
            },
            media: Vec::new(),
            trust_level: nika_core::trust::TrustLevel::Trusted,
        }
    }

    /// Create a skipped result
    ///
    /// Used when a task is skipped due to cancellation or other reasons.
    pub fn skipped(reason: impl Into<String>) -> Self {
        Self {
            output: Arc::new(Value::Null),
            duration: Duration::ZERO,
            status: TaskOutcome::Skipped {
                reason: reason.into(),
            },
            media: Vec::new(),
            trust_level: nika_core::trust::TrustLevel::Trusted,
        }
    }

    /// Attach media references to this result.
    pub fn with_media(mut self, media: Vec<crate::media::MediaRef>) -> Self {
        self.media = media;
        self
    }

    /// Set the trust level for this result (Nika Shield).
    pub fn with_trust(mut self, level: nika_core::trust::TrustLevel) -> Self {
        self.trust_level = level;
        self
    }

    /// Check if task succeeded (strict — excludes partial success)
    pub fn is_success(&self) -> bool {
        matches!(self.status, TaskOutcome::Success)
    }

    /// Check if task output is usable by downstream tasks.
    ///
    /// Returns true for both Success and PartialSuccess. Used for dependency
    /// gating: downstream tasks can run if the upstream produced usable output.
    pub fn is_usable(&self) -> bool {
        matches!(
            self.status,
            TaskOutcome::Success | TaskOutcome::PartialSuccess { .. } | TaskOutcome::Skipped { .. }
        )
    }

    /// Check if task failed due to a dependency failure
    pub fn is_dependency_failed(&self) -> bool {
        matches!(self.status, TaskOutcome::DependencyFailed { .. })
    }

    /// Check if task was skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self.status, TaskOutcome::Skipped { .. })
    }

    /// Get the failed dependency name if this is a DependencyFailed result
    pub fn failed_dependency(&self) -> Option<&str> {
        match &self.status {
            TaskOutcome::DependencyFailed { dependency } => Some(dependency),
            _ => None,
        }
    }

    /// Get error message if failed
    pub fn error(&self) -> Option<&str> {
        match &self.status {
            TaskOutcome::Failed(e) => Some(e),
            TaskOutcome::PartialSuccess { error_summary, .. } => Some(error_summary),
            TaskOutcome::DependencyFailed { dependency } => Some(dependency),
            TaskOutcome::Skipped { reason } => Some(reason),
            TaskOutcome::Success => None,
        }
    }

    /// Get output as string (zero-copy for String values)
    pub fn output_str(&self) -> Cow<'_, str> {
        match &*self.output {
            Value::String(s) => Cow::Borrowed(s),
            other => Cow::Owned(other.to_string()),
        }
    }

    /// Estimate the in-memory size of the output in bytes.
    ///
    /// Uses string length for strings, recursive estimate for JSON objects/arrays.
    pub fn output_size_estimate(&self) -> usize {
        estimate_value_size(&self.output)
    }

    /// Maximum allowed task output size (50 MB).
    pub const MAX_OUTPUT_SIZE: usize = 50 * 1024 * 1024;

    /// If output exceeds MAX_OUTPUT_SIZE, truncate it and return the original size.
    /// Returns `Some(original_size)` if truncation happened, `None` otherwise.
    pub fn truncate_if_oversized(&mut self) -> Option<usize> {
        let size = self.output_size_estimate();
        if size > Self::MAX_OUTPUT_SIZE {
            let truncated = format!(
                "[TRUNCATED: output was {} bytes, limit is {} bytes]",
                size,
                Self::MAX_OUTPUT_SIZE
            );
            self.output = Arc::new(Value::String(truncated));
            Some(size)
        } else {
            None
        }
    }
}

/// Recursively estimate the in-memory byte size of a serde_json::Value.
/// Bounded to 128 levels of nesting to prevent stack overflow on pathological inputs.
fn estimate_value_size(value: &Value) -> usize {
    estimate_value_size_bounded(value, 0)
}

fn estimate_value_size_bounded(value: &Value, depth: usize) -> usize {
    if depth > 128 {
        return 1024; // Conservative estimate for deeply nested values
    }
    match value {
        Value::Null | Value::Bool(_) => 8,
        Value::Number(_) => 16,
        Value::String(s) => s.len() + 24,
        Value::Array(arr) => {
            24 + arr
                .iter()
                .map(|v| estimate_value_size_bounded(v, depth + 1))
                .sum::<usize>()
        }
        Value::Object(map) => {
            48 + map
                .iter()
                .map(|(k, v)| k.len() + 24 + estimate_value_size_bounded(v, depth + 1))
                .sum::<usize>()
        }
    }
}

/// Thread-safe storage for task results (lock-free)
///
/// Uses `Arc<str>` keys for zero-cost cloning with same Arc used in events.
///
/// Added context storage for workflow `context:` block.
/// Added inputs storage for workflow `inputs:` block.
#[derive(Clone)]
pub struct RunContext {
    /// Task results: task_id → TaskResult
    results: Arc<DashMap<Arc<str>, TaskResult, FxBuildHasher>>,

    /// Context loaded at workflow start
    ///
    /// Contains files loaded from the `context:` block.
    /// Accessible via `{{context.files.alias}}` bindings.
    context: Arc<RwLock<LoadedContext>>,

    /// Input parameters with defaults
    ///
    /// Contains input definitions from the `inputs:` block.
    /// Accessible via `{{inputs.param}}` bindings.
    inputs: Arc<RwLock<FxHashMap<String, Value>>>,

    /// Side-channel for media refs produced by invoke tasks.
    /// Written by run_invoke() after MediaProcessor completes.
    /// Read (and drained) by the runner after building TaskResult.
    media_staging: Arc<DashMap<Arc<str>, Vec<crate::media::MediaRef>, FxBuildHasher>>,

    /// Compressed records: task_id → `Arc<Record>` (zero-copy reads)
    records: Arc<DashMap<Arc<str>, Arc<crate::runtime::record::Record>, FxBuildHasher>>,

    /// Shared per-run media budget (500MB default).
    /// Lives here so all invoke tasks in a single run share one budget.
    media_budget: Arc<crate::media::MediaBudget>,

    /// Workspace root for CAS store path resolution.
    /// Set by Runner at workflow start. Defaults to current_dir().
    workspace_root: Arc<RwLock<PathBuf>>,

    /// Optional NikaVault for `$vault.SERVICE.FIELD` bindings.
    ///
    /// Set by the runner at workflow start. When `None`, vault bindings
    /// return a clear error ("vault not configured").
    vault: Option<Arc<nika_vault::NikaVault>>,

    /// How the workflow was invoked — determines the trust floor for `inputs:`
    /// bindings (Nika Shield, P0-3 hardening). Required argument to `new()`
    /// so embedded SDK consumers cannot accidentally fail open.
    invocation_source: nika_core::trust::InvocationSource,
}

impl Default for RunContext {
    /// Default constructor — uses `InvocationSource::Unknown` which is
    /// **fail-closed** (inputs treated as `Untrusted`). Production callers
    /// must use `RunContext::new(InvocationSource::*)` instead so the trust
    /// floor is explicit.
    fn default() -> Self {
        Self::with_invocation_source(nika_core::trust::InvocationSource::Unknown)
    }
}

impl RunContext {
    /// Create a new run context with an explicit invocation source.
    ///
    /// **The caller MUST specify the source.** Use `InvocationSource::Cli`
    /// for local CLI runs, `Serve` for `nika serve`, `Test` for unit tests,
    /// `NestedRun { ceiling }` from inside `nika:run`, and `Unknown` only
    /// when the embedding SDK genuinely cannot tell — `Unknown` fails closed
    /// (inputs treated as `Untrusted`).
    pub fn new(invocation_source: nika_core::trust::InvocationSource) -> Self {
        Self::with_invocation_source(invocation_source)
    }

    /// Internal constructor — same as `new()` but spelled out for clarity
    /// at call sites that need to thread the source through a builder.
    fn with_invocation_source(invocation_source: nika_core::trust::InvocationSource) -> Self {
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            results: Arc::new(DashMap::with_hasher(FxBuildHasher)),
            context: Arc::default(),
            inputs: Arc::default(),
            records: Arc::new(DashMap::with_hasher(FxBuildHasher)),
            media_staging: Arc::new(DashMap::with_hasher(FxBuildHasher)),
            media_budget: Arc::new({
                let max = std::env::var("NIKA_MEDIA_BUDGET")
                    .ok()
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(crate::media::MediaBudget::DEFAULT_MAX_PER_RUN);
                crate::media::MediaBudget::with_max_per_run(max)
            }),
            workspace_root: Arc::new(RwLock::new(workspace_root)),
            vault: None,
            invocation_source,
        }
    }

    /// How this workflow run was invoked — determines the trust floor for
    /// `inputs:` bindings (Nika Shield).
    #[inline]
    #[must_use]
    pub fn invocation_source(&self) -> nika_core::trust::InvocationSource {
        self.invocation_source
    }

    /// Override the invocation source after construction. Used by the runner
    /// builder pattern (`Runner::with_invocation_source`).
    pub fn set_invocation_source(&mut self, source: nika_core::trust::InvocationSource) {
        self.invocation_source = source;
    }

    /// Insert a task result (accepts `Arc<str>` for zero-cost key reuse)
    pub fn insert(&self, task_id: Arc<str>, result: TaskResult) {
        self.results.insert(task_id, result);
    }

    /// Get a task result
    pub fn get(&self, task_id: &str) -> Option<TaskResult> {
        self.results.get(task_id).map(|r| r.value().clone())
    }

    /// Get the trust level of a completed task's output (Nika Shield).
    pub fn get_trust(&self, task_id: &str) -> Option<nika_core::trust::TrustLevel> {
        self.results.get(task_id).map(|r| r.trust_level)
    }

    /// Check task status without cloning the full TaskResult (M11 perf fix).
    pub fn status_of(&self, task_id: &str) -> Option<TaskOutcome> {
        self.results.get(task_id).map(|r| r.status.clone())
    }

    /// Check if task exists
    pub fn contains(&self, task_id: &str) -> bool {
        self.results.contains_key(task_id)
    }

    /// Check if a task completed successfully without cloning the full TaskResult.
    ///
    /// Returns None if the task doesn't exist, Some(true) if succeeded, Some(false) if failed.
    /// Avoids O(T*D) TaskResult cloning in get_ready_tasks() hot path.
    ///
    /// CONTRACT: DependencyFailed results MUST return Some(false), not None.
    /// This enables single-pass cascade propagation in get_ready_tasks() —
    /// when task A fails and B depends on A, B is marked DependencyFailed
    /// and stored in the same retain pass, so C (which depends on B) can
    /// see B's result immediately without a second loop iteration.
    pub fn is_completed_successfully(&self, task_id: &str) -> Option<bool> {
        self.results.get(task_id).map(|r| r.value().is_usable())
    }

    /// Iterate over all task results (cloned).
    ///
    /// Returns (task_id, TaskResult) pairs for all stored results.
    /// Used by integrity checks at workflow end.
    ///
    /// Note: for_each tasks store both individual iteration entries (task\[0\], task\[1\], ...)
    /// and an aggregated parent entry (task). Media refs appear in both, so callers
    /// doing per-file checks may see duplicates. This is acceptable for warn-only checks.
    pub fn iter_results(&self) -> Vec<(Arc<str>, TaskResult)> {
        self.results
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect()
    }

    /// Check if task succeeded
    pub fn is_success(&self, task_id: &str) -> bool {
        self.get(task_id).is_some_and(|r| r.is_success())
    }

    /// Check if task failed (either directly or due to dependency failure).
    /// Uses status_of() to avoid cloning the full TaskResult (M11 fix).
    pub fn is_failed(&self, task_id: &str) -> bool {
        self.status_of(task_id).is_some_and(|s| {
            matches!(
                s,
                TaskOutcome::Failed(_) | TaskOutcome::DependencyFailed { .. }
            )
        })
    }

    /// Check if task failed due to a dependency failure
    pub fn is_dependency_failed(&self, task_id: &str) -> bool {
        self.status_of(task_id)
            .is_some_and(|s| matches!(s, TaskOutcome::DependencyFailed { .. }))
    }

    /// Get the failed dependency name if task has DependencyFailed status
    pub fn get_failed_dependency(&self, task_id: &str) -> Option<String> {
        self.get(task_id)
            .and_then(|r| r.failed_dependency().map(String::from))
    }

    /// Get just the output Value for a task (for JSONPath resolution)
    /// Returns `Arc<Value>` for O(1) cloning instead of deep copy
    pub fn get_output(&self, task_id: &str) -> Option<Arc<Value>> {
        self.results.get(task_id).map(|r| Arc::clone(&r.output))
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // MEDIA STAGING
    // ═══════════════════════════════════════════════════════════════════════════

    /// Stage media refs for a task (called from run_invoke).
    pub fn set_media(&self, task_id: &Arc<str>, media: Vec<crate::media::MediaRef>) {
        if !media.is_empty() {
            self.media_staging.insert(Arc::clone(task_id), media);
        }
    }

    /// Take staged media refs for a task (called from runner after building TaskResult).
    /// Returns empty vec if no media was staged.
    pub fn take_media(&self, task_id: &Arc<str>) -> Vec<crate::media::MediaRef> {
        self.media_staging
            .remove(task_id)
            .map(|(_, v)| v)
            .unwrap_or_default()
    }

    /// Get the shared per-run media budget.
    pub fn media_budget(&self) -> &Arc<crate::media::MediaBudget> {
        &self.media_budget
    }

    // ── Record storage ────────────────────────────────────────────

    /// Store a compressed record for a task.
    pub fn set_record(&self, task_id: Arc<str>, record: crate::runtime::record::Record) {
        self.records.insert(task_id, Arc::new(record));
    }

    /// Get a record for a task (if compression was applied). O(1) clone via Arc.
    pub fn get_record(&self, task_id: &str) -> Option<Arc<crate::runtime::record::Record>> {
        self.records.get(task_id).map(|r| Arc::clone(r.value()))
    }

    /// Check if a task has a compressed record.
    pub fn has_record(&self, task_id: &str) -> bool {
        self.records.contains_key(task_id)
    }

    /// Iterate all records. O(1) clone per entry via Arc.
    pub fn iter_records(&self) -> Vec<(Arc<str>, Arc<crate::runtime::record::Record>)> {
        self.records
            .iter()
            .map(|r| (Arc::clone(r.key()), Arc::clone(r.value())))
            .collect()
    }

    /// Set the workspace root (called by Runner at workflow start).
    pub fn set_workspace_root(&self, root: PathBuf) {
        *self.workspace_root.write() = root;
    }

    /// Get the workspace root path (cloned).
    pub fn workspace_root(&self) -> PathBuf {
        self.workspace_root.read().clone()
    }

    /// Set the NikaVault for `$vault.SERVICE.FIELD` bindings.
    pub fn set_vault(&mut self, vault: Arc<nika_vault::NikaVault>) {
        self.vault = Some(vault);
    }

    /// Get a credential field from the vault.
    ///
    /// Returns `Ok(Some(value))` if the service/field exists.
    /// Returns `Ok(None)` if no vault is configured or field not found.
    /// Returns `Err` if vault I/O or crypto fails.
    pub fn vault_get_credential(
        &self,
        service: &str,
        field: &str,
    ) -> Result<Option<String>, nika_vault::VaultError> {
        let vault = match &self.vault {
            Some(v) => v,
            None => return Ok(None),
        };
        vault
            .get_credential(service, field)
            .map(|opt| opt.map(|s| secrecy::ExposeSecret::expose_secret(&s).to_string()))
    }

    /// Resolve a dot-separated path (e.g., "weather.summary")
    ///
    /// Uses jsonpath module internally for unified path resolution.
    /// Supports both simple dot notation and array indices.
    ///
    /// Media paths are intercepted before standard output resolution:
    /// - `"task_id.media"` → full media array as JSON
    /// - `"task_id.media[0].hash"` → specific media ref field
    /// - `"task_id.media[0].path"` → specific media ref field
    pub fn resolve_path(&self, path: &str) -> Option<Value> {
        let mut parts = path.splitn(2, '.');
        let task_id = parts.next()?;

        // If no remaining path, return the whole output (clone from Arc)
        let Some(remaining) = parts.next() else {
            let output = self.get_output(task_id)?;
            return Some((*output).clone());
        };

        // Intercept media paths: task_id.media, task_id.media[0].hash, etc.
        if remaining == "media"
            || remaining.starts_with("media.")
            || remaining.starts_with("media[")
        {
            let result = self.results.get(task_id)?.value().clone();
            if result.media.is_empty() {
                // Full array access → return empty array.
                // Indexed access (media[0].hash) → return None so callers
                // can provide helpful "no matching media" errors.
                if remaining == "media" {
                    return Some(Value::Array(vec![]));
                }
                return None;
            }
            let media_json = serde_json::to_value(&result.media).ok()?;
            if remaining == "media" {
                return Some(media_json);
            }
            let media_remaining = &remaining[5..]; // skip "media"
            if let Some(dot_rest) = media_remaining.strip_prefix('.') {
                return jsonpath::resolve(&media_json, dot_rest).ok().flatten();
            }
            if media_remaining.starts_with('[') {
                return jsonpath::resolve(&media_json, media_remaining)
                    .ok()
                    .flatten();
            }
            return Some(media_json);
        }

        let output = self.get_output(task_id)?;

        // Use jsonpath for path resolution (handles both dots and array indices)
        // Arc<Value> derefs to &Value, so this works without changes
        match jsonpath::resolve(&output, remaining) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(path = %remaining, error = %e, "JSONPath resolution failed for task output");
                None
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // CONTEXT STORAGE
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set workflow context
    ///
    /// Called by Runner at workflow start after loading context files.
    pub fn set_context(&self, context: LoadedContext) {
        *self.context.write() = context;
    }

    /// Set loaded skills (from `skills:` block) for `{{skills.NAME}}` template resolution.
    pub fn set_skills(&self, skills: rustc_hash::FxHashMap<String, Value>) {
        self.context.write().skills = skills;
    }

    /// Get a context file by alias
    ///
    /// Returns the loaded value for `{{context.files.alias}}` bindings.
    pub fn get_context_file(&self, alias: &str) -> Option<Value> {
        self.context.read().get_file(alias).cloned()
    }

    /// Get session data
    ///
    /// Returns the loaded session for `{{context.session.key}}` bindings.
    pub fn get_context_session(&self) -> Option<Value> {
        self.context.read().get_session().cloned()
    }

    /// Check if context is loaded
    pub fn has_context(&self) -> bool {
        !self.context.read().is_empty()
    }

    /// Resolve a context path
    ///
    /// Supports:
    /// - `context.files.alias` → file content
    /// - `context.files.alias.field` → nested field
    /// - `context.session` → session data
    /// - `context.session.field` → session field
    pub fn resolve_context_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.len() < 2 {
            return None;
        }

        let context = self.context.read();

        match parts[1] {
            "files" => {
                if parts.len() < 3 {
                    return None;
                }
                let alias = parts[2];
                let value = context.get_file(alias)?;

                if parts.len() == 3 {
                    // context.files.alias → full file content
                    Some(value.clone())
                } else {
                    // context.files.alias.field → nested path
                    let remaining = parts[3..].join(".");
                    match jsonpath::resolve(value, &remaining) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(path = %remaining, error = %e, "JSONPath resolution failed for context file");
                            None
                        }
                    }
                }
            }
            "session" => {
                let session = context.get_session()?;

                if parts.len() == 2 {
                    // context.session → full session
                    Some(session.clone())
                } else {
                    // context.session.field → nested path
                    let remaining = parts[2..].join(".");
                    match jsonpath::resolve(session, &remaining) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(path = %remaining, error = %e, "JSONPath resolution failed for session");
                            None
                        }
                    }
                }
            }
            // Shorthand: context.<alias> → context.files.<alias>
            // Allows {{context.readme}} instead of {{context.files.readme}}
            alias => {
                let value = context.get_file(alias)?;
                if parts.len() == 2 {
                    Some(value.clone())
                } else {
                    let remaining = parts[2..].join(".");
                    match jsonpath::resolve(value, &remaining) {
                        Ok(v) => v,
                        Err(e) => {
                            tracing::warn!(path = %remaining, error = %e, "JSONPath resolution failed for context shorthand");
                            None
                        }
                    }
                }
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════════════════
    // INPUTS STORAGE
    // ═══════════════════════════════════════════════════════════════════════════

    /// Set workflow inputs
    ///
    /// Called by Runner at workflow start with input definitions.
    /// Each input is a JSON object with `type`, `default`, `description`, etc.
    pub fn set_inputs(&self, inputs: FxHashMap<String, Value>) {
        *self.inputs.write() = inputs;
    }

    /// Get an input's value by name
    ///
    /// Supports two formats:
    /// - Full form: `{ type: string, default: "value" }` → extracts `default` field
    /// - Shorthand: `"value"` or `123` or `true` → uses value directly
    ///
    /// Returns `None` if input doesn't exist.
    pub fn get_input_default(&self, name: &str) -> Option<Value> {
        let inputs = self.inputs.read();
        let definition = inputs.get(name)?;

        // Check if this is a full input definition with a `default` field
        // or a shorthand value (string, number, bool, array)
        if let Some(obj) = definition.as_object() {
            // Full form: { type, default, description, ... }
            // Check for 'default' field, or 'value' as alternative
            if let Some(default_val) = obj.get("default").or_else(|| obj.get("value")) {
                return Some(default_val.clone());
            }
            // If object has type/description but no default, return None
            if obj.contains_key("type") || obj.contains_key("description") {
                return None;
            }
        }

        // Shorthand: the value itself is the default
        // e.g., `name: "TestUser"` or `count: 5`
        Some(definition.clone())
    }

    /// Check if inputs are loaded
    pub fn has_inputs(&self) -> bool {
        !self.inputs.read().is_empty()
    }

    /// Resolve an input path
    ///
    /// Supports:
    /// - `inputs.param` → default value of parameter
    /// - `inputs.param.field` → nested field in default value (if object)
    pub fn resolve_input_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() || parts[0] != "inputs" {
            return None;
        }
        if parts.len() < 2 {
            return None;
        }

        let param_name = parts[1];
        let default_value = self.get_input_default(param_name)?;

        if parts.len() == 2 {
            // inputs.param → full default value
            Some(default_value)
        } else {
            // inputs.param.field → nested path in default value
            let remaining = parts[2..].join(".");
            match jsonpath::resolve(&default_value, &remaining) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(path = %remaining, error = %e, "JSONPath resolution failed for input default");
                    None
                }
            }
        }
    }

    /// Resolve a `skills.*` path to its value.
    ///
    /// Path format: `skills.<alias>` or `skills.<alias>.<field>`
    pub fn resolve_skills_path(&self, path: &str) -> Option<Value> {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return None;
        }

        let alias = parts[0];
        let context = self.context.read();
        let value = context.get_skill(alias)?;

        if parts.len() == 1 {
            Some(value.clone())
        } else {
            let remaining = parts[1..].join(".");
            match jsonpath::resolve(value, &remaining) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(path = %remaining, error = %e, "JSONPath resolution failed for skill");
                    None
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Sprint 2 P0-3: invocation source must thread through RunContext.
    #[test]
    fn invocation_source_round_trips() {
        use nika_core::trust::{InvocationSource, TrustLevel};

        let cli = RunContext::new(InvocationSource::Cli);
        assert_eq!(cli.invocation_source(), InvocationSource::Cli);
        assert_eq!(cli.invocation_source().input_trust(), TrustLevel::Trusted);

        let serve = RunContext::new(InvocationSource::Serve);
        assert_eq!(serve.invocation_source(), InvocationSource::Serve);
        assert_eq!(
            serve.invocation_source().input_trust(),
            TrustLevel::Untrusted
        );

        // Default fails closed (Unknown → Untrusted).
        let def = RunContext::default();
        assert_eq!(def.invocation_source(), InvocationSource::Unknown);
        assert_eq!(def.invocation_source().input_trust(), TrustLevel::Untrusted);
    }

    /// `set_invocation_source` is the builder hook used by `Runner`.
    #[test]
    fn invocation_source_can_be_overridden() {
        use nika_core::trust::{InvocationSource, TrustLevel};

        let mut ctx = RunContext::new(InvocationSource::Cli);
        ctx.set_invocation_source(InvocationSource::NestedRun {
            ceiling: TrustLevel::Untrusted,
        });
        assert_eq!(ctx.invocation_source().input_trust(), TrustLevel::Untrusted);
    }

    #[test]
    fn insert_and_get_result() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"key": "value"}), Duration::from_secs(1)),
        );

        let result = store.get("task1").unwrap();
        assert!(result.is_success());
        assert_eq!(result.output["key"], "value");
    }

    #[test]
    fn success_str_converts_to_value() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task1"),
            TaskResult::success_str("hello", Duration::from_secs(1)),
        );

        let result = store.get("task1").unwrap();
        assert_eq!(*result.output, Value::String("hello".to_string()));
        assert_eq!(result.output_str(), "hello");
    }

    #[test]
    fn failed_result() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task1"),
            TaskResult::failed("oops", Duration::from_secs(1)),
        );

        let result = store.get("task1").unwrap();
        assert!(!result.is_success());
        assert_eq!(result.error(), Some("oops"));
    }

    #[test]
    fn resolve_simple_path() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("weather"),
            TaskResult::success(json!({"summary": "Sunny"}), Duration::from_secs(1)),
        );

        let value = store.resolve_path("weather.summary").unwrap();
        assert_eq!(value, "Sunny");
    }

    #[test]
    fn resolve_nested_path() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("flights"),
            TaskResult::success(
                json!({"cheapest": {"price": 89, "airline": "AF"}}),
                Duration::from_secs(1),
            ),
        );

        assert_eq!(store.resolve_path("flights.cheapest.price").unwrap(), 89);
        assert_eq!(
            store.resolve_path("flights.cheapest.airline").unwrap(),
            "AF"
        );
    }

    #[test]
    fn resolve_array_index() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("data"),
            TaskResult::success(
                json!({"items": ["first", "second"]}),
                Duration::from_secs(1),
            ),
        );

        assert_eq!(store.resolve_path("data.items.0").unwrap(), "first");
        assert_eq!(store.resolve_path("data.items.1").unwrap(), "second");
    }

    #[test]
    fn resolve_path_not_found() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"a": 1}), Duration::from_secs(1)),
        );

        assert!(store.resolve_path("task1.nonexistent").is_none());
        assert!(store.resolve_path("unknown.field").is_none());
    }

    // =========================================================================
    // Concurrent Access Tests
    // =========================================================================

    #[test]
    fn concurrent_writes_all_stored() {
        use std::thread;

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let store_arc = Arc::new(store);

        let handles: Vec<_> = (0..100)
            .map(|i| {
                let store = Arc::clone(&store_arc);
                thread::spawn(move || {
                    store.insert(
                        Arc::from(format!("task_{}", i)),
                        TaskResult::success(json!({"index": i}), Duration::from_millis(i)),
                    );
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }

        // All 100 keys should exist
        for i in 0..100 {
            assert!(
                store_arc.contains(&format!("task_{}", i)),
                "task_{} should exist",
                i
            );
        }
    }

    #[test]
    fn concurrent_reads_during_writes() {
        use std::thread;

        let store = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));

        // Pre-populate some data
        for i in 0..50 {
            store.insert(
                Arc::from(format!("initial_{}", i)),
                TaskResult::success(json!({"value": i}), Duration::from_millis(i)),
            );
        }

        let store_writer = Arc::clone(&store);
        let store_reader = Arc::clone(&store);

        // Spawn writer thread
        let writer = thread::spawn(move || {
            for i in 0..100 {
                store_writer.insert(
                    Arc::from(format!("new_{}", i)),
                    TaskResult::success(json!({"new": i}), Duration::from_millis(i)),
                );
            }
        });

        // Spawn reader thread - should not block
        let reader = thread::spawn(move || {
            let mut read_count = 0;
            for i in 0..50 {
                if store_reader.get(&format!("initial_{}", i)).is_some() {
                    read_count += 1;
                }
            }
            read_count
        });

        writer.join().unwrap();
        let reads = reader.join().unwrap();

        // Reader should have been able to read existing data
        assert_eq!(reads, 50, "Should read all 50 initial entries");

        // Verify writer completed
        for i in 0..100 {
            assert!(store.contains(&format!("new_{}", i)));
        }
    }

    #[test]
    fn overwrite_existing_task() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Insert initial value
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"version": 1}), Duration::from_secs(1)),
        );

        // Overwrite with new value
        store.insert(
            Arc::from("task1"),
            TaskResult::success(json!({"version": 2}), Duration::from_secs(2)),
        );

        let result = store.get("task1").unwrap();
        assert_eq!(result.output["version"], 2);
        assert_eq!(result.duration, Duration::from_secs(2));
    }

    // =========================================================================
    // Edge Case Tests
    // =========================================================================

    #[test]
    fn contains_and_is_success() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Non-existent task
        assert!(!store.contains("nonexistent"));
        assert!(!store.is_success("nonexistent"));

        // Successful task
        store.insert(
            Arc::from("success"),
            TaskResult::success(json!(1), Duration::from_secs(1)),
        );
        assert!(store.contains("success"));
        assert!(store.is_success("success"));

        // Failed task
        store.insert(
            Arc::from("failed"),
            TaskResult::failed("error", Duration::from_secs(1)),
        );
        assert!(store.contains("failed"));
        assert!(!store.is_success("failed"));
    }

    #[test]
    fn get_output_returns_arc() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let big_json = json!({
            "large": "data".repeat(1000),
            "nested": {"deep": {"value": 42}}
        });

        store.insert(
            Arc::from("big"),
            TaskResult::success(big_json.clone(), Duration::from_secs(1)),
        );

        // get_output should return Arc (cheap clone)
        let output1 = store.get_output("big").unwrap();
        let output2 = store.get_output("big").unwrap();

        // Both should point to same data (Arc comparison)
        assert!(Arc::ptr_eq(&output1, &output2));
    }

    #[test]
    fn resolve_task_only_returns_full_output() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"a": 1, "b": 2}), Duration::from_secs(1)),
        );

        // Just task name should return full output
        let full = store.resolve_path("task").unwrap();
        assert_eq!(full, json!({"a": 1, "b": 2}));
    }

    #[test]
    fn resolve_deeply_nested_path() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("deep"),
            TaskResult::success(
                json!({"level1": {"level2": {"level3": {"level4": "found"}}}}),
                Duration::from_secs(1),
            ),
        );

        let value = store
            .resolve_path("deep.level1.level2.level3.level4")
            .unwrap();
        assert_eq!(value, "found");
    }

    #[test]
    fn resolve_mixed_array_object_path() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("mixed"),
            TaskResult::success(
                json!({
                    "users": [
                        {"name": "Alice", "scores": [90, 85, 92]},
                        {"name": "Bob", "scores": [78, 82]}
                    ]
                }),
                Duration::from_secs(1),
            ),
        );

        assert_eq!(store.resolve_path("mixed.users.0.name").unwrap(), "Alice");
        assert_eq!(store.resolve_path("mixed.users.1.name").unwrap(), "Bob");
        assert_eq!(store.resolve_path("mixed.users.0.scores.2").unwrap(), 92);
    }

    #[test]
    fn output_str_cow_borrowed_for_strings() {
        let result = TaskResult::success_str("hello", Duration::from_secs(1));

        let cow = result.output_str();
        // Should be borrowed (no allocation for string values)
        assert!(matches!(cow, std::borrow::Cow::Borrowed(_)));
        assert_eq!(&*cow, "hello");
    }

    #[test]
    fn output_str_cow_owned_for_non_strings() {
        let result = TaskResult::success(json!({"num": 42}), Duration::from_secs(1));

        let cow = result.output_str();
        // Should be owned (converted to string)
        assert!(matches!(cow, std::borrow::Cow::Owned(_)));
        assert!(cow.contains("42"));
    }

    #[test]
    fn empty_task_id_resolves_nothing() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!(1), Duration::from_secs(1)),
        );

        // Empty path should return None
        assert!(store.resolve_path("").is_none());
    }

    #[test]
    fn clone_is_shallow() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task"),
            TaskResult::success(json!({"value": 42}), Duration::from_secs(1)),
        );

        // Clone the store
        let cloned = store.clone();

        // Both should see the same data (shared Arc<DashMap>)
        assert_eq!(
            store.get("task").unwrap().output,
            cloned.get("task").unwrap().output
        );

        // Insert into original
        store.insert(
            Arc::from("new"),
            TaskResult::success(json!(1), Duration::from_secs(1)),
        );

        // Clone should also see it (same underlying DashMap)
        assert!(cloned.contains("new"));
    }

    // =========================================================================
    // Context Storage Tests
    // =========================================================================

    #[test]
    fn test_context_default_is_empty() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert!(!store.has_context());
    }

    #[test]
    fn test_set_and_get_context_file() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context
            .files
            .insert("brand".to_string(), json!("# Brand Guide"));

        store.set_context(context);

        assert!(store.has_context());
        assert_eq!(
            store.get_context_file("brand"),
            Some(json!("# Brand Guide"))
        );
        assert!(store.get_context_file("nonexistent").is_none());
    }

    #[test]
    fn test_set_and_get_context_session() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context.session = Some(json!({"focus_areas": ["rust", "ai"]}));

        store.set_context(context);

        assert!(store.has_context());
        let session = store.get_context_session().unwrap();
        assert!(session["focus_areas"].is_array());
    }

    #[test]
    fn test_resolve_context_path_files() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context.files.insert(
            "persona".to_string(),
            json!({"name": "Agent", "role": "assistant"}),
        );

        store.set_context(context);

        // Full file
        assert_eq!(
            store.resolve_context_path("context.files.persona"),
            Some(json!({"name": "Agent", "role": "assistant"}))
        );

        // Nested field
        assert_eq!(
            store.resolve_context_path("context.files.persona.name"),
            Some(json!("Agent"))
        );

        // Missing file
        assert!(store
            .resolve_context_path("context.files.missing")
            .is_none());
    }

    #[test]
    fn test_resolve_context_path_session() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context.session = Some(json!({"focus": "rust", "level": 3}));

        store.set_context(context);

        // Full session
        assert_eq!(
            store.resolve_context_path("context.session"),
            Some(json!({"focus": "rust", "level": 3}))
        );

        // Nested field
        assert_eq!(
            store.resolve_context_path("context.session.focus"),
            Some(json!("rust"))
        );
        assert_eq!(
            store.resolve_context_path("context.session.level"),
            Some(json!(3))
        );
    }

    #[test]
    fn test_resolve_context_shorthand() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut context = LoadedContext::new();
        context
            .files
            .insert("readme".to_string(), json!("# Hello World"));
        context
            .files
            .insert("config".to_string(), json!({"debug": true, "port": 8080}));
        store.set_context(context);

        // Shorthand: context.readme → context.files.readme
        assert_eq!(
            store.resolve_context_path("context.readme"),
            Some(json!("# Hello World"))
        );

        // Shorthand with nested path: context.config.port
        assert_eq!(
            store.resolve_context_path("context.config.port"),
            Some(json!(8080))
        );

        // Full path still works
        assert_eq!(
            store.resolve_context_path("context.files.readme"),
            Some(json!("# Hello World"))
        );
    }

    #[test]
    fn test_resolve_context_path_invalid() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut context = LoadedContext::new();
        context.files.insert("brand".to_string(), json!("content"));

        store.set_context(context);

        // Invalid paths
        assert!(store.resolve_context_path("context").is_none());
        assert!(store.resolve_context_path("context.invalid").is_none());
        assert!(store.resolve_context_path("context.files").is_none());
        assert!(store.resolve_context_path("other.path").is_none());
    }

    // =========================================================================
    // Inputs Storage Tests
    // =========================================================================

    #[test]
    fn test_inputs_default_is_empty() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert!(!store.has_inputs());
    }

    #[test]
    fn test_set_and_get_input_default() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "description": "Research topic",
                "default": "AI QR code generation"
            }),
        );

        store.set_inputs(inputs);

        assert!(store.has_inputs());
        assert_eq!(
            store.get_input_default("topic"),
            Some(json!("AI QR code generation"))
        );
        assert!(store.get_input_default("nonexistent").is_none());
    }

    #[test]
    fn test_get_input_default_without_default() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        // Input without default field
        inputs.insert(
            "required_param".to_string(),
            json!({
                "type": "string",
                "description": "A required parameter"
            }),
        );

        store.set_inputs(inputs);

        // Should return None for input without default
        assert!(store.get_input_default("required_param").is_none());
    }

    #[test]
    fn test_resolve_input_path_simple() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "AI trends 2025"
            }),
        );
        inputs.insert(
            "depth".to_string(),
            json!({
                "type": "string",
                "default": "comprehensive"
            }),
        );

        store.set_inputs(inputs);

        // Resolve inputs.topic
        assert_eq!(
            store.resolve_input_path("inputs.topic"),
            Some(json!("AI trends 2025"))
        );

        // Resolve inputs.depth
        assert_eq!(
            store.resolve_input_path("inputs.depth"),
            Some(json!("comprehensive"))
        );

        // Missing input
        assert!(store.resolve_input_path("inputs.missing").is_none());
    }

    #[test]
    fn test_resolve_input_path_nested() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "config".to_string(),
            json!({
                "type": "object",
                "default": {
                    "theme": "dark",
                    "version": 2,
                    "nested": {
                        "deep": "value"
                    }
                }
            }),
        );

        store.set_inputs(inputs);

        // Resolve nested fields
        assert_eq!(
            store.resolve_input_path("inputs.config.theme"),
            Some(json!("dark"))
        );
        assert_eq!(
            store.resolve_input_path("inputs.config.version"),
            Some(json!(2))
        );
        assert_eq!(
            store.resolve_input_path("inputs.config.nested.deep"),
            Some(json!("value"))
        );
    }

    #[test]
    fn test_resolve_input_path_invalid() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        let mut inputs = FxHashMap::default();
        inputs.insert(
            "topic".to_string(),
            json!({
                "type": "string",
                "default": "test"
            }),
        );

        store.set_inputs(inputs);

        // Invalid paths
        assert!(store.resolve_input_path("inputs").is_none());
        assert!(store.resolve_input_path("other.path").is_none());
        assert!(store.resolve_input_path("").is_none());
    }

    // =========================================================================
    // Media Path Resolution Tests
    // =========================================================================

    /// Helper: create a TaskResult with media refs for testing.
    fn task_with_media() -> TaskResult {
        use std::path::PathBuf;

        let media = vec![
            crate::media::MediaRef {
                hash: "blake3:af1349b9".to_string(),
                mime_type: "image/png".to_string(),
                size_bytes: 4096,
                path: PathBuf::from("/tmp/cas/af/1349b9"),
                extension: "png".to_string(),
                created_by: "gen_img".to_string(),
                metadata: serde_json::Map::new(),
            },
            crate::media::MediaRef {
                hash: "blake3:deadbeef".to_string(),
                mime_type: "audio/wav".to_string(),
                size_bytes: 8192,
                path: PathBuf::from("/tmp/cas/de/adbeef"),
                extension: "wav".to_string(),
                created_by: "gen_img".to_string(),
                metadata: serde_json::Map::new(),
            },
        ];
        TaskResult::success(json!({"prompt": "a cat"}), Duration::from_secs(1)).with_media(media)
    }

    #[test]
    fn resolve_media_full_array() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let value = store.resolve_path("gen_img.media").unwrap();
        let arr = value.as_array().expect("media should be an array");
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["hash"], "blake3:af1349b9");
        assert_eq!(arr[1]["hash"], "blake3:deadbeef");
    }

    #[test]
    fn resolve_media_index_hash() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let hash = store.resolve_path("gen_img.media[0].hash").unwrap();
        assert_eq!(hash, "blake3:af1349b9");

        let hash2 = store.resolve_path("gen_img.media[1].hash").unwrap();
        assert_eq!(hash2, "blake3:deadbeef");
    }

    #[test]
    fn resolve_media_index_mime_type() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let mime = store.resolve_path("gen_img.media[0].mime_type").unwrap();
        assert_eq!(mime, "image/png");

        let mime2 = store.resolve_path("gen_img.media[1].mime_type").unwrap();
        assert_eq!(mime2, "audio/wav");
    }

    #[test]
    fn resolve_media_empty_returns_empty_array() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        // Task with no media
        store.insert(
            Arc::from("no_media"),
            TaskResult::success(json!({"text": "hello"}), Duration::from_secs(1)),
        );

        let value = store.resolve_path("no_media.media").unwrap();
        assert_eq!(value, json!([]));
    }

    #[test]
    fn resolve_media_index_path() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let path = store.resolve_path("gen_img.media[0].path").unwrap();
        assert_eq!(path, "/tmp/cas/af/1349b9");
    }

    #[test]
    fn resolve_media_index_size_bytes() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let size = store.resolve_path("gen_img.media[0].size_bytes").unwrap();
        assert_eq!(size, 4096);
    }

    #[test]
    fn resolve_media_index_extension() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let ext = store.resolve_path("gen_img.media[0].extension").unwrap();
        assert_eq!(ext, "png");
    }

    #[test]
    fn resolve_media_out_of_bounds() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        // Index beyond array length should return None
        assert!(store.resolve_path("gen_img.media[99].hash").is_none());
    }

    #[test]
    fn resolve_media_does_not_shadow_output() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        // Standard output field should still resolve normally
        let prompt = store.resolve_path("gen_img.prompt").unwrap();
        assert_eq!(prompt, "a cat");
    }

    #[test]
    fn iter_results_returns_all_entries() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(
            Arc::from("task1"),
            TaskResult::success_str("out1", Duration::from_millis(10)),
        );
        store.insert(
            Arc::from("task2"),
            TaskResult::success_str("out2", Duration::from_millis(20)),
        );
        store.insert(
            Arc::from("task3"),
            TaskResult::failed("err", Duration::from_millis(5)),
        );

        let results = store.iter_results();
        assert_eq!(results.len(), 3);

        // All task IDs should be present
        let ids: Vec<String> = results.iter().map(|(id, _)| id.to_string()).collect();
        assert!(ids.contains(&"task1".to_string()));
        assert!(ids.contains(&"task2".to_string()));
        assert!(ids.contains(&"task3".to_string()));
    }

    #[test]
    fn iter_results_includes_media_refs() {
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gen_img"), task_with_media());

        let results = store.iter_results();
        let (_, result) = results
            .iter()
            .find(|(id, _)| id.as_ref() == "gen_img")
            .unwrap();
        assert_eq!(result.media.len(), 2);
        assert_eq!(result.media[0].hash, "blake3:af1349b9");
    }

    // ═══════════════════════════════════════════════════════════════
    // MEDIA TOOL INVOKE RESULT — Template binding integration
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn invoke_json_result_accessible_via_template_binding() {
        // When invoke: nika:thumbnail returns a JSON string like:
        // {"hash":"blake3:abc","mime_type":"image/png","size_bytes":1234,"metadata":{"width":256}}
        // The result is stored as Value::String(json_str).
        // Downstream tasks must be able to access {{with.thumb.hash}} etc.

        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        // Simulate what run_invoke + make_task_result does: stores JSON as Value::String
        let invoke_output = r#"{"hash":"blake3:abc123","mime_type":"image/png","size_bytes":1234,"metadata":{"width":256,"height":192}}"#;
        store.insert(
            Arc::from("thumb"),
            TaskResult::success_str(invoke_output, Duration::from_millis(100)),
        );

        // These must resolve correctly via auto-parse of JSON strings
        let hash = store.resolve_path("thumb.hash").unwrap();
        assert_eq!(
            hash, "blake3:abc123",
            "{{{{with.thumb.hash}}}} must resolve"
        );

        let mime = store.resolve_path("thumb.mime_type").unwrap();
        assert_eq!(
            mime, "image/png",
            "{{{{with.thumb.mime_type}}}} must resolve"
        );

        let size = store.resolve_path("thumb.size_bytes").unwrap();
        assert_eq!(size, 1234, "{{{{with.thumb.size_bytes}}}} must resolve");

        // Nested metadata access
        let width = store.resolve_path("thumb.metadata.width").unwrap();
        assert_eq!(width, 256, "{{{{with.thumb.metadata.width}}}} must resolve");

        let height = store.resolve_path("thumb.metadata.height").unwrap();
        assert_eq!(
            height, 192,
            "{{{{with.thumb.metadata.height}}}} must resolve"
        );
    }

    #[test]
    fn invoke_json_result_with_array_accessible() {
        // nika:dominant_color returns {"colors":[{"r":255,"g":0,"b":0,"hex":"#ff0000"}],"count":1}
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let invoke_output = r##"{"colors":[{"r":255,"g":0,"b":0,"hex":"#ff0000"},{"r":0,"g":0,"b":255,"hex":"#0000ff"}],"count":2}"##;
        store.insert(
            Arc::from("colors"),
            TaskResult::success_str(invoke_output, Duration::from_millis(50)),
        );

        let count = store.resolve_path("colors.count").unwrap();
        assert_eq!(count, 2);

        let first_hex = store.resolve_path("colors.colors[0].hex").unwrap();
        assert_eq!(first_hex, "#ff0000");

        let second_r = store.resolve_path("colors.colors[1].r").unwrap();
        assert_eq!(second_r, 0);
    }

    #[test]
    fn invoke_dimensions_result_accessible() {
        // nika:dimensions returns {"width":1024,"height":768,"orientation":"landscape"}
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let invoke_output = r#"{"width":1024,"height":768,"orientation":"landscape"}"#;
        store.insert(
            Arc::from("dim"),
            TaskResult::success_str(invoke_output, Duration::from_millis(10)),
        );

        assert_eq!(store.resolve_path("dim.width").unwrap(), 1024);
        assert_eq!(store.resolve_path("dim.height").unwrap(), 768);
        assert_eq!(store.resolve_path("dim.orientation").unwrap(), "landscape");
    }

    #[test]
    fn enriched_media_ref_metadata_accessible() {
        // MediaRef with enriched metadata must be accessible
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        let mut metadata = serde_json::Map::new();
        metadata.insert("width".into(), json!(512));
        metadata.insert("height".into(), json!(384));
        metadata.insert("thumbhash".into(), json!("dGVzdA=="));

        let media = vec![crate::media::MediaRef {
            hash: "blake3:enriched123".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 2048,
            path: std::path::PathBuf::from("/cas/en/riched123"),
            extension: "png".to_string(),
            created_by: "gen".to_string(),
            metadata,
        }];

        store.insert(
            Arc::from("gen"),
            TaskResult::success(json!("image generated"), Duration::from_secs(1)).with_media(media),
        );

        // Media ref fields
        assert_eq!(
            store.resolve_path("gen.media[0].hash").unwrap(),
            "blake3:enriched123"
        );
        // Enriched metadata
        assert_eq!(
            store.resolve_path("gen.media[0].metadata.width").unwrap(),
            512
        );
        assert_eq!(
            store.resolve_path("gen.media[0].metadata.height").unwrap(),
            384
        );
        assert_eq!(
            store
                .resolve_path("gen.media[0].metadata.thumbhash")
                .unwrap(),
            "dGVzdA=="
        );
    }

    #[test]
    fn chained_invoke_bindings_work() {
        // Simulate: gen → media[0].hash → thumb (invoke) → dim (invoke)
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);

        // Task "gen" has media
        let media = vec![crate::media::MediaRef {
            hash: "blake3:source_hash".to_string(),
            mime_type: "image/png".to_string(),
            size_bytes: 5000,
            path: std::path::PathBuf::from("/cas/so/urce"),
            extension: "png".to_string(),
            created_by: "gen".to_string(),
            metadata: serde_json::Map::new(),
        }];
        store.insert(
            Arc::from("gen"),
            TaskResult::success(json!("ok"), Duration::from_secs(1)).with_media(media),
        );

        // Task "thumb" returns invoke result (JSON string)
        store.insert(
            Arc::from("thumb"),
            TaskResult::success_str(
                r#"{"hash":"blake3:thumb_hash","size_bytes":1500,"metadata":{"width":256}}"#,
                Duration::from_millis(200),
            ),
        );

        // Task "dim" returns dimensions (JSON string)
        store.insert(
            Arc::from("dim"),
            TaskResult::success_str(
                r#"{"width":256,"height":192,"orientation":"landscape"}"#,
                Duration::from_millis(10),
            ),
        );

        // Verify full chain is accessible
        assert_eq!(
            store.resolve_path("gen.media[0].hash").unwrap(),
            "blake3:source_hash"
        );
        assert_eq!(
            store.resolve_path("thumb.hash").unwrap(),
            "blake3:thumb_hash"
        );
        assert_eq!(store.resolve_path("thumb.metadata.width").unwrap(), 256);
        assert_eq!(store.resolve_path("dim.width").unwrap(), 256);
        assert_eq!(store.resolve_path("dim.orientation").unwrap(), "landscape");
    }

    // ═══════════════════════════════════════════════════════════════
    // Record storage tests
    // ═══════════════════════════════════════════════════════════════

    fn make_test_record(task_id: &str) -> crate::runtime::record::Record {
        crate::runtime::record::Record {
            task_id: Arc::from(task_id),
            summary: format!("Summary of {task_id}"),
            key_findings: vec!["finding1".to_string()],
            raw_output: None,
            confidence: 0.9,
            tokens_original: 1000,
            tokens_compressed: 100,
            compression_model: "mock".to_string(),
            compression_cost_usd: 0.0,
            compression_duration: std::time::Duration::ZERO,
        }
    }

    #[test]
    fn test_record_set_get_roundtrip() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        let record = make_test_record("task1");
        ctx.set_record("task1".into(), record);
        let got = ctx.get_record("task1").expect("record should exist");
        assert_eq!(got.summary, "Summary of task1");
        assert_eq!(got.confidence, 0.9);
    }

    #[test]
    fn test_record_has_record() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert!(!ctx.has_record("task1"));
        ctx.set_record("task1".into(), make_test_record("task1"));
        assert!(ctx.has_record("task1"));
        assert!(!ctx.has_record("task2"));
    }

    #[test]
    fn test_record_iter_records() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        ctx.set_record("a".into(), make_test_record("a"));
        ctx.set_record("b".into(), make_test_record("b"));
        let records = ctx.iter_records();
        assert_eq!(records.len(), 2);
    }

    #[test]
    fn test_record_missing_returns_none() {
        let ctx = RunContext::new(nika_core::trust::InvocationSource::Test);
        assert!(ctx.get_record("nonexistent").is_none());
    }

    // =========================================================================
    // BUG-10: Skipped tasks must NOT block downstream
    // =========================================================================

    #[test]
    fn skipped_task_is_usable() {
        // BUG-10: is_usable() must return true for Skipped tasks.
        // Skipped tasks (via when: false) produce null output but should
        // NOT prevent downstream tasks from running.
        let result = TaskResult::skipped("when: false");
        assert!(
            result.is_usable(),
            "Skipped tasks must be usable by downstream tasks"
        );
    }

    #[test]
    fn skipped_task_is_completed_successfully() {
        // BUG-10: is_completed_successfully() delegates to is_usable().
        // Skipped tasks must return Some(true) so downstream tasks proceed.
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("gated"), TaskResult::skipped("when: false"));

        assert_eq!(
            store.is_completed_successfully("gated"),
            Some(true),
            "Skipped task must be treated as completed successfully"
        );
    }

    #[test]
    fn skipped_task_output_is_null() {
        // Skipped tasks output Value::Null — downstream must use ?? or default()
        let result = TaskResult::skipped("when: false");
        assert_eq!(*result.output, Value::Null);
    }

    #[test]
    fn skipped_task_is_not_failed() {
        // Skipped is distinct from Failed — must not appear in is_failed()
        let store = RunContext::new(nika_core::trust::InvocationSource::Test);
        store.insert(Arc::from("skipped"), TaskResult::skipped("when: false"));

        assert!(
            !store.is_failed("skipped"),
            "Skipped must not be treated as failed"
        );
        assert!(!store.is_dependency_failed("skipped"));
        assert!(
            !store.is_success("skipped"),
            "Skipped is not strict success either"
        );
    }

    #[test]
    fn test_output_size_estimate_string() {
        let result = TaskResult::success("hello world", Duration::from_secs(1));
        // 11 chars + 24 overhead
        assert_eq!(result.output_size_estimate(), 35);
    }

    #[test]
    fn test_truncate_if_oversized_small_output_unchanged() {
        let mut result = TaskResult::success("small output", Duration::from_secs(1));
        assert!(result.truncate_if_oversized().is_none());
        assert_eq!(result.output_str(), "small output");
    }

    #[test]
    fn test_truncate_if_oversized_large_output_truncated() {
        // Create a string just over 50MB
        let large = "x".repeat(51 * 1024 * 1024);
        let mut result = TaskResult::success(large, Duration::from_secs(1));
        let original_size = result.truncate_if_oversized();
        assert!(original_size.is_some(), "Should have been truncated");
        assert!(
            result.output_str().starts_with("[TRUNCATED:"),
            "Output should be truncated message, got: {}",
            &result.output_str()[..50]
        );
        assert!(
            result.output_size_estimate() < TaskResult::MAX_OUTPUT_SIZE,
            "Truncated output should be under limit"
        );
    }
}
