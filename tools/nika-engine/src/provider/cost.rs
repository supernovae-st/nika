// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

use rustc_hash::FxHashMap;
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
    /// PERF(L3): eq_ignore_ascii_case avoids to_lowercase() allocation
    pub fn parse(s: &str) -> Option<Self> {
        if s.eq_ignore_ascii_case("claude") || s.eq_ignore_ascii_case("anthropic") {
            Some(Self::Claude)
        } else if s.eq_ignore_ascii_case("openai") || s.eq_ignore_ascii_case("gpt") {
            Some(Self::OpenAI)
        } else if s.eq_ignore_ascii_case("mistral") {
            Some(Self::Mistral)
        } else if s.eq_ignore_ascii_case("groq") {
            Some(Self::Groq)
        } else if s.eq_ignore_ascii_case("deepseek") {
            Some(Self::DeepSeek)
        } else if s.eq_ignore_ascii_case("gemini") || s.eq_ignore_ascii_case("google") {
            Some(Self::Gemini)
        } else if s.eq_ignore_ascii_case("xai")
            || s.eq_ignore_ascii_case("grok")
            || s.eq_ignore_ascii_case("x-ai")
        {
            Some(Self::XAi)
        } else if s.eq_ignore_ascii_case("native") {
            Some(Self::Native)
        } else if s.starts_with("openai-compat:") {
            // Custom endpoints use OpenAI-compatible API
            Some(Self::OpenAI)
        } else {
            None
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

    /// Get the canonical provider ID (lowercase) used for API-level config
    /// (e.g., stop_sequences key mapping, additional_params).
    pub fn to_provider_id(&self) -> &'static str {
        match self {
            Self::Claude => "anthropic",
            Self::OpenAI => "openai",
            Self::Mistral => "mistral",
            Self::Groq => "groq",
            Self::DeepSeek => "deepseek",
            Self::Gemini => "gemini",
            Self::XAi => "xai",
            Self::Native => "native",
        }
    }

    /// Check if provider is free (local or free tier)
    pub fn is_free(&self) -> bool {
        matches!(self, Self::Native)
    }

    /// Maximum allowed temperature for this provider.
    /// Anthropic/Mistral/DeepSeek: 1.0, OpenAI/Gemini/xAI/Groq/Native: 2.0.
    pub fn max_temperature(&self) -> f64 {
        match self {
            Self::Claude | Self::Mistral | Self::DeepSeek => 1.0,
            Self::OpenAI | Self::Gemini | Self::XAi | Self::Groq | Self::Native => 2.0,
        }
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

    /// Calculate cost with cached token discount.
    ///
    /// `cache_discount` is the fraction of input price charged for cached tokens:
    /// - Anthropic: 0.1 (10% of input rate)
    /// - OpenAI: 0.5 (50% of input rate)
    /// - Others: 0.1 (conservative default)
    pub fn calculate_with_cache(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cached_tokens: u64,
        cache_discount: f64,
    ) -> f64 {
        // Cap cached_tokens at input_tokens to prevent over-claiming
        let actual_cached = cached_tokens.min(input_tokens);
        let effective_input = input_tokens - actual_cached;
        let cached_cost =
            (actual_cached as f64 / 1_000_000.0) * self.input_per_million * cache_discount;
        let input_cost = (effective_input as f64 / 1_000_000.0) * self.input_per_million;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * self.output_per_million;
        let cost = cached_cost + input_cost + output_cost;
        if cost.is_finite() {
            cost
        } else {
            0.0
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// PRICING TABLES (per million tokens, as of March 2026)
// ═══════════════════════════════════════════════════════════════════════════

/// Claude models pricing
static CLAUDE_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
    // Claude 4.6 Opus
    m.insert("claude-opus-4-6", ModelPricing::new(15.0, 75.0));
    // Claude 4 Opus
    m.insert("claude-opus-4-20250514", ModelPricing::new(15.0, 75.0));
    m.insert("claude-opus-4", ModelPricing::new(15.0, 75.0));
    // Claude 4.6 Sonnet
    m.insert("claude-sonnet-4-6-20250514", ModelPricing::new(3.0, 15.0));
    m.insert("claude-sonnet-4-6", ModelPricing::new(3.0, 15.0));
    // Claude 4 Sonnet
    m.insert("claude-sonnet-4-20250514", ModelPricing::new(3.0, 15.0));
    m.insert("claude-sonnet-4", ModelPricing::new(3.0, 15.0));
    // Claude 4.5 Haiku
    m.insert("claude-haiku-4-5-20251001", ModelPricing::new(0.8, 4.0));
    m.insert("claude-haiku-4-5", ModelPricing::new(0.8, 4.0));
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
static OPENAI_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
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
    // o3
    m.insert("o3", ModelPricing::new(2.0, 8.0));
    m.insert("o3-2025-04-16", ModelPricing::new(2.0, 8.0));
    // o4-mini
    m.insert("o4-mini", ModelPricing::new(1.1, 4.4));
    m.insert("o4-mini-2025-04-16", ModelPricing::new(1.1, 4.4));
    m
});

/// Mistral models pricing
static MISTRAL_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
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
static GROQ_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
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
static DEEPSEEK_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
    m.insert("deepseek-chat", ModelPricing::new(0.14, 0.28));
    m.insert("deepseek-reasoner", ModelPricing::new(0.55, 2.19));
    m.insert("deepseek-coder", ModelPricing::new(0.14, 0.28));
    m
});

