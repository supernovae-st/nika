// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Orchestrate configuration for goal-driven workflow orchestration.
//!
//! When a workflow has `goal:` + `orchestrate:`, the runtime wraps all tasks
//! into an agent loop that pursues the goal with configurable limits.

use serde::Deserialize;

/// Configuration for the orchestrator agent loop.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrchestrateConfig {
    /// Maximum orchestration rounds (default: 10)
    #[serde(default = "default_max_rounds")]
    pub max_rounds: u32,

    /// Target confidence level 0.0-1.0 (default: 0.85)
    #[serde(default = "default_confidence_target")]
    pub confidence_target: f64,

    /// Optional agent preset reference (from `agents:` block)
    pub agent: Option<String>,

    /// Maximum cost in USD before stopping
    pub max_cost_usd: Option<f64>,
}

impl OrchestrateConfig {
    /// Validate configuration values. Clamps confidence_target to 0.0-1.0.
    pub fn validate(&mut self) {
        self.confidence_target = self.confidence_target.clamp(0.0, 1.0);
        if self.max_rounds == 0 {
            self.max_rounds = 1;
        }
    }
}

impl Default for OrchestrateConfig {
    fn default() -> Self {
        Self {
            max_rounds: default_max_rounds(),
            confidence_target: default_confidence_target(),
            agent: None,
            max_cost_usd: None,
        }
    }
}

fn default_max_rounds() -> u32 {
    10
}

fn default_confidence_target() -> f64 {
    0.85
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = OrchestrateConfig::default();
        assert_eq!(config.max_rounds, 10);
        assert!((config.confidence_target - 0.85).abs() < f64::EPSILON);
        assert!(config.agent.is_none());
        assert!(config.max_cost_usd.is_none());
    }

    #[test]
    fn test_deserialize_full_config() {
        let json = serde_json::json!({
            "max_rounds": 20,
            "confidence_target": 0.95,
            "agent": "researcher",
            "max_cost_usd": 5.0
        });
        let config: OrchestrateConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.max_rounds, 20);
        assert!((config.confidence_target - 0.95).abs() < f64::EPSILON);
        assert_eq!(config.agent.as_deref(), Some("researcher"));
        assert_eq!(config.max_cost_usd, Some(5.0));
    }

    #[test]
    fn test_deserialize_partial_config() {
        let json = serde_json::json!({
            "max_rounds": 5
        });
        let config: OrchestrateConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.max_rounds, 5);
        assert!((config.confidence_target - 0.85).abs() < f64::EPSILON);
    }

    #[test]
    fn test_deserialize_empty_config() {
        let json = serde_json::json!({});
        let config: OrchestrateConfig = serde_json::from_value(json).unwrap();
        assert_eq!(config.max_rounds, 10);
    }

    #[test]
    fn test_deserialize_rejects_unknown_fields() {
        let json = serde_json::json!({
            "max_rounds": 10,
            "unknown_field": true
        });
        let result: Result<OrchestrateConfig, _> = serde_json::from_value(json);
        assert!(result.is_err());
    }
}
