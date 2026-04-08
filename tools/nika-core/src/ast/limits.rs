// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent Limits Configuration
//!
//! Defines resource limits for agent execution including turns, tokens,
//! cost, and duration constraints with configurable actions on limit reach.
//!
//! ## Features
//!
//! - **max_turns**: Maximum agentic loop iterations
//! - **max_tokens**: Total token budget (input + output)
//! - **max_cost_usd**: Cost ceiling per execution
//! - **max_duration_secs**: Wall-clock timeout
//!
//! ## Example
//!
//! ```yaml
//! agent:
//!   prompt: "Research {{topic}}"
//!   limits:
//!     max_turns: 20
//!     max_tokens: 50000
//!     max_cost_usd: 2.00
//!     max_duration_secs: 300
//!     on_limit_reached:
//!       action: complete_partial
//!       save_progress: true
//! ```

use serde::Deserialize;

use crate::error::CoreError;

// ═══════════════════════════════════════════════════════════════════════════
// LimitsConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for agent execution limits.
///
/// Defines resource constraints that prevent runaway execution
/// and enable cost control for LLM-based agents.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LimitsConfig {
    /// Maximum number of agentic loop turns (0 = unlimited)
    #[serde(default)]
    pub max_turns: u32,

    /// Maximum total tokens (input + output) (0 = unlimited)
    #[serde(default)]
    pub max_tokens: u64,

    /// Maximum cost in USD (0.0 = unlimited)
    #[serde(default)]
    pub max_cost_usd: f64,

    /// Maximum execution duration in seconds (0 = unlimited)
    #[serde(default)]
    pub max_duration_secs: u64,

    /// Action to take when a limit is reached
    #[serde(default)]
    pub on_limit_reached: OnLimitReachedConfig,
}

impl LimitsConfig {
    /// Check if any limits are configured.
    pub fn has_limits(&self) -> bool {
        self.max_turns > 0
            || self.max_tokens > 0
            || self.max_cost_usd > 0.0
            || self.max_duration_secs > 0
    }

    /// Check if turns limit is configured.
    pub fn has_turns_limit(&self) -> bool {
        self.max_turns > 0
    }

    /// Check if tokens limit is configured.
    pub fn has_tokens_limit(&self) -> bool {
        self.max_tokens > 0
    }

    /// Check if cost limit is configured.
    pub fn has_cost_limit(&self) -> bool {
        self.max_cost_usd > 0.0
    }

    /// Check if duration limit is configured.
    pub fn has_duration_limit(&self) -> bool {
        self.max_duration_secs > 0
    }

