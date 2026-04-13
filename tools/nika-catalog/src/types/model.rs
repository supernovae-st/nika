// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities and pricing types.
//!
//! Capabilities are resolved via pattern-matching function (not phf) because
//! model names are open-ended. Pricing uses 2-pass matching (exact + contains).

/// How to send the token limit to the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenLimitParam {
    /// Standard: `"max_tokens"` in JSON body.
    MaxTokens,
    /// `OpenAI` reasoning models (o-series, gpt-5.x): `"max_completion_tokens"`.
    MaxCompletionTokens,
}

/// Capabilities of a specific model on a specific provider.
///
/// Provider-aware: the same model name gets different treatment depending
/// on the provider (e.g. `o3` on `OpenAI` vs custom vLLM endpoint).
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct ModelCapabilities {
    /// Which JSON field to use for token limits.
    pub token_limit_param: TokenLimitParam,
    /// Model accepts the `temperature` parameter.
    pub supports_temperature: bool,
    /// Model accepts `stop` / `stop_sequences` parameter.
    pub supports_stop_sequences: bool,
    /// Model supports extended thinking (Claude) or `reasoning_effort` (`OpenAI`).
    pub supports_thinking: bool,
    /// Model supports image/vision input.
    pub supports_vision: bool,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            token_limit_param: TokenLimitParam::MaxTokens,
            supports_temperature: true,
            supports_stop_sequences: true,
            supports_thinking: false,
            supports_vision: true,
        }
    }
}

/// Pricing for a known model pattern.
///
/// Two-pass matching: exact match first, then `contains()` fallback.
/// More specific patterns MUST appear before less specific ones.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ModelPricing {
    /// Provider name (title case for display, e.g. `"Anthropic"`).
    pub provider: &'static str,
    /// Model name pattern (e.g. `"sonnet-4"`). Matched by exact then contains.
    pub model_pattern: &'static str,
    /// Price per million input tokens in USD.
    pub input_per_million: f64,
    /// Price per million output tokens in USD.
    pub output_per_million: f64,
}

/// Cost estimate result.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub struct CostEstimate {
    /// Estimated cost in USD.
    pub usd: f64,
    /// Input rate per million tokens.
    pub input_rate_per_million: f64,
    /// Output rate per million tokens.
    pub output_rate_per_million: f64,
    /// Model identifier used for the estimate.
    pub model: String,
    /// Provider name.
    pub provider: String,
}
