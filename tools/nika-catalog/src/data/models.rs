// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities and pricing catalog.
//!
//! Capabilities use pattern-matching (NOT phf) — model names are open-ended.
//! Pricing uses 2-pass matching: exact match first, then `contains()` fallback.

use crate::types::model::{CostEstimate, ModelCapabilities, ModelPricing, TokenLimitParam};

// ═══════════════════════════════════════════════════════════════════════════
// Model capabilities (pattern-matching function)
// ═══════════════════════════════════════════════════════════════════════════

/// Resolve capabilities for a given provider + model combination.
///
/// **Provider-aware:** the same model name gets different treatment depending
/// on the provider. `o3` on `OpenAI` gets `max_completion_tokens`, but
/// `o3-finetune` on a custom vLLM endpoint gets `max_tokens`.
///
/// This is the ONLY function that should know about model-specific quirks.
#[must_use]
pub fn model_capabilities(provider: &str, model: &str) -> ModelCapabilities {
    let lower = model.to_lowercase();
    // Normalize provider via catalog lookup (e.g. "claude" → "anthropic", "grok" → "xai")
    let canonical = crate::data::find_provider(provider).map(|p| p.id);
    let prov = canonical.unwrap_or(provider).to_lowercase();

    // Only apply reasoning-model rules for actual OpenAI API endpoints.
    let is_openai_api = matches!(prov.as_str(), "openai" | "openrouter");

    if is_openai_api {
        // o-series: max_completion_tokens + NO temperature
        if is_o_series(&lower) {
            return ModelCapabilities {
                token_limit_param: TokenLimitParam::MaxCompletionTokens,
                supports_temperature: false,
                supports_thinking: true,
                ..Default::default()
            };
        }
        // gpt-5.x: max_completion_tokens + YES temperature
        if is_gpt5(&lower) {
            return ModelCapabilities {
                token_limit_param: TokenLimitParam::MaxCompletionTokens,
                supports_temperature: true,
                supports_thinking: true,
                ..Default::default()
            };
        }
    }

    // Anthropic / Claude (alias "claude" already normalized to "anthropic")
    if prov == "anthropic" || lower.starts_with("claude") {
        return ModelCapabilities {
            supports_thinking: true,
            ..Default::default()
        };
    }

    // DeepSeek (alias "deep-seek" already normalized to "deepseek")
    if prov == "deepseek" {
        if lower == "deepseek-reasoner" {
            return ModelCapabilities {
                supports_temperature: false,
                supports_vision: false,
                ..Default::default()
            };
        }
        return ModelCapabilities {
            supports_vision: false,
            ..Default::default()
        };
    }

    // xAI (alias "grok" already normalized to "xai")
    if prov == "xai" && lower == "grok-4" {
        return ModelCapabilities {
            supports_stop_sequences: false,
            ..Default::default()
        };
    }

    // Everything else: safe defaults
    ModelCapabilities::default()
}

fn is_o_series(lower: &str) -> bool {
    lower == "o1"
        || lower.starts_with("o1-")
        || lower == "o3"
        || lower.starts_with("o3-")
        || lower == "o4"
        || lower.starts_with("o4-")
}

fn is_gpt5(lower: &str) -> bool {
    lower == "gpt-5" || lower.starts_with("gpt-5-") || lower.starts_with("gpt-5.")
}

// ═══════════════════════════════════════════════════════════════════════════
// Pricing catalog (61 entries, sorted by provider → specificity)
// ═══════════════════════════════════════════════════════════════════════════

