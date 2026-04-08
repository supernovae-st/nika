// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Typed enums for event fields that were previously stringly-typed.
//!
//! These replace `String` fields in EventKind variants with compile-time
//! checked enum types. Serde serializes to/from snake_case strings for
//! trace file backward compatibility.

use serde::{Deserialize, Serialize};

/// Guardrail type identifier.
///
/// Used in GuardrailPassed, GuardrailFailed, GuardrailEscalation events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GuardrailType {
    Length,
    Schema,
    Regex,
    Llm,
}

impl GuardrailType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Length => "length",
            Self::Schema => "schema",
            Self::Regex => "regex",
            Self::Llm => "llm",
        }
    }
}

impl std::fmt::Display for GuardrailType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl GuardrailType {
    /// Parse from string (for bridging from nika-core guardrails module).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "length" => Some(Self::Length),
            "schema" => Some(Self::Schema),
            "regex" => Some(Self::Regex),
            "llm" => Some(Self::Llm),
            _ => None,
        }
    }
}

/// Severity level for guardrail escalations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Agent turn event kind.
///
/// Used in AgentTurn events to distinguish turn lifecycle phases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnKind {
    Started,
    Continue,
    NaturalCompletion,
    ExplicitCompletion,
}

impl AgentTurnKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "started",
            Self::Continue => "continue",
            Self::NaturalCompletion => "natural_completion",
            Self::ExplicitCompletion => "explicit_completion",
        }
    }
}

impl std::fmt::Display for AgentTurnKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for AgentTurnKind {
    fn from(s: String) -> Self {
        match s.as_str() {
            "started" => Self::Started,
            "continue" => Self::Continue,
            "natural_completion" => Self::NaturalCompletion,
            "explicit_completion" => Self::ExplicitCompletion,
            // Fallback: map unknown stop reasons to Continue
            _ => Self::Continue,
        }
    }
}

impl From<&str> for AgentTurnKind {
    fn from(s: &str) -> Self {
        Self::from(s.to_string())
    }
}

/// LLM finish reason from provider response.
///
/// Covers known reasons plus `Other` for provider-specific or dynamic values.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishReason {
    Stop,
    EndTurn,
    ToolUse,
    MaxTokens,
    StopSequence,
    Mock,
    StructuredOutputRetry,
    StructuredOutputRepair,
    /// Unknown or dynamic reason from provider
    #[serde(untagged)]
    Other(String),
}

impl FinishReason {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Stop => "stop",
            Self::EndTurn => "end_turn",
            Self::ToolUse => "tool_use",
            Self::MaxTokens => "max_tokens",
            Self::StopSequence => "stop_sequence",
            Self::Mock => "mock",
            Self::StructuredOutputRetry => "structured_output_retry",
            Self::StructuredOutputRepair => "structured_output_repair",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for FinishReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for FinishReason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "stop" => Self::Stop,
            "end_turn" => Self::EndTurn,
            "tool_use" => Self::ToolUse,
            "max_tokens" => Self::MaxTokens,
            "stop_sequence" => Self::StopSequence,
            "mock" => Self::Mock,
            "structured_output_retry" => Self::StructuredOutputRetry,
            "structured_output_repair" => Self::StructuredOutputRepair,
            _ => Self::Other(s),
        }
    }
}

impl From<&str> for FinishReason {
    fn from(s: &str) -> Self {
        Self::from(s.to_string())
    }
}

/// Agent stop reason (why the agent loop ended).
///
/// Covers known reasons plus `Other` for dynamic/formatted values.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStopReason {
    EndTurn,
    NaturalCompletion,
    MaxTurns,
    LowConfidenceRetry,
    GuardrailRetry,
    Natural,
    ToolUse,
    /// Unknown or dynamic reason
    #[serde(untagged)]
    Other(String),
}

impl AgentStopReason {
    pub fn as_str(&self) -> &str {
        match self {
            Self::EndTurn => "end_turn",
            Self::NaturalCompletion => "natural_completion",
            Self::MaxTurns => "max_turns",
            Self::LowConfidenceRetry => "low_confidence_retry",
            Self::GuardrailRetry => "guardrail_retry",
            Self::Natural => "natural",
            Self::ToolUse => "tool_use",
            Self::Other(s) => s.as_str(),
        }
    }
}

impl std::fmt::Display for AgentStopReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for AgentStopReason {
    fn from(s: String) -> Self {
        match s.as_str() {
            "end_turn" => Self::EndTurn,
            "natural_completion" => Self::NaturalCompletion,
            "max_turns" => Self::MaxTurns,
            "low_confidence_retry" => Self::LowConfidenceRetry,
            "guardrail_retry" => Self::GuardrailRetry,
            "natural" => Self::Natural,
            "tool_use" => Self::ToolUse,
            _ => Self::Other(s),
        }
    }
}

impl From<&str> for AgentStopReason {
    fn from(s: &str) -> Self {
        Self::from(s.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guardrail_type_serde_roundtrip() {
        for (json, expected) in [
            ("\"length\"", GuardrailType::Length),
            ("\"schema\"", GuardrailType::Schema),
            ("\"regex\"", GuardrailType::Regex),
            ("\"llm\"", GuardrailType::Llm),
        ] {
            let parsed: GuardrailType = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn agent_turn_kind_serde_roundtrip() {
        for (json, expected) in [
            ("\"started\"", AgentTurnKind::Started),
            ("\"continue\"", AgentTurnKind::Continue),
            ("\"natural_completion\"", AgentTurnKind::NaturalCompletion),
            ("\"explicit_completion\"", AgentTurnKind::ExplicitCompletion),
        ] {
            let parsed: AgentTurnKind = serde_json::from_str(json).unwrap();
            assert_eq!(parsed, expected);
            assert_eq!(serde_json::to_string(&parsed).unwrap(), json);
        }
    }

    #[test]
    fn finish_reason_known_values() {
        assert_eq!(FinishReason::from("stop"), FinishReason::Stop);
        assert_eq!(FinishReason::from("end_turn"), FinishReason::EndTurn);
        assert_eq!(FinishReason::from("mock"), FinishReason::Mock);
        assert_eq!(
            FinishReason::from("structured_output_retry"),
            FinishReason::StructuredOutputRetry
        );
    }

    #[test]
    fn finish_reason_unknown_passthrough() {
        let reason = FinishReason::from("limit_exceeded:cost");
        assert_eq!(
            reason,
            FinishReason::Other("limit_exceeded:cost".to_string())
        );
        assert_eq!(reason.to_string(), "limit_exceeded:cost");
    }

    #[test]
    fn agent_stop_reason_known_values() {
        assert_eq!(AgentStopReason::from("end_turn"), AgentStopReason::EndTurn);
        assert_eq!(
            AgentStopReason::from("max_turns"),
            AgentStopReason::MaxTurns
        );
        assert_eq!(
            AgentStopReason::from("guardrail_retry"),
            AgentStopReason::GuardrailRetry
        );
    }

    #[test]
    fn agent_stop_reason_unknown_passthrough() {
        let reason = AgentStopReason::from("Completed { success: true }");
        assert_eq!(
            reason,
            AgentStopReason::Other("Completed { success: true }".to_string())
        );
    }
}
