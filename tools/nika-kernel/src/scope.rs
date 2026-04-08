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
    fn source(&self) -> &nika_core::trust::InvocationSource;

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
// Shared output type
// ─────────────────────────────────────────────────────────────────────

/// Output from a completed task.
#[derive(Debug, Clone)]
pub struct TaskOutput {
    pub value: Value,
    pub duration: Duration,
    pub trust: TrustLevel,
}
