// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent Completion Configuration
//!
//! Defines how agents signal task completion and what validation
//! is required before accepting the result.
//!
//! ## Completion Modes
//!
//! - **explicit**: Agent must call `nika:complete` tool (recommended)
//! - **natural**: Completes when agent has no more tool calls
//! - **pattern**: Completes when output matches a pattern
//!
//! ## Example
//!
//! ```yaml
//! agent:
//!   prompt: "Research {{topic}}"
//!   completion:
//!     mode: explicit
//!     signal:
//!       tool: nika:complete
//!       fields:
//!         required: [result]
//!         optional: [confidence, reason]
//!     instruction:
//!       tone: concise
//!       lang: en
//! ```

use regex::Regex;
use serde::Deserialize;

use crate::error::CoreError;

// ═══════════════════════════════════════════════════════════════════════════
// Constants
// ═══════════════════════════════════════════════════════════════════════════

/// Default tool for explicit completion
const DEFAULT_SIGNAL_TOOL: &str = "nika:complete";

/// Default confidence threshold
const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// Default max retries for low confidence
const DEFAULT_MAX_RETRIES: u32 = 2;

// ═══════════════════════════════════════════════════════════════════════════
// CompletionConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for agent completion behavior.
///
/// Controls how an agent signals task completion and what validation
/// is performed before accepting the result.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CompletionConfig {
    /// Completion mode
    #[serde(default)]
    pub mode: CompletionMode,

    /// Signal configuration (for mode: explicit)
    #[serde(default)]
    pub signal: Option<SignalConfig>,

    /// Pattern matching (for mode: pattern)
    #[serde(default)]
    pub patterns: Vec<PatternConfig>,

    /// Confidence configuration
    #[serde(default)]
    pub confidence: Option<ConfidenceConfig>,

    /// Instruction generation settings
    #[serde(default)]
    pub instruction: Option<InstructionConfig>,
}

impl CompletionConfig {
    /// Generate system instruction for the agent based on completion config.
    ///
    /// This instruction is automatically injected into the agent's system prompt
    /// to inform it how to signal completion.
    pub fn generate_system_instruction(&self) -> String {
        match self.mode {
            CompletionMode::Explicit => self.generate_explicit_instruction(),
            CompletionMode::Natural => String::new(), // No instruction needed
            CompletionMode::Pattern => self.generate_pattern_instruction(),
        }
    }

    fn generate_explicit_instruction(&self) -> String {
        let signal = self
            .signal
            .as_ref()
            .map(|s| &s.tool)
            .map(String::as_str)
            .unwrap_or(DEFAULT_SIGNAL_TOOL);

        let fields = self.signal.as_ref().map(|s| &s.fields);

        let tone = self
            .instruction
            .as_ref()
            .map(|i| &i.tone)
            .unwrap_or(&InstructionTone::Concise);

        let lang = self
            .instruction
            .as_ref()
            .and_then(|i| i.lang.as_ref())
            .map(String::as_str)
            .unwrap_or("en");

        match (tone, lang) {
            (InstructionTone::Concise, "fr") => {
                let mut instruction =
                    format!("Quand tu as terminé, appelle l'outil {} avec:\n", signal);
                if let Some(f) = fields {
                    for field in &f.required {
                        instruction.push_str(&format!("• {} (requis)\n", field));
                    }
                    for field in &f.optional {
                        instruction.push_str(&format!("• {} (optionnel)\n", field));
                    }
                } else {
                    instruction.push_str("• result (requis)\n");
                }
                if let Some(conf) = &self.confidence {
                    instruction.push_str(&format!(
                        "\nConfidence minimum acceptée: {}\n",
                        conf.threshold
                    ));
                }
                instruction
            }
            (InstructionTone::Concise, _) => {
                let mut instruction = format!("When complete, call {} with:\n", signal);
                if let Some(f) = fields {
                    for field in &f.required {
                        instruction.push_str(&format!("• {} (required)\n", field));
                    }
                    for field in &f.optional {
                        instruction.push_str(&format!("• {} (optional)\n", field));
                    }
                } else {
                    instruction.push_str("• result (required)\n");
                }
                if let Some(conf) = &self.confidence {
                    instruction.push_str(&format!(
                        "\nMinimum accepted confidence: {}\n",
                        conf.threshold
                    ));
                }
                instruction
            }
            (InstructionTone::Detailed, "fr") => {
                let mut instruction = format!(
                    "INSTRUCTIONS DE COMPLÉTION:\n\
                     Quand vous avez terminé votre tâche, vous DEVEZ appeler l'outil {} \
                     pour signaler la complétion.\n\n\
                     Paramètres:\n",
                    signal
                );
                if let Some(f) = fields {
                    for field in &f.required {
                        instruction
                            .push_str(&format!("• {} (REQUIS): Valeur obligatoire\n", field));
                    }
                    for field in &f.optional {
                        instruction
                            .push_str(&format!("• {} (optionnel): Valeur recommandée\n", field));
                    }
                }
                instruction
            }
            (InstructionTone::Detailed, _) => {
                let mut instruction = format!(
                    "COMPLETION INSTRUCTIONS:\n\
                     When you have completed your task, you MUST call the {} tool \
                     to signal completion.\n\n\
                     Parameters:\n",
                    signal
                );
                if let Some(f) = fields {
                    for field in &f.required {
                        instruction.push_str(&format!("• {} (REQUIRED): Mandatory value\n", field));
                    }
                    for field in &f.optional {
                        instruction
                            .push_str(&format!("• {} (optional): Recommended value\n", field));
                    }
                }
                instruction
            }
        }
    }