    /// Validate the limits configuration.
    pub fn validate(&self) -> Result<(), CoreError> {
        // Cost must be non-negative
        if self.max_cost_usd < 0.0 {
            return Err(CoreError::ValidationError {
                reason: format!(
                    "limits.max_cost_usd must be non-negative, got {}",
                    self.max_cost_usd
                ),
            });
        }

        // Validate on_limit_reached
        self.on_limit_reached.validate()?;

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// OnLimitReachedConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for behavior when a limit is reached.
#[derive(Debug, Clone, Deserialize)]
pub struct OnLimitReachedConfig {
    /// Action to take when limit is reached
    #[serde(default)]
    pub action: LimitAction,

    /// Whether to save partial progress
    #[serde(default = "default_save_progress")]
    pub save_progress: bool,

    /// Custom message for partial completion
    #[serde(default)]
    pub message: Option<String>,
}

impl Default for OnLimitReachedConfig {
    fn default() -> Self {
        Self {
            action: LimitAction::default(),
            save_progress: default_save_progress(),
            message: None,
        }
    }
}

impl OnLimitReachedConfig {
    /// Validate the on_limit_reached configuration.
    pub fn validate(&self) -> Result<(), CoreError> {
        // Escalate action requires save_progress to be useful
        if self.action == LimitAction::Escalate && !self.save_progress {
            // Just a warning, not an error
        }
        Ok(())
    }
}

fn default_save_progress() -> bool {
    true
}

// ═══════════════════════════════════════════════════════════════════════════
// LimitAction
// ═══════════════════════════════════════════════════════════════════════════

/// Action to take when a limit is reached.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitAction {
    /// Complete with partial results (recommended)
    #[default]
    CompletePartial,

    /// Fail the task with an error
    Fail,

    /// Escalate to human or supervisor agent
    Escalate,
}

impl LimitAction {
    /// Get a human-readable description of the action.
    pub fn description(&self) -> &'static str {
        match self {
            LimitAction::CompletePartial => "complete with partial results",
            LimitAction::Fail => "fail the task",
            LimitAction::Escalate => "escalate to human/supervisor",
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LimitType
// ═══════════════════════════════════════════════════════════════════════════

/// Type of limit that was reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitType {
    /// Maximum turns exceeded
    Turns,
    /// Maximum tokens exceeded
    Tokens,
    /// Maximum cost exceeded
    Cost,
    /// Maximum duration exceeded
    Duration,
}

impl LimitType {
    /// Get a human-readable name for the limit type.
    pub fn name(&self) -> &'static str {
        match self {
            LimitType::Turns => "turns",
            LimitType::Tokens => "tokens",
            LimitType::Cost => "cost",
            LimitType::Duration => "duration",
        }
    }
}

impl std::fmt::Display for LimitType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// LimitStatus
// ═══════════════════════════════════════════════════════════════════════════

/// Current status of a limit check.
#[derive(Debug, Clone)]
pub struct LimitStatus {
    /// Type of limit
    pub limit_type: LimitType,
    /// Current value
    pub current: f64,
    /// Maximum allowed value
    pub maximum: f64,
    /// Percentage used (0.0-1.0)
    pub usage_pct: f64,
    /// Whether the limit has been exceeded
    pub exceeded: bool,
}

impl LimitStatus {
    /// Create a new limit status.
    pub fn new(limit_type: LimitType, current: f64, maximum: f64) -> Self {
        let usage_pct = if maximum > 0.0 {
            (current / maximum).min(1.0)
        } else {
            0.0
        };
        Self {
            limit_type,
            current,
            maximum,
            usage_pct,
            exceeded: maximum > 0.0 && current >= maximum,
        }
    }

    /// Get remaining capacity.
    pub fn remaining(&self) -> f64 {
        if self.maximum > 0.0 {
            (self.maximum - self.current).max(0.0)
        } else {
            f64::INFINITY
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    // ========================================================================
    // LimitsConfig parsing tests
    // ========================================================================

    #[test]
    fn parse_limits_config_full() {
        let yaml = r#"
max_turns: 20
max_tokens: 50000
max_cost_usd: 2.00
max_duration_secs: 300
on_limit_reached:
  action: complete_partial
  save_progress: true
  message: "Limit reached, returning partial results"
"#;
        let config: LimitsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_turns, 20);
        assert_eq!(config.max_tokens, 50000);
        assert_eq!(config.max_cost_usd, 2.00);
        assert_eq!(config.max_duration_secs, 300);
        assert_eq!(config.on_limit_reached.action, LimitAction::CompletePartial);
        assert!(config.on_limit_reached.save_progress);
        assert!(config.on_limit_reached.message.is_some());
    }

    #[test]
    fn parse_limits_config_defaults() {
        let yaml = "";
        let config: LimitsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_turns, 0); // 0 = unlimited
        assert_eq!(config.max_tokens, 0); // 0 = unlimited
        assert!((config.max_cost_usd - 0.0).abs() < f64::EPSILON); // 0.0 = unlimited
        assert_eq!(config.max_duration_secs, 0); // 0 = unlimited
        assert_eq!(config.on_limit_reached.action, LimitAction::CompletePartial);
        assert!(config.on_limit_reached.save_progress);
    }

    #[test]
    fn parse_limits_config_partial() {
        let yaml = r#"
max_turns: 10
max_cost_usd: 1.50
"#;
        let config: LimitsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.max_turns, 10);
        assert_eq!(config.max_cost_usd, 1.50);
        assert_eq!(config.max_tokens, 0); // default
        assert_eq!(config.max_duration_secs, 0); // default
    }

    // ========================================================================
    // LimitAction parsing tests
    // ========================================================================

