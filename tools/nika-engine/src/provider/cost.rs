//! Cost calculation for LLM providers
//!
//! Provides pricing tables and cost calculation for all supported providers.
//! Prices are in USD per million tokens.
//!
//! ## Usage
//!
//! ```rust,ignore
//! use nika::provider::cost::{calculate_cost, ProviderKind};
//!
//! // Calculate cost for 1000 input tokens and 500 output tokens
//! let cost = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", 1000, 500);
//! println!("Cost: ${:.6}", cost);
//! ```

use std::collections::HashMap;
use std::sync::LazyLock;

// ═══════════════════════════════════════════════════════════════════════════
// PROVIDER KIND
// ═══════════════════════════════════════════════════════════════════════════

/// Provider kind for cost calculation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    Claude,
    OpenAI,
    Mistral,
    Groq,
    DeepSeek,
    Gemini,
    /// xAI (Grok) provider
    XAi,
    /// Native local provider - GGUF models via mistral.rs
    Native,
}

impl ProviderKind {
    /// Parse provider kind from string (case-insensitive)
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "claude" | "anthropic" => Some(Self::Claude),
            "openai" | "gpt" => Some(Self::OpenAI),
            "mistral" => Some(Self::Mistral),
            "groq" => Some(Self::Groq),
            "deepseek" => Some(Self::DeepSeek),
            "gemini" | "google" => Some(Self::Gemini),
            "xai" | "grok" | "x-ai" => Some(Self::XAi),
            "native" => Some(Self::Native),
            _ => None,
        }
    }

    /// Get provider name
    pub fn name(&self) -> &'static str {
        match self {
            Self::Claude => "Claude",
            Self::OpenAI => "OpenAI",
            Self::Mistral => "Mistral",
            Self::Groq => "Groq",
            Self::DeepSeek => "DeepSeek",
            Self::Gemini => "Gemini",
            Self::XAi => "xAI",
            Self::Native => "Native",
        }
    }

    /// Check if provider is free (local or free tier)
    pub fn is_free(&self) -> bool {
        matches!(self, Self::Native)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL PRICING
// ═══════════════════════════════════════════════════════════════════════════

/// Pricing for a model (per million tokens)
#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    /// Cost per million input tokens
    pub input_per_million: f64,
    /// Cost per million output tokens
    pub output_per_million: f64,
}

impl ModelPricing {
    /// Create new pricing
    pub const fn new(input_per_million: f64, output_per_million: f64) -> Self {
        Self {
            input_per_million,
            output_per_million,
        }
    }

    /// Calculate cost for given token counts
    ///
    /// Returns 0.0 if the result is NaN or Infinity (prevents NDJSON serialization failures).
    pub fn calculate(&self, input_tokens: u64, output_tokens: u64) -> f64 {
        let cost = (input_tokens as f64 / 1_000_000.0) * self.input_per_million
            + (output_tokens as f64 / 1_000_000.0) * self.output_per_million;
        if cost.is_finite() {
            cost
        } else {
            0.0
        }
    }

    /// Calculate cost with cached token discount (cached tokens at 10% of input rate).
    pub fn calculate_with_cache(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cached_tokens: u64,
    ) -> f64 {
        let effective_input = input_tokens.saturating_sub(cached_tokens);
        let cached_cost = (cached_tokens as f64 / 1_000_000.0) * self.input_per_million * 0.1;
        let input_cost = (effective_input as f64 / 1_000_000.0) * self.input_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_million;
        let cost = cached_cost + input_cost + output_cost;
        if cost.is_finite() { cost } else { 0.0 }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PRICING TABLES (per million tokens, as of March 2025)
// ═══════════════════════════════════════════════════════════════════════════

/// Claude models pricing
static CLAUDE_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Claude 4 Opus
    m.insert("claude-opus-4-20250514", ModelPricing::new(15.0, 75.0));
    m.insert("claude-opus-4", ModelPricing::new(15.0, 75.0));
    // Claude 4 Sonnet (latest)
    m.insert("claude-sonnet-4-20250514", ModelPricing::new(3.0, 15.0));
    m.insert("claude-sonnet-4-6", ModelPricing::new(3.0, 15.0));
    m.insert("claude-sonnet-4", ModelPricing::new(3.0, 15.0));
    // Claude 3.5 Sonnet
    m.insert("claude-3-5-sonnet-20241022", ModelPricing::new(3.0, 15.0));
    m.insert("claude-3-5-sonnet-latest", ModelPricing::new(3.0, 15.0));
    // Claude 3.5 Haiku
    m.insert("claude-3-5-haiku-20241022", ModelPricing::new(0.8, 4.0));
    m.insert("claude-3-5-haiku-latest", ModelPricing::new(0.8, 4.0));
    // Claude 3 Opus
    m.insert("claude-3-opus-20240229", ModelPricing::new(15.0, 75.0));
    m.insert("claude-3-opus-latest", ModelPricing::new(15.0, 75.0));
    // Claude 3 Sonnet
    m.insert("claude-3-sonnet-20240229", ModelPricing::new(3.0, 15.0));
    // Claude 3 Haiku
    m.insert("claude-3-haiku-20240307", ModelPricing::new(0.25, 1.25));
    m
});

