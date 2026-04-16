// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Routing configuration — the `routing:` section at workflow root.

use serde::{Deserialize, Serialize};

/// Routing configuration for dynamic task selection.
///
/// When set, the runtime uses an LLM to route to the appropriate task
/// based on input analysis rather than following the static DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RoutingConfig {
    /// Whether routing is enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Model to use for routing decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl RoutingConfig {
    /// Create a new routing configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            enabled: false,
            model: None,
        }
    }
}

impl Default for RoutingConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_is_disabled() {
        let r = RoutingConfig::new();
        assert!(!r.enabled);
        assert!(r.model.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let r = RoutingConfig {
            enabled: true,
            model: Some("claude-sonnet-4-5-20250514".into()),
        };
        let json = serde_json::to_string(&r).expect("serialize");
        let back: RoutingConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(back.enabled);
        assert_eq!(back.model.as_deref(), Some("claude-sonnet-4-5-20250514"));
    }
}
