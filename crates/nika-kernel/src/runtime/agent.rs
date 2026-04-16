// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent configuration + outcome types — agent-v2 kernel hook.
//!
//! These types define the contract for agent loop behavior:
//! strategy, limits, error policy, and result reporting.
//! Business logic lives in `nika-verb-agent` (Phase 3).

use nika_error::checkpoint::{AgentCheckpoint, ToolCallRecord};
use nika_error::cost::Cost;
use serde::{Deserialize, Serialize};

// CompressionPolicy descended to nika-error/compression.rs (Phase 0).
pub use nika_error::compression::CompressionPolicy;

/// Configuration for an agent loop.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentLoopConfig {
    /// Maximum number of turns before stopping.
    pub max_turns: u32,
    /// Planning strategy (`ReAct` or `ReWOO`).
    pub planning: PlanningStrategy,
    /// Whether to execute tool calls in parallel.
    pub parallel_tools: bool,
    /// Reflection configuration (optional).
    pub reflection: Option<ReflectionConfig>,
    /// Context compression configuration (optional).
    pub compression: Option<CompressionPolicy>,
    /// How to handle tool errors.
    pub tool_error_policy: ToolErrorPolicy,
    /// Session identifier for correlation.
    pub session_id: Option<String>,
    /// Pre-existing tool call records to inject (for resume).
    pub inject_records: Vec<ToolCallRecord>,
}

impl AgentLoopConfig {
    /// Create a new agent loop config with required `max_turns`.
    #[must_use]
    pub fn new(max_turns: u32) -> Self {
        Self {
            max_turns,
            planning: PlanningStrategy::default(),
            parallel_tools: false,
            reflection: None,
            compression: None,
            tool_error_policy: ToolErrorPolicy::default(),
            session_id: None,
            inject_records: Vec::new(),
        }
    }
}

/// Outcome of an agent loop execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct AgentOutcome {
    /// Final output text.
    pub output: String,
    /// Why the agent stopped.
    pub stop_reason: AgentStopReason,
    /// Number of turns completed.
    pub turns: u32,
    /// Total tokens consumed.
    pub total_tokens: u64,
    /// Accumulated cost in USD (f64).
    ///
    /// Prefer [`Self::cost`] (exact nano-USD) for billing; scheduled for
    /// removal in v0.85.
    #[deprecated(
        since = "0.81.0",
        note = "use `cost: Option<Cost>` instead; `cost_usd` will be removed in v0.85"
    )]
    pub cost_usd: Option<f64>,
    /// Accumulated cost as nano-USD `Cost`. Preferred over `cost_usd` for
    /// billing aggregation and ledger reconciliation.
    pub cost: Option<Cost>,
    /// Final checkpoint (for resume/inspect).
    pub checkpoint: Option<AgentCheckpoint>,
}

impl AgentOutcome {
    /// Create a new agent outcome.
    #[must_use]
    #[allow(deprecated)] // cost_usd initialized for bw-compat; removal in v0.85
    pub fn new(
        output: impl Into<String>,
        stop_reason: AgentStopReason,
        turns: u32,
        total_tokens: u64,
    ) -> Self {
        Self {
            output: output.into(),
            stop_reason,
            turns,
            total_tokens,
            cost_usd: None,
            cost: None,
            checkpoint: None,
        }
    }
}

/// Reason an agent loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AgentStopReason {
    /// Agent completed successfully (natural end).
    Completed,
    /// Agent called explicit completion tool.
    ExplicitCompletion,
    /// Hit `max_turns` limit.
    TurnsLimit,
    /// Hit token budget limit.
    TokensLimit,
    /// Hit cost budget limit.
    CostLimit,
    /// Hit duration limit.
    DurationLimit,
    /// Agent loop failed.
    Failed,
}

/// Planning strategy for the agent loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PlanningStrategy {
    /// `ReAct`: observe → think → act (default).
    #[default]
    React,
    /// `ReWOO`: plan all steps first, then execute.
    ReWoo,
}

/// How to handle tool execution errors.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ToolErrorPolicy {
    /// Report errors to the LLM for self-correction (default).
    #[default]
    ReportToLlm,
    /// Retry transient errors up to a maximum.
    RetryTransient {
        /// Maximum retry attempts.
        max: u32,
    },
    /// Fail the agent loop immediately on any error.
    FailFast,
}

/// Reflection configuration.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ReflectionConfig {
    /// Whether reflection is enabled.
    pub enabled: bool,
    /// Quality threshold (0.0–1.0) below which to reflect.
    pub threshold: f32,
    /// Maximum reflection retry attempts.
    pub max_retries: u32,
}

impl ReflectionConfig {
    /// Create a new reflection config.
    #[must_use]
    pub fn new(threshold: f32, max_retries: u32) -> Self {
        Self {
            enabled: true,
            threshold,
            max_retries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_loop_config_new_defaults() {
        let config = AgentLoopConfig::new(10);
        assert_eq!(config.max_turns, 10);
        assert_eq!(config.planning, PlanningStrategy::React);
        assert!(!config.parallel_tools);
        assert!(config.reflection.is_none());
        assert!(config.compression.is_none());
        assert_eq!(config.tool_error_policy, ToolErrorPolicy::ReportToLlm);
        assert!(config.session_id.is_none());
        assert!(config.inject_records.is_empty());
    }

    #[test]
    #[allow(deprecated)] // reads cost_usd to assert None before v0.85 removal
    fn agent_outcome_new() {
        let outcome = AgentOutcome::new("done", AgentStopReason::Completed, 5, 1000);
        assert_eq!(outcome.output, "done");
        assert_eq!(outcome.stop_reason, AgentStopReason::Completed);
        assert_eq!(outcome.turns, 5);
        assert!(outcome.cost_usd.is_none());
        assert!(outcome.cost.is_none());
        assert!(outcome.checkpoint.is_none());
    }

    #[test]
    fn agent_stop_reason_serde_roundtrip() {
        let reason = AgentStopReason::ExplicitCompletion;
        let json = serde_json::to_string(&reason).expect("serialize");
        assert_eq!(json, "\"explicit_completion\"");
        let back: AgentStopReason = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, reason);
    }

    #[test]
    fn planning_strategy_default_is_react() {
        assert_eq!(PlanningStrategy::default(), PlanningStrategy::React);
    }

    #[test]
    fn tool_error_policy_default_is_report() {
        assert_eq!(ToolErrorPolicy::default(), ToolErrorPolicy::ReportToLlm);
    }

    #[test]
    fn reflection_config_new() {
        let config = ReflectionConfig::new(0.7, 3);
        assert!(config.enabled);
        assert_eq!(config.threshold, 0.7);
        assert_eq!(config.max_retries, 3);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn agent_types_send_sync() {
        _assert_send_sync::<AgentLoopConfig>();
        _assert_send_sync::<AgentOutcome>();
        _assert_send_sync::<ReflectionConfig>();
    }
}
