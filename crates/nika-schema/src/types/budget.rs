// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Budget configuration — the `budget:` section at workflow or task level.
//!
//! Budgets control resource consumption: token limits, cost caps, and
//! time bounds. They are hierarchical (workflow-level defaults can be
//! overridden per-task).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Budget configuration for a workflow or task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct BudgetConfig {
    /// Maximum total tokens (input + output) for this scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Maximum output tokens per LLM call.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Maximum cost in USD for this scope.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_usd: Option<f64>,
    /// Maximum wall-clock duration in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<u64>,
    /// Maximum number of LLM calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_calls: Option<u32>,
}

impl BudgetConfig {
    /// Create an empty budget configuration (no limits).
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_tokens: None,
            max_output_tokens: None,
            max_cost_usd: None,
            max_duration_secs: None,
            max_calls: None,
        }
    }

    /// Returns `true` if no budget limits are set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.max_tokens.is_none()
            && self.max_output_tokens.is_none()
            && self.max_cost_usd.is_none()
            && self.max_duration_secs.is_none()
            && self.max_calls.is_none()
    }
}

impl Default for BudgetConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// A resolved budget with concrete limits.
///
/// This is the post-merge result of combining workflow-level and task-level
/// budget configurations. All fields are concrete (no templates).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct Budget {
    /// Maximum total tokens.
    pub max_tokens: Option<u64>,
    /// Maximum output tokens per call.
    pub max_output_tokens: Option<u32>,
    /// Maximum cost in nano-USD (for precision).
    pub max_cost_nano_usd: Option<u64>,
    /// Maximum duration in seconds.
    pub max_duration_secs: Option<u64>,
    /// Maximum number of LLM calls.
    pub max_calls: Option<u32>,
}

impl Budget {
    /// Create an empty budget (no limits).
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_tokens: None,
            max_output_tokens: None,
            max_cost_nano_usd: None,
            max_duration_secs: None,
            max_calls: None,
        }
    }

    /// Returns `true` if no limits are set.
    #[must_use]
    pub fn is_unlimited(&self) -> bool {
        self.max_tokens.is_none()
            && self.max_output_tokens.is_none()
            && self.max_cost_nano_usd.is_none()
            && self.max_duration_secs.is_none()
            && self.max_calls.is_none()
    }
}

impl Default for Budget {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Budget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_unlimited() {
            return write!(f, "unlimited");
        }
        let mut parts = Vec::new();
        if let Some(t) = self.max_tokens {
            parts.push(format!("tokens={t}"));
        }
        if let Some(c) = self.max_cost_nano_usd {
            #[allow(clippy::cast_precision_loss)]
            parts.push(format!("cost=${:.4}", c as f64 / 1_000_000_000.0));
        }
        if let Some(d) = self.max_duration_secs {
            parts.push(format!("duration={d}s"));
        }
        if let Some(n) = self.max_calls {
            parts.push(format!("calls={n}"));
        }
        write!(f, "{}", parts.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_config_empty() {
        let bc = BudgetConfig::new();
        assert!(bc.is_empty());
    }

    #[test]
    fn budget_config_not_empty() {
        let bc = BudgetConfig {
            max_tokens: Some(10_000),
            ..BudgetConfig::new()
        };
        assert!(!bc.is_empty());
    }

    #[test]
    fn budget_config_serde_roundtrip() {
        let bc = BudgetConfig {
            max_tokens: Some(50_000),
            max_cost_usd: Some(1.50),
            max_duration_secs: Some(300),
            ..BudgetConfig::new()
        };
        let json = serde_json::to_string(&bc).expect("serialize");
        let back: BudgetConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.max_tokens, Some(50_000));
        assert!((back.max_cost_usd.unwrap() - 1.50).abs() < f64::EPSILON);
    }

    #[test]
    fn budget_unlimited_display() {
        assert_eq!(Budget::new().to_string(), "unlimited");
    }

    #[test]
    fn budget_display_with_limits() {
        let b = Budget {
            max_tokens: Some(10_000),
            max_cost_nano_usd: Some(1_500_000_000), // $1.50
            ..Budget::new()
        };
        let display = b.to_string();
        assert!(display.contains("tokens=10000"));
        assert!(display.contains("cost=$1.5000"));
    }
}
