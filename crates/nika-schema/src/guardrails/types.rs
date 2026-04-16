// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Guardrail type definitions — the vocabulary for output validation.

use std::fmt;

use serde::{Deserialize, Serialize};

/// What to do when a guardrail check fails.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum EscalationAction {
    /// Retry the LLM call with the failure feedback.
    #[default]
    Retry,
    /// Escalate to a human or supervisor agent.
    Escalate,
    /// Fail the task immediately.
    Fail,
}

impl fmt::Display for EscalationAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Retry => write!(f, "retry"),
            Self::Escalate => write!(f, "escalate"),
            Self::Fail => write!(f, "fail"),
        }
    }
}

/// A guardrail configuration — tagged union by `type` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum GuardrailConfig {
    /// Length-based validation (word/character counts).
    Length(LengthGuardrail),
    /// JSON Schema validation.
    Schema(SchemaGuardrail),
    /// Regex pattern matching.
    Regex(RegexGuardrail),
    /// LLM-as-judge validation.
    Llm(LlmGuardrail),
}

impl GuardrailConfig {
    /// Returns the guardrail's optional ID.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Length(g) => g.id.as_deref(),
            Self::Schema(g) => g.id.as_deref(),
            Self::Regex(g) => g.id.as_deref(),
            Self::Llm(g) => g.id.as_deref(),
        }
    }

    /// Returns the escalation action for this guardrail.
    #[must_use]
    pub fn on_failure(&self) -> EscalationAction {
        match self {
            Self::Length(g) => g.on_failure,
            Self::Schema(g) => g.on_failure,
            Self::Regex(g) => g.on_failure,
            Self::Llm(g) => g.on_failure,
        }
    }
}

/// Length-based guardrail — validates word and character counts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LengthGuardrail {
    /// Optional identifier for this guardrail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Minimum word count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_words: Option<u32>,
    /// Maximum word count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_words: Option<u32>,
    /// Minimum character count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_chars: Option<u32>,
    /// Maximum character count.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_chars: Option<u32>,
    /// Custom error message on failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Action on failure.
    #[serde(default)]
    pub on_failure: EscalationAction,
}

impl LengthGuardrail {
    /// Create a new empty length guardrail.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: None,
            min_words: None,
            max_words: None,
            min_chars: None,
            max_chars: None,
            message: None,
            on_failure: EscalationAction::default(),
        }
    }
}

impl Default for LengthGuardrail {
    fn default() -> Self {
        Self::new()
    }
}

/// Schema guardrail — validates output against a JSON Schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SchemaGuardrail {
    /// Optional identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// JSON Schema to validate against.
    pub json_schema: serde_json::Value,
    /// Custom error message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Action on failure.
    #[serde(default)]
    pub on_failure: EscalationAction,
}

impl SchemaGuardrail {
    /// Create a new schema guardrail with the given schema.
    #[must_use]
    pub fn new(json_schema: serde_json::Value) -> Self {
        Self {
            id: None,
            json_schema,
            message: None,
            on_failure: EscalationAction::default(),
        }
    }
}

/// Regex guardrail — validates output matches (or doesn't match) a pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RegexGuardrail {
    /// Optional identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Regex pattern to match.
    pub pattern: String,
    /// If true, the check passes when the pattern does NOT match.
    #[serde(default)]
    pub negate: bool,
    /// Custom error message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Action on failure.
    #[serde(default)]
    pub on_failure: EscalationAction,
}

impl RegexGuardrail {
    /// Create a new regex guardrail.
    #[must_use]
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            id: None,
            pattern: pattern.into(),
            negate: false,
            message: None,
            on_failure: EscalationAction::default(),
        }
    }
}

/// LLM-as-judge guardrail — uses an LLM to evaluate output quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LlmGuardrail {
    /// Optional identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// Prompt for the judge LLM.
    pub judge_prompt: String,
    /// Regex pattern to match in judge output for passing.
    pub pass_pattern: String,
    /// Model to use for judging (defaults to task model).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Max tokens for judge response.
    #[serde(default = "default_judge_max_tokens")]
    pub max_tokens: u32,
    /// Temperature for judge (lower = more deterministic).
    #[serde(default = "default_judge_temperature")]
    pub temperature: f64,
    /// Custom error message.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Action on failure.
    #[serde(default)]
    pub on_failure: EscalationAction,
}

fn default_judge_max_tokens() -> u32 {
    256
}

fn default_judge_temperature() -> f64 {
    0.0
}

impl LlmGuardrail {
    /// Create a new LLM guardrail.
    #[must_use]
    pub fn new(judge_prompt: impl Into<String>, pass_pattern: impl Into<String>) -> Self {
        Self {
            id: None,
            judge_prompt: judge_prompt.into(),
            pass_pattern: pass_pattern.into(),
            model: None,
            max_tokens: default_judge_max_tokens(),
            temperature: default_judge_temperature(),
            message: None,
            on_failure: EscalationAction::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escalation_action_default_is_retry() {
        assert_eq!(EscalationAction::default(), EscalationAction::Retry);
    }

    #[test]
    fn escalation_action_display() {
        assert_eq!(EscalationAction::Retry.to_string(), "retry");
        assert_eq!(EscalationAction::Fail.to_string(), "fail");
        assert_eq!(EscalationAction::Escalate.to_string(), "escalate");
    }

    #[test]
    fn guardrail_config_length_serde() {
        let g = GuardrailConfig::Length(LengthGuardrail {
            min_words: Some(10),
            max_words: Some(100),
            ..LengthGuardrail::new()
        });
        let json = serde_json::to_string(&g).expect("serialize");
        assert!(json.contains(r#""type":"length""#));
        let back: GuardrailConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, GuardrailConfig::Length(l) if l.min_words == Some(10)));
    }

    #[test]
    fn guardrail_config_schema_serde() {
        let g =
            GuardrailConfig::Schema(SchemaGuardrail::new(serde_json::json!({"type": "object"})));
        let json = serde_json::to_string(&g).expect("serialize");
        assert!(json.contains(r#""type":"schema""#));
        let back: GuardrailConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, GuardrailConfig::Schema(_)));
    }

    #[test]
    fn guardrail_config_regex_serde() {
        let g = GuardrailConfig::Regex(RegexGuardrail {
            pattern: r"^[A-Z]".into(),
            negate: true,
            ..RegexGuardrail::new(r"^[A-Z]")
        });
        let json = serde_json::to_string(&g).expect("serialize");
        let back: GuardrailConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, GuardrailConfig::Regex(r) if r.negate));
    }

    #[test]
    fn guardrail_config_llm_serde() {
        let g = GuardrailConfig::Llm(LlmGuardrail::new(
            "Is this response factual?",
            r"(?i)yes|correct",
        ));
        let json = serde_json::to_string(&g).expect("serialize");
        let back: GuardrailConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(matches!(back, GuardrailConfig::Llm(l) if l.max_tokens == 256));
    }

    #[test]
    fn guardrail_config_id_accessor() {
        let g = GuardrailConfig::Length(LengthGuardrail {
            id: Some("len-check".into()),
            ..LengthGuardrail::new()
        });
        assert_eq!(g.id(), Some("len-check"));

        let g2 = GuardrailConfig::Regex(RegexGuardrail::new(".*"));
        assert_eq!(g2.id(), None);
    }

    #[test]
    fn guardrail_config_on_failure_accessor() {
        let g = GuardrailConfig::Length(LengthGuardrail {
            on_failure: EscalationAction::Fail,
            ..LengthGuardrail::new()
        });
        assert_eq!(g.on_failure(), EscalationAction::Fail);
    }
}
