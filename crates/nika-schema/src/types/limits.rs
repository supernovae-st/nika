// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Limits configuration — the `limits:` section on agent tasks.
//!
//! Controls runtime resource limits for agent execution: max turns,
//! tool call limits, and timeout enforcement.

use std::fmt;

use serde::{Deserialize, Serialize};

/// What action to take when a limit is reached.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum LimitAction {
    /// Stop the agent and return whatever it has so far.
    #[default]
    Stop,
    /// Return an error.
    Fail,
    /// Warn but continue (best-effort).
    Warn,
}

impl fmt::Display for LimitAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Stop => write!(f, "stop"),
            Self::Fail => write!(f, "fail"),
            Self::Warn => write!(f, "warn"),
        }
    }
}

/// Current status of a limit check.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum LimitStatus {
    /// Within limits.
    #[default]
    Ok,
    /// Approaching the limit (>80%).
    Warning,
    /// Limit reached.
    Exceeded,
}

impl LimitStatus {
    /// Create a new limit status (defaults to `Ok`).
    #[must_use]
    pub fn new() -> Self {
        Self::Ok
    }
}

/// What type of resource is being limited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum LimitType {
    /// Number of agent turns (LLM calls in the loop).
    Turns,
    /// Number of tool calls.
    ToolCalls,
    /// Total tokens consumed.
    Tokens,
    /// Wall-clock time in seconds.
    Duration,
}

impl fmt::Display for LimitType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Turns => write!(f, "turns"),
            Self::ToolCalls => write!(f, "tool_calls"),
            Self::Tokens => write!(f, "tokens"),
            Self::Duration => write!(f, "duration"),
        }
    }
}

/// Limits configuration for agent tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LimitsConfig {
    /// Maximum number of agent turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Maximum number of tool calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tool_calls: Option<u32>,
    /// Maximum total tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Maximum wall-clock duration in seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<u64>,
    /// Action to take when any limit is reached.
    #[serde(default)]
    pub on_limit: LimitAction,
}

impl LimitsConfig {
    /// Create a new empty limits configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            max_turns: None,
            max_tool_calls: None,
            max_tokens: None,
            max_duration_secs: None,
            on_limit: LimitAction::default(),
        }
    }

    /// Returns `true` if no limits are configured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.max_turns.is_none()
            && self.max_tool_calls.is_none()
            && self.max_tokens.is_none()
            && self.max_duration_secs.is_none()
    }
}

impl Default for LimitsConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limit_action_display() {
        assert_eq!(LimitAction::Stop.to_string(), "stop");
        assert_eq!(LimitAction::Fail.to_string(), "fail");
        assert_eq!(LimitAction::Warn.to_string(), "warn");
    }

    #[test]
    fn limit_type_display() {
        assert_eq!(LimitType::Turns.to_string(), "turns");
        assert_eq!(LimitType::ToolCalls.to_string(), "tool_calls");
    }

    #[test]
    fn limits_config_empty() {
        let lc = LimitsConfig::new();
        assert!(lc.is_empty());
    }

    #[test]
    fn limits_config_serde_roundtrip() {
        let lc = LimitsConfig {
            max_turns: Some(10),
            max_tool_calls: Some(50),
            on_limit: LimitAction::Fail,
            ..LimitsConfig::new()
        };
        let json = serde_json::to_string(&lc).expect("serialize");
        let back: LimitsConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.max_turns, Some(10));
        assert_eq!(back.max_tool_calls, Some(50));
        assert_eq!(back.on_limit, LimitAction::Fail);
    }

    #[test]
    fn limit_status_default_is_ok() {
        assert_eq!(LimitStatus::default(), LimitStatus::Ok);
    }
}
