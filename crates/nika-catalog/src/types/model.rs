// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities and pricing types.
//!
//! Capabilities are resolved via pattern-matching function (not phf) because
//! model names are open-ended. Pricing uses 2-pass matching (exact + contains).

#[cfg(feature = "capabilities")]
use crate::types::{Modality, ParamFlag, TokenizerFamily};

/// How to send the token limit to the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenLimitParam {
    /// Standard: `"max_tokens"` in JSON body.
    MaxTokens,
    /// `OpenAI` Chat Completions reasoning models (o-series, gpt-5.x):
    /// `"max_completion_tokens"`.
    MaxCompletionTokens,
    /// `OpenAI` Responses API (`/v1/responses`): `"max_output_tokens"`.
    ///
    /// Reserved for Session 2b — no rule in `data/model-capabilities.toml`
    /// maps to this variant yet, but the variant is materialised now so the
    /// `#[non_exhaustive]` enum can grow without a schema bump.
    MaxOutputTokens,
}

/// Capabilities of a specific model on a specific provider.
///
/// Provider-aware: the same model name gets different treatment depending
/// on the provider (e.g. `o3` on `OpenAI` vs custom vLLM endpoint).
///
/// # Session 2b additions (4 new fields — + 1 retired)
///
/// - [`Self::input_modalities`] / [`Self::output_modalities`] — precise
///   per-modality capability; replaces the retired `supports_vision: bool`
///   (retired in Session 3 — use `input_modalities.contains(&Modality::Image)`).
/// - [`Self::tokenizer`] — known tokenizer family for context-window
///   estimation; `None` when the provider does not disclose.
/// - [`Self::supported_parameters`] — API-level optional-parameter
///   capability flags (e.g. `reasoning_effort`, native `json_schema`).
/// - [`Self::supports_system_messages`] — `false` on `OpenAI` o-series
///   (the API silently reclassifies `system` → `developer`) and on
///   chat-less providers like Voyage (embedding only).
#[derive(Debug, Clone, PartialEq, Eq)]
// Four boolean capability flags that each name a distinct wire-protocol
// concern (temperature param, stop-sequences param, reasoning mode,
// system-message role). Lifting them into a bitflags enum would save
// nothing and obscure serde/Debug output.
#[allow(clippy::struct_excessive_bools)]
#[non_exhaustive]
pub struct ModelCapabilities {
    /// Which JSON field to use for token limits.
    pub token_limit_param: TokenLimitParam,
    /// Model accepts the `temperature` parameter.
    pub supports_temperature: bool,
    /// Model accepts `stop` / `stop_sequences` parameter.
    pub supports_stop_sequences: bool,
    /// Model exposes a reasoning / extended-thinking mode.
    ///
    /// Covers Claude `thinking_budget`, `OpenAI` `reasoning_effort`, `DeepSeek`
    /// reasoner path, and the `/reasoning` param on `OpenRouter` / mainland-
    /// China providers. Named `reasoning` (not `supports_thinking`) to match
    /// the 2026 industry convention — `LiteLLM` `supports_reasoning`, `models.dev`
    /// `reasoning`, `OpenRouter` `reasoning` — rather than the
    /// Anthropic-specific "thinking" jargon.
    pub reasoning: bool,
    /// Modalities this model accepts as input. Emitted by `build.rs` from
    /// the TOML `input_modalities = ["text", "image", …]` list, sorted by
    /// declaration order of [`Modality`] so a `binary_search` with the
    /// derived `Ord` works on the slice.
    ///
    /// Invariant: always contains [`Modality::Text`] for chat-capable
    /// models (enforced at build time in `validate_caps_patch`).
    #[cfg(feature = "capabilities")]
    pub input_modalities: &'static [Modality],
    /// Modalities this model produces as output. Usually `[Modality::Text]`
    /// for chat models; `[Modality::Audio]` for dedicated TTS, etc.
    #[cfg(feature = "capabilities")]
    pub output_modalities: &'static [Modality],
    /// Known tokenizer family for this model. `None` = provider does not
    /// disclose (e.g. xAI custom `SentencePiece`). Consumers fall back to
    /// conservative character-based estimation when `None`.
    #[cfg(feature = "capabilities")]
    pub tokenizer: Option<TokenizerFamily>,
    /// API-level parameter capability flags (sorted by declaration order).
    /// The Gate 2 key flag ([`ParamFlag::StructuredOutputNative`]) lives
    /// here — runtime dispatch uses `contains()` to pick the native
    /// `json_schema` path vs the prompt-engineering fallback.
    #[cfg(feature = "capabilities")]
    pub supported_parameters: &'static [ParamFlag],
    /// Whether this model accepts a `system` role message.
    ///
    /// `false` on:
    /// - `OpenAI` o-series (`o1*`, `o3*`, `o4*`): `system` is silently
    ///   reclassified as `developer` by the API — the runtime MUST send
    ///   `developer` as canonical to avoid undefined behaviour.
    /// - Voyage (embedding-only, no chat API at all).
    pub supports_system_messages: bool,
    /// Maximum context window size in tokens. `None` = not specified by the
    /// capability rule (caller should look up the provider's model entry).
    ///
    /// When both `context_window_tokens` and `max_output_tokens` are `Some`,
    /// the invariant `max_output_tokens <= context_window_tokens` is enforced
    /// at build time by `validate_caps_patch`.
    pub context_window_tokens: Option<u32>,
    /// Maximum output tokens the model can produce in a single response.
    /// `None` = not specified by the capability rule.
    pub max_output_tokens: Option<u32>,
}

