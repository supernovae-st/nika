// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Types for the rig-based agent loop
//!
//! Contains: RigAgentStatus, RigAgentLoopResult, GuardrailCheckResult, StreamingResult,
//! and the ToolChoice conversion.

use rig::message::ToolChoice as RigToolChoice;
use serde_json::Value;

use crate::ast::ToolChoice as NikaToolChoice;

// ═══════════════════════════════════════════════════════════════════════════
// Types
// ═══════════════════════════════════════════════════════════════════════════

/// Status of the rig-based agent execution
#[derive(Debug, Clone, PartialEq)]
pub enum RigAgentStatus {
    /// Agent completed naturally (no more tool calls)
    NaturalCompletion,
    /// Agent called nika:complete tool explicitly
    ExplicitCompletion,
    /// Agent completed with confidence above threshold
    HighConfidence(f64),
    /// Agent completed with confidence below threshold
    LowConfidence(f64),
    /// Agent completed but flagged for review via routing
    FlaggedForReview(f64),
    /// Agent escalated to human/other agent via routing
    Escalated(f64),
    /// Maximum turns reached
    MaxTurnsReached,
    /// Token budget exceeded
    TokenBudgetExceeded,
    /// Cost budget exceeded
    CostLimitReached,
    /// Duration limit exceeded
    DurationLimitReached,
    /// Partial completion with checkpoint saved
    PartialCompletion,
    /// Agent failed with error
    Failed,
}

impl RigAgentStatus {
    /// Convert to canonical snake_case string for event logging.
    /// Aligns with Anthropic API's stop_reason values.
    pub fn as_canonical_str(&self) -> &'static str {
        match self {
            Self::NaturalCompletion => "end_turn",
            Self::ExplicitCompletion => "tool_complete",
            Self::HighConfidence(_) => "tool_complete_high",
            Self::LowConfidence(_) => "tool_complete_low",
            Self::FlaggedForReview(_) => "tool_complete_flagged",
            Self::Escalated(_) => "escalated",
            Self::MaxTurnsReached => "max_turns",
            Self::TokenBudgetExceeded => "max_tokens",
            Self::CostLimitReached => "max_cost",
            Self::DurationLimitReached => "max_duration",
            Self::PartialCompletion => "partial",
            Self::Failed => "error",
        }
    }

    /// Check if this status represents successful completion
    pub fn is_completed(&self) -> bool {
        matches!(
            self,
            Self::NaturalCompletion
                | Self::ExplicitCompletion
                | Self::HighConfidence(_)
                | Self::FlaggedForReview(_) // Accepted, just flagged
        )
    }

    /// Check if this status requires retry
    pub fn requires_retry(&self) -> bool {
        matches!(self, Self::LowConfidence(_))
    }

    /// Check if this status was escalated
    pub fn is_escalated(&self) -> bool {
        matches!(self, Self::Escalated(_))
    }

    /// Check if this status is flagged for review
    pub fn is_flagged(&self) -> bool {
        matches!(self, Self::FlaggedForReview(_))
    }

    /// Get confidence value if available
    pub fn confidence(&self) -> Option<f64> {
        match self {
            Self::HighConfidence(c)
            | Self::LowConfidence(c)
            | Self::FlaggedForReview(c)
            | Self::Escalated(c) => Some(*c),
            _ => None,
        }
    }

    /// Check if a resource limit was reached
    pub fn is_limit_reached(&self) -> bool {
        matches!(
            self,
            Self::MaxTurnsReached
                | Self::TokenBudgetExceeded
                | Self::CostLimitReached
                | Self::DurationLimitReached
        )
    }

    /// Check if this is a partial completion
    pub fn is_partial(&self) -> bool {
        matches!(self, Self::PartialCompletion)
    }
}

/// Result of running the rig-based agent loop
#[derive(Debug)]
pub struct RigAgentLoopResult {
    /// Final status
    pub status: RigAgentStatus,
    /// Number of turns executed
    pub turns: usize,
    /// Final output from agent
    pub final_output: Value,
    /// Total tokens used (if available)
    pub total_tokens: u64,
    /// Confidence level from nika:complete
    pub confidence: Option<f64>,
    /// Number of retries due to low confidence
    pub retry_count: u32,
    /// Whether all guardrails passed
    pub guardrails_passed: bool,
    /// Total cost in USD
    pub cost_usd: f64,
    /// Partial result if limit reached
    pub partial_result: Option<crate::runtime::partial::PartialResult>,
}

/// Result of guardrail check
///
/// Determines the next action based on guardrail results and their
/// `on_failure` configuration.
#[derive(Debug, Clone, PartialEq)]
pub enum GuardrailCheckResult {
    /// All guardrails passed
    AllPassed,

    /// Some guardrails failed with `on_failure: retry` (default behavior)
    ///
    /// Contains the error messages from each failed guardrail for retry feedback.
    FailedRetry(Vec<String>),

    /// Some guardrails failed with `on_failure: escalate` - needs human intervention
    FailedEscalate,

    /// Some guardrails failed with `on_failure: fail` - immediate task failure
    FailedImmediate,
}

impl GuardrailCheckResult {
    /// Check if all guardrails passed
    pub fn is_passed(&self) -> bool {
        matches!(self, Self::AllPassed)
    }

    /// Check if we should retry (default failure behavior)
    pub fn should_retry(&self) -> bool {
        matches!(self, Self::FailedRetry(_))
    }

    /// Get failure messages for retry feedback.
    ///
    /// Returns the error messages from failed guardrails, or an empty slice
    /// if the result is not `FailedRetry`.
    pub fn failure_messages(&self) -> &[String] {
        match self {
            Self::FailedRetry(msgs) => msgs,
            _ => &[],
        }
    }

    /// Check if we should escalate to human/supervisor
    pub fn should_escalate(&self) -> bool {
        matches!(self, Self::FailedEscalate)
    }

    /// Check if we should fail immediately
    pub fn should_fail(&self) -> bool {
        matches!(self, Self::FailedImmediate)
    }
}

/// Result from streaming execution with token tracking.
///
/// Used internally by streaming helpers to capture both the response
/// content and token usage from `StreamedAssistantContent::Final`.
#[derive(Debug, Clone)]
pub(super) struct StreamingResult {
    /// The accumulated response text
    pub(super) response: String,
    /// Input tokens used (from token_usage())
    pub(super) input_tokens: u64,
    /// Output tokens used (from token_usage())
    pub(super) output_tokens: u64,
    /// Cached input tokens (from prompt caching, e.g. Anthropic cache_read)
    pub(super) cached_input_tokens: u64,
    /// Optional thinking/reasoning content (Claude extended thinking)
    pub(super) thinking: Option<String>,
    /// Time to first token in milliseconds (None if not captured)
    pub(super) ttft_ms: Option<u64>,
}

// ═══════════════════════════════════════════════════════════════════════════
// ToolChoice Conversion
// ═══════════════════════════════════════════════════════════════════════════

/// Convert Nika's ToolChoice to rig-core's ToolChoice.
///
/// Nika's ToolChoice maps directly to rig-core's ToolChoice variants:
/// - `Auto` → LLM decides when to use tools (default)
/// - `Required` → Must use at least one tool per turn
/// - `None` → Never use tools (text-only response)
impl From<NikaToolChoice> for RigToolChoice {
    fn from(choice: NikaToolChoice) -> Self {
        match choice {
            NikaToolChoice::Auto => RigToolChoice::Auto,
            NikaToolChoice::Required => RigToolChoice::Required,
            NikaToolChoice::None => RigToolChoice::None,
        }
    }
}
