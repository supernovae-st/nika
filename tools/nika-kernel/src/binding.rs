// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! BindingStore trait — abstraction over runtime data access for binding resolution.
//!
//! This trait decouples `binding/resolve.rs` from `RunContext` (engine internals),
//! enabling the binding resolver to live in nika-core or nika-runtime instead of
//! the engine monolith. RunContext implements this trait in nika-engine.
//!
//! # Design (S21)
//!
//! The 8 methods map 1:1 to the RunContext methods used by `resolve.rs`:
//!
//! | Method | RunContext method | Path pattern |
//! |--------|-----------------|--------------|
//! | `get_output` | `get_output` | `$task_id` |
//! | `is_failed` | `is_failed` | (internal) |
//! | `resolve_path` | `resolve_path` | `$task_id.field.sub` |
//! | `get_record` | `get_record` | (compression records) |
//! | `resolve_input_path` | `resolve_input_path` | `inputs.param` |
//! | `resolve_context_path` | `resolve_context_path` | `context.files.alias` |
//! | `resolve_skills_path` | `resolve_skills_path` | `skills.alias` |
//! | `vault_get_credential` | `vault_get_credential` | `$vault.service.field` |
//!
//! # NOT in scope
//!
//! This trait does NOT cover:
//! - Media staging (`resolve_path` handles media refs through JSON)
//! - Event emission (caller's responsibility)
//! - Template resolution (separate concern in binding/template.rs)

use serde_json::Value;
use std::sync::Arc;

/// Runtime data access for the binding resolver.
///
/// Abstracts how task outputs, inputs, context, skills, and vault
/// credentials are looked up. The engine's `RunContext` implements
/// this trait; tests use an in-memory mock.
///
/// All methods are sync — binding resolution reads from in-memory
/// data structures (DashMap, HashMap), never from I/O.
pub trait BindingStore: Send + Sync {
    /// Get the output value of a completed task.
    ///
    /// Returns `None` if the task hasn't run or has no output.
    /// Returns `Arc<Value>` for O(1) cloning.
    fn get_output(&self, task_id: &str) -> Option<Arc<Value>>;

    /// Check if a task has failed (directly or via dependency).
    fn is_failed(&self, task_id: &str) -> bool;

    /// Resolve a dot-separated path like `task_id.field.sub[0]`.
    ///
    /// Handles:
    /// - `task_id` → full output
    /// - `task_id.field` → JSONPath into output
    /// - `task_id.media` → media array
    /// - `task_id.media[0].hash` → specific media ref field
    fn resolve_path(&self, path: &str) -> Option<Value>;

    /// Get the compression record for a task (if any).
    ///
    /// Returns an opaque JSON value from `Record::to_binding_value()`:
    /// `{"summary": String, "key_findings": [String], "confidence": f64, ...}`.
    /// The caller navigates fields directly. When segments are empty,
    /// the resolver extracts the `"summary"` string for `$task` bindings.
    fn get_record(&self, task_id: &str) -> Option<Value>;

    /// Resolve an `inputs.*` path to its default value.
    ///
    /// Path format: `inputs.param` or `inputs.param.field`
    fn resolve_input_path(&self, path: &str) -> Option<Value>;

    /// Resolve a `context.*` path (files and session).
    ///
    /// Path format: `context.files.alias`, `context.files.alias.field`,
    /// `context.session`, `context.session.field`.
    /// Shorthand: `context.alias` → `context.files.alias`.
    ///
    /// Note: env vars use `BindingSource::Env`, not this method.
    fn resolve_context_path(&self, path: &str) -> Option<Value>;

    /// Resolve a skills path to its loaded content.
    ///
    /// Path format: `alias` or `alias.field` (the `skills.` prefix is
    /// stripped by the caller before dispatch — do NOT expect it here).
    fn resolve_skills_path(&self, path: &str) -> Option<Value>;

    /// Look up a credential from the encrypted vault.
    ///
    /// Returns `Ok(Some(value))` if found, `Ok(None)` if not configured
    /// or field not found, `Err` on I/O or crypto failure.
    ///
    /// # Sync contract
    ///
    /// This method is sync. Implementations must pre-fetch and cache vault
    /// credentials during workflow init (before binding resolution starts).
    /// If lazy vault access becomes needed, this signature must change to async.
    fn vault_get_credential(
        &self,
        service: &str,
        field: &str,
    ) -> Result<Option<String>, BindingStoreError>;
}

/// Errors from vault credential lookup.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindingStoreError {
    /// Vault I/O or decryption failure.
    #[error("vault error: {reason}")]
    Vault { reason: String },
}