/// Static pricing table — model patterns matched by exact name or `contains()`.
///
/// **Ordering matters for `contains()` fallback**: more specific patterns MUST
/// appear before less specific ones within each provider.
pub static ALL_PRICING: &[ModelPricing] = &[
    // ── Anthropic ───────────────────────────────────────────────
    ModelPricing { provider: "Anthropic", model_pattern: "claude-3-haiku", input_per_million: 0.25, output_per_million: 1.25 },
    ModelPricing { provider: "Anthropic", model_pattern: "opus-4", input_per_million: 15.0, output_per_million: 75.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "sonnet-4", input_per_million: 3.0, output_per_million: 15.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "haiku-4", input_per_million: 0.8, output_per_million: 4.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "claude-3-5-sonnet", input_per_million: 3.0, output_per_million: 15.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "claude-3-5-haiku", input_per_million: 0.8, output_per_million: 4.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "claude-3-opus", input_per_million: 15.0, output_per_million: 75.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "claude-3-sonnet", input_per_million: 3.0, output_per_million: 15.0 },
    // ── OpenAI (ordering critical for contains fallback) ────────
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4o-mini", input_per_million: 0.15, output_per_million: 0.6 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4o", input_per_million: 2.5, output_per_million: 10.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4.1-nano", input_per_million: 0.1, output_per_million: 0.4 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4.1-mini", input_per_million: 0.4, output_per_million: 1.6 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4.1", input_per_million: 2.0, output_per_million: 8.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-3.5-turbo", input_per_million: 0.5, output_per_million: 1.5 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4-turbo", input_per_million: 10.0, output_per_million: 30.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4", input_per_million: 30.0, output_per_million: 60.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-5.4-nano", input_per_million: 0.10, output_per_million: 0.40 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-5.4-mini", input_per_million: 0.40, output_per_million: 1.60 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-5.4", input_per_million: 2.00, output_per_million: 8.00 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-5.2-pro", input_per_million: 21.0, output_per_million: 168.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-5.2", input_per_million: 1.75, output_per_million: 14.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-5", input_per_million: 1.75, output_per_million: 14.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "o1-mini", input_per_million: 3.0, output_per_million: 12.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "o1", input_per_million: 15.0, output_per_million: 60.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "o3-mini", input_per_million: 1.1, output_per_million: 4.4 },
    ModelPricing { provider: "OpenAI", model_pattern: "o4-mini", input_per_million: 1.1, output_per_million: 4.4 },
    ModelPricing { provider: "OpenAI", model_pattern: "o3", input_per_million: 2.0, output_per_million: 8.0 },
    // o4-mini BEFORE o4 (contains "o4" substring)
    ModelPricing { provider: "OpenAI", model_pattern: "o4", input_per_million: 2.0, output_per_million: 8.0 },
    // ── Mistral ─────────────────────────────────────────────────
    ModelPricing { provider: "Mistral", model_pattern: "mistral-large", input_per_million: 2.0, output_per_million: 6.0 },
    ModelPricing { provider: "Mistral", model_pattern: "mistral-medium", input_per_million: 2.7, output_per_million: 8.1 },
    ModelPricing { provider: "Mistral", model_pattern: "mistral-small", input_per_million: 0.2, output_per_million: 0.6 },
    ModelPricing { provider: "Mistral", model_pattern: "codestral", input_per_million: 0.3, output_per_million: 0.9 },
    ModelPricing { provider: "Mistral", model_pattern: "ministral-8b", input_per_million: 0.1, output_per_million: 0.1 },
    ModelPricing { provider: "Mistral", model_pattern: "ministral-3b", input_per_million: 0.04, output_per_million: 0.04 },
    ModelPricing { provider: "Mistral", model_pattern: "pixtral-large", input_per_million: 2.0, output_per_million: 6.0 },
    ModelPricing { provider: "Mistral", model_pattern: "pixtral-12b", input_per_million: 0.15, output_per_million: 0.15 },
    // ── Groq ────────────────────────────────────────────────────
    ModelPricing { provider: "Groq", model_pattern: "llama-3.3-70b-specdec", input_per_million: 0.59, output_per_million: 0.99 },
    ModelPricing { provider: "Groq", model_pattern: "llama-3.3-70b", input_per_million: 0.59, output_per_million: 0.79 },
    ModelPricing { provider: "Groq", model_pattern: "llama-3.1-70b", input_per_million: 0.59, output_per_million: 0.79 },
    ModelPricing { provider: "Groq", model_pattern: "llama-3.1-8b", input_per_million: 0.05, output_per_million: 0.08 },
    ModelPricing { provider: "Groq", model_pattern: "llama3-70b", input_per_million: 0.59, output_per_million: 0.79 },
    ModelPricing { provider: "Groq", model_pattern: "llama3-8b", input_per_million: 0.05, output_per_million: 0.08 },
    ModelPricing { provider: "Groq", model_pattern: "mixtral-8x7b", input_per_million: 0.24, output_per_million: 0.24 },
    ModelPricing { provider: "Groq", model_pattern: "gemma2-9b", input_per_million: 0.20, output_per_million: 0.20 },
    // ── DeepSeek ────────────────────────────────────────────────
    ModelPricing { provider: "DeepSeek", model_pattern: "deepseek-chat", input_per_million: 0.14, output_per_million: 0.28 },
    ModelPricing { provider: "DeepSeek", model_pattern: "deepseek-reasoner", input_per_million: 0.55, output_per_million: 2.19 },
    ModelPricing { provider: "DeepSeek", model_pattern: "deepseek-coder", input_per_million: 0.14, output_per_million: 0.28 },
    // ── Gemini ──────────────────────────────────────────────────
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.5-flash", input_per_million: 0.15, output_per_million: 0.6 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.5-pro", input_per_million: 1.25, output_per_million: 10.0 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.0-flash-exp", input_per_million: 0.0, output_per_million: 0.0 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.0-flash-thinking", input_per_million: 0.0, output_per_million: 0.0 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.0-flash", input_per_million: 0.1, output_per_million: 0.4 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-1.5-flash-8b", input_per_million: 0.0375, output_per_million: 0.15 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-1.5-flash", input_per_million: 0.075, output_per_million: 0.3 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-1.5-pro", input_per_million: 1.25, output_per_million: 5.0 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-pro", input_per_million: 0.5, output_per_million: 1.5 },
    // ── xAI ─────────────────────────────────────────────────────
    ModelPricing { provider: "xAI", model_pattern: "grok-4", input_per_million: 3.0, output_per_million: 15.0 },
    ModelPricing { provider: "xAI", model_pattern: "grok-3-mini-fast", input_per_million: 0.1, output_per_million: 0.4 },
    ModelPricing { provider: "xAI", model_pattern: "grok-3-mini", input_per_million: 0.3, output_per_million: 0.5 },
    ModelPricing { provider: "xAI", model_pattern: "grok-3-fast", input_per_million: 0.6, output_per_million: 4.0 },
    ModelPricing { provider: "xAI", model_pattern: "grok-3", input_per_million: 3.0, output_per_million: 15.0 },
    ModelPricing { provider: "xAI", model_pattern: "grok-2", input_per_million: 2.0, output_per_million: 10.0 },
];

