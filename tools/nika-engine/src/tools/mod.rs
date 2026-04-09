// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Tool Context + Shield path reconnaissance.
//!
//! File tool implementations (read, write, edit, glob, grep) have moved to
//! `nika-builtin`. This module retains:
//!
//! - [`ToolContext`] — working directory + permission mode for the executor
//! - [`PermissionMode`] — 4-level permission gate
//! - [`check_path_readable`] — Shield Item 3b: blocks untrusted reads of sensitive files
//! - [`DynamicSubmitTool`] — structured output tool injection (Layer 0b)

mod context;
mod submit_tool;
#[cfg(test)]
mod tests_shield_path_check;

pub use context::{PermissionMode, ToolContext};
pub use submit_tool::{DynamicSubmitTool, ToolDefinition};

use crate::error::NikaError;
use std::path::{Path, PathBuf};

// ═══════════════════════════════════════════════════════════════════════════
// SHIELD: PATH RECONNAISSANCE BLOCK
// ═══════════════════════════════════════════════════════════════════════════

/// Files that an untrusted agent must never read. Sprint 2 Item 3b.
///
/// Resolved by file name (after canonicalization, so symlink-bait fails).
pub const SENSITIVE_FILE_NAMES: &[&str] = &[".mcp.json", "nika.toml"];

/// Suffixes that an untrusted agent must never read. Sprint 2 Item 3b.
pub const SENSITIVE_FILE_SUFFIXES: &[&str] = &[".nika.yaml", ".nika.yml"];

/// File-name patterns matching dot-env families. We treat any file whose
/// name starts with `.env` (`.env`, `.env.local`, `.env.production`, …)
/// as sensitive.
#[inline]
fn is_dotenv_family(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

/// Check whether a path is readable by the calling task. Sprint 2 Item 3b.
///
/// For tainted agents (untrusted upstream inputs, not `trust: elevated`),
/// blocks reads of `nika.toml`, `.mcp.json`, `.env*`, and any `*.nika.yaml`
/// workflow file. Resolves symlinks before checking so a symlink-bait
/// attack (`innocent.txt` → `nika.toml`) is defeated.
///
/// Returns `Ok(())` for trusted callers, elevated callers, and any path
/// that is not on the sensitive list.
pub fn check_path_readable(
    path: &Path,
    caller_trust: nika_core::trust::TrustLevel,
    caller_elevated: bool,
) -> Result<(), NikaError> {
    if !caller_trust.is_untrusted() || caller_elevated {
        return Ok(());
    }

    // Canonicalize first to defeat symlink-bait attacks. If canonicalization
    // fails (e.g. file doesn't exist yet), fall back to the literal path —
    // the caller's read will then fail with the underlying I/O error and
    // there's nothing to leak.
    let canonical = path.canonicalize().unwrap_or_else(|_| PathBuf::from(path));
    let file_name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let blocked = SENSITIVE_FILE_NAMES.contains(&file_name)
        || SENSITIVE_FILE_SUFFIXES
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
        || is_dotenv_family(file_name);

    if blocked {
        let task_id = nika_kernel::task_local::current_task_id()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(NikaError::CapabilityDenied {
            task_id,
            action: "nika:read".to_string(),
            reason: format!(
                "tainted agent cannot read sensitive file: {file_name} \
                 (canonical: {})",
                canonical.display()
            ),
        });
    }

    Ok(())
}
