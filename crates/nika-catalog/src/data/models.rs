// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities and pricing catalog.
//!
//! Capabilities use pattern-matching (NOT phf) — model names are open-ended.
//! Pricing uses 2-pass matching: exact match first, then `contains()` fallback.

#[cfg(feature = "capabilities")]
use crate::types::model::ModelCapabilities;
#[cfg(feature = "pricing")]
use crate::types::model::{CostEstimate, ModelPricing};

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
/// `build.rs` into a `&'static [CapRule]` slice. The resolver:
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
    use crate::data::{CAPABILITY_DEFAULTS, CAPABILITY_RULES, find_provider};

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

    patch.materialize(&CAPABILITY_DEFAULTS)
}

// ═══════════════════════════════════════════════════════════════════════════
// Pricing catalog — generated from data/model-pricing.toml by build.rs
// ═══════════════════════════════════════════════════════════════════════════
//
// `ALL_PRICING` lives in `data::generated` (via include! of $OUT_DIR/
// model_pricing.rs). Re-exported here as `crate::data::ALL_PRICING`
// by `data/mod.rs`.

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
    use crate::data::ALL_PRICING;
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
/// (e.g. `"openai"`), a catalog alias (e.g. `"gemini"` ↔ snapshot
/// `"google"`), or the capitalised display form used in the pricing
/// table (e.g. `"OpenAI"`).
#[cfg(feature = "pricing")]
#[must_use]
pub fn find_pricing_scoped(provider: &str, model: &str) -> Option<&'static ModelPricing> {
    use crate::data::ALL_PRICING;
    // Pass 1: exact model match within provider scope.
    if let Some(p) = ALL_PRICING
        .iter()
        .find(|p| pricing_provider_matches(p.provider, provider) && model == p.model_pattern)
    {
        return Some(p);
    }
    // Pass 2: contains() fallback within provider scope.
    ALL_PRICING
        .iter()
        .find(|p| pricing_provider_matches(p.provider, provider) && model.contains(p.model_pattern))
}

/// A snapshot row and a query name the same catalog seat.
///
/// models.dev ships Gemini under `google`; the engine id is `gemini`
/// with alias `google`. Byte-equal ids still match without a catalog
/// hit so unknown overlay providers keep working.
#[cfg(feature = "pricing")]
fn pricing_provider_matches(row_provider: &str, query: &str) -> bool {
    if row_provider.eq_ignore_ascii_case(query) {
        return true;
    }
    match (
        crate::data::find_provider(row_provider),
        crate::data::find_provider(query),
    ) {
        (Some(row), Some(asked)) => row.id.eq_ignore_ascii_case(asked.id),
        _ => false,
    }
}

/// Find pricing for a model string that MAY carry a `provider/` prefix —
/// the resolved form the engine passes around (`"openai/gpt-4o-mini"`).
///
/// A prefixed string resolves via [`find_pricing_scoped`] (contains-fallback
/// never crosses provider boundaries) · a bare string falls back to
/// [`find_pricing`]. This is the ONE model-string → pricing resolution:
/// the check-time cost floor (`nika-schema` check/cost) and the runtime's
/// task-completion pricing both call it — the two surfaces can never
/// disagree on which row prices a model.
#[cfg(feature = "pricing")]
#[must_use]
pub fn find_pricing_for(model: &str) -> Option<&'static ModelPricing> {
    match model.split_once('/') {
        Some((provider, name)) => find_pricing_scoped(provider, name),
        None => find_pricing(model),
    }
}

