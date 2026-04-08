// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shared types for the Daemon ↔ LSP bridge.
//!
//! These types are used by both `nika-daemon` (protocol) and `nika-lsp-core` (handlers).
//! They live in `nika-core` to avoid circular dependencies.

use serde::{Deserialize, Serialize};

use super::providers::ProviderCategory;

/// Where a provider's API key was found.
///
/// Parallel to `nika_daemon::protocol::SecretSource` but in nika-core scope,
/// usable by nika-lsp-core without depending on nika-daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum KeySource {
    /// From an environment variable.
    Env,
    /// From the encrypted vault (via daemon).
    Vault,
    /// Not found.
    NotFound,
}

/// Provider status with key availability — used in LSP completions and diagnostics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderStatusInfo {
    /// Provider identifier (e.g., "anthropic").
    pub id: String,
    /// Human-readable name (e.g., "Anthropic Claude").
    pub name: String,
    /// Whether an API key is available.
    pub has_key: bool,
    /// Where the key was found.
    pub source: KeySource,
    /// Provider category.
    pub category: ProviderCategory,
    /// Environment variable name for the key.
    pub env_var: String,
}

/// Summary of a workflow run — used in hover and code lens.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRunInfo {
    /// Job identifier.
    pub job_id: String,
    /// Job state ("completed", "failed", "running", etc.).
    pub state: String,
    /// Workflow file path.
    pub workflow: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// ISO 8601 start timestamp (None if pending).
    pub started_at: Option<String>,
    /// ISO 8601 completion timestamp (None if still running).
    pub completed_at: Option<String>,
    /// Process exit code (None if still running or pending).
    pub exit_code: Option<i32>,
}

/// Daemon capabilities and stats — used in status bar and hover.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DaemonCapabilities {
    /// Daemon version string.
    pub version: String,
    /// Seconds since daemon started.
    pub uptime_secs: u64,
    /// Number of cached LLM responses.
    pub cache_entries: usize,
    /// Cache hit rate (0.0 - 1.0).
    pub cache_hit_rate: f64,
    /// Number of active jobs.
    pub active_jobs: usize,
    /// Whether file watch is active.
    pub watch_active: bool,
    /// Total USD saved by cache hits.
    pub total_cost_saved: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_status_info_roundtrip() {
        let info = ProviderStatusInfo {
            id: "anthropic".into(),
            name: "Anthropic Claude".into(),
            has_key: true,
            source: KeySource::Env,
            category: ProviderCategory::Llm,
            env_var: "ANTHROPIC_API_KEY".into(),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: ProviderStatusInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }

    #[test]
    fn key_source_all_variants() {
        for source in [KeySource::Env, KeySource::Vault, KeySource::NotFound] {
            let json = serde_json::to_string(&source).unwrap();
            let back: KeySource = serde_json::from_str(&json).unwrap();
            assert_eq!(source, back);
        }
    }

    #[test]
    fn workflow_run_info_roundtrip() {
        let info = WorkflowRunInfo {
            job_id: "abc-123".into(),
            state: "completed".into(),
            workflow: "test.nika.yaml".into(),
            created_at: "2026-03-27T12:00:00Z".into(),
            started_at: Some("2026-03-27T12:00:01Z".into()),
            completed_at: Some("2026-03-27T12:00:03Z".into()),
            exit_code: Some(0),
        };
        let json = serde_json::to_string(&info).unwrap();
        let back: WorkflowRunInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(info, back);
    }

    #[test]
    fn daemon_capabilities_roundtrip() {
        let caps = DaemonCapabilities {
            version: "0.50.0".into(),
            uptime_secs: 3600,
            cache_entries: 42,
            cache_hit_rate: 0.87,
            active_jobs: 2,
            watch_active: true,
            total_cost_saved: 1.23,
        };
        let json = serde_json::to_string(&caps).unwrap();
        let back: DaemonCapabilities = serde_json::from_str(&json).unwrap();
        assert_eq!(caps, back);
    }
}