/// OpenAI models pricing
static OPENAI_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // GPT-4o
    m.insert("gpt-4o", ModelPricing::new(2.5, 10.0));
    m.insert("gpt-4o-2024-11-20", ModelPricing::new(2.5, 10.0));
    m.insert("gpt-4o-mini", ModelPricing::new(0.15, 0.6));
    m.insert("gpt-4o-mini-2024-07-18", ModelPricing::new(0.15, 0.6));
    // GPT-4 Turbo
    m.insert("gpt-4-turbo", ModelPricing::new(10.0, 30.0));
    m.insert("gpt-4-turbo-2024-04-09", ModelPricing::new(10.0, 30.0));
    m.insert("gpt-4-turbo-preview", ModelPricing::new(10.0, 30.0));
    // GPT-4
    m.insert("gpt-4", ModelPricing::new(30.0, 60.0));
    m.insert("gpt-4-0613", ModelPricing::new(30.0, 60.0));
    // GPT-3.5 Turbo
    m.insert("gpt-3.5-turbo", ModelPricing::new(0.5, 1.5));
    m.insert("gpt-3.5-turbo-0125", ModelPricing::new(0.5, 1.5));
    // o1
    m.insert("o1", ModelPricing::new(15.0, 60.0));
    m.insert("o1-2024-12-17", ModelPricing::new(15.0, 60.0));
    m.insert("o1-preview", ModelPricing::new(15.0, 60.0));
    m.insert("o1-mini", ModelPricing::new(3.0, 12.0));
    m.insert("o1-mini-2024-09-12", ModelPricing::new(3.0, 12.0));
    // GPT-4.1
    m.insert("gpt-4.1", ModelPricing::new(2.0, 8.0));
    m.insert("gpt-4.1-2025-04-14", ModelPricing::new(2.0, 8.0));
    m.insert("gpt-4.1-mini", ModelPricing::new(0.4, 1.6));
    m.insert("gpt-4.1-mini-2025-04-14", ModelPricing::new(0.4, 1.6));
    m.insert("gpt-4.1-nano", ModelPricing::new(0.1, 0.4));
    m.insert("gpt-4.1-nano-2025-04-14", ModelPricing::new(0.1, 0.4));
    // o3-mini
    m.insert("o3-mini", ModelPricing::new(1.1, 4.4));
    m.insert("o3-mini-2025-01-31", ModelPricing::new(1.1, 4.4));
    m
});

/// Mistral models pricing
static MISTRAL_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Large
    m.insert("mistral-large-latest", ModelPricing::new(2.0, 6.0));
    m.insert("mistral-large-2411", ModelPricing::new(2.0, 6.0));
    // Medium (deprecated but still available)
    m.insert("mistral-medium-latest", ModelPricing::new(2.7, 8.1));
    // Small
    m.insert("mistral-small-latest", ModelPricing::new(0.2, 0.6));
    m.insert("mistral-small-2409", ModelPricing::new(0.2, 0.6));
    // Codestral
    m.insert("codestral-latest", ModelPricing::new(0.3, 0.9));
    m.insert("codestral-2501", ModelPricing::new(0.3, 0.9));
    // Ministral
    m.insert("ministral-8b-latest", ModelPricing::new(0.1, 0.1));
    m.insert("ministral-3b-latest", ModelPricing::new(0.04, 0.04));
    // Pixtral
    m.insert("pixtral-large-latest", ModelPricing::new(2.0, 6.0));
    m.insert("pixtral-12b-2409", ModelPricing::new(0.15, 0.15));
    m
});

