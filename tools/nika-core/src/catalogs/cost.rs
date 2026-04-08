// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model pricing catalog — shared between daemon (EstimateCost) and LSP (inlay hints).
//!
//! Pure data module. No I/O, no async, no runtime dependencies.

use serde::{Deserialize, Serialize};

/// Pricing for a known model pattern.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPricing {
    pub provider: &'static str,
    pub model_pattern: &'static str,
    pub input_per_million: f64,
    pub output_per_million: f64,
}

/// Cost estimate result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostEstimate {
    pub usd: f64,
    pub input_rate_per_million: f64,
    pub output_rate_per_million: f64,
    pub model: String,
    pub provider: String,
}

/// Static pricing table — model patterns matched by exact name or `contains()`.
///
/// Two-pass matching: exact match first, then `contains()` fallback (first match wins).
/// More specific patterns MUST appear before less specific ones within each provider.
/// Keep in sync with nika-engine pricing tables.
pub static KNOWN_PRICING: &[ModelPricing] = &[
    // ═══════════════════════════════════════════════════════════
    // Anthropic
    // ═══════════════════════════════════════════════════════════
    // Claude 3 Haiku (old, cheaper pricing — must be before haiku-4)
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "claude-3-haiku",
        input_per_million: 0.25,
        output_per_million: 1.25,
    },
    // Pattern matches: opus-4, sonnet-4, haiku-4 cover all Claude 4.x variants
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "opus-4",
        input_per_million: 15.0,
        output_per_million: 75.0,
    },
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "sonnet-4",
        input_per_million: 3.0,
        output_per_million: 15.0,
    },
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "haiku-4",
        input_per_million: 0.8,
        output_per_million: 4.0,
    },
    // Claude 3.5 (matches via sonnet-4 and haiku-4 patterns won't work, so explicit)
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "claude-3-5-sonnet",
        input_per_million: 3.0,
        output_per_million: 15.0,
    },
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "claude-3-5-haiku",
        input_per_million: 0.8,
        output_per_million: 4.0,
    },
    // Claude 3 legacy
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "claude-3-opus",
        input_per_million: 15.0,
        output_per_million: 75.0,
    },
    ModelPricing {
        provider: "Anthropic",
        model_pattern: "claude-3-sonnet",
        input_per_million: 3.0,
        output_per_million: 15.0,
    },
    // ═══════════════════════════════════════════════════════════
    // OpenAI — more specific patterns BEFORE less specific
    // ═══════════════════════════════════════════════════════════
    // OpenAI: ORDERING CRITICAL for contains() fallback.
    // gpt-4o-mini before gpt-4o, gpt-4.1-* before gpt-4-turbo before gpt-4
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4o-mini",
        input_per_million: 0.15,
        output_per_million: 0.6,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4o",
        input_per_million: 2.5,
        output_per_million: 10.0,
    },
    // GPT-4.1 variants MUST be before gpt-4 (contains "gpt-4")
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4.1-nano",
        input_per_million: 0.1,
        output_per_million: 0.4,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4.1-mini",
        input_per_million: 0.4,
        output_per_million: 1.6,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4.1",
        input_per_million: 2.0,
        output_per_million: 8.0,
    },
    // GPT-3.5 Turbo (before gpt-4 since it contains "gpt-3")
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-3.5-turbo",
        input_per_million: 0.5,
        output_per_million: 1.5,
    },
    // GPT-4 Turbo (before gpt-4 base)
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4-turbo",
        input_per_million: 10.0,
        output_per_million: 30.0,
    },
    // GPT-4 base (catch-all for gpt-4-0613 etc.)
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-4",
        input_per_million: 30.0,
        output_per_million: 60.0,
    },
    // GPT-5 family: most specific patterns first (pro before base, newer before older)
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-5.4-nano",
        input_per_million: 0.10,
        output_per_million: 0.40,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-5.4-mini",
        input_per_million: 0.40,
        output_per_million: 1.60,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-5.4",
        input_per_million: 2.00,
        output_per_million: 8.00,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-5.2-pro",
        input_per_million: 21.0,
        output_per_million: 168.0,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-5.2",
        input_per_million: 1.75,
        output_per_million: 14.0,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "gpt-5",
        input_per_million: 1.75,
        output_per_million: 14.0,
    },
    // o1-mini before o1
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "o1-mini",
        input_per_million: 3.0,
        output_per_million: 12.0,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "o1",
        input_per_million: 15.0,
        output_per_million: 60.0,
    },
    // o3-mini before o3, o4-mini
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "o3-mini",
        input_per_million: 1.1,
        output_per_million: 4.4,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "o4-mini",
        input_per_million: 1.1,
        output_per_million: 4.4,
    },
    ModelPricing {
        provider: "OpenAI",
        model_pattern: "o3",
        input_per_million: 2.0,
        output_per_million: 8.0,
    },
    // ═══════════════════════════════════════════════════════════
    // Mistral — specific models before patterns
    // ═══════════════════════════════════════════════════════════
    ModelPricing {
        provider: "Mistral",
        model_pattern: "mistral-large",
        input_per_million: 2.0,
        output_per_million: 6.0,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "mistral-medium",
        input_per_million: 2.7,
        output_per_million: 8.1,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "mistral-small",
        input_per_million: 0.2,
        output_per_million: 0.6,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "codestral",
        input_per_million: 0.3,
        output_per_million: 0.9,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "ministral-8b",
        input_per_million: 0.1,
        output_per_million: 0.1,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "ministral-3b",
        input_per_million: 0.04,
        output_per_million: 0.04,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "pixtral-large",
        input_per_million: 2.0,
        output_per_million: 6.0,
    },
    ModelPricing {
        provider: "Mistral",
        model_pattern: "pixtral-12b",
        input_per_million: 0.15,
        output_per_million: 0.15,
    },
    // ═══════════════════════════════════════════════════════════
    // Groq — specdec before versatile (different output pricing)
    // ═══════════════════════════════════════════════════════════
    ModelPricing {
        provider: "Groq",
        model_pattern: "llama-3.3-70b-specdec",
        input_per_million: 0.59,
        output_per_million: 0.99,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "llama-3.3-70b",
        input_per_million: 0.59,
        output_per_million: 0.79,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "llama-3.1-70b",
        input_per_million: 0.59,
        output_per_million: 0.79,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "llama-3.1-8b",
        input_per_million: 0.05,
        output_per_million: 0.08,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "llama3-70b",
        input_per_million: 0.59,
        output_per_million: 0.79,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "llama3-8b",
        input_per_million: 0.05,
        output_per_million: 0.08,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "mixtral-8x7b",
        input_per_million: 0.24,
        output_per_million: 0.24,
    },
    ModelPricing {
        provider: "Groq",
        model_pattern: "gemma2-9b",
        input_per_million: 0.20,
        output_per_million: 0.20,
    },
    // ═══════════════════════════════════════════════════════════
    // DeepSeek
    // ═══════════════════════════════════════════════════════════
    ModelPricing {
        provider: "DeepSeek",
        model_pattern: "deepseek-chat",
        input_per_million: 0.14,
        output_per_million: 0.28,
    },
    ModelPricing {
        provider: "DeepSeek",
        model_pattern: "deepseek-reasoner",
        input_per_million: 0.55,
        output_per_million: 2.19,
    },
    ModelPricing {
        provider: "DeepSeek",
        model_pattern: "deepseek-coder",
        input_per_million: 0.14,
        output_per_million: 0.28,
    },
    // ═══════════════════════════════════════════════════════════
    // Gemini — more specific before less specific
    // ═══════════════════════════════════════════════════════════
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-2.5-flash",
        input_per_million: 0.15,
        output_per_million: 0.6,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-2.5-pro",
        input_per_million: 1.25,
        output_per_million: 10.0,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-2.0-flash-exp",
        input_per_million: 0.0,
        output_per_million: 0.0,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-2.0-flash-thinking",
        input_per_million: 0.0,
        output_per_million: 0.0,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-2.0-flash",
        input_per_million: 0.1,
        output_per_million: 0.4,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-1.5-flash-8b",
        input_per_million: 0.0375,
        output_per_million: 0.15,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-1.5-flash",
        input_per_million: 0.075,
        output_per_million: 0.3,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-1.5-pro",
        input_per_million: 1.25,
        output_per_million: 5.0,
    },
    ModelPricing {
        provider: "Gemini",
        model_pattern: "gemini-pro",
        input_per_million: 0.5,
        output_per_million: 1.5,
    },
    // ═══════════════════════════════════════════════════════════
    // xAI — more specific before less specific (grok-4 before grok-3)
    // ═══════════════════════════════════════════════════════════
    ModelPricing {
        provider: "xAI",
        model_pattern: "grok-4",
        input_per_million: 3.0,
        output_per_million: 15.0,
    },
    ModelPricing {
        provider: "xAI",
        model_pattern: "grok-3-mini-fast",
        input_per_million: 0.1,
        output_per_million: 0.4,
    },
    ModelPricing {
        provider: "xAI",
        model_pattern: "grok-3-mini",
        input_per_million: 0.3,
        output_per_million: 0.5,
    },
    ModelPricing {
        provider: "xAI",
        model_pattern: "grok-3-fast",
        input_per_million: 0.6,
        output_per_million: 4.0,
    },
    ModelPricing {
        provider: "xAI",
        model_pattern: "grok-3",
        input_per_million: 3.0,
        output_per_million: 15.0,
    },
    ModelPricing {
        provider: "xAI",
        model_pattern: "grok-2",
        input_per_million: 2.0,
        output_per_million: 10.0,
    },
];

