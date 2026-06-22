// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Token usage statistics — shared between provider and billing.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Token usage statistics.
///
/// All new fields (post-v0.80) are `Option<u64>` for backward compatibility.
/// Producers opt in by setting individual fields. The struct is
/// `#[non_exhaustive]` so future additions are non-breaking.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub struct TokenUsage {
    /// Input/prompt tokens.
    pub input_tokens: u64,
    /// Output/completion tokens.
    pub output_tokens: u64,
    /// Tokens read from cache (prompt caching).
    pub cache_read_tokens: Option<u64>,
    /// Tokens written to cache.
    pub cache_write_tokens: Option<u64>,
    // ─── New fields (v0.81 ADR-033 expansion) ─────────────────────
    /// Cache creation tokens (Anthropic-specific).
    pub cache_creation_tokens: Option<u64>,
    /// Reasoning/chain-of-thought tokens (`OpenAI` o1/o3/o4).
    pub reasoning_tokens: Option<u64>,
    /// Extended thinking tokens (Anthropic).
    pub thinking_tokens: Option<u64>,
    /// Audio input tokens (`OpenAI` realtime).
    pub audio_input_tokens: Option<u64>,
    /// Audio output tokens (`OpenAI` realtime).
    pub audio_output_tokens: Option<u64>,
    /// Image input tokens (vision).
    pub image_input_tokens: Option<u64>,
    /// Image output tokens (Gemini image gen).
    pub image_output_tokens: Option<u64>,
    /// Video input tokens (Gemini).
    pub video_input_tokens: Option<u64>,
    /// Accepted prediction tokens (speculative decoding).
    pub accepted_prediction_tokens: Option<u64>,
    /// Rejected prediction tokens (speculative decoding).
    pub rejected_prediction_tokens: Option<u64>,
    /// Total tokens (provider-reported, may differ from input+output sum).
    pub total_tokens: Option<u64>,
    /// Search context size (Perplexity sonar).
    pub search_context_tokens: Option<u64>,
    /// Citation tokens.
    pub citation_tokens: Option<u64>,
    /// Number of API requests this represents (batch awareness).
    pub num_requests: Option<u32>,
}

impl TokenUsage {
    /// Create a new token usage with input and output counts.
    ///
    /// All new (v0.81+) fields default to `None`.
    #[must_use]
    pub fn new(input_tokens: u64, output_tokens: u64) -> Self {
        Self {
            input_tokens,
            output_tokens,
            cache_read_tokens: None,
            cache_write_tokens: None,
            cache_creation_tokens: None,
            reasoning_tokens: None,
            thinking_tokens: None,
            audio_input_tokens: None,
            audio_output_tokens: None,
            image_input_tokens: None,
            image_output_tokens: None,
            video_input_tokens: None,
            accepted_prediction_tokens: None,
            rejected_prediction_tokens: None,
            total_tokens: None,
            search_context_tokens: None,
            citation_tokens: None,
            num_requests: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_usage_new() {
        let usage = TokenUsage::new(100, 50);
        assert_eq!(usage.input_tokens, 100);
        assert_eq!(usage.output_tokens, 50);
        assert!(usage.cache_read_tokens.is_none());
        assert!(usage.cache_creation_tokens.is_none());
        assert!(usage.reasoning_tokens.is_none());
        assert!(usage.thinking_tokens.is_none());
        assert!(usage.num_requests.is_none());
    }

    #[test]
    fn token_usage_default_is_zero() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
    }

    #[test]
    fn token_usage_serde_roundtrip() {
        let usage = TokenUsage::new(100, 50);
        let json = serde_json::to_string(&usage).expect("serialize");
        let back: TokenUsage = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(usage, back);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn token_usage_is_send_sync() {
        _assert_send_sync::<TokenUsage>();
    }
}
