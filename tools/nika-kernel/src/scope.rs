// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TaskScope splinter traits — compile-time capability enforcement.
//!
//! Instead of `&mut dyn RunContext` (1995 LOC god struct), verb functions
//! take only the trait splinters they need:
//!
//! ```text
//! async fn run_fetch(scope: &mut S, ...) where S: BindingScope + MediaStaging
//! ```
//!
//! The compiler enforces that fetch cannot touch `TaskResults::insert`
//! or `RecordStore::push`. Capability-based separation at compile time.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use nika_core::trust::TrustLevel;

// ─────────────────────────────────────────────────────────────────────
// Splinter traits
// ─────────────────────────────────────────────────────────────────────

/// Access to completed task results.
pub trait TaskResults: Send + Sync {
    /// Insert a task result.
    fn insert(&self, task_id: Arc<str>, result: TaskOutput);

    /// Get a task result by ID.
    fn get(&self, task_id: &str) -> Option<TaskOutput>;

    /// Get the trust level of a task result.
    fn get_trust(&self, task_id: &str) -> Option<TrustLevel>;
}

/// Binding resolution scope (read-only access to resolved values).
pub trait BindingScope: Send + Sync {
    /// Resolve a path like `$task_id.field.nested` to a JSON value.
    fn resolve_path(&self, path: &str) -> Option<Value>;
}

/// Media blob staging area for invoke tasks.
pub trait MediaStaging: Send + Sync {
    /// Stage media references produced by a task.
    fn stage(&self, task_id: &str, refs: Vec<Value>);

    /// Take (drain) staged media refs for a task.
    fn take(&self, task_id: &str) -> Vec<Value>;
}

/// Compressed record storage.
pub trait RecordStore: Send + Sync {
    /// Write a record for a task.
    fn write_record(&self, task_id: &str, record: Value);

    /// Read all records.
    fn read_records(&self) -> Vec<(String, Value)>;
}

/// Secrets / vault access.
pub trait VaultLookup: Send + Sync {
    /// Look up a secret by service and field name.
    fn get_secret(&self, service: &str, field: &str) -> Option<String>;
}

/// Workflow invocation context (read-only metadata).
pub trait InvocationContext: Send + Sync {
    /// The invocation source (CLI, serve, nested run, etc.)
    fn source(&self) -> nika_core::trust::InvocationSource;

    /// Working directory for the workflow.
    fn working_dir(&self) -> PathBuf;

    /// Project root (parent of nika.toml).
    fn project_root(&self) -> Option<PathBuf>;

    /// Workflow inputs.
    fn inputs(&self) -> &Value;
}

// ─────────────────────────────────────────────────────────────────────
// Umbrella trait + blanket impl
// ─────────────────────────────────────────────────────────────────────

/// Full task scope — all 6 splinter traits combined.
///
/// `RunContext` implements this. At the dispatch boundary in `nika-runtime`,
/// use `&mut dyn TaskScope`. Inside each verb crate, narrow to the specific
/// splinters needed.
pub trait TaskScope:
    TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext
{
}

/// Blanket impl: any type implementing all 6 splinters is a TaskScope.
impl<T> TaskScope for T where
    T: TaskResults + BindingScope + MediaStaging + RecordStore + VaultLookup + InvocationContext
{
}

// ─────────────────────────────────────────────────────────────────────
// Phase 12 traits — builtin tool dependencies
// ─────────────────────────────────────────────────────────────────────

/// All the state needed to execute a nested workflow.
///
/// Constructed by `RunTool` in nika-builtin (after Shield checks and
/// task_local reads), passed to `EngineRunExecutor` in nika-engine.
#[derive(Debug)]
pub struct RunSpec {
    /// Path to the workflow file (mutually exclusive with `yaml_content`).
    pub path: Option<PathBuf>,
    /// Inline YAML content (mutually exclusive with `path`).
    pub yaml_content: Option<String>,
    /// Label used for logging when `yaml_content` is set.
    pub label: String,
    /// Trust level of the calling task (for ceiling propagation).
    pub caller_trust: nika_core::trust::TrustLevel,
    /// Optional context passed into the child workflow.
    pub parent_context: Option<Value>,
    /// Nesting depth at which to execute (depth + 1 from caller).
    pub depth: u32,
    /// Maximum allowed depth (clamped by RunTool before construction).
    pub max_depth: u32,
    /// Execution timeout.
    pub timeout: Duration,
    /// Parent chain for cycle detection (already includes the new path).
    pub parent_chain: Vec<PathBuf>,
}