    fn generate_pattern_instruction(&self) -> String {
        if self.patterns.is_empty() {
            return String::new();
        }

        let lang = self
            .instruction
            .as_ref()
            .and_then(|i| i.lang.as_ref())
            .map(String::as_str)
            .unwrap_or("en");

        let patterns: Vec<&str> = self
            .patterns
            .iter()
            .filter(|p| p.pattern_type != PatternType::Regex)
            .map(|p| p.value.as_str())
            .collect();

        if patterns.is_empty() {
            return String::new();
        }

        match lang {
            "fr" => format!(
                "Quand tu as terminé, termine ta réponse avec: {}\n",
                patterns.join(" ou ")
            ),
            _ => format!(
                "When complete, end your response with: {}\n",
                patterns.join(" or ")
            ),
        }
    }

    /// Check if the given output matches any completion pattern.
    ///
    /// Returns `true` if mode is Pattern and output matches a pattern,
    /// or if mode is Explicit/Natural (always returns false - handled elsewhere).
    pub fn check_pattern_match(&self, output: &str) -> bool {
        if self.mode != CompletionMode::Pattern {
            return false;
        }

        for pattern in &self.patterns {
            if pattern.matches(output) {
                return true;
            }
        }
        false
    }

    /// Get the effective completion mode.
    pub fn effective_mode(&self) -> CompletionMode {
        self.mode.clone()
    }

    /// Validate the completion configuration.
    pub fn validate(&self) -> Result<(), CoreError> {
        // Pattern mode requires at least one pattern
        if self.mode == CompletionMode::Pattern && self.patterns.is_empty() {
            return Err(CoreError::ValidationError {
                reason: "completion.mode: pattern requires at least one pattern definition".into(),
            });
        }

        // Validate confidence threshold
        if let Some(conf) = &self.confidence {
            if conf.threshold < 0.0 || conf.threshold > 1.0 {
                return Err(CoreError::ValidationError {
                    reason: format!(
                        "confidence.threshold must be between 0.0 and 1.0, got {}",
                        conf.threshold
                    ),
                });
            }
        }

        // Validate regex patterns
        for pattern in &self.patterns {
            if pattern.pattern_type == PatternType::Regex && Regex::new(&pattern.value).is_err() {
                return Err(CoreError::ValidationError {
                    reason: format!("Invalid regex pattern: {}", pattern.value),
                });
            }
        }

        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CompletionMode
// ═══════════════════════════════════════════════════════════════════════════

/// How the agent signals task completion.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CompletionMode {
    /// Agent must call `nika:complete` tool (recommended for complex tasks)
    #[default]
    Explicit,

    /// Completes when agent has no more tool calls (simple tasks)
    Natural,

    /// Completes when output matches a pattern
    Pattern,
}

// ═══════════════════════════════════════════════════════════════════════════
// SignalConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for the completion signal tool.
#[derive(Debug, Clone, Deserialize)]
pub struct SignalConfig {
    /// Tool name to call for completion (default: nika:complete)
    #[serde(default = "default_signal_tool")]
    pub tool: String,

