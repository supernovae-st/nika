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

/// Static pricing table — 23 known model patterns.
///
/// Matching is done via `contains()` or exact match, checked in order (first match wins).
/// Keep in sync with provider documentation.
pub static KNOWN_PRICING: &[ModelPricing] = &[
    // Anthropic
    ModelPricing { provider: "Anthropic", model_pattern: "opus-4", input_per_million: 15.0, output_per_million: 75.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "sonnet-4", input_per_million: 3.0, output_per_million: 15.0 },
    ModelPricing { provider: "Anthropic", model_pattern: "haiku-3.5", input_per_million: 0.8, output_per_million: 4.0 },
    // OpenAI
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4o-mini", input_per_million: 0.15, output_per_million: 0.6 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4o", input_per_million: 2.5, output_per_million: 10.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4.1-nano", input_per_million: 0.1, output_per_million: 0.4 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4.1-mini", input_per_million: 0.4, output_per_million: 1.6 },
    ModelPricing { provider: "OpenAI", model_pattern: "gpt-4.1", input_per_million: 2.0, output_per_million: 8.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "o1", input_per_million: 15.0, output_per_million: 60.0 },
    ModelPricing { provider: "OpenAI", model_pattern: "o3-mini", input_per_million: 1.1, output_per_million: 4.4 },
    ModelPricing { provider: "OpenAI", model_pattern: "o4-mini", input_per_million: 1.1, output_per_million: 4.4 },
    ModelPricing { provider: "OpenAI", model_pattern: "o3", input_per_million: 10.0, output_per_million: 40.0 },
    // Mistral
    ModelPricing { provider: "Mistral", model_pattern: "mistral-large", input_per_million: 2.0, output_per_million: 6.0 },
    ModelPricing { provider: "Mistral", model_pattern: "mistral-small", input_per_million: 0.2, output_per_million: 0.6 },
    // Groq (llama models)
    ModelPricing { provider: "Groq", model_pattern: "llama-3.3-70b", input_per_million: 0.59, output_per_million: 0.79 },
    ModelPricing { provider: "Groq", model_pattern: "llama-3.1-8b", input_per_million: 0.05, output_per_million: 0.08 },
    // DeepSeek
    ModelPricing { provider: "DeepSeek", model_pattern: "deepseek-chat", input_per_million: 0.14, output_per_million: 0.28 },
    ModelPricing { provider: "DeepSeek", model_pattern: "deepseek-reasoner", input_per_million: 0.55, output_per_million: 2.19 },
    // Gemini
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.5-flash", input_per_million: 0.15, output_per_million: 0.6 },
    ModelPricing { provider: "Gemini", model_pattern: "gemini-2.5-pro", input_per_million: 1.25, output_per_million: 10.0 },
    // xAI
    ModelPricing { provider: "xAI", model_pattern: "grok-3-mini", input_per_million: 0.3, output_per_million: 0.5 },
    ModelPricing { provider: "xAI", model_pattern: "grok-3", input_per_million: 3.0, output_per_million: 15.0 },
];

/// Find pricing for a model by pattern matching.
///
/// Returns the first match from [`KNOWN_PRICING`]. Matching uses `contains()`.
pub fn find_pricing(model: &str) -> Option<&'static ModelPricing> {
    KNOWN_PRICING.iter().find(|p| model.contains(p.model_pattern))
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
        assert!(KNOWN_PRICING.len() >= 22, "Expected at least 22 pricing entries");
    }

    #[test]
    fn gpt41_variants_differentiated() {
        let nano = find_pricing("gpt-4.1-nano").unwrap();
        let mini = find_pricing("gpt-4.1-mini").unwrap();
        let base = find_pricing("gpt-4.1").unwrap();
        assert!(nano.input_per_million < mini.input_per_million);
        assert!(mini.input_per_million < base.input_per_million);
    }
}
