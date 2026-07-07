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

    /// Fold another round-trip's usage into this accumulator — the one
    /// arithmetic for multi-call tasks (schema retries · agent turns), so
    /// every consumer sums the same way and the ledger bills what was
    /// actually spent.
    ///
    /// Scalars saturate (a corrupt provider count must not wrap a bill).
    /// Optional meters sum when both sides report; when only one side
    /// reports, its value carries — `None` means "not reported", not zero,
    /// so a provider that reports a meter on one round-trip and omits it on
    /// the next still totals honestly.
    pub fn absorb(&mut self, other: &Self) {
        /// `Option` meters: saturating sum when both report · the reporting
        /// side wins otherwise.
        fn fold(a: Option<u64>, b: Option<u64>) -> Option<u64> {
            match (a, b) {
                (Some(x), Some(y)) => Some(x.saturating_add(y)),
                (x, y) => x.or(y),
            }
        }
        self.input_tokens = self.input_tokens.saturating_add(other.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(other.output_tokens);
        self.cache_read_tokens = fold(self.cache_read_tokens, other.cache_read_tokens);
        self.cache_write_tokens = fold(self.cache_write_tokens, other.cache_write_tokens);
        self.cache_creation_tokens = fold(self.cache_creation_tokens, other.cache_creation_tokens);
        self.reasoning_tokens = fold(self.reasoning_tokens, other.reasoning_tokens);
        self.thinking_tokens = fold(self.thinking_tokens, other.thinking_tokens);
        self.audio_input_tokens = fold(self.audio_input_tokens, other.audio_input_tokens);
        self.audio_output_tokens = fold(self.audio_output_tokens, other.audio_output_tokens);
        self.image_input_tokens = fold(self.image_input_tokens, other.image_input_tokens);
        self.image_output_tokens = fold(self.image_output_tokens, other.image_output_tokens);
        self.video_input_tokens = fold(self.video_input_tokens, other.video_input_tokens);
        self.accepted_prediction_tokens = fold(
            self.accepted_prediction_tokens,
            other.accepted_prediction_tokens,
        );
        self.rejected_prediction_tokens = fold(
            self.rejected_prediction_tokens,
            other.rejected_prediction_tokens,
        );
        self.total_tokens = fold(self.total_tokens, other.total_tokens);
        self.search_context_tokens = fold(self.search_context_tokens, other.search_context_tokens);
        self.citation_tokens = fold(self.citation_tokens, other.citation_tokens);
        self.num_requests = match (self.num_requests, other.num_requests) {
            (Some(x), Some(y)) => Some(x.saturating_add(y)),
            (x, y) => x.or(y),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absorb_sums_scalars_and_saturates() {
        let mut a = TokenUsage::new(10, 5);
        a.absorb(&TokenUsage::new(20, 7));
        assert_eq!(a.input_tokens, 30);
        assert_eq!(a.output_tokens, 12);
        // A corrupt provider count must not wrap the bill.
        let mut top = TokenUsage::new(u64::MAX, 1);
        top.absorb(&TokenUsage::new(1, u64::MAX));
        assert_eq!(top.input_tokens, u64::MAX);
        assert_eq!(top.output_tokens, u64::MAX);
    }

    #[test]
    fn absorb_option_meters_report_honestly() {
        // Some+Some sums · Some+None keeps the reporting side (None means
        // "not reported", never zero) · None+None stays unreported.
        let mut a = TokenUsage::new(0, 0);
        a.reasoning_tokens = Some(100);
        a.thinking_tokens = None;
        a.cache_read_tokens = Some(3);
        let mut b = TokenUsage::new(0, 0);
        b.reasoning_tokens = Some(50);
        b.thinking_tokens = Some(9);
        b.cache_read_tokens = None;
        a.absorb(&b);
        assert_eq!(a.reasoning_tokens, Some(150), "both report → sum");
        assert_eq!(
            a.thinking_tokens,
            Some(9),
            "only the other reports → carried"
        );
        assert_eq!(a.cache_read_tokens, Some(3), "only self reports → kept");
        assert_eq!(a.total_tokens, None, "neither reports → stays unreported");
    }

    #[test]
    fn absorb_counts_requests() {
        let mut a = TokenUsage::new(1, 1);
        a.num_requests = Some(1);
        let mut b = TokenUsage::new(1, 1);
        b.num_requests = Some(2);
        a.absorb(&b);
        assert_eq!(a.num_requests, Some(3));
    }

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