/// Gemini models pricing
static GEMINI_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
    // Gemini 2.5
    m.insert("gemini-2.5-pro", ModelPricing::new(1.25, 10.0));
    m.insert(
        "gemini-2.5-pro-preview-05-06",
        ModelPricing::new(1.25, 10.0),
    );
    m.insert("gemini-2.5-flash", ModelPricing::new(0.15, 0.60));
    m.insert(
        "gemini-2.5-flash-preview-05-20",
        ModelPricing::new(0.15, 0.60),
    );
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
static XAI_PRICING: LazyLock<FxHashMap<&'static str, ModelPricing>> = LazyLock::new(|| {
    let mut m = FxHashMap::default();
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

/// List all known models with pricing for a provider.
///
/// Returns `(model_name, pricing)` pairs sorted by model name.
/// Used by `nika model list` to display cloud models with costs.
pub fn list_provider_models(provider: ProviderKind) -> Vec<(&'static str, ModelPricing)> {
    if provider.is_free() {
        return Vec::new();
    }
    let table: &FxHashMap<&'static str, ModelPricing> = match provider {
        ProviderKind::Claude => &CLAUDE_PRICING,
        ProviderKind::OpenAI => &OPENAI_PRICING,
        ProviderKind::Mistral => &MISTRAL_PRICING,
        ProviderKind::Groq => &GROQ_PRICING,
        ProviderKind::DeepSeek => &DEEPSEEK_PRICING,
        ProviderKind::Gemini => &GEMINI_PRICING,
        ProviderKind::XAi => &XAI_PRICING,
        ProviderKind::Native => return Vec::new(),
    };
    // Deduplicate: skip date-suffixed variants (e.g., "gpt-4o-2024-11-20" when "gpt-4o" exists)
    let mut entries: Vec<(&str, ModelPricing)> = table.iter().map(|(&k, &v)| (k, v)).collect();
    entries.sort_by_key(|(name, _)| name.len());
    let all_names: Vec<&str> = entries.iter().map(|(n, _)| *n).collect();
    let mut result: Vec<(&str, ModelPricing)> = Vec::new();
    for &(name, pricing) in &entries {
        // A name is a date variant if a shorter name is its prefix followed by -YYYY
        // (at least 4 digits after the dash, e.g., -2024-11-20, -20250514)
        // This avoids false positives like claude-sonnet-4-6 being treated as
        // a date variant of claude-sonnet-4 (the -6 is a version, not a date).
        let is_date_variant = all_names.iter().any(|shorter| {
            if shorter.len() >= name.len() || !name.starts_with(shorter) {
                return false;
            }
            let suffix = &name[shorter.len()..];
            // Must start with - followed by at least 4 digits (YYYY)
            suffix.starts_with('-')
                && suffix.len() >= 5
                && suffix[1..5].chars().all(|c| c.is_ascii_digit())
        });
        if !is_date_variant {
            result.push((name, pricing));
        }
    }
    result.sort_by(|(a, _), (b, _)| a.cmp(b));
    result
}

// ═══════════════════════════════════════════════════════════════════════════
// MODEL METADATA — capability tags and context windows
// ═══════════════════════════════════════════════════════════════════════════

/// Capability tag for a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelTag {
    Reasoning,
    Code,
    Balanced,
    Fast,
    Vision,
}

impl ModelTag {
    pub fn label(self) -> &'static str {
        match self {
            Self::Reasoning => "reasoning",
            Self::Code => "code",
            Self::Balanced => "balanced",
            Self::Fast => "fast",
            Self::Vision => "vision",
        }
    }
}