    #[test]
    fn parse_limit_action_complete_partial() {
        let yaml = r#"
on_limit_reached:
  action: complete_partial
"#;
        let config: LimitsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.on_limit_reached.action, LimitAction::CompletePartial);
    }

    #[test]
    fn parse_limit_action_fail() {
        let yaml = r#"
on_limit_reached:
  action: fail
"#;
        let config: LimitsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.on_limit_reached.action, LimitAction::Fail);
    }

    #[test]
    fn parse_limit_action_escalate() {
        let yaml = r#"
on_limit_reached:
  action: escalate
"#;
        let config: LimitsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.on_limit_reached.action, LimitAction::Escalate);
    }

    // ========================================================================
    // has_limits tests
    // ========================================================================

    #[test]
    fn has_limits_false_when_all_zero() {
        let config = LimitsConfig::default();
        assert!(!config.has_limits());
    }

    #[test]
    fn has_limits_true_when_turns_set() {
        let config = LimitsConfig {
            max_turns: 10,
            ..Default::default()
        };
        assert!(config.has_limits());
        assert!(config.has_turns_limit());
        assert!(!config.has_tokens_limit());
    }

    #[test]
    fn has_limits_true_when_tokens_set() {
        let config = LimitsConfig {
            max_tokens: 50000,
            ..Default::default()
        };
        assert!(config.has_limits());
        assert!(config.has_tokens_limit());
    }

    #[test]
    fn has_limits_true_when_cost_set() {
        let config = LimitsConfig {
            max_cost_usd: 2.00,
            ..Default::default()
        };
        assert!(config.has_limits());
        assert!(config.has_cost_limit());
    }

    #[test]
    fn has_limits_true_when_duration_set() {
        let config = LimitsConfig {
            max_duration_secs: 300,
            ..Default::default()
        };
        assert!(config.has_limits());
        assert!(config.has_duration_limit());
    }

    // ========================================================================
    // Validation tests
    // ========================================================================

    #[test]
    fn validate_config_valid() {
        let config = LimitsConfig {
            max_turns: 20,
            max_tokens: 50000,
            max_cost_usd: 2.00,
            max_duration_secs: 300,
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_negative_cost_invalid() {
        let config = LimitsConfig {
            max_cost_usd: -1.00,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("max_cost_usd"));
        assert!(err.to_string().contains("non-negative"));
    }

    #[test]
    fn validate_zero_values_valid() {
        let config = LimitsConfig::default();
        assert!(config.validate().is_ok());
    }

    // ========================================================================
    // LimitStatus tests
    // ========================================================================

    #[test]
    fn limit_status_not_exceeded() {
        let status = LimitStatus::new(LimitType::Turns, 5.0, 20.0);
        assert!(!status.exceeded);
        assert_eq!(status.usage_pct, 0.25);
        assert_eq!(status.remaining(), 15.0);
    }

    #[test]
    fn limit_status_exceeded() {
        let status = LimitStatus::new(LimitType::Tokens, 50000.0, 50000.0);
        assert!(status.exceeded);
        assert_eq!(status.usage_pct, 1.0);
        assert_eq!(status.remaining(), 0.0);
    }

    #[test]
    fn limit_status_over_exceeded() {
        let status = LimitStatus::new(LimitType::Cost, 3.50, 2.00);
        assert!(status.exceeded);
        assert_eq!(status.usage_pct, 1.0); // Capped at 1.0
        assert_eq!(status.remaining(), 0.0);
    }

    #[test]
    fn limit_status_unlimited() {
        let status = LimitStatus::new(LimitType::Duration, 100.0, 0.0);
        assert!(!status.exceeded);
        assert_eq!(status.usage_pct, 0.0);
        assert!(status.remaining().is_infinite());
    }

    // ========================================================================
    // LimitType tests
    // ========================================================================

    #[test]
    fn limit_type_names() {
        assert_eq!(LimitType::Turns.name(), "turns");
        assert_eq!(LimitType::Tokens.name(), "tokens");
        assert_eq!(LimitType::Cost.name(), "cost");
        assert_eq!(LimitType::Duration.name(), "duration");
    }

    #[test]
    fn limit_type_display() {
        assert_eq!(format!("{}", LimitType::Turns), "turns");
        assert_eq!(format!("{}", LimitType::Cost), "cost");
    }

    // ========================================================================
    // LimitAction description tests
    // ========================================================================

    #[test]
    fn limit_action_descriptions() {
        assert_eq!(
            LimitAction::CompletePartial.description(),
            "complete with partial results"
        );
        assert_eq!(LimitAction::Fail.description(), "fail the task");
        assert_eq!(
            LimitAction::Escalate.description(),
            "escalate to human/supervisor"
        );
    }
}