/// Groq models pricing (very low cost)
static GROQ_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Llama 3.3
    m.insert("llama-3.3-70b-versatile", ModelPricing::new(0.59, 0.79));
    m.insert("llama-3.3-70b-specdec", ModelPricing::new(0.59, 0.99));
    // Llama 3.1
    m.insert("llama-3.1-70b-versatile", ModelPricing::new(0.59, 0.79));
    m.insert("llama-3.1-8b-instant", ModelPricing::new(0.05, 0.08));
    // Llama 3
    m.insert("llama3-70b-8192", ModelPricing::new(0.59, 0.79));
    m.insert("llama3-8b-8192", ModelPricing::new(0.05, 0.08));
    // Mixtral
    m.insert("mixtral-8x7b-32768", ModelPricing::new(0.24, 0.24));
    // Gemma
    m.insert("gemma2-9b-it", ModelPricing::new(0.20, 0.20));
    m
});

/// DeepSeek models pricing
static DEEPSEEK_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    m.insert("deepseek-chat", ModelPricing::new(0.14, 0.28));
    m.insert("deepseek-reasoner", ModelPricing::new(0.55, 2.19));
    m.insert("deepseek-coder", ModelPricing::new(0.14, 0.28));
    m
});

/// Gemini models pricing
static GEMINI_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Gemini 2.0
    m.insert("gemini-2.0-flash", ModelPricing::new(0.1, 0.4));
    m.insert("gemini-2.0-flash-exp", ModelPricing::new(0.0, 0.0)); // Free preview
    m.insert("gemini-2.0-flash-thinking", ModelPricing::new(0.0, 0.0)); // Free preview
                                                                        // Gemini 1.5
    m.insert("gemini-1.5-pro", ModelPricing::new(1.25, 5.0));
    m.insert("gemini-1.5-pro-latest", ModelPricing::new(1.25, 5.0));
    m.insert("gemini-1.5-flash", ModelPricing::new(0.075, 0.3));
    m.insert("gemini-1.5-flash-latest", ModelPricing::new(0.075, 0.3));
    m.insert("gemini-1.5-flash-8b", ModelPricing::new(0.0375, 0.15));
    // Gemini 1.0 Pro
    m.insert("gemini-pro", ModelPricing::new(0.5, 1.5));
    m
});

/// xAI (Grok) models pricing
static XAI_PRICING: LazyLock<HashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = HashMap::new();
    // Grok-3
    m.insert("grok-3", ModelPricing::new(3.0, 15.0));
    // Grok-3 Fast
    m.insert("grok-3-fast", ModelPricing::new(0.6, 4.0));
    // Grok-3 Mini
    m.insert("grok-3-mini", ModelPricing::new(0.3, 0.5));
    // Grok-3 Mini Fast
    m.insert("grok-3-mini-fast", ModelPricing::new(0.1, 0.4));
    // Grok-2
    m.insert("grok-2", ModelPricing::new(2.0, 10.0));
    m
});

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT PRICING (fallback)
// ═══════════════════════════════════════════════════════════════════════════

/// Default pricing when model is unknown (conservative estimate)
const DEFAULT_PRICING: ModelPricing = ModelPricing::new(5.0, 15.0);

/// Free pricing for local models
const FREE_PRICING: ModelPricing = ModelPricing::new(0.0, 0.0);

// ═══════════════════════════════════════════════════════════════════════════
// COST CALCULATION
// ═══════════════════════════════════════════════════════════════════════════

