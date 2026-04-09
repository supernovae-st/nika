// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task-local cells for cross-crate trust and depth propagation.
//!
//! These cells are declared here (nika-kernel, L0.5) rather than in
//! nika-engine so that nika-builtin file tools and RunTool can read them
//! without importing nika-engine — which would create a dependency cycle.
//!
//! # Lifecycle
//!
//! The runner (`nika-engine/src/runtime/task_dispatch.rs`) sets these via
//! `.scope()` before each builtin dispatch. Tools read them via the accessor
//! functions below.
//!
//! Outside a runner-driven dispatch (e.g. `nika invoke nika:read foo.md`),
//! every accessor returns a safe default (Trusted, false, None, 0, empty vec).
//!
//! # Security invariant
//!
//! Trust is propagated via these cells — NEVER as a function argument to
//! `BuiltinTool::call`. All shield checks must read from
//! [`current_task_trust()`] and [`current_task_elevated()`].

use nika_core::trust::TrustLevel;
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::Arc;

tokio::task_local! {
    /// Workflow nesting depth. Uses task_local! for isolation between
    /// concurrent workflows (replaces the global AtomicU32 that had races).
    pub static WORKFLOW_DEPTH: Cell<u32>;

    /// ID of the task currently dispatching a builtin tool.
    ///
    /// `None` outside a runner-driven dispatch.
    pub static CURRENT_TASK_ID: Option<Arc<str>>;

    /// Trust level of the calling task.
    ///
    /// Used by the Shield (file path check, run depth ceiling, spotlight).
    /// Defaults to `Trusted` outside a runner context so standalone
    /// invocations (`nika invoke`) are never spuriously blocked.
    pub static CURRENT_TASK_TRUST: TrustLevel;

    /// Whether the calling task has `trust: elevated`.
    ///
    /// Elevated tasks bypass the Shield path restriction and spotlight fence.
    /// Defaults to `false` — never auto-elevate when context is missing.
    pub static CURRENT_TASK_ELEVATED: bool;

    /// Canonical workflow file paths in the parent chain.
    ///
    /// Used by `nika:run` for cycle detection: re-entering a workflow that is
    /// already in the chain triggers NIKA-387.
    pub static PARENT_CHAIN: Vec<PathBuf>;
}

// ─────────────────────────────────────────────────────────────────────
// Read-only accessors — safe defaults when outside a runner context
// ─────────────────────────────────────────────────────────────────────

/// Current workflow nesting depth. Returns `0` outside a runner context.
#[inline]
pub fn current_depth() -> u32 {
    WORKFLOW_DEPTH.try_with(|d| d.get()).unwrap_or(0)
}

/// ID of the currently dispatching task. Returns `None` outside a runner context.
#[inline]
pub fn current_task_id() -> Option<Arc<str>> {
    CURRENT_TASK_ID.try_with(|id| id.clone()).unwrap_or(None)
}

/// Trust level of the currently dispatching task. Returns `Trusted` outside
/// a runner context (conservative default for standalone `nika invoke`).
#[inline]
pub fn current_task_trust() -> TrustLevel {
    CURRENT_TASK_TRUST
        .try_with(|t| *t)
        .unwrap_or(TrustLevel::Trusted)
}

/// Whether the current task has `trust: elevated`. Returns `false` outside
/// a runner context (never auto-elevate).
#[inline]
pub fn current_task_elevated() -> bool {
    CURRENT_TASK_ELEVATED.try_with(|e| *e).unwrap_or(false)
}

/// Snapshot of the parent workflow chain. Returns an empty vec outside a
/// nested-run context.
#[inline]
pub fn current_parent_chain() -> Vec<PathBuf> {
    PARENT_CHAIN.try_with(|c| c.clone()).unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_current_depth_returns_zero_outside_scope() {
        assert_eq!(current_depth(), 0);
    }

    #[tokio::test]
    async fn test_current_depth_inside_scope() {
        let depth = WORKFLOW_DEPTH
            .scope(Cell::new(7), async { current_depth() })
            .await;
        assert_eq!(depth, 7);
    }

    #[tokio::test]
    async fn test_current_task_trust_defaults_to_trusted() {
        assert_eq!(current_task_trust(), TrustLevel::Trusted);
    }

    #[tokio::test]
    async fn test_current_task_trust_inside_scope() {
        let trust = CURRENT_TASK_TRUST
            .scope(TrustLevel::Untrusted, async { current_task_trust() })
            .await;
        assert_eq!(trust, TrustLevel::Untrusted);
    }

    #[tokio::test]
    async fn test_current_task_elevated_defaults_to_false() {
        assert!(!current_task_elevated());
    }

    #[tokio::test]
    async fn test_current_task_elevated_inside_scope() {
        let elevated = CURRENT_TASK_ELEVATED
            .scope(true, async { current_task_elevated() })
            .await;
        assert!(elevated);
    }

    #[tokio::test]
    async fn test_current_task_id_defaults_to_none() {
        assert_eq!(current_task_id(), None);
    }

    #[tokio::test]
    async fn test_current_task_id_inside_scope() {
        let id: Arc<str> = Arc::from("task-42");
        let result = CURRENT_TASK_ID
            .scope(Some(Arc::clone(&id)), async { current_task_id() })
            .await;
        assert_eq!(result, Some(id));
    }

    #[tokio::test]
    async fn test_current_parent_chain_defaults_to_empty() {
        assert!(current_parent_chain().is_empty());
    }

    #[tokio::test]
    async fn test_current_parent_chain_inside_scope() {
        let chain = vec![PathBuf::from("/a/b.nika.yaml")];
        let result = PARENT_CHAIN
            .scope(chain.clone(), async { current_parent_chain() })
            .await;
        assert_eq!(result, chain);
    }
}