    /// Field requirements for the completion call
    #[serde(default)]
    pub fields: SignalFields,
}

impl Default for SignalConfig {
    fn default() -> Self {
        Self {
            tool: DEFAULT_SIGNAL_TOOL.to_string(),
            fields: SignalFields::default(),
        }
    }
}

fn default_signal_tool() -> String {
    DEFAULT_SIGNAL_TOOL.to_string()
}

/// Field requirements for completion signal.
#[derive(Debug, Clone, Deserialize)]
pub struct SignalFields {
    /// Required fields (must be present in completion call)
    #[serde(default = "default_required_fields")]
    pub required: Vec<String>,

    /// Optional fields (can be included)
    #[serde(default)]
    pub optional: Vec<String>,
}

impl Default for SignalFields {
    fn default() -> Self {
        Self {
            required: default_required_fields(),
            optional: Vec::new(),
        }
    }
}

fn default_required_fields() -> Vec<String> {
    vec!["result".to_string()]
}

// ═══════════════════════════════════════════════════════════════════════════
// PatternConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Pattern matching configuration for pattern-based completion.
#[derive(Debug, Clone, Deserialize)]
pub struct PatternConfig {
    /// Pattern value to match
    pub value: String,

    /// How to match the pattern
    #[serde(default, rename = "type")]
    pub pattern_type: PatternType,

    /// Cached compiled regex (avoids recompiling every agent turn)
    #[serde(skip)]
    compiled_regex: std::sync::OnceLock<Option<Regex>>,
}

impl PatternConfig {
    /// Create a new PatternConfig.
    pub fn new(value: impl Into<String>, pattern_type: PatternType) -> Self {
        Self {
            value: value.into(),
            pattern_type,
            compiled_regex: std::sync::OnceLock::new(),
        }
    }

    /// Check if the given output matches this pattern.
    pub fn matches(&self, output: &str) -> bool {
        match self.pattern_type {
            PatternType::Exact => output == self.value,
            PatternType::Contains => output.contains(&self.value),
            PatternType::Regex => {
                let regex = self
                    .compiled_regex
                    .get_or_init(|| Regex::new(&self.value).ok());
                regex
                    .as_ref()
                    .map(|re| re.is_match(output))
                    .unwrap_or(false)
            }
        }
    }
}

/// How to match a completion pattern.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PatternType {
    /// Exact string match
    Exact,

    /// Substring match (default)
    #[default]
    Contains,

    /// Regular expression match
    Regex,
}

// ═══════════════════════════════════════════════════════════════════════════
// ConfidenceConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Confidence-based completion validation.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfidenceConfig {
    /// Minimum confidence to accept (0.0-1.0)
    #[serde(default = "default_confidence_threshold")]
    pub threshold: f64,

    /// Action when confidence is below threshold
    #[serde(default)]
    pub on_low: OnLowConfidenceConfig,

    /// Advanced confidence-based routing
    #[serde(default)]
    pub routing: Option<ConfidenceRouting>,
}

impl Default for ConfidenceConfig {
    fn default() -> Self {
        Self {
            threshold: DEFAULT_CONFIDENCE_THRESHOLD,
            on_low: OnLowConfidenceConfig::default(),
            routing: None,
        }
    }
}

fn default_confidence_threshold() -> f64 {
    DEFAULT_CONFIDENCE_THRESHOLD
}