/// Get pricing for a specific model
pub fn get_model_pricing(provider: ProviderKind, model: &str) -> ModelPricing {
    if provider.is_free() {
        return FREE_PRICING;
    }

    let pricing = match provider {
        ProviderKind::Claude => CLAUDE_PRICING.get(model),
        ProviderKind::OpenAI => OPENAI_PRICING.get(model),
        ProviderKind::Mistral => MISTRAL_PRICING.get(model),
        ProviderKind::Groq => GROQ_PRICING.get(model),
        ProviderKind::DeepSeek => DEEPSEEK_PRICING.get(model),
        ProviderKind::Gemini => GEMINI_PRICING.get(model),
        ProviderKind::XAi => XAI_PRICING.get(model),
        ProviderKind::Native => return FREE_PRICING,
    };

    pricing.copied().unwrap_or(DEFAULT_PRICING)
}

/// Calculate cost for a given number of tokens
///
/// # Arguments
///
/// * `provider` - The provider kind
/// * `model` - The model identifier
/// * `input_tokens` - Number of input tokens
/// * `output_tokens` - Number of output tokens
///
/// # Returns
///
/// Cost in USD
pub fn calculate_cost(
    provider: ProviderKind,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
) -> f64 {
    let pricing = get_model_pricing(provider, model);
    pricing.calculate(input_tokens, output_tokens)
}

/// Calculate cost with cached token discount.
pub fn calculate_cost_with_cache(
    provider: ProviderKind,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
) -> f64 {
    let pricing = get_model_pricing(provider, model);
    pricing.calculate_with_cache(input_tokens, output_tokens, cached_tokens)
}

/// Estimate cost before execution based on estimated tokens
///
/// # Arguments
///
/// * `provider` - The provider kind
/// * `model` - The model identifier
/// * `estimated_input` - Estimated input tokens
/// * `estimated_output` - Estimated output tokens
///
/// # Returns
///
/// Estimated cost in USD
pub fn estimate_cost(
    provider: ProviderKind,
    model: &str,
    estimated_input: u64,
    estimated_output: u64,
) -> f64 {
    calculate_cost(provider, model, estimated_input, estimated_output)
}