/// Find pricing for a model.
///
/// Two-pass matching:
/// 1. Exact match on `model_pattern` (for models like "gpt-4o-mini")
/// 2. `contains()` fallback (for dated variants like "claude-sonnet-4-20250514")
pub fn find_pricing(model: &str) -> Option<&'static ModelPricing> {
    // Pass 1: exact match
    if let Some(p) = KNOWN_PRICING.iter().find(|p| model == p.model_pattern) {
        return Some(p);
    }
    // Pass 2: contains() fallback (first match wins — ordering matters)
    KNOWN_PRICING
        .iter()
        .find(|p| model.contains(p.model_pattern))
}

/// Estimate cost for a model invocation.
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

/// Get a formatted cost label for a model (used by inlay hints).
///
/// Returns `None` for unknown models. Format: `" (Provider · $in/$out per 1M)"`
pub fn model_cost_label(model: &str) -> Option<String> {
    let pricing = find_pricing(model)?;
    Some(format!(
        " ({} \u{00b7} ${}/{})",
        pricing.provider,
        format_price(pricing.input_per_million),
        format_price(pricing.output_per_million),
    ))
}

fn format_price(price: f64) -> String {
    if price >= 1.0 {
        format!("{}", price)
    } else {
        format!("{:.2}", price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_pricing_anthropic_sonnet() {
        let p = find_pricing("claude-sonnet-4-20250514").unwrap();
        assert_eq!(p.provider, "Anthropic");
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_million - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn find_pricing_anthropic_opus() {
        let p = find_pricing("claude-opus-4-20250514").unwrap();
        assert_eq!(p.provider, "Anthropic");
        assert!((p.input_per_million - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn find_pricing_openai_gpt4o() {
        let p = find_pricing("gpt-4o").unwrap();
        assert_eq!(p.provider, "OpenAI");
        assert!((p.input_per_million - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn find_pricing_openai_gpt4o_mini() {
        let p = find_pricing("gpt-4o-mini").unwrap();
        assert_eq!(p.provider, "OpenAI");
        assert!((p.input_per_million - 0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn find_pricing_anthropic_haiku() {
        // Must match both short and dated model IDs
        let p = find_pricing("claude-haiku-4-5-20251001").unwrap();
        assert_eq!(p.provider, "Anthropic");
        assert!((p.input_per_million - 0.8).abs() < f64::EPSILON);

        let p2 = find_pricing("claude-haiku-4-5").unwrap();
        assert_eq!(p2.provider, "Anthropic");
    }

    #[test]
    fn find_pricing_unknown_returns_none() {
        assert!(find_pricing("some-random-model").is_none());
    }

    #[test]
    fn estimate_cost_sonnet_1k_tokens() {
        let est = estimate_cost("claude-sonnet-4-20250514", 1000, 500).unwrap();
        // (1000 * 3.0 + 500 * 15.0) / 1_000_000 = (3000 + 7500) / 1_000_000 = 0.0105
        assert!((est.usd - 0.0105).abs() < 1e-10);
        assert_eq!(est.provider, "Anthropic");
        assert_eq!(est.model, "claude-sonnet-4-20250514");
    }

    #[test]
    fn estimate_cost_unknown_returns_none() {
        assert!(estimate_cost("mystery-model", 1000, 500).is_none());
    }

    #[test]
    fn model_cost_label_sonnet_format() {
        let label = model_cost_label("claude-sonnet-4-20250514").unwrap();
        assert!(label.contains("Anthropic"));
        assert!(label.contains("$3"));
        assert!(label.contains("15"));
    }

    #[test]
    fn model_cost_label_gpt4o_mini_format() {
        let label = model_cost_label("gpt-4o-mini").unwrap();
        assert!(label.contains("OpenAI"));
        assert!(label.contains("$0.15"));
    }

    #[test]
    fn model_cost_label_unknown() {
        assert!(model_cost_label("random-model").is_none());
    }

    #[test]
    fn cost_estimate_serde_roundtrip() {
        let est = CostEstimate {
            usd: 0.018,
            input_rate_per_million: 3.0,
            output_rate_per_million: 15.0,
            model: "claude-sonnet-4-20250514".into(),
            provider: "anthropic".into(),
        };
        let json = serde_json::to_string(&est).unwrap();
        let back: CostEstimate = serde_json::from_str(&json).unwrap();
        assert_eq!(est, back);
    }

    #[test]
    fn known_pricing_count() {
        assert!(
            KNOWN_PRICING.len() >= 50,
            "Expected at least 50 pricing entries, got {}",
            KNOWN_PRICING.len()
        );
    }

    #[test]
    fn gpt5_pricing() {
        let p = find_pricing("gpt-5.2").unwrap();
        assert!((p.input_per_million - 1.75).abs() < f64::EPSILON);
        assert!((p.output_per_million - 14.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gpt5_pro_pricing() {
        let p = find_pricing("gpt-5.2-pro").unwrap();
        assert!((p.input_per_million - 21.0).abs() < f64::EPSILON);
        assert!((p.output_per_million - 168.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gpt5_dated_variant_fallback() {
        // gpt-5.2-2025-12-11 should match gpt-5.2 via contains() fallback
        let p = find_pricing("gpt-5.2-2025-12-11").unwrap();
        assert!((p.input_per_million - 1.75).abs() < f64::EPSILON);
    }

    #[test]
    fn gpt54_pricing() {
        let base = find_pricing("gpt-5.4").unwrap();
        let mini = find_pricing("gpt-5.4-mini").unwrap();
        let nano = find_pricing("gpt-5.4-nano").unwrap();
        assert!(nano.input_per_million < mini.input_per_million);
        assert!(mini.input_per_million < base.input_per_million);
    }

    #[test]
    fn grok4_pricing() {
        let p = find_pricing("grok-4").unwrap();
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_million - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gpt41_variants_differentiated() {
        let nano = find_pricing("gpt-4.1-nano").unwrap();
        let mini = find_pricing("gpt-4.1-mini").unwrap();
        let base = find_pricing("gpt-4.1").unwrap();
        assert!(nano.input_per_million < mini.input_per_million);
        assert!(mini.input_per_million < base.input_per_million);
    }

    // ─────────────────────────────────────────────────────────────
    // Two-pass matching tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn exact_match_takes_priority() {
        // "gpt-4o-mini" should match the exact entry, not "gpt-4o" via contains()
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
    fn specdec_different_from_versatile() {
        let specdec = find_pricing("llama-3.3-70b-specdec").unwrap();
        let versatile = find_pricing("llama-3.3-70b-versatile").unwrap();
        assert!(
            (specdec.output_per_million - 0.99).abs() < f64::EPSILON,
            "specdec should have 0.99 output"
        );
        assert!(
            (versatile.output_per_million - 0.79).abs() < f64::EPSILON,
            "versatile should have 0.79 output"
        );
    }

    // ─────────────────────────────────────────────────────────────
    // New model coverage tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn claude_3_haiku_old_pricing() {
        let p = find_pricing("claude-3-haiku-20240307").unwrap();
        assert!((p.input_per_million - 0.25).abs() < f64::EPSILON);
    }

    #[test]
    fn gpt4_turbo_pricing() {
        let p = find_pricing("gpt-4-turbo").unwrap();
        assert!((p.input_per_million - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gpt35_turbo_pricing() {
        let p = find_pricing("gpt-3.5-turbo").unwrap();
        assert!((p.input_per_million - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn o1_mini_pricing() {
        let p = find_pricing("o1-mini").unwrap();
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_million - 12.0).abs() < f64::EPSILON);
    }

    #[test]
    fn mistral_medium_pricing() {
        let p = find_pricing("mistral-medium-latest").unwrap();
        assert!((p.input_per_million - 2.7).abs() < f64::EPSILON);
    }

    #[test]
    fn codestral_pricing() {
        let p = find_pricing("codestral-latest").unwrap();
        assert!((p.input_per_million - 0.3).abs() < f64::EPSILON);
    }

    #[test]
    fn mixtral_pricing() {
        let p = find_pricing("mixtral-8x7b-32768").unwrap();
        assert!((p.input_per_million - 0.24).abs() < f64::EPSILON);
    }

    #[test]
    fn deepseek_coder_pricing() {
        let p = find_pricing("deepseek-coder").unwrap();
        assert!((p.input_per_million - 0.14).abs() < f64::EPSILON);
    }

    #[test]
    fn gemini_20_flash_pricing() {
        let p = find_pricing("gemini-2.0-flash").unwrap();
        assert!((p.input_per_million - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn gemini_15_pro_pricing() {
        let p = find_pricing("gemini-1.5-pro").unwrap();
        assert!((p.input_per_million - 1.25).abs() < f64::EPSILON);
        assert!((p.output_per_million - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn grok_3_fast_pricing() {
        let p = find_pricing("grok-3-fast").unwrap();
        assert!((p.input_per_million - 0.6).abs() < f64::EPSILON);
    }

    #[test]
    fn grok_2_pricing() {
        let p = find_pricing("grok-2").unwrap();
        assert!((p.input_per_million - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn gemini_free_preview() {
        let p = find_pricing("gemini-2.0-flash-exp").unwrap();
        assert!((p.input_per_million - 0.0).abs() < f64::EPSILON);
    }

    // ─────────────────────────────────────────────────────────────
    // Contract tests — capabilities + pricing consistency
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn all_openai_reasoning_models_have_pricing() {
        use crate::catalogs::capabilities::{model_capabilities, TokenLimitParam};
        let reasoning_models = [
            "o1",
            "o3",
            "o3-mini",
            "o3-pro",
            "o4-mini",
            "gpt-5",
            "gpt-5.2",
            "gpt-5.2-pro",
            "gpt-5.4",
            "gpt-5.4-mini",
            "gpt-5.4-nano",
        ];
        for model in reasoning_models {
            let caps = model_capabilities("openai", model);
            assert_eq!(
                caps.token_limit_param,
                TokenLimitParam::MaxCompletionTokens,
                "'{model}' should use max_completion_tokens"
            );
            assert!(
                find_pricing(model).is_some(),
                "Missing pricing for reasoning model '{model}'"
            );
        }
    }

    #[test]
    fn standard_models_use_max_tokens() {
        use crate::catalogs::capabilities::{model_capabilities, TokenLimitParam};
        let standard_models = [
            ("openai", "gpt-4o"),
            ("openai", "gpt-4.1"),
            ("anthropic", "claude-sonnet-4-6"),
            ("gemini", "gemini-2.5-flash"),
            ("mistral", "mistral-large-latest"),
            ("deepseek", "deepseek-chat"),
        ];
        for (provider, model) in standard_models {
            let caps = model_capabilities(provider, model);
            assert_eq!(
                caps.token_limit_param,
                TokenLimitParam::MaxTokens,
                "'{model}' on '{provider}' should use max_tokens"
            );
        }
    }
}