/// The pricing math — ONE site for every estimate surface.
///
/// The token split follows the `OTel` `gen_ai` semantics the wires
/// normalize to: `input_tokens` INCLUDES the cache subsets (read +
/// write), so the full-rate portion is the remainder. Cache rates fall
/// back to the input rate when the catalog does not disclose them (the
/// pre-@1.1 approximation — documented, and rare now that the snapshot
/// carries upstream cache rates).
///
/// Reasoning is a subset of `output_tokens` and prices at the OUTPUT rate:
/// `reasoning_tokens_per_million` is declared but NO shipped row sets it
/// (`no_row_declares_a_reasoning_rate_yet` pins that) — an arm would be
/// unreachable; the day a row discloses one, that test fires with it.
#[cfg(feature = "pricing")]
// REASON: casting `u64` token counts to `f64` for the rate multiplication.
// Token counts cap at provider context windows (≤ 10 M = 2²³), well below
// the `f64` precision horizon (2⁵³). A saturating conversion would hide a
// real data bug if that ever changed; the cast is the right primitive here.
#[allow(clippy::cast_precision_loss)]
pub(crate) fn usd_for_split(
    pricing: &ModelPricing,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> f64 {
    // Saturating: a provider reporting cache subsets larger than the
    // whole input is a wire bug — clamp the full-rate portion at zero,
    // never let it go negative.
    let uncached = input_tokens
        .saturating_sub(cache_read_tokens)
        .saturating_sub(cache_write_tokens);
    let read_rate = pricing
        .cache_read_per_million
        .unwrap_or(pricing.input_per_million);
    let write_rate = pricing
        .cache_write_per_million
        .unwrap_or(pricing.input_per_million);
    (uncached as f64 * pricing.input_per_million
        + cache_read_tokens as f64 * read_rate
        + cache_write_tokens as f64 * write_rate
        + output_tokens as f64 * pricing.output_per_million)
        / 1_000_000.0
}

/// The cache-blind projection of [`usd_for_split`] (both estimate
/// surfaces that predate the split keep their exact behavior).
#[cfg(feature = "pricing")]
fn usd_for(pricing: &ModelPricing, input_tokens: u64, output_tokens: u64) -> f64 {
    usd_for_split(pricing, input_tokens, output_tokens, 0, 0)
}

/// Estimate cost for a model invocation.
#[cfg(feature = "pricing")]
#[must_use]
pub fn estimate_cost(model: &str, input_tokens: u64, output_tokens: u64) -> Option<CostEstimate> {
    let pricing = find_pricing(model)?;
    Some(CostEstimate::new(
        usd_for(pricing, input_tokens, output_tokens),
        pricing.input_per_million,
        pricing.output_per_million,
        model.to_string(),
        pricing.provider.to_string(),
    ))
}

/// Estimate cost for a RESOLVED model string (`provider/name` or bare) —
/// [`find_pricing_for`] resolution × the same per-million math as
/// [`estimate_cost`]. `None` when the model has no catalog price (mock ·
/// unknown local) — the caller emits nothing rather than a fake number.
#[cfg(feature = "pricing")]
#[must_use]
pub fn estimate_cost_for(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> Option<CostEstimate> {
    let pricing = find_pricing_for(model)?;
    Some(CostEstimate::new(
        usd_for(pricing, input_tokens, output_tokens),
        pricing.input_per_million,
        pricing.output_per_million,
        model.to_string(),
        pricing.provider.to_string(),
    ))
}

/// Estimate cost for a RESOLVED model string with the FULL usage split —
/// cache reads/writes priced at their catalog rates when disclosed
/// (falling back to the input rate otherwise).
///
/// `input_tokens` INCLUDES the cache subsets — the `OTel` `gen_ai`
/// semantics every wire normalizes to; the full-rate portion is the
/// remainder. `None` when the model has no catalog price (mock ·
/// unknown local) — the caller emits nothing rather than a fake number.
#[cfg(feature = "pricing")]
#[must_use]
pub fn estimate_cost_usage_for(
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_write_tokens: u64,
) -> Option<CostEstimate> {
    let pricing = find_pricing_for(model)?;
    Some(CostEstimate::new(
        usd_for_split(
            pricing,
            input_tokens,
            output_tokens,
            cache_read_tokens,
            cache_write_tokens,
        ),
        pricing.input_per_million,
        pricing.output_per_million,
        model.to_string(),
        pricing.provider.to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "pricing")]
    use crate::data::ALL_PRICING;
    #[cfg(feature = "capabilities")]
    use crate::types::Modality;
    #[cfg(feature = "capabilities")]
    use crate::types::model::TokenLimitParam;

    // ── Pricing count ───────────────────────────────────────────

    #[test]
    fn pricing_count() {
        // DERIVED, never pinned (the born-stale law): the generated
        // catalog moves with upstream — the structural truths are
        // non-emptiness and every cloud provider being represented.
        assert!(ALL_PRICING.len() > 100, "got {}", ALL_PRICING.len());
        for provider in [
            "anthropic",
            "openai",
            "google",
            "deepseek",
            "mistral",
            "xai",
            "groq",
            "openrouter",
            "huggingface",
            "nvidia",
        ] {
            assert!(
                ALL_PRICING.iter().any(|p| p.provider == provider),
                "no pricing rules for {provider}"
            );
        }
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
    fn gpt4p1_is_not_reasoning() {
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
        assert!(!caps.input_modalities.contains(&Modality::Image));
    }

    #[test]
    fn deepseek_chat_no_vision() {
        let caps = model_capabilities("deepseek", "deepseek-chat");
        assert!(caps.supports_temperature);
        assert!(!caps.input_modalities.contains(&Modality::Image));
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
        // A dated variant contains-matches its base pattern (providers
        // are the ENGINE ids since the models.dev regen — the display
        // names never matched eq_ignore_ascii_case("google", …)).
        let p = find_pricing("claude-sonnet-4-5-20991231").unwrap();
        assert_eq!(p.provider, "anthropic");
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn unknown_model_returns_none() {
        assert!(find_pricing("some-random-model").is_none());
    }

    #[test]
    fn estimate_cost_sonnet_1k_tokens() {
        let est = estimate_cost("claude-sonnet-4-5-20991231", 1000, 500).unwrap();
        let expected = (1000.0 * 3.0 + 500.0 * 15.0) / 1_000_000.0;
        assert!((est.usd - expected).abs() < 1e-10);
    }

    #[test]
    fn estimate_cost_zero_tokens_returns_zero_usd() {
        let est = estimate_cost("gpt-4o", 0, 0).expect("known model should return Some");
        assert!(
            est.usd.abs() < f64::EPSILON,
            "zero tokens must produce zero cost, got {}",
            est.usd,
        );
        assert_eq!(est.provider, "openai");
        assert_eq!(est.model, "gpt-4o");
    }

    #[test]
    fn estimate_cost_nonexistent_model_returns_none() {
        assert!(
            estimate_cost("nonexistent-model-xyz", 100, 50).is_none(),
            "unknown model must return None",
        );
    }

    // ── find_pricing_for / estimate_cost_for (resolved strings) ──

    #[test]
    fn find_pricing_for_provider_prefixed_resolves_scoped() {
        let p = find_pricing_for("openai/gpt-4o-mini").expect("priced model");
        assert_eq!(p.provider, "openai");
        // The prefixed form and the explicit scoped call resolve to the
        // SAME row — one resolution, two spellings.
        let scoped = find_pricing_scoped("openai", "gpt-4o-mini").expect("scoped hit");
        assert!(std::ptr::eq(p, scoped));
    }

    #[test]
    fn find_pricing_for_bare_falls_back_unscoped() {
        let p = find_pricing_for("gpt-4o-mini").expect("bare model resolves");
        let unscoped = find_pricing("gpt-4o-mini").expect("unscoped hit");
        assert!(std::ptr::eq(p, unscoped));
    }

    #[test]
    fn find_pricing_for_mock_and_unknown_local_are_none() {
        // The honest-absence contract: a mock or local model prices to
        // None — the runtime emits NO cost fields for these.
        assert!(find_pricing_for("mock/mock-echo").is_none());
        assert!(find_pricing_for("ollama/qwen3.5:4b").is_none());
    }

    /// B18 / issue 1306: `groq/grok-3` is not an xAI seat. Contains-fallback
    /// must not price grok-3 through groq, or MODELS would call it ready.
    #[test]
    fn groq_grok_3_is_not_priced_ready() {
        assert!(
            find_pricing_for("groq/grok-3").is_none(),
            "grok-3 is xAI — groq must not inherit its snapshot row"
        );
        assert!(find_pricing_for("xai/grok-3").is_some());
        assert!(find_pricing_scoped("groq", "grok-3").is_none());
    }

    /// B20 / issue 1297: the snapshot prices Gemini Flash under models.dev's
    /// `google` id; the engine seat is `gemini` (alias `google`). Lookup
    /// must resolve the runnable form to that real row — `priced: false`
    /// on `gemini/gemini-2.5-flash` is how `--max-cost-usd` failed to bound
    /// a live cloud call.
    #[test]
    fn find_pricing_for_gemini_flash_uses_the_google_snapshot_row() {
        let p = find_pricing_for("gemini/gemini-2.5-flash")
            .expect("gemini flash must resolve to a snapshot row");
        assert_eq!(p.model_pattern, "gemini-2.5-flash");
        assert!(
            p.output_per_million > 0.0 && p.input_per_million > 0.0,
            "a real snapshot row, not a zero placeholder: {p:?}"
        );
        let via_alias = find_pricing_scoped("google", "gemini-2.5-flash").expect("google alias");
        let via_id = find_pricing_scoped("gemini", "gemini-2.5-flash").expect("gemini id");
        assert!(
            std::ptr::eq(via_alias, via_id) && std::ptr::eq(p, via_id),
            "google and gemini must price the same row"
        );
    }

    #[test]
    fn estimate_cost_for_prefixed_matches_scoped_math() {
        let est = estimate_cost_for("openai/gpt-4o-mini", 1000, 500).expect("priced");
        let p = find_pricing_scoped("openai", "gpt-4o-mini").expect("scoped hit");
        let expected = (1000.0 * p.input_per_million + 500.0 * p.output_per_million) / 1e6;
        assert!((est.usd - expected).abs() < 1e-12);
        assert_eq!(est.model, "openai/gpt-4o-mini", "carries the queried form");
        assert_eq!(est.provider, "openai");
    }

    #[test]
    fn estimate_cost_for_unpriced_is_none() {
        assert!(estimate_cost_for("mock/mock-echo", 100, 50).is_none());
        assert!(estimate_cost_for("llamacpp/local-gguf", 100, 50).is_none());
    }

    #[test]
    fn estimate_cost_for_zero_tokens_is_zero_usd() {
        let est = estimate_cost_for("openai/gpt-4o", 0, 0).expect("priced");
        assert!(est.usd.abs() < f64::EPSILON);
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
            by_provider
                .entry(p.provider)
                .or_default()
                .push(p.model_pattern);
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
        assert_eq!(p.provider, "openai");
        assert!((p.input_per_million - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn scoped_pricing_contains_fallback() {
        // Dated variant — falls through to the "claude-sonnet-4"
        // supplement row (referenced-but-retired · kept for the
        // provider default_model integrity gate).
        let p = find_pricing_scoped("anthropic", "claude-sonnet-4-20250514").unwrap();
        assert_eq!(p.provider, "anthropic");
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

    // ── Pricing data invariants (all 62 entries) ────────────────

    #[test]
    fn all_pricing_rates_finite_non_negative() {
        for p in ALL_PRICING {
            assert!(
                p.input_per_million.is_finite() && p.input_per_million >= 0.0,
                "{}/{}: input_per_million = {} is invalid",
                p.provider,
                p.model_pattern,
                p.input_per_million,
            );
            assert!(
                p.output_per_million.is_finite() && p.output_per_million >= 0.0,
                "{}/{}: output_per_million = {} is invalid",
                p.provider,
                p.model_pattern,
                p.output_per_million,
            );
            if let Some(v) = p.cache_write_per_million {
                assert!(
                    v.is_finite() && v >= 0.0,
                    "{}/{}: cache_write invalid",
                    p.provider,
                    p.model_pattern
                );
            }
            if let Some(v) = p.cache_read_per_million {
                assert!(
                    v.is_finite() && v >= 0.0,
                    "{}/{}: cache_read invalid",
                    p.provider,
                    p.model_pattern
                );
            }
            if let Some(v) = p.image_per_million {
                assert!(
                    v.is_finite() && v >= 0.0,
                    "{}/{}: image invalid",
                    p.provider,
                    p.model_pattern
                );
            }
            if let Some(v) = p.reasoning_tokens_per_million {
                assert!(
                    v.is_finite() && v >= 0.0,
                    "{}/{}: reasoning invalid",
                    p.provider,
                    p.model_pattern
                );
            }
        }
    }

    #[test]
    fn all_pricing_cache_axes_populated_image_reasoning_reserved() {
        // Cost Intelligence 2026-07-07: the refresh emits upstream cache
        // rates (models.dev cache_read/cache_write) — a healthy snapshot
        // carries them on a meaningful share of rows (anthropic + openai
        // at minimum disclose them). Image/reasoning axes stay RESERVED:
        // the refresh does not emit them yet and the cost math must not
        // silently start reading a value nobody vendored.
        let cached = ALL_PRICING
            .iter()
            .filter(|p| p.cache_read_per_million.is_some())
            .count();
        assert!(cached > 50, "cache_read rates vendored (got {cached})");
        // NOTE: `read ≤ input` is NOT asserted per-row — upstream carries
        // real exceptions (openrouter tencent/hy3: read 0.5 > input 0.2,
        // verified 2026-07-07). The math is an honest passthrough of the
        // catalog; the flagship-discount property is pinned separately on
        // anthropic (`live_catalog_cache_read_discount_holds`).
        for p in ALL_PRICING {
            assert_eq!(
                p.image_per_million, None,
                "{}/{} image axis is reserved",
                p.provider, p.model_pattern
            );
            assert_eq!(
                p.reasoning_tokens_per_million, None,
                "{}/{} reasoning axis is reserved",
                p.provider, p.model_pattern
            );
        }
    }

    #[test]
    fn pricing_snapshot_provenance_is_machine_readable() {
        // The doctor/check surfaces read THIS — codegen already validated
        // shape at build; pin the contract from the consumer side too.
        let s = crate::pricing_snapshot();
        assert!(s.source.starts_with("https://"), "got {}", s.source);
        assert_eq!(s.as_of.len(), 10, "ISO date, got {}", s.as_of);
        assert_eq!(s.source_sha256_16.len(), 16, "got {}", s.source_sha256_16);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Proptest pricing invariants
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(all(test, feature = "pricing"))]
mod pricing_proptests {
    use super::*;
    use proptest::prelude::*;

    /// Arbitrary model-name strings for fuzzing pricing lookup.
    fn model_name() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-z0-9._/-]{0,50}").expect("valid regex")
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(1000))]

        /// find_pricing is deterministic: same input → same output.
        #[test]
        fn idempotent(model in model_name()) {
            let a = find_pricing(&model);
            let b = find_pricing(&model);
            prop_assert_eq!(a.map(|p| p.model_pattern), b.map(|p| p.model_pattern));
        }

        /// If exact match exists, find_pricing returns it (not a contains hit).
        #[test]
        fn exact_before_contains(model in model_name()) {
            if let Some(p) = find_pricing(&model) {
                // If the model string equals the pattern, it was an exact match.
                if model == p.model_pattern {
                    // Verify it's the FIRST exact match in the table.
                    let first_exact = crate::data::ALL_PRICING.iter()
                        .find(|e| model == e.model_pattern);
                    prop_assert_eq!(first_exact.map(|e| e.model_pattern), Some(p.model_pattern));
                }
            }
        }

        /// Scoped lookup never returns a different provider.
        #[test]
        fn scoped_never_cross_provider(
            provider in "[A-Za-z]{1,20}",
            model in model_name(),
        ) {
            if let Some(p) = find_pricing_scoped(&provider, &model) {
                prop_assert!(
                    p.provider.eq_ignore_ascii_case(&provider),
                    "scoped lookup returned wrong provider: {} for query {}",
                    p.provider, provider,
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Session 2b onward — provenance tests (post-legacy-parity)
// ═══════════════════════════════════════════════════════════════════════════
//
// The Session 2a→2b transition deleted the `legacy_model_capabilities` body
// + its `proptest! { 10_000 cases }` parity harness. Legacy only knew the 5
// Session-2a fields; after 2b adds modalities/tokenizer/supported_parameters
// /supports_system_messages, byte-for-byte parity is no longer meaningful.
//
// These tests replace it with an explicit oracle: every rule in
// `data/model-capabilities.toml` is exercised by a canonical (provider,
// model) pair that MUST resolve to a specific expected value. If a rule
// is accidentally shadowed (e.g. someone adds `openai-gpt4-family` before
// `openai-gpt4o-family`), one of the `assert_eq!` calls below fails with
// a message pointing at the offending rule.
//
// The insta snapshot test remains as the reviewable wide-angle oracle.

#[cfg(all(test, feature = "capabilities"))]
mod provenance_tests {
    use super::model_capabilities;
    use crate::types::{JsonMode, Modality, ParamFlag, TokenizerFamily};

    /// Snapshot of resolved capabilities for representative (provider, model)
    /// pairs. Reviewable `.snap` file catches silent semantic breakage.
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

    /// Core Session 2a semantics — tokenizer-agnostic behavioural asserts
    /// that survive the 2b migration because they only reference fields
    /// that existed before 2b. Covers alias resolution + fallthrough.
    #[test]
    fn session_2a_core_spot_checks() {
        use crate::types::model::TokenLimitParam;

        // OpenAI reasoning family — max_completion_tokens + no temperature.
        for (prov, model) in [
            ("openai", "o1"),
            ("openai", "o3"),
            ("openai", "o1-preview"),
            ("openai", "o3-mini"),
            ("openai", "o4-mini"),
            ("openrouter", "o3"),
            ("openai", "O3"),    // case-insensitive
            ("OpEnAi", "gpt-5"), // canonical provider mixed-case
        ] {
            let c = model_capabilities(prov, model);
            assert_eq!(
                c.token_limit_param,
                TokenLimitParam::MaxCompletionTokens,
                "reasoning family must get max_completion_tokens: {prov}/{model}",
            );
        }

        // Claude family — reasoning=true via prefix, independent of provider.
        // Note: "anthropic/claude-*" on openrouter does NOT match the
        // `claude-family-any-provider` rule (prefix "claude") and needs an
        // explicit `openrouter-claude` rule (scheduled for the full 28-rule
        // migration commit). Only bare "claude-*" is asserted here.
        for (prov, model) in [
            ("anthropic", "claude-opus-4-20250514"),
            ("claude", "claude-sonnet-4-20250514"),
        ] {
            let c = model_capabilities(prov, model);
            assert!(
                c.reasoning,
                "claude family: reasoning=true ({prov}/{model})"
            );
        }

        // DeepSeek — loses vision + reasoner rejects temperature.
        let r = model_capabilities("deepseek", "deepseek-reasoner");
        assert!(
            !r.input_modalities.contains(&Modality::Image),
            "deepseek-reasoner: no vision"
        );
        assert!(!r.supports_temperature, "deepseek-reasoner: no temperature");
        let c = model_capabilities("deep-seek", "deepseek-chat");
        assert!(
            !c.input_modalities.contains(&Modality::Image),
            "deep-seek alias: no vision"
        );

        // xAI grok-4 — still uses existing rule (kind=exact for Session 2a;
        // 2b expands the prefix list. Exact "grok-4" still matches.)
        let g = model_capabilities("xai", "grok-4");
        assert!(!g.supports_stop_sequences, "grok-4: no stop sequences");
        assert!(
            model_capabilities("grok", "grok-4") == g,
            "grok alias → xai"
        );

        // Unknown provider + model → defaults.
        let u = model_capabilities("unknown", "unknown-model");
        assert_eq!(u.token_limit_param, TokenLimitParam::MaxTokens);
        assert!(u.supports_temperature);
        assert!(u.input_modalities.contains(&Modality::Image));
    }

    /// Session 2b additions — baseline defaults round-trip through
    /// `materialize()` to the public struct. These are exercised here
    /// rather than in the types test module because we need the full
    /// `model_capabilities()` pipeline (TOML → `CAPABILITY_DEFAULTS` →
    /// materialize).
    ///
    /// After the 28 TOML rules land in the same commit, this test grows
    /// with per-rule provenance asserts (`o3` tokenizer=o200k, claude PDF,
    /// deepseek `json_mode=Object`, magistral reasoning, etc.).
    #[test]
    fn session_2b_default_fields_materialize() {
        // Unknown model → full defaults from [defaults] block.
        let c = model_capabilities("unknown-provider", "unknown-model");
        assert!(
            c.input_modalities.contains(&Modality::Text),
            "default input_modalities must contain text"
        );
        assert!(c.output_modalities.contains(&Modality::Text));
        assert!(
            c.supports_system_messages,
            "default: supports_system_messages=true"
        );
        // Default tokenizer = None. TOML [defaults] intentionally omits the
        // field (most providers don't disclose their tokenizer), so for an
        // unknown provider the materialised tokenizer MUST be None. Earlier
        // draft of this assert had a dead `|| == Some(Cl100k)` branch that
        // made it vacuous (is_none() was always true); tightened per
        // Session 2b review swarm P1.
        assert!(
            c.tokenizer.is_none(),
            "default tokenizer must be None (TOML [defaults] omits the field)"
        );
        // Default json_mode = None (rules set it for specific models).
        assert!(
            c.json_mode.is_none(),
            "default json_mode must be None (rules set it)"
        );
        // Default context window / max output = None. TOML [defaults]
        // intentionally omits these (values are per-model, not per-family).
        assert!(
            c.context_window_tokens.is_none(),
            "default context_window_tokens must be None"
        );
        assert!(
            c.max_output_tokens.is_none(),
            "default max_output_tokens must be None"
        );
    }

    /// Per-rule provenance spot-checks — every rule is exercised by the
    /// canonical (provider, model) pair that should trigger it. If a rule
    /// is accidentally shadowed (e.g. `openai-gpt4-family` before
    /// `openai-gpt4o-family`), the corresponding assert below fails with a
    /// Session-2B per-rule provenance · [1]-[8] · the openai families (o-series · gpt4o/4p1/4 tokenizers) — one
    /// (provider, model) probe per rule, failure names the rule.
    #[test]
    fn session_2b_openai_rules() {
        // [1] openai-o-series-exact — o1 + o3 vision YES, developer role.
        let o3 = model_capabilities("openai", "o3");
        assert_eq!(
            o3.tokenizer,
            Some(TokenizerFamily::O200k),
            "o3: must match o-series-exact rule"
        );
        assert!(
            !o3.supports_system_messages,
            "o-series: developer role canonical (system silently reclassified)"
        );
        assert!(
            o3.input_modalities.contains(&Modality::Image),
            "o3: vision YES (not same as o3-mini)"
        );
        assert!(
            o3.supported_parameters
                .contains(&ParamFlag::ReasoningEffort)
        );

        let o1 = model_capabilities("openai", "o1");
        assert!(
            o1.input_modalities.contains(&Modality::Image),
            "o1: vision YES"
        );
        assert!(!o1.supports_system_messages, "o1: developer role");

        // [2] openai-o4-family — o4-mini vision YES, stop=false.
        let o4mini = model_capabilities("openai", "o4-mini");
        assert!(
            o4mini.input_modalities.contains(&Modality::Image),
            "o4-mini: vision YES (April 2025)"
        );
        assert_eq!(o4mini.tokenizer, Some(TokenizerFamily::O200k));
        assert!(!o4mini.supports_system_messages, "o4-mini: developer role");
        assert!(
            !o4mini.supports_stop_sequences,
            "o4-mini: stop NOT supported per OpenAI SDK docstring"
        );

        // [3] openai-o-series-family — o3-mini text-only, o1-mini o200k.
        let o3mini = model_capabilities("openai", "o3-mini");
        assert!(
            !o3mini.input_modalities.contains(&Modality::Image),
            "o3-mini: text only, NOT o3"
        );
        assert!(!o3mini.supports_system_messages, "o3-mini: developer role");

        let o1mini = model_capabilities("openai", "o1-mini");
        assert!(
            !o1mini.input_modalities.contains(&Modality::Image),
            "o1-mini: text only (never got vision update)"
        );
        assert_eq!(
            o1mini.tokenizer,
            Some(TokenizerFamily::O200k),
            "o1-mini: o200k (tiktoken model.py prefix o1-)"
        );

        // [6] openai-gpt4o-family — o200k (NOT cl100k).
        let gpt4o = model_capabilities("openai", "gpt-4o");
        assert_eq!(
            gpt4o.tokenizer,
            Some(TokenizerFamily::O200k),
            "gpt-4o: must match gpt4o-family rule (NOT gpt-4-family)"
        );
        assert_ne!(
            gpt4o.tokenizer,
            Some(TokenizerFamily::Cl100k),
            "ORDERING: if this fails, gpt4-family fired before gpt4o-family"
        );

        // [7] openai-gpt4p1-family — o200k (NOT cl100k).
        let gpt4p1 = model_capabilities("openai", "gpt-4.1");
        assert_eq!(
            gpt4p1.tokenizer,
            Some(TokenizerFamily::O200k),
            "gpt-4.1: o200k confirmed from tiktoken source"
        );
        assert_ne!(
            gpt4p1.tokenizer,
            Some(TokenizerFamily::Cl100k),
            "ORDERING: if this fails, gpt4-family fired before gpt4p1-family"
        );

        // [8] openai-gpt4-family — cl100k legacy.
        let gpt4 = model_capabilities("openai", "gpt-4");
        assert_eq!(
            gpt4.tokenizer,
            Some(TokenizerFamily::Cl100k),
            "gpt-4 legacy: cl100k (GPT-4 original, not 4.1/4o)"
        );
    }

    /// Session-2B per-rule provenance · [9] · claude on any provider (PDF + prompt-caching) — one
    /// (provider, model) probe per rule, failure names the rule.
    #[test]
    fn session_2b_claude_any_provider() {
        // [9] claude-family-any-provider — PDF + prompt-caching.
        let claude = model_capabilities("anthropic", "claude-sonnet-4-5-20250929");
        assert_eq!(claude.tokenizer, Some(TokenizerFamily::ClaudeV3));
        assert!(
            claude
                .supported_parameters
                .contains(&ParamFlag::PromptCaching),
            "prompt-caching: from claude-family rule"
        );
        assert_eq!(
            claude.json_mode,
            Some(JsonMode::Schema),
            "claude: json_schema enforcement"
        );
        assert!(
            claude
                .supported_parameters
                .contains(&ParamFlag::ThinkingBudget)
        );
        assert!(
            claude.supports_system_messages,
            "Claude: system messages YES"
        );
        assert!(
            claude.input_modalities.contains(&Modality::Pdf),
            "claude: PDF supported (all active models)"
        );
    }

    /// Session-2B per-rule provenance · [11]-[15] · deepseek json_object-only + the grok ladder — one
    /// (provider, model) probe per rule, failure names the rule.
    #[test]
    fn session_2b_deepseek_and_xai() {
        // [11-12] deepseek — json_object only (NOT json_schema).
        let deepseek = model_capabilities("deepseek", "deepseek-chat");
        assert!(
            !deepseek.input_modalities.contains(&Modality::Image),
            "deepseek: no vision API"
        );
        assert_eq!(
            deepseek.json_mode,
            Some(JsonMode::Object),
            "deepseek: json_object only (NOT json_schema)"
        );
        assert_eq!(deepseek.tokenizer, Some(TokenizerFamily::DeepSeek));

        let deepseek_r = model_capabilities("deepseek", "deepseek-reasoner");
        assert!(
            deepseek_r.reasoning,
            "deepseek-reasoner: reasoning=true (was MISSING pre-2b)"
        );
        assert_eq!(
            deepseek_r.json_mode,
            Some(JsonMode::Object),
            "deepseek-reasoner: json_object only"
        );

        // [13] xai-grok4 — reasoning, no temperature, no stop, grok tokenizer.
        let grok4 = model_capabilities("xai", "grok-4");
        assert!(grok4.reasoning, "grok-4: reasoning=true");
        assert!(!grok4.supports_temperature, "grok-4: no temperature");
        assert!(!grok4.supports_stop_sequences, "grok-4: no stop");
        assert_eq!(
            grok4.tokenizer,
            Some(TokenizerFamily::Grok),
            "grok-4: Grok tokenizer (Session 4b — was None)"
        );

        // [14] xai-grok3-mini — P0 FIX (reasoning model).
        let grok3mini = model_capabilities("xai", "grok-3-mini");
        assert!(
            grok3mini.reasoning,
            "grok-3-mini: P0 — IS a reasoning model"
        );
        assert!(!grok3mini.supports_temperature);
        assert!(
            !grok3mini.input_modalities.contains(&Modality::Image),
            "grok-3-mini: text-only"
        );
        assert!(
            grok3mini
                .supported_parameters
                .contains(&ParamFlag::ReasoningEffort),
            "grok-3-mini: supports reasoning_effort (unlike grok-4)"
        );

        // [15] xai-any-model — grok-3 text-only, not reasoning.
        let grok3 = model_capabilities("xai", "grok-3");
        assert!(!grok3.reasoning, "grok-3: not reasoning (grok-3-mini is)");
        assert!(
            !grok3.input_modalities.contains(&Modality::Image),
            "grok-3: text-only (MUST override default=true)"
        );
    }

    /// Session-2B per-rule provenance · [16]-[22] · gemini PDF split + mistral vision/reasoning — one
    /// (provider, model) probe per rule, failure names the rule.
    #[test]
    fn session_2b_gemini_and_mistral() {
        // [16-17] gemini — Pro has PDF, Flash does not.
        let gemini_pro = model_capabilities("gemini", "gemini-2.5-pro");
        assert!(
            gemini_pro.input_modalities.contains(&Modality::Pdf),
            "gemini-2.5-pro: PDF confirmed"
        );
        assert!(gemini_pro.input_modalities.contains(&Modality::Audio));
        assert!(gemini_pro.input_modalities.contains(&Modality::Video));

        let gemini_flash = model_capabilities("gemini", "gemini-2.5-flash");
        assert!(
            gemini_flash.reasoning,
            "gemini-2.5-flash thinks by default (B21 / issue 1305)"
        );
        assert!(
            gemini_flash.input_modalities.contains(&Modality::Video),
            "gemini-2.5-flash: video confirmed"
        );
        assert!(
            !gemini_flash.input_modalities.contains(&Modality::Pdf),
            "gemini-2.5-flash: PDF absent (Pro-only)"
        );

        // [19] mistral-small-3/4 — vision YES.
        let mistral_small = model_capabilities("mistral", "mistral-small-latest");
        assert!(
            mistral_small.input_modalities.contains(&Modality::Image),
            "mistral-small-latest: Small 4.0 vision confirmed"
        );
        let mistral_small_4 = model_capabilities("mistral", "mistral-small-4-0-26-03");
        assert!(
            mistral_small_4.input_modalities.contains(&Modality::Image),
            "mistral-small-4-0-26-03: prefix mistral-small-4 catches versioned ID"
        );

        // [21] magistral-family — reasoning + multimodal.
        let magistral = model_capabilities("mistral", "magistral-medium-latest");
        assert!(
            magistral.reasoning,
            "magistral: always-on reasoning model (confirmed docs.mistral.ai)"
        );
        assert!(
            magistral.input_modalities.contains(&Modality::Image),
            "magistral-medium: frontier multimodal reasoning"
        );

        // [22] mistral-any-model — codestral text-only + json_schema.
        let codestral = model_capabilities("mistral", "codestral-2508");
        assert!(
            !codestral.input_modalities.contains(&Modality::Image),
            "codestral: text-only"
        );
        assert_eq!(
            codestral.json_mode,
            Some(JsonMode::Schema),
            "codestral: json_schema on chat completions endpoint"
        );
    }

    /// Session-2B per-rule provenance · [23]+ · bedrock/openrouter prefixed claude routing — one
    /// (provider, model) probe per rule, failure names the rule.
    #[test]
    fn session_2b_aggregator_prefixes() {
        // [23] bedrock-claude — anthropic.claude-* prefix.
        let bedrock_claude = model_capabilities("bedrock", "anthropic.claude-sonnet-4-6-v1:0");
        assert_eq!(
            bedrock_claude.tokenizer,
            Some(TokenizerFamily::ClaudeV3),
            "bedrock-claude rule must fire (NOT defaults)"
        );
        assert!(bedrock_claude.input_modalities.contains(&Modality::Pdf));

        // [25] openrouter-claude — anthropic/claude-* prefix.
        let or_claude = model_capabilities("openrouter", "anthropic/claude-sonnet-4-6");
        assert_eq!(
            or_claude.tokenizer,
            Some(TokenizerFamily::ClaudeV3),
            "openrouter-claude rule must fire"
        );

        // [26] native-any — text-only local GGUF.
        let native = model_capabilities("native", "qwen-3-8b");
        assert!(
            !native.input_modalities.contains(&Modality::Image),
            "native: text-only override"
        );
        assert!(!native.input_modalities.contains(&Modality::Image));

        // [27] voyage-any — embedding-only.
        let voyage = model_capabilities("voyage", "voyage-3");
        assert!(
            !voyage.supports_system_messages,
            "voyage: no chat API, no system messages"
        );
        assert!(
            !voyage.input_modalities.contains(&Modality::Image),
            "voyage: embedding-only"
        );
        assert!(
            voyage.json_mode.is_none(),
            "voyage: no json_mode (embedding-only)"
        );

        // [28] mock-full — all flags.
        let mock = model_capabilities("mock", "any-model");
        assert_eq!(
            mock.json_mode,
            Some(JsonMode::Schema),
            "mock: json_schema enforcement"
        );
        assert!(
            mock.supported_parameters
                .contains(&ParamFlag::ThinkingBudget)
        );
        assert!(mock.supported_parameters.contains(&ParamFlag::WebSearch));
        assert!(mock.input_modalities.contains(&Modality::Audio));
        assert!(mock.input_modalities.contains(&Modality::Video));
        assert!(mock.input_modalities.contains(&Modality::Pdf));
    }

    /// Post-materialize invariant: `reasoning=true` MUST come with at
    /// least one reasoning parameter flag (`ReasoningEffort` or
    /// `ThinkingBudget`). A model that advertises reasoning but exposes
    /// no knob to control it is a TOML authoring bug.
    ///
    /// Note: magistral and grok-4 reason automatically (no knob) — both
    /// are excluded here and documented as known gaps. The invariant is
    /// enforced only on models where reasoning IS runtime-controllable.
    #[test]
    fn reasoning_models_have_controllable_parameter_flag() {
        for (prov, model) in [
            ("anthropic", "claude-sonnet-4-20250514"),
            ("openai", "o3"),
            ("openai", "o4-mini"),
            ("openai", "o3-mini"),
            ("xai", "grok-3-mini"),
            ("deepseek", "deepseek-reasoner"),
        ] {
            let c = model_capabilities(prov, model);
            if c.reasoning {
                let has_effort = c.supported_parameters.contains(&ParamFlag::ReasoningEffort);
                let has_budget = c.supported_parameters.contains(&ParamFlag::ThinkingBudget);
                // deepseek-reasoner has neither — reasoning is automatic,
                // exposed on the model endpoint. Document the known gap.
                if prov == "deepseek" {
                    continue;
                }
                assert!(
                    has_effort || has_budget,
                    "{prov}/{model}: reasoning=true but no ReasoningEffort / ThinkingBudget flag",
                );
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Proptest capabilities invariants — `models/capabilities_proptests.rs`
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod capabilities_proptests;