/// Format cost as USD string with appropriate precision
pub fn format_cost(cost: f64) -> String {
    if cost < 0.0001 {
        format!("${:.6}", cost)
    } else if cost < 0.01 {
        format!("${:.4}", cost)
    } else if cost < 1.0 {
        format!("${:.3}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ───────────────────────────────────────────────────────────────────────
    // ProviderKind tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn provider_from_str_claude() {
        assert_eq!(ProviderKind::parse("claude"), Some(ProviderKind::Claude));
        assert_eq!(ProviderKind::parse("anthropic"), Some(ProviderKind::Claude));
        assert_eq!(ProviderKind::parse("CLAUDE"), Some(ProviderKind::Claude));
    }

    #[test]
    fn provider_from_str_openai() {
        assert_eq!(ProviderKind::parse("openai"), Some(ProviderKind::OpenAI));
        assert_eq!(ProviderKind::parse("gpt"), Some(ProviderKind::OpenAI));
    }

    #[test]
    fn provider_from_str_all() {
        assert!(ProviderKind::parse("mistral").is_some());
        assert!(ProviderKind::parse("groq").is_some());
        assert!(ProviderKind::parse("deepseek").is_some());
        assert!(ProviderKind::parse("gemini").is_some());
        assert!(ProviderKind::parse("native").is_some());
    }

    #[test]
    fn provider_from_str_unknown() {
        assert_eq!(ProviderKind::parse("unknown"), None);
        assert_eq!(ProviderKind::parse(""), None);
    }

    #[test]
    fn provider_is_free() {
        assert!(ProviderKind::Native.is_free());
        assert!(!ProviderKind::Claude.is_free());
        assert!(!ProviderKind::OpenAI.is_free());
    }

    // ───────────────────────────────────────────────────────────────────────
    // ModelPricing tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn pricing_calculate_simple() {
        let pricing = ModelPricing::new(10.0, 30.0); // $10/M input, $30/M output
        let cost = pricing.calculate(1_000_000, 1_000_000);
        assert!((cost - 40.0).abs() < 0.0001);
    }

    #[test]
    fn pricing_calculate_fractional() {
        let pricing = ModelPricing::new(3.0, 15.0); // Claude Sonnet
        let cost = pricing.calculate(1000, 500);
        // 1000 * 3/1M + 500 * 15/1M = 0.003 + 0.0075 = 0.0105
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn pricing_calculate_zero_tokens() {
        let pricing = ModelPricing::new(10.0, 30.0);
        let cost = pricing.calculate(0, 0);
        assert!((cost - 0.0).abs() < 0.0001);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Cost calculation tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn calculate_cost_claude_sonnet() {
        let cost = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", 10_000, 5_000);
        // 10000 * 3/1M + 5000 * 15/1M = 0.03 + 0.075 = 0.105
        assert!((cost - 0.105).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_openai_gpt4o() {
        let cost = calculate_cost(ProviderKind::OpenAI, "gpt-4o", 10_000, 5_000);
        // 10000 * 2.5/1M + 5000 * 10/1M = 0.025 + 0.05 = 0.075
        assert!((cost - 0.075).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_native_free() {
        let cost = calculate_cost(ProviderKind::Native, "llama3.2-q4", 1_000_000, 1_000_000);
        assert!((cost - 0.0).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_unknown_model() {
        let cost = calculate_cost(ProviderKind::Claude, "unknown-model", 1_000_000, 1_000_000);
        // Should use default pricing (5, 15)
        assert!((cost - 20.0).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_groq() {
        let cost = calculate_cost(
            ProviderKind::Groq,
            "llama-3.3-70b-versatile",
            100_000,
            50_000,
        );
        // 100000 * 0.59/1M + 50000 * 0.79/1M = 0.059 + 0.0395 = 0.0985
        assert!((cost - 0.0985).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_deepseek() {
        let cost = calculate_cost(ProviderKind::DeepSeek, "deepseek-chat", 100_000, 50_000);
        // 100000 * 0.14/1M + 50000 * 0.28/1M = 0.014 + 0.014 = 0.028
        assert!((cost - 0.028).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_gemini() {
        let cost = calculate_cost(ProviderKind::Gemini, "gemini-2.0-flash", 100_000, 50_000);
        // 100000 * 0.1/1M + 50000 * 0.4/1M = 0.01 + 0.02 = 0.03
        assert!((cost - 0.03).abs() < 0.0001);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Format cost tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn format_cost_tiny() {
        assert_eq!(format_cost(0.00001), "$0.000010");
    }

    #[test]
    fn format_cost_small() {
        assert_eq!(format_cost(0.005), "$0.0050");
    }

    #[test]
    fn format_cost_medium() {
        assert_eq!(format_cost(0.105), "$0.105");
    }

    #[test]
    fn format_cost_large() {
        assert_eq!(format_cost(5.50), "$5.50");
    }

    // ───────────────────────────────────────────────────────────────────────
    // Pricing table coverage
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn claude_pricing_coverage() {
        assert!(CLAUDE_PRICING.contains_key("claude-sonnet-4-6"));
        assert!(CLAUDE_PRICING.contains_key("claude-opus-4"));
        assert!(CLAUDE_PRICING.contains_key("claude-3-5-haiku-latest"));
    }

    #[test]
    fn openai_pricing_coverage() {
        assert!(OPENAI_PRICING.contains_key("gpt-4o"));
        assert!(OPENAI_PRICING.contains_key("gpt-4o-mini"));
        assert!(OPENAI_PRICING.contains_key("o1"));
    }

    #[test]
    fn mistral_pricing_coverage() {
        assert!(MISTRAL_PRICING.contains_key("mistral-large-latest"));
        assert!(MISTRAL_PRICING.contains_key("mistral-small-latest"));
    }

    #[test]
    fn all_providers_have_pricing() {
        // Each paid provider should have at least one model
        assert!(!CLAUDE_PRICING.is_empty());
        assert!(!OPENAI_PRICING.is_empty());
        assert!(!MISTRAL_PRICING.is_empty());
        assert!(!GROQ_PRICING.is_empty());
        assert!(!DEEPSEEK_PRICING.is_empty());
        assert!(!GEMINI_PRICING.is_empty());
    }
}
