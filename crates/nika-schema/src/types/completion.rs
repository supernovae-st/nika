// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent completion configuration — how agents signal task completion.
//!
//! Defines completion modes, signal configuration, pattern matching,
//! and confidence scoring. The runtime orchestration of these modes
//! lives in `nika-verb-agent` (L2); only the configuration types live here.
//!
//! # Completion modes
//!
//! - **Explicit**: Agent must call `nika:complete` tool (recommended).
//! - **Natural**: Completes when agent has no more tool calls.
//! - **Pattern**: Completes when output matches a regex pattern.

use std::fmt;

use serde::{Deserialize, Serialize};

/// How an agent signals task completion.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum CompletionMode {
    /// Agent must explicitly call a completion tool.
    #[default]
    Explicit,
    /// Completion inferred from agent having no more tool calls.
    Natural,
    /// Completion when output matches a pattern.
    Pattern,
}

impl fmt::Display for CompletionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Explicit => write!(f, "explicit"),
            Self::Natural => write!(f, "natural"),
            Self::Pattern => write!(f, "pattern"),
        }
    }
}

/// Configuration for the explicit completion signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SignalConfig {
    /// Tool name for completion (default: `nika:complete`).
    #[serde(default = "default_signal_tool")]
    pub tool: String,
    /// Field configuration for the completion signal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<SignalFields>,
}

fn default_signal_tool() -> String {
    "nika:complete".into()
}

impl SignalConfig {
    /// Create a new signal config with default tool.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tool: default_signal_tool(),
            fields: None,
        }
    }
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Field requirements for the completion signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct SignalFields {
    /// Required fields in the completion payload.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub required: Vec<String>,
    /// Optional fields in the completion payload.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub optional: Vec<String>,
}

impl SignalFields {
    /// Create new empty signal fields.
    #[must_use]
    pub fn new() -> Self {
        Self {
            required: Vec::new(),
            optional: Vec::new(),
        }
    }
}

impl Default for SignalFields {
    fn default() -> Self {
        Self::new()
    }
}

/// Pattern matching configuration for pattern-based completion.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PatternConfig {
    /// Regex pattern to match against agent output.
    pub pattern: String,
    /// Whether to negate the match (complete when NOT matching).
    #[serde(default)]
    pub negate: bool,
}

impl PatternConfig {
    /// Create a new pattern config.
    #[must_use]
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            negate: false,
        }
    }
}

/// Confidence scoring configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ConfidenceConfig {
    /// Minimum confidence threshold (0.0 - 1.0) to accept completion.
    #[serde(default = "default_confidence_threshold")]
    pub threshold: f64,
    /// Maximum retries when confidence is below threshold.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
}

fn default_confidence_threshold() -> f64 {
    0.7
}

fn default_max_retries() -> u32 {
    2
}

impl ConfidenceConfig {
    /// Create a new confidence config with defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            threshold: default_confidence_threshold(),
            max_retries: default_max_retries(),
        }
    }
}

impl Default for ConfidenceConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Instruction generation settings for agent system prompts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct InstructionConfig {
    /// Tone of the generated instruction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tone: Option<String>,
    /// Language for the instruction (e.g. "en", "fr").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

impl InstructionConfig {
    /// Create a new empty instruction config.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tone: None,
            lang: None,
        }
    }
}

impl Default for InstructionConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// Full completion configuration for an agent task.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct CompletionConfig {
    /// Completion mode.
    #[serde(default)]
    pub mode: CompletionMode,
    /// Signal configuration (for `mode: explicit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<SignalConfig>,
    /// Pattern configurations (for `mode: pattern`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<PatternConfig>,
    /// Confidence scoring configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<ConfidenceConfig>,
    /// Instruction generation settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instruction: Option<InstructionConfig>,
}

impl CompletionConfig {
    /// Create a new completion config with default explicit mode.
    #[must_use]
    pub fn new() -> Self {
        Self {
            mode: CompletionMode::default(),
            signal: None,
            patterns: Vec::new(),
            confidence: None,
            instruction: None,
        }
    }

    /// Create an explicit-mode completion config.
    #[must_use]
    pub fn explicit() -> Self {
        Self {
            mode: CompletionMode::Explicit,
            signal: Some(SignalConfig::new()),
            ..Self::new()
        }
    }

    /// Create a natural-mode completion config.
    #[must_use]
    pub fn natural() -> Self {
        Self {
            mode: CompletionMode::Natural,
            ..Self::new()
        }
    }
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_mode_default_is_explicit() {
        assert_eq!(CompletionMode::default(), CompletionMode::Explicit);
    }

    #[test]
    fn completion_mode_display() {
        assert_eq!(CompletionMode::Explicit.to_string(), "explicit");
        assert_eq!(CompletionMode::Natural.to_string(), "natural");
        assert_eq!(CompletionMode::Pattern.to_string(), "pattern");
    }

    #[test]
    fn signal_config_default_tool() {
        let s = SignalConfig::new();
        assert_eq!(s.tool, "nika:complete");
    }

    #[test]
    fn confidence_config_defaults() {
        let c = ConfidenceConfig::new();
        assert!((c.threshold - 0.7).abs() < f64::EPSILON);
        assert_eq!(c.max_retries, 2);
    }

    #[test]
    fn completion_config_explicit() {
        let cc = CompletionConfig::explicit();
        assert_eq!(cc.mode, CompletionMode::Explicit);
        assert!(cc.signal.is_some());
    }

    #[test]
    fn completion_config_natural() {
        let cc = CompletionConfig::natural();
        assert_eq!(cc.mode, CompletionMode::Natural);
    }

    #[test]
    fn serde_roundtrip() {
        let cc = CompletionConfig {
            mode: CompletionMode::Explicit,
            signal: Some(SignalConfig {
                tool: "nika:complete".into(),
                fields: Some(SignalFields {
                    required: vec!["result".into()],
                    optional: vec!["confidence".into(), "reason".into()],
                }),
            }),
            confidence: Some(ConfidenceConfig {
                threshold: 0.8,
                max_retries: 3,
            }),
            ..CompletionConfig::new()
        };
        let json = serde_json::to_string(&cc).expect("serialize");
        let back: CompletionConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.mode, CompletionMode::Explicit);
        let signal = back.signal.as_ref().unwrap();
        assert_eq!(signal.tool, "nika:complete");
        let fields = signal.fields.as_ref().unwrap();
        assert_eq!(fields.required, vec!["result"]);
        assert_eq!(back.confidence.as_ref().unwrap().max_retries, 3);
    }

    #[test]
    fn serde_minimal() {
        let cc: CompletionConfig = serde_json::from_str("{}").expect("deserialize");
        assert_eq!(cc.mode, CompletionMode::Explicit);
        assert!(cc.signal.is_none());
        assert!(cc.patterns.is_empty());
    }

    #[test]
    fn pattern_config_new() {
        let p = PatternConfig::new(r"DONE|COMPLETE");
        assert_eq!(p.pattern, r"DONE|COMPLETE");
        assert!(!p.negate);
    }

    #[test]
    fn completion_mode_serde_roundtrip() {
        for mode in [
            CompletionMode::Explicit,
            CompletionMode::Natural,
            CompletionMode::Pattern,
        ] {
            let json = serde_json::to_string(&mode).expect("serialize");
            let back: CompletionMode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, mode);
        }
    }
}