impl Default for ModelCapabilities {
    fn default() -> Self {
        Self {
            token_limit_param: TokenLimitParam::MaxTokens,
            supports_temperature: true,
            supports_stop_sequences: true,
            reasoning: false,
            #[cfg(feature = "capabilities")]
            input_modalities: &[Modality::Text, Modality::Image],
            #[cfg(feature = "capabilities")]
            output_modalities: &[Modality::Text],
            #[cfg(feature = "capabilities")]
            tokenizer: None,
            #[cfg(feature = "capabilities")]
            supported_parameters: &[],
            supports_system_messages: true,
            context_window_tokens: None,
            max_output_tokens: None,
        }
    }
}

impl ModelCapabilities {
    /// Explicit constructor for every field.
    ///
    /// [`ModelCapabilities`] is `#[non_exhaustive]`, so external crates
    /// cannot use struct-literal syntax. `new()` is the required entry
    /// point (invariant #19 — per-crate `new()` on every
    /// `#[non_exhaustive]` struct).
    ///
    /// Consider [`ModelCapabilities::default`] + direct field assignment
    /// if only one or two fields differ from the baseline.
    ///
    /// The 4 Session 2b slice/option fields are only present under the
    /// `capabilities` feature; with the feature off, `new()` keeps the
    /// minimal Session 2a shape (4 args) for backwards-compat with
    /// `minimal`-feature consumers.
    #[must_use]
    // REASON: four booleans each name a distinct wire-protocol capability
    // (see the struct-level comment). Grouping them into a bitflags / bools
    // struct would obscure serde and Debug output with zero readability win.
    #[allow(clippy::fn_params_excessive_bools)]
    // REASON: 9 parameters = 1 per capability field; by design. Grouping
    // into sub-structs would hide the wire-protocol mapping.
    #[cfg_attr(feature = "capabilities", allow(clippy::too_many_arguments))]
    #[cfg(feature = "capabilities")]
    pub const fn new(
        token_limit_param: TokenLimitParam,
        supports_temperature: bool,
        supports_stop_sequences: bool,
        reasoning: bool,
        input_modalities: &'static [Modality],
        output_modalities: &'static [Modality],
        tokenizer: Option<TokenizerFamily>,
        supported_parameters: &'static [ParamFlag],
        supports_system_messages: bool,
        context_window_tokens: Option<u32>,
        max_output_tokens: Option<u32>,
    ) -> Self {
        Self {
            token_limit_param,
            supports_temperature,
            supports_stop_sequences,
            reasoning,
            input_modalities,
            output_modalities,
            tokenizer,
            supported_parameters,
            supports_system_messages,
            context_window_tokens,
            max_output_tokens,
        }
    }