/// Find pricing for a model.
///
/// Two-pass matching:
/// 1. Exact match on `model_pattern`
/// 2. `contains()` fallback (first match wins — ordering matters)
///
/// ⚠ The unscoped variant is vulnerable to cross-provider contains-collisions
/// (e.g. `gpt-4o` could pick up an Azure pattern when the caller meant `OpenAI`).
/// Prefer [`find_pricing_scoped`] when the provider is known.
#[must_use]
pub fn find_pricing(model: &str) -> Option<&'static ModelPricing> {
    // Pass 1: exact match
    if let Some(p) = ALL_PRICING.iter().find(|p| model == p.model_pattern) {
        return Some(p);
    }
    // Pass 2: contains() fallback (first match wins)
    ALL_PRICING.iter().find(|p| model.contains(p.model_pattern))
}

/// Find pricing for `model` scoped to a specific `provider`.
///
/// Fixes the silent-wrong-cost bug where `find_pricing("gpt-4o")` could
/// resolve to an Azure pattern because a broader contains-match landed
/// first. When the caller knows which provider issued the request, this
/// variant restricts the search to pricing rows whose `provider` field
/// matches (case-insensitively) so contains-fallback never crosses
/// provider boundaries.
///
/// Provider matching is case-insensitive and accepts the provider id
/// (e.g. `"openai"`) or the capitalised display form used in
/// [`ALL_PRICING`] (e.g. `"OpenAI"`).
#[must_use]
pub fn find_pricing_scoped(provider: &str, model: &str) -> Option<&'static ModelPricing> {
    let prov_lower = provider.to_ascii_lowercase();
    // Pass 1: exact model match within provider scope.
    if let Some(p) = ALL_PRICING
        .iter()
        .find(|p| p.provider.eq_ignore_ascii_case(&prov_lower) && model == p.model_pattern)
    {
        return Some(p);
    }
    // Pass 2: contains() fallback within provider scope.
    ALL_PRICING
        .iter()
        .find(|p| p.provider.eq_ignore_ascii_case(&prov_lower) && model.contains(p.model_pattern))
}