/// Action to take when confidence is below threshold.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct OnLowConfidenceConfig {
    /// Action to take
    #[serde(default)]
    pub action: LowConfidenceAction,

    /// Max retries before escalating
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,

    /// Feedback message for retry
    #[serde(default)]
    pub feedback: Option<String>,
}

fn default_max_retries() -> u32 {
    DEFAULT_MAX_RETRIES
}

/// Action when confidence is too low.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LowConfidenceAction {
    /// Retry with feedback (default)
    #[default]
    Retry,

    /// Escalate to human or senior agent
    Escalate,

    /// Accept anyway (not recommended)
    Accept,
}

/// Advanced confidence-based routing.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfidenceRouting {
    /// High confidence route (>= 0.85 typically)
    pub high: ConfidenceRoute,

    /// Medium confidence route (>= threshold)
    pub medium: ConfidenceRoute,

    /// Low confidence route (< threshold)
    pub low: ConfidenceRoute,
}

/// A single confidence route configuration.
#[derive(Debug, Clone, Deserialize)]
pub struct ConfidenceRoute {
    /// Minimum confidence for this route (optional)
    #[serde(default)]
    pub min: Option<f64>,

    /// Action to take
    pub action: RouteAction,

    /// Target for escalation (if action is escalate)
    #[serde(default)]
    pub escalate_to: Option<String>,
}

/// Route action type.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteAction {
    /// Accept the result
    Accept,

    /// Accept but flag for review
    AcceptWithFlag,

    /// Retry with feedback
    Retry,

    /// Escalate to human or other agent
    Escalate,
}

// ═══════════════════════════════════════════════════════════════════════════
// InstructionConfig
// ═══════════════════════════════════════════════════════════════════════════

/// Configuration for generated completion instructions.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct InstructionConfig {
    /// Tone of the instruction
    #[serde(default)]
    pub tone: InstructionTone,

    /// Language for the instruction (en, fr, auto)
    #[serde(default)]
    pub lang: Option<String>,
}

