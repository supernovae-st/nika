// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities and pricing catalog.
//!
//! Capabilities use pattern-matching (NOT phf) — model names are open-ended.
//! Pricing uses 2-pass matching: exact match first, then `contains()` fallback.

#[cfg(feature = "pricing")]
use crate::types::model::{CostEstimate, ModelPricing};
#[cfg(feature = "capabilities")]
use crate::types::model::ModelCapabilities;

// ═══════════════════════════════════════════════════════════════════════════
// Model capabilities (pattern-matching function)
// ═══════════════════════════════════════════════════════════════════════════

/// Resolve capabilities for a given provider + model combination.
///
/// **Provider-aware:** the same model name gets different treatment depending
/// on the provider. `o3` on `OpenAI` gets `max_completion_tokens`, but
/// `o3-finetune` on a custom vLLM endpoint gets `max_tokens`.
///
/// # How it works
///
/// Rules live in `data/model-capabilities.toml` and are materialised by
/// `build.rs` into a `&'static [Rule]` slice. The resolver:
///
///   1. canonicalises the provider via [`crate::data::find_provider`]
///      (so `"claude"` → `"anthropic"`, `"grok"` → `"xai"`, etc.);
///   2. walks `CAPABILITY_RULES` in file order, skipping rules whose
///      `providers` scope or `api_dialect` scope doesn't match;
///   3. on the first matching rule, merges its `caps` into a `CapPatch`
///      accumulator seeded from `CAPABILITY_DEFAULTS` and breaks;
///   4. materialises into a full [`ModelCapabilities`] using
///      [`ModelCapabilities::default`] for any still-unset field.
///
/// Zero heap allocations: every string comparison uses
/// `eq_ignore_ascii_case` on the raw slice, no `to_lowercase()`.
#[cfg(feature = "capabilities")]
#[must_use]
#[inline]
pub fn model_capabilities(provider: &str, model: &str) -> ModelCapabilities {
    use crate::data::{find_provider, CAPABILITY_DEFAULTS, CAPABILITY_RULES};

    // Canonicalise provider + surface api_dialect in one phf hit.
    let provider_entry = find_provider(provider);
    let canonical = provider_entry.map_or(provider, |p| p.id);
    let dialect = provider_entry.and_then(|p| p.api_dialect);

    let mut patch = CAPABILITY_DEFAULTS;

    for rule in CAPABILITY_RULES {
        // Provider scope: empty list = global; otherwise canonical must be
        // in the list (case-insensitive ASCII).
        if !rule.providers.is_empty()
            && !rule
                .providers
                .iter()
                .any(|p| canonical.eq_ignore_ascii_case(p))
        {
            continue;
        }
        // Dialect scope: `None` on the rule = any dialect; otherwise the
        // provider must declare that exact dialect.
        if let Some(required) = rule.api_dialect {
            match dialect {
                Some(d) if d.eq_ignore_ascii_case(required) => {}
                _ => continue,
            }
        }
        if !rule.matcher.matches(model) {
            continue;
        }
        // First match wins.
        patch = patch.merge_with(rule.caps);
        break;
    }

    patch.materialize()
}

// ═══════════════════════════════════════════════════════════════════════════
// Pricing catalog (62 entries, sorted by provider → specificity)
// ═══════════════════════════════════════════════════════════════════════════