/// Extended metadata for a model.
#[derive(Debug, Clone)]
pub struct ModelMeta {
    pub tags: &'static [ModelTag],
    pub context_window: u32,
}

const MODEL_META: &[(&str, ModelMeta)] = &[
    (
        "claude-opus",
        ModelMeta {
            tags: &[ModelTag::Reasoning, ModelTag::Vision],
            context_window: 200_000,
        },
    ),
    (
        "claude-sonnet",
        ModelMeta {
            tags: &[ModelTag::Balanced, ModelTag::Code, ModelTag::Vision],
            context_window: 200_000,
        },
    ),
    (
        "claude-haiku",
        ModelMeta {
            tags: &[ModelTag::Fast, ModelTag::Vision],
            context_window: 200_000,
        },
    ),
    (
        "gpt-4o-mini",
        ModelMeta {
            tags: &[ModelTag::Fast, ModelTag::Vision],
            context_window: 128_000,
        },
    ),
    (
        "gpt-4o",
        ModelMeta {
            tags: &[ModelTag::Balanced, ModelTag::Vision],
            context_window: 128_000,
        },
    ),
    (
        "gpt-4.1-mini",
        ModelMeta {
            tags: &[ModelTag::Fast, ModelTag::Code],
            context_window: 1_000_000,
        },
    ),
    (
        "gpt-4.1-nano",
        ModelMeta {
            tags: &[ModelTag::Fast],
            context_window: 1_000_000,
        },
    ),
    (
        "gpt-4.1",
        ModelMeta {
            tags: &[ModelTag::Code, ModelTag::Balanced],
            context_window: 1_000_000,
        },
    ),
    (
        "o3",
        ModelMeta {
            tags: &[ModelTag::Reasoning],
            context_window: 200_000,
        },
    ),
    (
        "o4-mini",
        ModelMeta {
            tags: &[ModelTag::Reasoning, ModelTag::Fast],
            context_window: 200_000,
        },
    ),
    (
        "o1",
        ModelMeta {
            tags: &[ModelTag::Reasoning],
            context_window: 200_000,
        },
    ),
    (
        "mistral-large",
        ModelMeta {
            tags: &[ModelTag::Balanced],
            context_window: 128_000,
        },
    ),
    (
        "mistral-small",
        ModelMeta {
            tags: &[ModelTag::Fast],
            context_window: 128_000,
        },
    ),
    (
        "codestral",
        ModelMeta {
            tags: &[ModelTag::Code],
            context_window: 256_000,
        },
    ),
    (
        "llama",
        ModelMeta {
            tags: &[ModelTag::Fast],
            context_window: 131_072,
        },
    ),
    (
        "deepseek-chat",
        ModelMeta {
            tags: &[ModelTag::Balanced],
            context_window: 128_000,
        },
    ),
    (
        "deepseek-reasoner",
        ModelMeta {
            tags: &[ModelTag::Reasoning],
            context_window: 128_000,
        },
    ),
    (
        "gemini-2.5-pro",
        ModelMeta {
            tags: &[ModelTag::Reasoning, ModelTag::Vision],
            context_window: 1_000_000,
        },
    ),
    (
        "gemini-2.5-flash",
        ModelMeta {
            tags: &[ModelTag::Fast, ModelTag::Vision],
            context_window: 1_000_000,
        },
    ),
    (
        "grok-3-mini",
        ModelMeta {
            tags: &[ModelTag::Fast],
            context_window: 131_072,
        },
    ),
    (
        "grok-3",
        ModelMeta {
            tags: &[ModelTag::Reasoning],
            context_window: 131_072,
        },
    ),
];

/// Look up metadata for a model by name (prefix match).
pub fn get_model_meta(model_name: &str) -> Option<&'static ModelMeta> {
    MODEL_META
        .iter()
        .find(|(prefix, _)| model_name.starts_with(prefix))
        .map(|(_, meta)| meta)
}

/// Format context window as human-readable.
pub fn format_context_window(tokens: u32) -> String {
    if tokens >= 1_000_000 {
        format!("{}M", tokens / 1_000_000)
    } else if tokens >= 1_000 {
        format!("{}K", tokens / 1_000)
    } else {
        format!("{tokens}")
    }
}

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

    pricing.copied().unwrap_or_else(|| {
        tracing::warn!(
            provider = %provider.name(),
            model = %model,
            "Unknown model — using default pricing ($5/$15 per M tokens)"
        );
        DEFAULT_PRICING
    })
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