/// Tone of generated completion instructions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InstructionTone {
    /// Brief, minimal instruction
    #[default]
    Concise,

    /// Detailed, explicit instruction
    Detailed,
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    // ========================================================================
    // CompletionMode parsing tests
    // ========================================================================

    #[test]
    fn parse_completion_mode_explicit() {
        let yaml = r#"
mode: explicit
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, CompletionMode::Explicit);
    }

    #[test]
    fn parse_completion_mode_natural() {
        let yaml = r#"
mode: natural
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, CompletionMode::Natural);
    }

    #[test]
    fn parse_completion_mode_pattern() {
        let yaml = r#"
mode: pattern
patterns:
  - value: "COMPLETE"
    type: exact
  - value: "DONE"
    type: contains
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, CompletionMode::Pattern);
        assert_eq!(config.patterns.len(), 2);
        assert_eq!(config.patterns[0].value, "COMPLETE");
        assert_eq!(config.patterns[0].pattern_type, PatternType::Exact);
        assert_eq!(config.patterns[1].pattern_type, PatternType::Contains);
    }

    #[test]
    fn parse_completion_mode_default_is_explicit() {
        let yaml = "";
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.mode, CompletionMode::Explicit);
    }

    // ========================================================================
    // SignalConfig parsing tests
    // ========================================================================

    #[test]
    fn parse_signal_config_full() {
        let yaml = r#"
mode: explicit
signal:
  tool: nika:complete
  fields:
    required:
      - result
    optional:
      - confidence
      - reason
      - sources
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        let signal = config.signal.unwrap();
        assert_eq!(signal.tool, "nika:complete");
        assert_eq!(signal.fields.required, vec!["result"]);
        assert_eq!(
            signal.fields.optional,
            vec!["confidence", "reason", "sources"]
        );
    }

    #[test]
    fn parse_signal_config_defaults() {
        let yaml = r#"
mode: explicit
signal: {}
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        let signal = config.signal.unwrap();
        assert_eq!(signal.tool, "nika:complete");
        assert_eq!(signal.fields.required, vec!["result"]);
    }

    // ========================================================================
    // PatternConfig tests
    // ========================================================================

    #[test]
    fn pattern_matches_exact() {
        let pattern = PatternConfig::new("DONE", PatternType::Exact);
        assert!(pattern.matches("DONE"));
        assert!(!pattern.matches("DONE!"));
        assert!(!pattern.matches("Task is DONE"));
    }

    #[test]
    fn pattern_matches_contains() {
        let pattern = PatternConfig::new("DONE", PatternType::Contains);
        assert!(pattern.matches("DONE"));
        assert!(pattern.matches("Task is DONE!"));
        assert!(!pattern.matches("Task is complete"));
    }

    #[test]
    fn pattern_matches_regex() {
        let pattern = PatternConfig::new(r"\[DONE:\w+\]", PatternType::Regex);
        assert!(pattern.matches("[DONE:SUCCESS]"));
        assert!(pattern.matches("Result: [DONE:COMPLETE]"));
        assert!(!pattern.matches("[DONE:]"));
        assert!(!pattern.matches("DONE"));
    }

    // ========================================================================
    // ConfidenceConfig parsing tests
    // ========================================================================

    #[test]
    fn parse_confidence_config() {
        let yaml = r#"
mode: explicit
confidence:
  threshold: 0.8
  on_low:
    action: retry
    max_retries: 3
    feedback: "Please verify your sources"
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        let conf = config.confidence.unwrap();
        assert_eq!(conf.threshold, 0.8);
        assert_eq!(conf.on_low.action, LowConfidenceAction::Retry);
        assert_eq!(conf.on_low.max_retries, 3);
        assert_eq!(
            conf.on_low.feedback,
            Some("Please verify your sources".to_string())
        );
    }

    #[test]
    fn parse_confidence_routing() {
        let yaml = r#"
confidence:
  threshold: 0.7
  routing:
    high:
      min: 0.85
      action: accept
    medium:
      min: 0.7
      action: accept_with_flag
    low:
      action: escalate
      escalate_to: human
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        let routing = config.confidence.unwrap().routing.unwrap();
        assert_eq!(routing.high.min, Some(0.85));
        assert_eq!(routing.high.action, RouteAction::Accept);
        assert_eq!(routing.medium.action, RouteAction::AcceptWithFlag);
        assert_eq!(routing.low.action, RouteAction::Escalate);
        assert_eq!(routing.low.escalate_to, Some("human".to_string()));
    }

    // ========================================================================
    // InstructionConfig tests
    // ========================================================================

    #[test]
    fn parse_instruction_config() {
        let yaml = r#"
mode: explicit
instruction:
  tone: detailed
  lang: fr
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();
        let instr = config.instruction.unwrap();
        assert_eq!(instr.tone, InstructionTone::Detailed);
        assert_eq!(instr.lang, Some("fr".to_string()));
    }

    // ========================================================================
    // System instruction generation tests
    // ========================================================================

    #[test]
    fn generate_instruction_explicit_concise_en() {
        let config = CompletionConfig {
            mode: CompletionMode::Explicit,
            signal: Some(SignalConfig {
                tool: "nika:complete".to_string(),
                fields: SignalFields {
                    required: vec!["result".to_string()],
                    optional: vec!["confidence".to_string()],
                },
            }),
            instruction: Some(InstructionConfig {
                tone: InstructionTone::Concise,
                lang: Some("en".to_string()),
            }),
            ..Default::default()
        };

        let instruction = config.generate_system_instruction();
        assert!(instruction.contains("nika:complete"));
        assert!(instruction.contains("result"));
        assert!(instruction.contains("required"));
        assert!(instruction.contains("confidence"));
        assert!(instruction.contains("optional"));
    }

    #[test]
    fn generate_instruction_explicit_concise_fr() {
        let config = CompletionConfig {
            mode: CompletionMode::Explicit,
            signal: Some(SignalConfig::default()),
            instruction: Some(InstructionConfig {
                tone: InstructionTone::Concise,
                lang: Some("fr".to_string()),
            }),
            ..Default::default()
        };

        let instruction = config.generate_system_instruction();
        assert!(instruction.contains("Quand tu as terminé"));
        assert!(instruction.contains("nika:complete"));
        assert!(instruction.contains("requis"));
    }

    #[test]
    fn generate_instruction_natural_is_empty() {
        let config = CompletionConfig {
            mode: CompletionMode::Natural,
            ..Default::default()
        };

        let instruction = config.generate_system_instruction();
        assert!(instruction.is_empty());
    }

    #[test]
    fn generate_instruction_pattern() {
        let config = CompletionConfig {
            mode: CompletionMode::Pattern,
            patterns: vec![
                PatternConfig::new("COMPLETE", PatternType::Contains),
                PatternConfig::new("DONE", PatternType::Contains),
            ],
            ..Default::default()
        };

        let instruction = config.generate_system_instruction();
        assert!(instruction.contains("COMPLETE"));
        assert!(instruction.contains("DONE"));
    }

    // ========================================================================
    // Validation tests
    // ========================================================================

    #[test]
    fn validate_confidence_threshold_valid() {
        let config = CompletionConfig {
            confidence: Some(ConfidenceConfig {
                threshold: 0.7,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(config.validate().is_ok());
    }

    #[test]
    fn validate_confidence_threshold_too_high() {
        let config = CompletionConfig {
            confidence: Some(ConfidenceConfig {
                threshold: 1.5,
                ..Default::default()
            }),
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("confidence.threshold"));
    }

    #[test]
    fn validate_confidence_threshold_negative() {
        let config = CompletionConfig {
            confidence: Some(ConfidenceConfig {
                threshold: -0.1,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_invalid_regex() {
        let config = CompletionConfig {
            mode: CompletionMode::Pattern,
            patterns: vec![PatternConfig::new("[invalid(", PatternType::Regex)],
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(err.to_string().contains("Invalid regex"));
    }

    // ========================================================================
    // Pattern match check tests
    // ========================================================================

    #[test]
    fn check_pattern_match_explicit_mode_always_false() {
        let config = CompletionConfig {
            mode: CompletionMode::Explicit,
            patterns: vec![PatternConfig::new("DONE", PatternType::Contains)],
            ..Default::default()
        };
        // Even with patterns, explicit mode doesn't use them
        assert!(!config.check_pattern_match("DONE"));
    }

    #[test]
    fn check_pattern_match_pattern_mode() {
        let config = CompletionConfig {
            mode: CompletionMode::Pattern,
            patterns: vec![
                PatternConfig::new("DONE", PatternType::Contains),
                PatternConfig::new(r"\[COMPLETE\]", PatternType::Regex),
            ],
            ..Default::default()
        };
        assert!(config.check_pattern_match("Task is DONE"));
        assert!(config.check_pattern_match("[COMPLETE]"));
        assert!(!config.check_pattern_match("Still working"));
    }

    // ========================================================================
    // Full config parsing test
    // ========================================================================

    #[test]
    fn parse_full_completion_config() {
        let yaml = r#"
mode: explicit
signal:
  tool: nika:complete
  fields:
    required: [result]
    optional: [confidence, reason, sources]
confidence:
  threshold: 0.7
  on_low:
    action: retry
    max_retries: 2
    feedback: "Confidence too low"
instruction:
  tone: concise
  lang: en
"#;
        let config: CompletionConfig = serde_yaml::from_str(yaml).unwrap();

        assert_eq!(config.mode, CompletionMode::Explicit);

        let signal = config.signal.clone().unwrap();
        assert_eq!(signal.tool, "nika:complete");
        assert_eq!(signal.fields.required, vec!["result"]);
        assert_eq!(signal.fields.optional.len(), 3);

        let conf = config.confidence.clone().unwrap();
        assert_eq!(conf.threshold, 0.7);
        assert_eq!(conf.on_low.action, LowConfidenceAction::Retry);

        let instr = config.instruction.clone().unwrap();
        assert_eq!(instr.tone, InstructionTone::Concise);

        assert!(config.validate().is_ok());
    }
}
