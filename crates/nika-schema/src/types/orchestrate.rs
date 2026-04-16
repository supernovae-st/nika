// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Orchestration configuration — the `orchestrate:` section at workflow root.

use serde::{Deserialize, Serialize};

/// Orchestration configuration for multi-agent workflows.
///
/// Controls how the runtime coordinates parallel task execution,
/// agent handoffs, and resource sharing between tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct OrchestrateConfig {
    /// Maximum number of tasks to run in parallel.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_parallel: Option<u32>,
    /// Whether to enable agent-to-agent handoff.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enable_handoff: Option<bool>,
}

impl OrchestrateConfig {
    /// Create a new empty orchestration configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_parallel: None,
            enable_handoff: None,
        }
    }
}

impl Default for OrchestrateConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_empty() {
        let o = OrchestrateConfig::new();
        assert!(o.max_parallel.is_none());
        assert!(o.enable_handoff.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let o = OrchestrateConfig {
            max_parallel: Some(4),
            enable_handoff: Some(true),
        };
        let json = serde_json::to_string(&o).expect("serialize");
        let back: OrchestrateConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.max_parallel, Some(4));
        assert_eq!(back.enable_handoff, Some(true));
    }
}