/// Get the cache discount rate for a provider.
///
/// Returns the fraction of input price charged for cached tokens:
/// - Anthropic: 0.1 (10% of input rate)
/// - OpenAI: 0.5 (50% of input rate)
/// - DeepSeek: 0.1 (10% of input rate)
/// - Others: 1.0 (no discount — provider doesn't support prompt caching)
pub fn cache_discount_for_provider(provider: ProviderKind) -> f64 {
    match provider {
        ProviderKind::Claude => 0.1,
        ProviderKind::OpenAI => 0.5,
        ProviderKind::DeepSeek => 0.1,
        // Providers without prompt caching: no discount
        _ => 1.0,
    }
}

/// Calculate cost with cached token discount (provider-aware rate).
pub fn calculate_cost_with_cache(
    provider: ProviderKind,
    model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cached_tokens: u64,
) -> f64 {
    let pricing = get_model_pricing(provider, model);
    let discount = cache_discount_for_provider(provider);
    pricing.calculate_with_cache(input_tokens, output_tokens, cached_tokens, discount)
}

/// Calculate cost for self-hosted endpoints using hourly rate.
///
/// Custom endpoints (vLLM, Ollama, etc.) don't have per-token pricing.
/// Cost is estimated as: `(duration_secs / 3600) × hourly_rate`.
pub fn calculate_hourly_cost(duration_secs: f64, hourly_rate: f64) -> f64 {
    if duration_secs <= 0.0 || hourly_rate <= 0.0 {
        return 0.0;
    }
    (duration_secs / 3600.0) * hourly_rate
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
    // Cache cost tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn calculate_cost_with_cache_normal() {
        let cost = calculate_cost_with_cache(
            ProviderKind::Claude,
            "claude-sonnet-4-6",
            10_000,
            5_000,
            2_000,
        );
        // effective_input = 10000 - 2000 = 8000
        // cached_cost = 2000 * 3.0/1M * 0.1 = 0.0006
        // input_cost = 8000 * 3.0/1M = 0.024
        // output_cost = 5000 * 15.0/1M = 0.075
        // total = 0.0006 + 0.024 + 0.075 = 0.0996
        assert!((cost - 0.0996).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_with_cache_overclaimed() {
        // cached_tokens > input_tokens — should cap at input_tokens
        let cost = calculate_cost_with_cache(
            ProviderKind::Claude,
            "claude-sonnet-4-6",
            100,
            50,
            200, // overclaimed: 200 > 100
        );
        // actual_cached = min(200, 100) = 100, effective_input = 0
        // cached_cost = 100 * 3.0/1M * 0.1 = 0.00003
        // input_cost = 0
        // output_cost = 50 * 15.0/1M = 0.00075
        // total = 0.00078
        assert!((cost - 0.00078).abs() < 0.0001);
    }

    #[test]
    fn calculate_cost_with_cache_zero_cached() {
        let with_cache =
            calculate_cost_with_cache(ProviderKind::Claude, "claude-sonnet-4-6", 1000, 500, 0);
        let without_cache = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", 1000, 500);
        assert!((with_cache - without_cache).abs() < f64::EPSILON);
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

    // ───────────────────────────────────────────────────────────────────────
    // list_provider_models tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn list_provider_models_all_cloud_providers_non_empty() {
        let providers = [
            ProviderKind::Claude,
            ProviderKind::OpenAI,
            ProviderKind::Mistral,
            ProviderKind::Groq,
            ProviderKind::DeepSeek,
            ProviderKind::Gemini,
            ProviderKind::XAi,
        ];
        for provider in providers {
            let models = list_provider_models(provider);
            assert!(
                !models.is_empty(),
                "{} should return at least 1 model",
                provider.name()
            );
        }
    }

    #[test]
    fn list_provider_models_native_returns_empty() {
        let models = list_provider_models(ProviderKind::Native);
        assert!(models.is_empty(), "Native provider should return no models");
    }

    #[test]
    fn list_provider_models_claude_deduplicates_date_variants() {
        let models = list_provider_models(ProviderKind::Claude);
        let names: Vec<&str> = models.iter().map(|(n, _)| *n).collect();
        // The base alias should survive dedup
        assert!(
            names.contains(&"claude-sonnet-4"),
            "claude-sonnet-4 should be in the deduplicated list"
        );
        // Date-suffixed variant should be filtered out
        assert!(
            !names.contains(&"claude-sonnet-4-20250514"),
            "claude-sonnet-4-20250514 should be deduplicated away"
        );
        // The -6 variant is also treated as a date variant of claude-sonnet-4
        // (prefix + -DIGIT pattern), so both it and its own date variant are removed
        assert!(
            !names.contains(&"claude-sonnet-4-6-20250514"),
            "claude-sonnet-4-6-20250514 should be deduplicated away"
        );
    }

    #[test]
    fn list_provider_models_openai_deduplicates_date_variants() {
        let models = list_provider_models(ProviderKind::OpenAI);
        let names: Vec<&str> = models.iter().map(|(n, _)| *n).collect();
        // The short alias should be present
        assert!(
            names.contains(&"gpt-4o"),
            "gpt-4o should be in the deduplicated list"
        );
        // The date-suffixed variant should be filtered out
        assert!(
            !names.contains(&"gpt-4o-2024-11-20"),
            "gpt-4o-2024-11-20 should be deduplicated away"
        );
        // Same for GPT-4.1
        assert!(
            names.contains(&"gpt-4.1"),
            "gpt-4.1 should be in the deduplicated list"
        );
        assert!(
            !names.contains(&"gpt-4.1-2025-04-14"),
            "gpt-4.1-2025-04-14 should be deduplicated away"
        );
    }

    #[test]
    fn list_provider_models_no_duplicates() {
        let providers = [
            ProviderKind::Claude,
            ProviderKind::OpenAI,
            ProviderKind::Mistral,
            ProviderKind::Groq,
            ProviderKind::DeepSeek,
            ProviderKind::Gemini,
            ProviderKind::XAi,
        ];
        for provider in providers {
            let models = list_provider_models(provider);
            let names: Vec<&str> = models.iter().map(|(n, _)| *n).collect();
            let mut seen = std::collections::HashSet::new();
            for name in &names {
                assert!(
                    seen.insert(name),
                    "{} has duplicate model: {}",
                    provider.name(),
                    name
                );
            }
        }
    }

    #[test]
    fn list_provider_models_pricing_positive() {
        // All returned models should have positive pricing, except known free previews
        let free_previews: &[&str] = &["gemini-2.0-flash-exp", "gemini-2.0-flash-thinking"];

        let providers = [
            ProviderKind::Claude,
            ProviderKind::OpenAI,
            ProviderKind::Mistral,
            ProviderKind::Groq,
            ProviderKind::DeepSeek,
            ProviderKind::Gemini,
            ProviderKind::XAi,
        ];
        for provider in providers {
            let models = list_provider_models(provider);
            for (name, pricing) in &models {
                if free_previews.contains(name) {
                    continue;
                }
                assert!(
                    pricing.input_per_million > 0.0,
                    "{}/{} should have input_per_million > 0, got {}",
                    provider.name(),
                    name,
                    pricing.input_per_million
                );
                assert!(
                    pricing.output_per_million > 0.0,
                    "{}/{} should have output_per_million > 0, got {}",
                    provider.name(),
                    name,
                    pricing.output_per_million
                );
            }
        }
    }

    #[test]
    fn list_provider_models_results_are_sorted() {
        let providers = [
            ProviderKind::Claude,
            ProviderKind::OpenAI,
            ProviderKind::Mistral,
            ProviderKind::Groq,
            ProviderKind::DeepSeek,
            ProviderKind::Gemini,
            ProviderKind::XAi,
        ];
        for provider in providers {
            let models = list_provider_models(provider);
            let names: Vec<&str> = models.iter().map(|(n, _)| *n).collect();
            let mut sorted = names.clone();
            sorted.sort();
            assert_eq!(
                names,
                sorted,
                "{} models should be sorted alphabetically",
                provider.name()
            );
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Model metadata tests
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn model_meta_claude_opus() {
        let meta = get_model_meta("claude-opus-4-20250514").unwrap();
        assert!(meta.tags.contains(&ModelTag::Reasoning));
        assert_eq!(meta.context_window, 200_000);
    }

    #[test]
    fn model_meta_claude_sonnet() {
        let meta = get_model_meta("claude-sonnet-4-6").unwrap();
        assert!(meta.tags.contains(&ModelTag::Balanced));
        assert!(meta.tags.contains(&ModelTag::Code));
    }

    #[test]
    fn model_meta_gpt4o() {
        let meta = get_model_meta("gpt-4o").unwrap();
        assert!(meta.tags.contains(&ModelTag::Vision));
    }

    #[test]
    fn model_meta_o3() {
        let meta = get_model_meta("o3").unwrap();
        assert!(meta.tags.contains(&ModelTag::Reasoning));
    }

    #[test]
    fn model_meta_gemini_pro() {
        let meta = get_model_meta("gemini-2.5-pro-preview").unwrap();
        assert_eq!(meta.context_window, 1_000_000);
    }

    #[test]
    fn model_meta_unknown_returns_none() {
        assert!(get_model_meta("nonexistent-model-xyz").is_none());
    }

    #[test]
    fn format_context_window_million() {
        assert_eq!(format_context_window(1_000_000), "1M");
    }

    #[test]
    fn format_context_window_thousand() {
        assert_eq!(format_context_window(200_000), "200K");
        assert_eq!(format_context_window(128_000), "128K");
    }

    #[test]
    fn format_context_window_small() {
        assert_eq!(format_context_window(512), "512");
    }

    #[test]
    fn model_tag_labels() {
        assert_eq!(ModelTag::Reasoning.label(), "reasoning");
        assert_eq!(ModelTag::Code.label(), "code");
        assert_eq!(ModelTag::Fast.label(), "fast");
        assert_eq!(ModelTag::Vision.label(), "vision");
        assert_eq!(ModelTag::Balanced.label(), "balanced");
    }

    // ───────────────────────────────────────────────────────────────────────
    // Bug regression: model.unwrap_or("default") gives wrong cost
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn default_string_is_not_a_valid_model_name() {
        // Regression: infer.rs used model.unwrap_or("default") for cost calculation,
        // which hits fallback pricing ($5/$15) instead of the actual model's pricing.
        // The fix uses provider.default_model() which returns a real model name.
        let fallback_cost = calculate_cost(ProviderKind::Claude, "default", 100_000, 50_000);
        let real_cost = calculate_cost(ProviderKind::Claude, "claude-sonnet-4-6", 100_000, 50_000);
        // "default" MUST NOT match any real pricing — proves the bug was real
        assert!(
            (fallback_cost - real_cost).abs() > 0.001,
            "Expected 'default' to use fallback pricing (different from real model), \
             but got same cost: ${:.6}",
            fallback_cost,
        );
    }

    #[test]
    fn all_provider_default_models_have_real_pricing() {
        // Regression: ensure provider.default_model() returns a model with real pricing
        // (not the $5/$15 fallback). This is what infer.rs now uses for cost calculation.
        let default_pricing = ModelPricing::new(5.0, 15.0); // fallback

        let provider_defaults: &[(ProviderKind, &str)] = &[
            (ProviderKind::Claude, "claude-sonnet-4-6"),
            (ProviderKind::OpenAI, "gpt-4o"),
            (ProviderKind::Mistral, "mistral-large-latest"),
            (ProviderKind::Groq, "llama-3.3-70b-versatile"),
            (ProviderKind::DeepSeek, "deepseek-chat"),
            (ProviderKind::Gemini, "gemini-2.0-flash"),
            (ProviderKind::XAi, "grok-3-fast"),
        ];

        for (provider, model) in provider_defaults {
            let pricing = get_model_pricing(*provider, model);
            assert!(
                (pricing.input_per_million - default_pricing.input_per_million).abs()
                    > f64::EPSILON
                    || (pricing.output_per_million - default_pricing.output_per_million).abs()
                        > f64::EPSILON,
                "{}/{} uses fallback pricing — add it to the pricing table!",
                provider.name(),
                model,
            );
        }
    }

    // ───────────────────────────────────────────────────────────────────────
    // Hourly cost tests (custom endpoints)
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn hourly_cost_60s_at_3_per_hour() {
        // 60s / 3600 * 3.0 = 0.05
        let cost = calculate_hourly_cost(60.0, 3.0);
        assert!((cost - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn hourly_cost_3600s_at_3_per_hour() {
        let cost = calculate_hourly_cost(3600.0, 3.0);
        assert!((cost - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hourly_cost_zero_duration() {
        assert!((calculate_hourly_cost(0.0, 3.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn hourly_cost_negative_duration() {
        assert!((calculate_hourly_cost(-1.0, 3.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn hourly_cost_zero_rate() {
        assert!((calculate_hourly_cost(60.0, 0.0)).abs() < f64::EPSILON);
    }

    // ───────────────────────────────────────────────────────────────────────
    // Pricing table sync test — engine models must be covered by core
    // ───────────────────────────────────────────────────────────────────────

    #[test]
    fn engine_models_covered_by_core_pricing() {
        let all_providers = [
            (ProviderKind::Claude, &*CLAUDE_PRICING),
            (ProviderKind::OpenAI, &*OPENAI_PRICING),
            (ProviderKind::Mistral, &*MISTRAL_PRICING),
            (ProviderKind::Groq, &*GROQ_PRICING),
            (ProviderKind::DeepSeek, &*DEEPSEEK_PRICING),
            (ProviderKind::Gemini, &*GEMINI_PRICING),
            (ProviderKind::XAi, &*XAI_PRICING),
        ];

        let mut missing = Vec::new();
        let mut price_mismatches = Vec::new();

        for (provider, table) in &all_providers {
            for (model, engine_pricing) in table.iter() {
                if let Some(core_pricing) = nika_core::catalogs::cost::find_pricing(model) {
                    // Check prices match within tolerance
                    if (core_pricing.input_per_million - engine_pricing.input_per_million).abs()
                        > 0.01
                    {
                        price_mismatches.push(format!(
                            "{:?}/{}: input core={} engine={}",
                            provider,
                            model,
                            core_pricing.input_per_million,
                            engine_pricing.input_per_million
                        ));
                    }
                    if (core_pricing.output_per_million - engine_pricing.output_per_million).abs()
                        > 0.01
                    {
                        price_mismatches.push(format!(
                            "{:?}/{}: output core={} engine={}",
                            provider,
                            model,
                            core_pricing.output_per_million,
                            engine_pricing.output_per_million
                        ));
                    }
                } else {
                    missing.push(format!("{:?}/{}", provider, model));
                }
            }
        }

        assert!(
            missing.is_empty(),
            "Models in engine but NOT in core pricing: {:?}",
            missing
        );
        assert!(
            price_mismatches.is_empty(),
            "Price mismatches between core and engine: {:?}",
            price_mismatches
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Property-based tests (proptest)
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod proptest_tests {
    use super::*;
    use proptest::prelude::*;

    fn arb_provider() -> impl Strategy<Value = ProviderKind> {
        prop_oneof![
            Just(ProviderKind::Claude),
            Just(ProviderKind::OpenAI),
            Just(ProviderKind::Mistral),
            Just(ProviderKind::Groq),
            Just(ProviderKind::DeepSeek),
            Just(ProviderKind::Gemini),
            Just(ProviderKind::XAi),
            Just(ProviderKind::Native),
        ]
    }

    fn arb_provider_with_model() -> impl Strategy<Value = (ProviderKind, &'static str)> {
        prop_oneof![
            Just((ProviderKind::Claude, "claude-sonnet-4-20250514")),
            Just((ProviderKind::Claude, "claude-opus-4-20250514")),
            Just((ProviderKind::OpenAI, "gpt-4o")),
            Just((ProviderKind::OpenAI, "o3")),
            Just((ProviderKind::Mistral, "mistral-large-latest")),
            Just((ProviderKind::Groq, "llama-3.3-70b-versatile")),
            Just((ProviderKind::DeepSeek, "deepseek-chat")),
            Just((ProviderKind::Gemini, "gemini-2.5-pro")),
            Just((ProviderKind::XAi, "grok-3")),
            Just((ProviderKind::Native, "anything")),
        ]
    }

    proptest! {
        // ── Property 1: Cost is always non-negative and finite ──
        #[test]
        fn cost_always_valid(
            (provider, model) in arb_provider_with_model(),
            input in 0u64..10_000_000_000,
            output in 0u64..10_000_000_000,
        ) {
            let cost = calculate_cost(provider, model, input, output);
            prop_assert!(cost >= 0.0, "Negative cost for {:?}/{}: {}", provider, model, cost);
            prop_assert!(cost.is_finite(), "Non-finite cost for {:?}/{}: {}", provider, model, cost);
        }

        // ── Property 2: Cache cost is always valid and <= uncached cost ──
        #[test]
        fn cost_with_cache_always_valid(
            (provider, model) in arb_provider_with_model(),
            input in 0u64..10_000_000_000,
            output in 0u64..10_000_000_000,
            cached in 0u64..10_000_000_000,
        ) {
            let cost = calculate_cost_with_cache(provider, model, input, output, cached);
            prop_assert!(cost >= 0.0, "Negative cached cost: {}", cost);
            prop_assert!(cost.is_finite(), "Non-finite cached cost: {}", cost);

            let uncached = calculate_cost(provider, model, input, output);
            // Allow tiny floating-point rounding tolerance (1e-6 relative)
            let tolerance = uncached.abs() * 1e-9 + 1e-12;
            prop_assert!(
                cost <= uncached + tolerance,
                "Cached cost {} > uncached cost {} (tolerance {}) for {:?}/{}",
                cost, uncached, tolerance, provider, model
            );
        }

        // ── Property 3: Zero tokens = zero cost ──
        #[test]
        fn cost_zero_tokens_is_zero(
            (provider, model) in arb_provider_with_model(),
        ) {
            let cost = calculate_cost(provider, model, 0, 0);
            prop_assert!((cost - 0.0).abs() < f64::EPSILON, "Zero tokens should be free, got {}", cost);
        }

        // ── Property 4: More input tokens = higher cost (monotonicity) ──
        #[test]
        fn cost_monotonically_increases_with_input(
            (provider, model) in arb_provider_with_model(),
            input1 in 0u64..5_000_000,
            delta in 1u64..5_000_000,
            output in 0u64..10_000_000,
        ) {
            let input2 = input1 + delta;
            let cost1 = calculate_cost(provider, model, input1, output);
            let cost2 = calculate_cost(provider, model, input2, output);
            prop_assert!(
                cost2 >= cost1,
                "More input tokens should cost >= less: {}({}) vs {}({})",
                cost1, input1, cost2, input2
            );
        }

        // ── Property 5: Native provider is always free ──
        #[test]
        fn native_always_free(input in 0u64..u64::MAX, output in 0u64..u64::MAX) {
            let cost = calculate_cost(ProviderKind::Native, "any-model", input, output);
            prop_assert!((cost - 0.0).abs() < f64::EPSILON, "Native must be free, got {}", cost);
        }

        // ── Property 6: format_cost never panics ──
        #[test]
        fn format_cost_never_panics(cost in -1000.0f64..1000.0) {
            let _ = format_cost(cost); // Must not panic
        }

        // ── Property 7: cache_discount is in [0.0, 1.0] for all providers ──
        #[test]
        fn cache_discount_in_range(provider in arb_provider()) {
            let discount = cache_discount_for_provider(provider);
            prop_assert!(
                (0.0..=1.0).contains(&discount),
                "Discount must be in [0,1], got {} for {:?}",
                discount,
                provider
            );
        }

        // ── Property 8: ModelPricing::calculate is consistent ──
        #[test]
        fn pricing_calculate_consistent(
            input_per_m in 0.0f64..100.0,
            output_per_m in 0.0f64..100.0,
            input in 0u64..10_000_000,
            output in 0u64..10_000_000,
        ) {
            let pricing = ModelPricing::new(input_per_m, output_per_m);
            let cost = pricing.calculate(input, output);
            prop_assert!(cost >= 0.0, "Cost must be non-negative");
            prop_assert!(cost.is_finite(), "Cost must be finite");
        }

        // ── Property 9: hourly cost is non-negative for valid inputs ──
        #[test]
        fn hourly_cost_non_negative(
            duration in 0.0f64..86400.0,
            rate in 0.0f64..100.0,
        ) {
            let cost = calculate_hourly_cost(duration, rate);
            prop_assert!(cost >= 0.0, "Hourly cost must be non-negative");
            prop_assert!(cost.is_finite(), "Hourly cost must be finite");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Per-provider temperature limits (M-orig8)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn provider_kind_max_temperature_anthropic() {
        assert!(
            (ProviderKind::Claude.max_temperature() - 1.0).abs() < f64::EPSILON,
            "Anthropic max temperature is 1.0"
        );
    }

    #[test]
    fn provider_kind_max_temperature_openai() {
        assert!(
            (ProviderKind::OpenAI.max_temperature() - 2.0).abs() < f64::EPSILON,
            "OpenAI max temperature is 2.0"
        );
    }

    #[test]
    fn provider_kind_max_temperature_mistral() {
        assert!(
            (ProviderKind::Mistral.max_temperature() - 1.0).abs() < f64::EPSILON,
            "Mistral max temperature is 1.0"
        );
    }

    #[test]
    fn provider_kind_max_temperature_deepseek() {
        assert!(
            (ProviderKind::DeepSeek.max_temperature() - 1.0).abs() < f64::EPSILON,
            "DeepSeek max temperature is 1.0"
        );
    }

    #[test]
    fn provider_kind_max_temperature_gemini() {
        assert!(
            (ProviderKind::Gemini.max_temperature() - 2.0).abs() < f64::EPSILON,
            "Gemini max temperature is 2.0"
        );
    }
}