/// Execute a nested workflow (nika:run).
///
/// Breaks the cycle: nika-builtin → nika-engine → nika-builtin.
/// nika-engine provides `EngineRunExecutor` wrapping `Runner`.
#[async_trait::async_trait]
pub trait RunExecutor: Send + Sync {
    /// Execute a nested workflow described by `spec`.
    ///
    /// The executor handles YAML parsing, Runner creation, and task_local
    /// scoping (WORKFLOW_DEPTH + PARENT_CHAIN).
    async fn run_workflow(
        &self,
        spec: RunSpec,
    ) -> Result<Value, crate::builtin::BuiltinError>;
}

/// Human-in-the-loop prompt (nika:prompt).
///
/// nika-engine provides an impl via `HitlHandler`.
#[async_trait::async_trait]
pub trait HitlPrompt: Send + Sync {
    /// Ask the user a question and return their response.
    ///
    /// In headless mode, returns the default value if provided.
    async fn ask(
        &self,
        message: &str,
        default: Option<&str>,
    ) -> Result<String, crate::builtin::BuiltinError>;
}

/// Minimal result from a successful blob store operation.
///
/// Projects the 3 fields that external consumers (nika-builtin, tests,
/// future verb crates) actually need. The full `StoreResult` (with
/// `path`, `verified`, `pipeline_ms`) is only available by downcasting
/// to the concrete `EngineMediaContext` via `inner()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobStoreResult {
    /// Algorithm-prefixed hash (e.g. "blake3:af1349…").
    pub hash: String,
    /// Stored size in bytes.
    pub size: u64,
    /// True when the same bytes were already in the store.
    pub deduplicated: bool,
}

/// Media CAS + cancellation context for media tools.
///
/// Object-safe: every method is either `async` (via `async_trait`) or
/// returns a non-generic type. `compute_blocking<F,T>` is intentionally
/// NOT on this trait — generic methods break vtable dispatch. Media tools
/// access `MediaToolContext.compute` directly inside nika-media.
///
/// nika-engine provides `EngineMediaContext` as the production impl.
/// nika-kernel-mock provides `MockMediaContext` for unit tests.
#[async_trait::async_trait]
pub trait MediaContext: Send + Sync {
    // ── CAS read ────────────────────────────────────────────────────
    /// Read a blob by its blake3 hash. Returns raw bytes.
    ///
    /// Returns `BuiltinError::Io` if the hash is unknown or I/O fails.
    async fn read_blob(&self, hash: &str) -> Result<Vec<u8>, crate::builtin::BuiltinError>;

    // ── CAS write with budget enforcement ───────────────────────────
    /// Store bytes in CAS with budget enforcement.
    ///
    /// `task_id` is used for budget attribution. On CAS failure the
    /// budget is automatically rolled back. Returns `BuiltinError::Io`
    /// on failure.
    async fn store_blob(
        &self,
        data: &[u8],
        task_id: &str,
    ) -> Result<BlobStoreResult, crate::builtin::BuiltinError>;

    // ── Path confinement ────────────────────────────────────────────
    /// Working directory boundary for import path validation.
    ///
    /// Returns `None` in REPL mode or when no project root is set.
    fn working_dir(&self) -> Option<&std::path::Path>;

    // ── Cancellation (hot path — non-allocating) ─────────────────────
    /// Returns `true` if the workflow has been cancelled.
    ///
    /// Called before and after every expensive operation. Note: rayon
    /// closures already dispatched will run to completion — cancellation
    /// checks happen BEFORE `compute.compute()` dispatch.
    fn is_cancelled(&self) -> bool;
}

#[cfg(test)]
mod _object_safety_asserts {
    use super::*;
    /// Compile-time guard: if someone adds a generic method to
    /// MediaContext this function declaration fails to compile.
    fn _assert_object_safe(_: &dyn MediaContext) {}
}

// ─────────────────────────────────────────────────────────────────────
// Shared output type
// ─────────────────────────────────────────────────────────────────────

/// Whether a task succeeded or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskOutcome {
    Success,
    Failed,
    Skipped,
}

/// Output from a completed task.
#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub value: Value,
    pub duration: Duration,
    pub trust: TrustLevel,
    pub outcome: TaskOutcome,
}