/// Estimate cost for a model invocation.
#[must_use]
#[allow(clippy::cast_precision_loss)]
pub fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<CostEstimate> {
    let pricing = find_pricing(model)?;
    let usd = (input_tokens as f64 * pricing.input_per_million
        + output_tokens as f64 * pricing.output_per_million)
        / 1_000_000.0;
    Some(CostEstimate {
        usd,
        input_rate_per_million: pricing.input_per_million,
        output_rate_per_million: pricing.output_per_million,
        model: model.to_string(),
        provider: pricing.provider.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Pricing count ───────────────────────────────────────────

    #[test]
    fn pricing_count() {
        assert_eq!(ALL_PRICING.len(), 62);
    }

    // ── Capabilities ────────────────────────────────────────────

    #[test]
    fn o3_needs_max_completion_tokens() {
        let caps = model_capabilities("openai", "o3");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
        assert!(caps.supports_thinking);
    }

    #[test]
    fn o4_mini_reasoning() {
        let caps = model_capabilities("openai", "o4-mini");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn gpt52_uses_max_completion_tokens_but_supports_temperature() {
        let caps = model_capabilities("openai", "gpt-5.2");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(caps.supports_temperature);
        assert!(caps.supports_thinking);
    }

    #[test]
    fn gpt41_is_not_reasoning() {
        let caps = model_capabilities("openai", "gpt-4.1");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn claude_supports_thinking() {
        let caps = model_capabilities("anthropic", "claude-sonnet-4-6");
        assert!(caps.supports_thinking);
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
    }

    #[test]
    fn deepseek_reasoner_no_vision_no_temperature() {
        let caps = model_capabilities("deepseek", "deepseek-reasoner");
        assert!(!caps.supports_temperature);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn deepseek_chat_no_vision() {
        let caps = model_capabilities("deepseek", "deepseek-chat");
        assert!(caps.supports_temperature);
        assert!(!caps.supports_vision);
    }

    #[test]
    fn grok4_rejects_stop_sequences() {
        let caps = model_capabilities("xai", "grok-4");
        assert!(!caps.supports_stop_sequences);
    }

    #[test]
    fn custom_endpoint_safe_defaults() {
        let caps = model_capabilities("h100", "o3-finetune");
        assert_eq!(
            caps.token_limit_param,
            TokenLimitParam::MaxTokens,
            "custom endpoints should use max_tokens even for o3-like names"
        );
    }

    #[test]
    fn openrouter_gets_openai_treatment() {
        let caps = model_capabilities("openrouter", "o3");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn case_insensitive_model_names() {
        let caps = model_capabilities("OpenAI", "GPT-5.2");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    // ── Kill mutants: each o-series pattern independently ───────

    #[test]
    fn o1_is_reasoning() {
        let caps = model_capabilities("openai", "o1");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn o1_pro_is_reasoning() {
        let caps = model_capabilities("openai", "o1-pro");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn o3_mini_is_reasoning() {
        let caps = model_capabilities("openai", "o3-mini");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn o4_is_reasoning() {
        let caps = model_capabilities("openai", "o4");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn gpt5_base_is_reasoning() {
        let caps = model_capabilities("openai", "gpt-5");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn gpt5_dash_variant_is_reasoning() {
        let caps = model_capabilities("openai", "gpt-5-turbo");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    #[test]
    fn gpt5_dot_variant_is_reasoning() {
        let caps = model_capabilities("openai", "gpt-5.4");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    // ── Kill mutants: provider alias detection ──────────────────

    #[test]
    fn gpt_provider_alias_gets_openai_treatment() {
        let caps = model_capabilities("gpt", "o3");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
    }

    // ── Kill mutants: xai grok-4 specific ───────────────────────

    #[test]
    fn grok_alias_grok4() {
        let caps = model_capabilities("grok", "grok-4");
        assert!(!caps.supports_stop_sequences);
    }

    #[test]
    fn xai_non_grok4_supports_stop() {
        let caps = model_capabilities("xai", "grok-3");
        assert!(caps.supports_stop_sequences);
    }

    // ── Kill mutants: anthropic temperature ─────────────────────

    #[test]
    fn o3_no_temperature() {
        let caps = model_capabilities("openai", "o3");
        assert!(!caps.supports_temperature);
    }

    #[test]
    fn standard_model_supports_temperature() {
        let caps = model_capabilities("openai", "gpt-4o");
        assert!(caps.supports_temperature);
    }

    // ── Pricing 2-pass matching ─────────────────────────────────

    #[test]
    fn exact_match_takes_priority() {
        let p = find_pricing("gpt-4o-mini").unwrap();
        assert!((p.input_per_million - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn contains_fallback_for_dated_variants() {
        let p = find_pricing("claude-sonnet-4-20250514").unwrap();
        assert_eq!(p.provider, "Anthropic");
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(find_pricing("some-random-model").is_none());
    }

    #[test]
    fn estimate_cost_sonnet_1k_tokens() {
        let est = estimate_cost("claude-sonnet-4-20250514", 1000, 500).unwrap();
        let expected = (1000.0 * 3.0 + 500.0 * 15.0) / 1_000_000.0;
        assert!((est.usd - expected).abs() < 1e-10);
    }

    #[test]
    fn gpt41_variants_differentiated() {
        let nano = find_pricing("gpt-4.1-nano").unwrap();
        let mini = find_pricing("gpt-4.1-mini").unwrap();
        let base = find_pricing("gpt-4.1").unwrap();
        assert!(nano.input_per_million < mini.input_per_million);
        assert!(mini.input_per_million < base.input_per_million);
    }

    #[test]
    fn specdec_different_from_versatile() {
        let specdec = find_pricing("llama-3.3-70b-specdec").unwrap();
        let versatile = find_pricing("llama-3.3-70b-versatile").unwrap();
        assert!((specdec.output_per_million - 0.99).abs() < f64::EPSILON);
        assert!((versatile.output_per_million - 0.79).abs() < f64::EPSILON);
    }

    #[test]
    fn all_pricing_entries_have_non_empty_fields() {
        for p in ALL_PRICING {
            assert!(!p.provider.is_empty());
            assert!(!p.model_pattern.is_empty());
        }
    }

    // ── Pattern ordering invariant ──────────────────────────────
    //
    // Two-pass contains-fallback only works when a longer pattern is
    // tested BEFORE a shorter one that it contains — otherwise
    // `gpt-4o-mini` matches `"gpt-4o"` before `"gpt-4o-mini"` and the
    // wrong price wins. The test below walks each provider's slice and
    // fails if any later pattern is a strict substring of an earlier one
    // in the same provider — that would mean the longer, more specific
    // one came second.

    #[test]
    fn pricing_patterns_ordered_longest_first_within_provider() {
        use std::collections::BTreeMap;
        let mut by_provider: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for p in ALL_PRICING {
            by_provider.entry(p.provider).or_default().push(p.model_pattern);
        }
        for (provider, patterns) in &by_provider {
            for i in 0..patterns.len() {
                for j in (i + 1)..patterns.len() {
                    let earlier = patterns[i];
                    let later = patterns[j];
                    // If a LATER entry contains an EARLIER entry as a
                    // substring, the later one is more specific and has
                    // been placed wrong.
                    assert!(
                        !later.contains(earlier) || later == earlier,
                        "{provider}: pattern {later:?} contains {earlier:?} — longer/more-specific pattern must come first (would cause wrong-cost match via contains-fallback)",
                    );
                }
            }
        }
    }

    // ── Scoped pricing lookup ────────────────────────────────────

    #[test]
    fn scoped_pricing_exact_match() {
        let p = find_pricing_scoped("openai", "gpt-4o").unwrap();
        assert_eq!(p.provider, "OpenAI");
        assert!((p.input_per_million - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn scoped_pricing_contains_fallback() {
        // Dated variant — should fall through to the "claude-sonnet-4" contains match.
        let p = find_pricing_scoped("anthropic", "claude-sonnet-4-20250514").unwrap();
        assert_eq!(p.provider, "Anthropic");
    }

    #[test]
    fn scoped_pricing_accepts_provider_id_or_display_form() {
        // "openai" (id) and "OpenAI" (display) should both resolve.
        let lower = find_pricing_scoped("openai", "gpt-4o").unwrap();
        let proper = find_pricing_scoped("OpenAI", "gpt-4o").unwrap();
        assert!(core::ptr::eq(lower, proper));
    }

    #[test]
    fn scoped_pricing_rejects_cross_provider_contains() {
        // "gpt-4o" ALSO contains-matches the "gpt-4" substring on any provider
        // that lists it. Scoped to a made-up provider, lookup must fail rather
        // than bleed into OpenAI rows.
        assert!(find_pricing_scoped("made-up-provider", "gpt-4o").is_none());
    }

    #[test]
    fn scoped_pricing_unknown_model_returns_none() {
        assert!(find_pricing_scoped("openai", "gpt-999-imaginary").is_none());
    }
}
