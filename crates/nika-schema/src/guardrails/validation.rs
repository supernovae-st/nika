// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Guardrail configuration validation — catches errors at parse time.
//!
//! These are structural checks on the guardrail config (not runtime checks
//! on LLM output). Runtime validation lives in `nika-verb-infer` / `nika-verb-agent`.

use crate::error::SchemaError;

use super::types::{GuardrailConfig, LengthGuardrail, LlmGuardrail, RegexGuardrail};

/// Validate a guardrail configuration at parse time.
///
/// Checks for structural issues like:
/// - Length guardrail with no constraints
/// - `min > max` in length guardrails
/// - Empty regex pattern
/// - Empty judge prompt in LLM guardrails
///
/// # Errors
///
/// Returns `SchemaError` if the configuration is structurally invalid.
pub fn validate_guardrail(guardrail: &GuardrailConfig) -> Result<(), SchemaError> {
    match guardrail {
        GuardrailConfig::Length(g) => validate_length(g),
        GuardrailConfig::Schema(_) => Ok(()), // Schema validity checked at runtime
        GuardrailConfig::Regex(g) => validate_regex(g),
        GuardrailConfig::Llm(g) => validate_llm(g),
    }
}

fn validate_length(g: &LengthGuardrail) -> Result<(), SchemaError> {
    // Must have at least one constraint
    if g.min_words.is_none()
        && g.max_words.is_none()
        && g.min_chars.is_none()
        && g.max_chars.is_none()
    {
        return Err(SchemaError::validation(
            "length guardrail must specify at least one constraint \
             (min_words, max_words, min_chars, or max_chars)",
        ));
    }

    // min must not exceed max
    if let (Some(min), Some(max)) = (g.min_words, g.max_words)
        && min > max
    {
        return Err(SchemaError::validation(format!(
            "length guardrail: min_words ({min}) exceeds max_words ({max})"
        )));
    }
    if let (Some(min), Some(max)) = (g.min_chars, g.max_chars)
        && min > max
    {
        return Err(SchemaError::validation(format!(
            "length guardrail: min_chars ({min}) exceeds max_chars ({max})"
        )));
    }

    Ok(())
}

fn validate_regex(g: &RegexGuardrail) -> Result<(), SchemaError> {
    if g.pattern.is_empty() {
        return Err(SchemaError::validation(
            "regex guardrail: pattern must not be empty",
        ));
    }
    Ok(())
}

fn validate_llm(g: &LlmGuardrail) -> Result<(), SchemaError> {
    if g.judge_prompt.is_empty() {
        return Err(SchemaError::validation(
            "LLM guardrail: judge_prompt must not be empty",
        ));
    }
    if g.pass_pattern.is_empty() {
        return Err(SchemaError::validation(
            "LLM guardrail: pass_pattern must not be empty",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrails::types::*;

    #[test]
    fn length_guardrail_valid() {
        let g = GuardrailConfig::Length(LengthGuardrail {
            min_words: Some(10),
            ..LengthGuardrail::new()
        });
        assert!(validate_guardrail(&g).is_ok());
    }

    #[test]
    fn length_guardrail_no_constraints() {
        let g = GuardrailConfig::Length(LengthGuardrail::new());
        assert!(validate_guardrail(&g).is_err());
    }

    #[test]
    fn length_guardrail_min_exceeds_max_words() {
        let g = GuardrailConfig::Length(LengthGuardrail {
            min_words: Some(100),
            max_words: Some(10),
            ..LengthGuardrail::new()
        });
        let err = validate_guardrail(&g).unwrap_err();
        assert!(err.to_string().contains("min_words"));
    }

    #[test]
    fn length_guardrail_min_exceeds_max_chars() {
        let g = GuardrailConfig::Length(LengthGuardrail {
            min_chars: Some(1000),
            max_chars: Some(100),
            ..LengthGuardrail::new()
        });
        let err = validate_guardrail(&g).unwrap_err();
        assert!(err.to_string().contains("min_chars"));
    }

    #[test]
    fn regex_guardrail_valid() {
        let g = GuardrailConfig::Regex(RegexGuardrail::new(r"^\d+$"));
        assert!(validate_guardrail(&g).is_ok());
    }

    #[test]
    fn regex_guardrail_empty_pattern() {
        let g = GuardrailConfig::Regex(RegexGuardrail::new(""));
        assert!(validate_guardrail(&g).is_err());
    }

    #[test]
    fn llm_guardrail_valid() {
        let g = GuardrailConfig::Llm(LlmGuardrail::new("Is this factual?", r"(?i)yes"));
        assert!(validate_guardrail(&g).is_ok());
    }

    #[test]
    fn llm_guardrail_empty_prompt() {
        let g = GuardrailConfig::Llm(LlmGuardrail::new("", "yes"));
        assert!(validate_guardrail(&g).is_err());
    }

    #[test]
    fn llm_guardrail_empty_pattern() {
        let g = GuardrailConfig::Llm(LlmGuardrail::new("Is this good?", ""));
        assert!(validate_guardrail(&g).is_err());
    }

    #[test]
    fn schema_guardrail_always_valid() {
        let g = GuardrailConfig::Schema(SchemaGuardrail::new(serde_json::json!({})));
        assert!(validate_guardrail(&g).is_ok());
    }
}