    /// 4-argument constructor used when the `capabilities` feature is off
    /// (the pre-Session-2b shape). Covered by build-time `cargo hack
    /// --feature-powerset`.
    #[must_use]
    #[allow(clippy::fn_params_excessive_bools)]
    #[cfg(not(feature = "capabilities"))]
    pub const fn new(
        token_limit_param: TokenLimitParam,
        supports_temperature: bool,
        supports_stop_sequences: bool,
        reasoning: bool,
    ) -> Self {
        Self {
            token_limit_param,
            supports_temperature,
            supports_stop_sequences,
            reasoning,
            supports_system_messages: true,
            context_window_tokens: None,
            max_output_tokens: None,
        }
    }
}

#[cfg(test)]
mod model_capabilities_tests {
    use super::{ModelCapabilities, TokenLimitParam};

    #[test]
    #[cfg(feature = "capabilities")]
    fn new_builds_all_fields() {
        use crate::types::{Modality, ParamFlag, TokenizerFamily};
        const INPUT: &[Modality] = &[Modality::Text, Modality::Image];
        const OUTPUT: &[Modality] = &[Modality::Text];
        const PARAMS: &[ParamFlag] = &[ParamFlag::StructuredOutputNative];
        let caps = ModelCapabilities::new(
            TokenLimitParam::MaxCompletionTokens,
            false,
            true,
            true,
            INPUT,
            OUTPUT,
            Some(TokenizerFamily::O200k),
            PARAMS,
            false,
            Some(128_000),
            Some(32_768),
        );
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
        assert!(caps.supports_stop_sequences);
        assert!(caps.reasoning);
        assert_eq!(caps.input_modalities, INPUT);
        assert_eq!(caps.output_modalities, OUTPUT);
        assert_eq!(caps.tokenizer, Some(TokenizerFamily::O200k));
        assert_eq!(caps.supported_parameters, PARAMS);
        assert!(!caps.supports_system_messages);
        assert_eq!(caps.context_window_tokens, Some(128_000));
        assert_eq!(caps.max_output_tokens, Some(32_768));
    }

    #[test]
    #[cfg(not(feature = "capabilities"))]
    fn new_builds_all_fields_minimal() {
        let caps = ModelCapabilities::new(TokenLimitParam::MaxCompletionTokens, false, true, true);
        assert_eq!(caps.token_limit_param, TokenLimitParam::MaxCompletionTokens);
        assert!(!caps.supports_temperature);
        assert!(caps.supports_stop_sequences);
        assert!(caps.reasoning);
        assert!(caps.supports_system_messages);
    }

    #[test]
    #[cfg(feature = "capabilities")]
    fn new_with_default_values_matches_default_impl() {
        use crate::types::Modality;
        const INPUT: &[Modality] = &[Modality::Text, Modality::Image];
        const OUTPUT: &[Modality] = &[Modality::Text];
        let via_new = ModelCapabilities::new(
            TokenLimitParam::MaxTokens,
            true,
            true,
            false,
            INPUT,
            OUTPUT,
            None,
            &[],
            true,
            None,
            None,
        );
        assert_eq!(via_new, ModelCapabilities::default());
    }
}

/// Pricing for a known model pattern.
///
/// Two-pass matching: exact match first, then `contains()` fallback.
/// More specific patterns MUST appear before less specific ones.
///
/// # Session 3 additions (3 new axes)
///
/// The original Session 2a `input_per_million` + `output_per_million` pair
/// is insufficient for 2026 pricing shapes:
///
/// - **Prompt caching** (`Anthropic`, `OpenAI`, `DeepSeek`, `Gemini`) can
///   cut input cost 90% on cache hits.
/// - **Image / multimodal inputs** price per-image separately from per-token
///   (`OpenAI` vision, `Gemini`, `xAI` `Grok-4`).
/// - **Reasoning-token billing** — o-series, `GPT-5`, `Claude` `thinking`,
///   `DeepSeek` `R1` all surface thinking tokens with a distinct rate.
///
/// The three fields are `Option<f64>` — `None` means "this axis is not
/// disclosed / not billed differently / data not yet populated". The full
/// TOML-driven pricing schema (`data/model-pricing.toml`) that replaces
/// `ALL_PRICING` with rule-based per-axis overrides is tracked as
/// **Phase E2** in the next session.
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
    /// Price per million cached input tokens (prompt caching) in USD.
    /// `None` = provider does not disclose a cache rate or this model
    /// does not support prompt caching.
    pub cached_input_per_million: Option<f64>,
    /// Price per image input in USD (flat per-image rate; applies only on
    /// providers that bill images separately from tokens — `OpenAI`,
    /// `Gemini`, `xAI`).
    pub image_per_million: Option<f64>,
    /// Price per million reasoning / thinking tokens in USD. Used by
    /// o-series, `GPT-5`, `Claude` thinking, `DeepSeek` `R1`. `None` means
    /// reasoning tokens are billed at the `output_per_million` rate.
    pub reasoning_tokens_per_million: Option<f64>,
}

