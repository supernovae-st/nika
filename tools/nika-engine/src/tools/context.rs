// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool Context — working directory + permission mode for the executor.
//!
//! File tool implementations have moved to `nika-builtin`. This module
//! retains `ToolContext` (working directory tracking, permission gating)
//! and `PermissionMode` which are used by the executor and router.

use std::path::PathBuf;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};


// ═══════════════════════════════════════════════════════════════════════════
// PERMISSION MODE
// ═══════════════════════════════════════════════════════════════════════════

/// Permission mode for tool operations.
///
/// Inspired by Gemini CLI's Yolo Mode and Claude Code's permission levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PermissionMode {
    /// Deny all file operations
    Deny,

    /// Ask before each operation (Plan mode)
    #[default]
    Plan,

    /// Auto-approve file edits and writes, ask for others
    AcceptEdits,

    /// Auto-approve all operations
    YoloMode,
}

impl PermissionMode {
    /// Get display name
    pub fn display_name(&self) -> &'static str {
        match self {
            PermissionMode::Deny => "Deny",
            PermissionMode::Plan => "Plan",
            PermissionMode::AcceptEdits => "AcceptEdits",
            PermissionMode::YoloMode => "YoloMode (Yolo)",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TOOL CONTEXT
// ═══════════════════════════════════════════════════════════════════════════

/// Shared context for executor tool setup.
///
/// Provides the working directory (security boundary) and permission mode.
/// Used by `BuiltinToolRouter::with_file_tools()` and `TaskExecutor`.
pub struct ToolContext {
    /// Working directory (security boundary).
    /// Mutable via RwLock so the executor can update it at runtime.
    working_dir: RwLock<PathBuf>,

    /// Current permission mode.
    permission_mode: RwLock<PermissionMode>,
}

impl ToolContext {
    /// Create a new tool context.
    pub fn new(working_dir: PathBuf, permission_mode: PermissionMode) -> Self {
        // Canonicalize to resolve symlinks (e.g., /var → /private/var on macOS)
        let working_dir = working_dir.canonicalize().unwrap_or(working_dir);
        Self {
            working_dir: RwLock::new(working_dir),
            permission_mode: RwLock::new(permission_mode),
        }
    }

    /// Get the working directory.
    pub fn working_dir(&self) -> PathBuf {
        self.working_dir.read().clone()
    }

    /// Update the working directory (called from executor on project root discovery).
    pub fn set_working_dir(&self, dir: PathBuf) {
        let dir = dir.canonicalize().unwrap_or(dir);
        *self.working_dir.write() = dir;
    }

    /// Get current permission mode.
    pub fn permission_mode(&self) -> PermissionMode {
        *self.permission_mode.read()
    }

    /// Set permission mode.
    pub fn set_permission_mode(&self, mode: PermissionMode) {
        *self.permission_mode.write() = mode;
    }
}

impl std::fmt::Debug for ToolContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolContext")
            .field("working_dir", &*self.working_dir.read())
            .field("permission_mode", &self.permission_mode())
            .finish()
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Arc;

    fn test_context() -> Arc<ToolContext> {
        let working_dir = env::current_dir().unwrap();
        Arc::new(ToolContext::new(working_dir, PermissionMode::YoloMode))
    }

    #[test]
    fn test_permission_mode_default() {
        assert_eq!(PermissionMode::default(), PermissionMode::Plan);
    }

    #[test]
    fn test_working_dir_set_and_get() {
        let ctx = test_context();
        let original = ctx.working_dir();

        let new_dir = std::env::temp_dir();
        ctx.set_working_dir(new_dir.clone());

        let updated = ctx.working_dir();
        assert_ne!(original, updated);
    }

    #[test]
    fn test_permission_mode_change() {
        let ctx = test_context();
        assert_eq!(ctx.permission_mode(), PermissionMode::YoloMode);

        ctx.set_permission_mode(PermissionMode::Plan);
        assert_eq!(ctx.permission_mode(), PermissionMode::Plan);
    }

    #[test]
    fn test_debug_format() {
        let ctx = test_context();
        let debug = format!("{:?}", ctx);
        assert!(debug.contains("ToolContext"));
        assert!(debug.contains("YoloMode"));
    }
}