/// Static pricing table — model patterns matched by exact name or `contains()`.
///
/// **Ordering matters for `contains()` fallback**: more specific patterns MUST
/// appear before less specific ones within each provider.
#[cfg(feature = "pricing")]
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
#[cfg(feature = "pricing")]
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
/// (e.g. `"openai"`) or the capitalised display form used in the
/// pricing table (e.g. `"OpenAI"`).
#[cfg(feature = "pricing")]
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
#[cfg(feature = "pricing")]
#[must_use]
// REASON: casting `u64` token counts to `f64` for the rate multiplication.
// Token counts cap at provider context windows (≤ 10 M = 2²³), well below
// the `f64` precision horizon (2⁵³). A saturating conversion would hide a
// real data bug if that ever changed; the cast is the right primitive here.
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
    #[cfg(feature = "capabilities")]
    use crate::types::model::TokenLimitParam;

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
        assert!(caps.reasoning);
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
        assert!(caps.reasoning);
    }

    #[test]
    fn gpt41_is_not_reasoning() {
        let caps = model_capabilities("openai", "gpt-4.1");
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(caps.supports_temperature);
    }

    #[test]
    fn claude_has_reasoning() {
        let caps = model_capabilities("anthropic", "claude-sonnet-4-6");
        assert!(caps.reasoning);
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

// ═══════════════════════════════════════════════════════════════════════════
// Parity test — new resolver vs legacy (pre-Session-2a) body
// ═══════════════════════════════════════════════════════════════════════════
//
// Lives in this file (not in `tests/`) so the `#[cfg(test)]` module can
// construct `#[non_exhaustive]` `ModelCapabilities` values directly and so
// the test runs under the workspace-wide `cargo test --workspace --lib`
// invocation (no `--test` flag required, no macOS Keychain risk).

#[cfg(all(test, feature = "capabilities"))]
mod parity_tests {
    //! Property-based parity between the new TOML-driven resolver and the
    //! hand-rolled body that shipped at HEAD `1a29bd32f`.
    //!
    //! The migration must be behaviour-preserving: a misplaced rule, missed
    //! alias, or wrong ordering would silently corrupt `token_limit_param`
    //! for every provider call. Proptest explores the edge-case space with
    //! 10 000 randomised inputs + 26 explicit spot-checks on aliases.
    //!
    //! The `reasoning` field name below reflects the Session 2a rename
    //! (`supports_thinking` → `reasoning`). Legacy *semantics* are unchanged.
    //!
    //! # 🗑️ Exit plan — DELETE at v0.90 tag (or Session 2b completion)
    //!
    //! This entire module is a **one-way bridge** from legacy to the new
    //! resolver. Once Session 2b extends [`ModelCapabilities`] with
    //! modalities + tokenizer + `supported_parameters`, the legacy body no
    //! longer represents full correctness — it only covers the 5 fields
    //! that existed at HEAD `1a29bd32f`. At that point:
    //!
    //!   1. delete `legacy_model_capabilities` + `is_o_series` + `is_gpt5`;
    //!   2. delete the proptest `capability_parity_across_any_input`;
    //!   3. KEEP `parity_spot_checks` (rename to `spot_checks`) and
    //!      `snapshot_capabilities_for_representative_models` — both keep
    //!      value as oracle tests even without the legacy oracle.
    //!
    //! The existence of `.to_lowercase()` in this module (lines below) is
    //! the signal: once legacy is gone, zero `.to_lowercase()` remains in
    //! `src/` full-stop (today: two, inside this gated harness).

    use super::{model_capabilities, ModelCapabilities};
    use crate::types::model::TokenLimitParam;

    /// Verbatim body of `model_capabilities` at HEAD `1a29bd32f`, with the
    /// single mechanical substitution `supports_thinking` → `reasoning` so
    /// the function populates the renamed field on the shared type.
    fn legacy_model_capabilities(provider: &str, model: &str) -> ModelCapabilities {
        let lower = model.to_lowercase();
        let canonical = crate::data::find_provider(provider).map(|p| p.id);
        let prov = canonical.unwrap_or(provider).to_lowercase();

        let is_openai_api = matches!(prov.as_str(), "openai" | "openrouter");

        if is_openai_api {
            if is_o_series(&lower) {
                return ModelCapabilities {
                    token_limit_param: TokenLimitParam::MaxCompletionTokens,
                    supports_temperature: false,
                    reasoning: true,
                    ..Default::default()
                };
            }
            if is_gpt5(&lower) {
                return ModelCapabilities {
                    token_limit_param: TokenLimitParam::MaxCompletionTokens,
                    supports_temperature: true,
                    reasoning: true,
                    ..Default::default()
                };
            }
        }

        if prov == "anthropic" || lower.starts_with("claude") {
            return ModelCapabilities {
                reasoning: true,
                ..Default::default()
            };
        }

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

        if prov == "xai" && lower == "grok-4" {
            return ModelCapabilities {
                supports_stop_sequences: false,
                ..Default::default()
            };
        }

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

    use proptest::prelude::*;

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(10_000))]

        /// Byte-for-byte parity across 10 000 randomised `(provider, model)`
        /// pairs. A single divergence fails the build; proptest shrinks to
        /// the minimal offending pair for the failure message.
        ///
        /// Regex coverage (widened in the Session 2a hardening pass):
        ///   * provider `[a-zA-Z]{0,20}` — includes empty, uppercase, mixed
        ///     case. Exercises the canonicalisation path on the provider axis
        ///     (legacy did `.to_lowercase()` on `prov`; new does
        ///     `eq_ignore_ascii_case` — both must agree on non-lowercase input).
        ///   * model `[a-zA-Z0-9._/-]{1,80}` — includes `/` (v0.74 slash
        ///     routing syntax `groq/llama-3.3-70b`), `_` (Hugging Face style
        ///     `Qwen2_5`), uppercase, and lengths up to 80 (legacy regex
        ///     capped at 40 — too short for `Qwen/Qwen3-235B-A22B-Thinking-2507`).
        #[test]
        fn capability_parity_across_any_input(
            provider in "[a-zA-Z]{0,20}",
            model in "[a-zA-Z0-9._/-]{1,80}",
        ) {
            let new_caps = model_capabilities(&provider, &model);
            let legacy_caps = legacy_model_capabilities(&provider, &model);
            prop_assert_eq!(
                new_caps,
                legacy_caps,
                "parity break: provider={:?} model={:?}",
                provider,
                model,
            );
        }
    }

    /// Snapshot of resolved capabilities for 30 representative
    /// (provider, model) pairs. Reviewable `.snap` file catches silent
    /// semantic breakage that leaves proptest happy (e.g. a new rule that
    /// changes `reasoning` on a model the parity test doesn't exercise).
    #[test]
    fn snapshot_capabilities_for_representative_models() {
        let cases: &[(&str, &str)] = &[
            // Anthropic family (canonical + alias + various versions).
            ("anthropic", "claude-opus-4-20250514"),
            ("anthropic", "claude-sonnet-4-5-20250929"),
            ("anthropic", "claude-haiku-4-5-20251001"),
            ("claude", "claude-sonnet-4-20250514"),
            // OpenAI — classic, o-series, gpt-5 family.
            ("openai", "gpt-4o"),
            ("openai", "gpt-4o-mini"),
            ("openai", "gpt-4.1"),
            ("openai", "o1"),
            ("openai", "o1-preview"),
            ("openai", "o3"),
            ("openai", "o3-mini"),
            ("openai", "o4-mini"),
            ("openai", "gpt-5"),
            ("openai", "gpt-5-turbo"),
            ("openai", "gpt-5.1"),
            // OpenRouter — aliased API surface.
            ("openrouter", "o3"),
            ("openrouter", "anthropic/claude-sonnet-4-5-20250929"),
            // Mistral, Groq, DeepSeek, Gemini, xAI, Cohere.
            ("mistral", "mistral-large-latest"),
            ("groq", "llama-3.3-70b-versatile"),
            ("deepseek", "deepseek-chat"),
            ("deepseek", "deepseek-reasoner"),
            ("deep-seek", "deepseek-chat"),
            ("gemini", "gemini-2.5-pro"),
            ("xai", "grok-3"),
            ("xai", "grok-4"),
            ("grok", "grok-4"),
            ("cohere", "command-r-plus"),
            // Bedrock, Azure, Vertex, Native, Mock.
            ("bedrock", "anthropic.claude-sonnet-4-20250514-v1:0"),
            ("azure", "gpt-4o"),
            ("native", "Qwen/Qwen3-8B"),
            // Unknown provider + model → safe defaults.
            ("unknown", "unknown-model"),
        ];
        let rendered: Vec<String> = cases
            .iter()
            .map(|(p, m)| format!("{p:>12} / {m:<40} → {:?}", model_capabilities(p, m)))
            .collect();
        insta::assert_snapshot!(rendered.join("\n"));
    }

    /// Explicit spot-checks for the 12 legacy shapes, including alias
    /// resolution (`claude` → `anthropic`, `grok` → `xai`, `deep-seek` →
    /// `deepseek`) and the fallthrough path for unknown providers. Random
    /// generation is unlikely to hit these exact combinations within 10 000
    /// cases, so we enumerate them.
    #[test]
    fn parity_spot_checks() {
        for case in [
            ("openai", "o1"),
            ("openai", "o1-preview"),
            ("openai", "o3"),
            ("openai", "o3-mini"),
            ("openai", "o4-mini"),
            ("openai", "gpt-5"),
            ("openai", "gpt-5-turbo"),
            ("openai", "gpt-5.1"),
            ("openrouter", "o3"),
            ("anthropic", "claude-opus-4-20250514"),
            ("claude", "claude-sonnet-4-20250514"),
            ("deepseek", "deepseek-reasoner"),
            ("deepseek", "deepseek-chat"),
            ("deep-seek", "deepseek-chat"),
            ("xai", "grok-4"),
            ("grok", "grok-4"),
            ("unknown", "unknown-model"),
            // Hardening pass — shapes the widened proptest regex CAN reach.
            ("", ""),                             // both empty
            ("", "o3"),                           // empty provider (runtime null-binding edge)
            ("OPENAI", "o3"),                     // uppercase canonical
            ("OpEnAi", "gpt-5"),                  // mixed-case canonical
            ("openai", "groq/llama-3.3-70b"),     // slash routing syntax (v0.74)
            ("native", "Qwen/Qwen3-8B"),          // HF-style slash path
            ("native", "Qwen2_5"),                // underscore (HF naming)
            ("openai", "O3"),                     // uppercase model
            ("openai", "O1-Preview"),             // mixed-case family
        ] {
            assert_eq!(
                model_capabilities(case.0, case.1),
                legacy_model_capabilities(case.0, case.1),
                "spot-check failed: {case:?}",
            );
        }
    }
}