impl ModelPricing {
    /// Explicit constructor — required because [`ModelPricing`] is
    /// `#[non_exhaustive]` (invariant #19).
    ///
    /// 7 args after Session 3: the 4 original axes plus three
    /// `Option<f64>` for cached input, image, and reasoning tokens.
    /// Pass `None` for any axis the provider does not disclose.
    #[must_use]
    // REASON: 7 args = 1 per pricing axis by design; grouping into sub-structs
    // would hide the per-provider axis-matrix the pricing table encodes.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        provider: &'static str,
        model_pattern: &'static str,
        input_per_million: f64,
        output_per_million: f64,
        cached_input_per_million: Option<f64>,
        image_per_million: Option<f64>,
        reasoning_tokens_per_million: Option<f64>,
    ) -> Self {
        Self {
            provider,
            model_pattern,
            input_per_million,
            output_per_million,
            cached_input_per_million,
            image_per_million,
            reasoning_tokens_per_million,
        }
    }
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

impl CostEstimate {
    /// Explicit constructor — required because [`CostEstimate`] is
    /// `#[non_exhaustive]` (invariant #19).
    #[must_use]
    pub fn new(
        usd: f64,
        input_rate_per_million: f64,
        output_rate_per_million: f64,
        model: String,
        provider: String,
    ) -> Self {
        Self {
            usd,
            input_rate_per_million,
            output_rate_per_million,
            model,
            provider,
        }
    }
}

#[cfg(test)]
mod model_pricing_tests {
    use super::ModelPricing;

    #[test]
    fn new_builds_all_7_axes() {
        // Regression guard for the Session 3 field expansion. If a future
        // session adds an 8th axis without updating every ALL_PRICING site,
        // this test plus the 62 call sites force a hard compile error
        // (invariant #19 teeth — that is the entire point).
        let p = ModelPricing::new("Anthropic", "sonnet-4", 3.0, 15.0, Some(0.30), None, None);
        assert_eq!(p.provider, "Anthropic");
        assert_eq!(p.model_pattern, "sonnet-4");
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_million - 15.0).abs() < f64::EPSILON);
        assert_eq!(p.cached_input_per_million, Some(0.30));
        assert_eq!(p.image_per_million, None);
        assert_eq!(p.reasoning_tokens_per_million, None);
    }

    #[test]
    #[cfg(feature = "pricing")]
    fn three_session3_axes_default_to_none_everywhere_in_all_pricing() {
        // TODO(Phase-E2): replace the all-None assertions with value-range
        // checks once the TOML-driven pricing catalog lands and real
        // per-provider rates (cached_input, image, reasoning_tokens) are
        // populated. Until then, the 62 ALL_PRICING entries are None for
        // the 3 new Session 3 axes — real values await per-provider
        // research. This test guards against a silent regression where a
        // future commit flips one to Some() without updating the
        // corresponding test + documentation.
        for entry in crate::data::models::ALL_PRICING {
            assert_eq!(
                entry.cached_input_per_million, None,
                "{} / {}: cache pricing awaits Phase E2 research — keep None until populated in TOML",
                entry.provider, entry.model_pattern,
            );
            assert_eq!(
                entry.image_per_million, None,
                "{} / {}: image pricing awaits Phase E2",
                entry.provider, entry.model_pattern,
            );
            assert_eq!(
                entry.reasoning_tokens_per_million, None,
                "{} / {}: reasoning-token pricing awaits Phase E2",
                entry.provider, entry.model_pattern,
            );
        }
    }
}
