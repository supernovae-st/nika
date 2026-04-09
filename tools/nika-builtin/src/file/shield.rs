// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shield path check for file tools.
//!
//! Nika Shield Item 3b: untrusted agents cannot read sensitive files
//! (`.mcp.json`, `nika.toml`, `.env*`, `*.nika.yaml`).
//!
//! This function reads trust state from `nika_kernel::task_local` so that
//! nika-builtin does not depend on nika-engine.

use crate::BuiltinError;
use nika_kernel::task_local::{current_task_elevated, current_task_id, current_task_trust};
use std::path::{Path, PathBuf};

/// Files that an untrusted agent must never read (Item 3b).
pub const SENSITIVE_FILE_NAMES: &[&str] = &[".mcp.json", "nika.toml"];

/// Suffixes that an untrusted agent must never read.
pub const SENSITIVE_FILE_SUFFIXES: &[&str] = &[".nika.yaml", ".nika.yml"];

#[inline]
fn is_dotenv_family(name: &str) -> bool {
    name == ".env" || name.starts_with(".env.")
}

/// Check whether `path` is readable by the calling task.
///
/// For untrusted callers that are not elevated, blocks reads of sensitive files
/// (`nika.toml`, `.mcp.json`, `.env*`, `*.nika.yaml`). Resolves symlinks first
/// to defeat symlink-bait attacks.
///
/// Returns `Ok(())` for trusted callers, elevated callers, or non-sensitive paths.
pub fn check_path_readable(path: &Path) -> Result<(), BuiltinError> {
    let trust = current_task_trust();
    let elevated = current_task_elevated();

    if !trust.is_untrusted() || elevated {
        return Ok(());
    }

    // Canonicalize to defeat symlink-bait. If canonicalization fails
    // (file doesn't exist yet), fall back to the literal path — the
    // I/O op will fail anyway and there is nothing to leak.
    let canonical = path
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    let file_name = canonical.file_name().and_then(|n| n.to_str()).unwrap_or("");

    let blocked = SENSITIVE_FILE_NAMES.contains(&file_name)
        || SENSITIVE_FILE_SUFFIXES
            .iter()
            .any(|suffix| file_name.ends_with(suffix))
        || is_dotenv_family(file_name);

    if blocked {
        let task_id = current_task_id()
            .map(|id| id.to_string())
            .unwrap_or_else(|| "<unknown>".to_string());
        return Err(BuiltinError::Denied {
            tool: format!("nika:read (task {task_id})"),
            reason: format!(
                "[NIKA-380] tainted agent cannot read sensitive file: {file_name} \
                 (canonical: {})",
                canonical.display()
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::task_local::{CURRENT_TASK_ELEVATED, CURRENT_TASK_TRUST};
    use nika_core::trust::TrustLevel;
    use tempfile::TempDir;

    async fn check_as_untrusted(path: &Path) -> Result<(), BuiltinError> {
        CURRENT_TASK_TRUST
            .scope(TrustLevel::Untrusted, async {
                CURRENT_TASK_ELEVATED.scope(false, async { check_path_readable(path) })
                    .await
            })
            .await
    }

    async fn check_as_trusted(path: &Path) -> Result<(), BuiltinError> {
        CURRENT_TASK_TRUST
            .scope(TrustLevel::Trusted, async { check_path_readable(path) })
            .await
    }

    #[tokio::test]
    async fn test_trusted_caller_passes_all_paths() {
        // Create a real .mcp.json in a tempdir so canonicalize works
        let d = TempDir::new().unwrap();
        let path = d.path().join(".mcp.json");
        std::fs::write(&path, "{}").unwrap();
        assert!(check_as_trusted(&path).await.is_ok());
    }

    #[tokio::test]
    async fn test_untrusted_mcp_json_blocked() {
        let d = TempDir::new().unwrap();
        let path = d.path().join(".mcp.json");
        std::fs::write(&path, "{}").unwrap();
        let result = check_as_untrusted(&path).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-380"));
    }

    #[tokio::test]
    async fn test_untrusted_nika_toml_blocked() {
        let d = TempDir::new().unwrap();
        let path = d.path().join("nika.toml");
        std::fs::write(&path, "[project]").unwrap();
        let result = check_as_untrusted(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_untrusted_env_file_blocked() {
        let d = TempDir::new().unwrap();
        let path = d.path().join(".env");
        std::fs::write(&path, "SECRET=123").unwrap();
        let result = check_as_untrusted(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_untrusted_env_local_blocked() {
        let d = TempDir::new().unwrap();
        let path = d.path().join(".env.local");
        std::fs::write(&path, "SECRET=123").unwrap();
        let result = check_as_untrusted(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_untrusted_nika_yaml_blocked() {
        let d = TempDir::new().unwrap();
        let path = d.path().join("workflow.nika.yaml");
        std::fs::write(&path, "schema: nika/workflow@0.12").unwrap();
        let result = check_as_untrusted(&path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_untrusted_normal_file_allowed() {
        let d = TempDir::new().unwrap();
        let path = d.path().join("data.json");
        std::fs::write(&path, "{}").unwrap();
        let result = check_as_untrusted(&path).await;
        assert!(result.is_ok(), "{:?}", result);
    }

    #[tokio::test]
    async fn test_elevated_untrusted_passes_sensitive() {
        let d = TempDir::new().unwrap();
        let path = d.path().join(".mcp.json");
        std::fs::write(&path, "{}").unwrap();
        let result = CURRENT_TASK_TRUST
            .scope(TrustLevel::Untrusted, async {
                CURRENT_TASK_ELEVATED
                    .scope(true, async { check_path_readable(&path) })
                    .await
            })
            .await;
        assert!(result.is_ok());
    }
}
