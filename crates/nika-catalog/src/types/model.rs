// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Model capabilities and pricing types.
//!
//! Capabilities are resolved via pattern-matching function (not phf) because
//! model names are open-ended. Pricing uses 2-pass matching (exact + contains).

use crate::types::JsonMode;
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
    /// Runtime dispatch checks individual flags (e.g. `ReasoningEffort`,
    /// `PromptCaching`) to enable optional parameters.
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
    /// Structured JSON output capability level.
    ///
    /// `None` = not specified by the capability rule (treat as no JSON
    /// mode support). `Some(Schema)` = full `json_schema` enforcement.
    /// `Some(Object)` = `json_object` format only (no schema validation).
    /// `Some(Unavailable)` = explicitly marked as no JSON support.
    ///
    /// Replaces `ParamFlag::StructuredOutputNative` (Session 4a) with
    /// finer granularity — the runtime dispatches: `Schema` → native
    /// `json_schema`; `Object` → `json_object`; else → prompt fallback.
    pub json_mode: Option<JsonMode>,
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
            json_mode: None,
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
        json_mode: Option<JsonMode>,
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
            json_mode,
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
            json_mode: None,
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
        const PARAMS: &[ParamFlag] = &[ParamFlag::PromptCaching];
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
            Some(crate::types::JsonMode::Schema),
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
        assert_eq!(caps.json_mode, Some(crate::types::JsonMode::Schema));
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
            None,
        );
        assert_eq!(via_new, ModelCapabilities::default());
    }
}

/// A sourced energy fact for one model — Wh per million OUTPUT tokens,
/// with the two axes that make two honest numbers comparable.
///
/// # Why OUTPUT tokens (`wh_per_mtok_out`)
///
/// Decode dominates measured inference energy (≥96% in the ML.energy
/// v3.0 methodology), so the honest per-token figure is per OUTPUT
/// token — a per-total figure would dilute the number with nearly-free
/// prefill and reward long prompts.
///
/// # The two axes: who measured × what was measured
///
/// - [`Self::provenance`] — WHO produced the number and how much to
///   trust it: `measured-local` (our own probe on this machine) ·
///   `independent-measured` (third-party benchmark, e.g. ML.energy) ·
///   `vendor-claim` (the provider's own figure) ·
///   `independent-estimate` (third-party modelling, not measurement).
/// - [`Self::scope`] — WHAT the number covers: `gpu` (accelerator
///   only) · `device` (whole host) · `fleet` (host + idle + PUE, the
///   Google-style datacenter figure). A GPU-only figure is roughly
///   half a fleet figure for the same model — without this axis two
///   truthful numbers are silently incomparable.
///
/// Both axes are validated against their closed sets at build time.
/// [`Self::source`] and [`Self::measured_at`] pin the claim to a
/// citable origin and a month — a number without either is refused by
/// the generator (the estate law applied to data: absence over guess).
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct ModelEnergy {
    /// Watt-hours per million OUTPUT tokens. Always finite and > 0 —
    /// a zero would claim free inference; the null is the absent table.
    pub wh_per_mtok_out: f64,
    /// Who produced the number: `"measured-local"` ·
    /// `"independent-measured"` · `"vendor-claim"` ·
    /// `"independent-estimate"` (closed set, build-time validated).
    pub provenance: &'static str,
    /// What the number covers: `"gpu"` · `"device"` · `"fleet"`
    /// (closed set, build-time validated).
    pub scope: &'static str,
    /// Citable origin of the figure (free text — benchmark name,
    /// hardware, runtime version). Never empty.
    pub source: &'static str,
    /// Month the figure was produced, ISO `YYYY-MM`. Energy figures
    /// rot with hardware and runtime generations; a dateless figure
    /// is not a fact.
    pub measured_at: &'static str,
}

impl ModelEnergy {
    /// Explicit constructor — required because [`ModelEnergy`] is
    /// `#[non_exhaustive]` (invariant #19) and the generated static is
    /// const-constructed.
    #[must_use]
    pub const fn new(
        wh_per_mtok_out: f64,
        provenance: &'static str,
        scope: &'static str,
        source: &'static str,
        measured_at: &'static str,
    ) -> Self {
        Self {
            wh_per_mtok_out,
            provenance,
            scope,
            source,
            measured_at,
        }
    }
}

/// One row of the vendored upstream model snapshot, keyed by pattern.
///
/// Two-pass matching: exact match first, then `contains()` fallback.
/// More specific patterns MUST appear before less specific ones.
///
/// # What this row carries (schema `@1.3`)
///
/// The type is named for its original and still-primary job — pricing —
/// but a row is what the upstream snapshot says about a model, and since
/// `@1.2` that includes four non-price facts (see *Upstream model facts*
/// below). They ride here rather than in a parallel table because they
/// come from the same payload, on the same day, under the same
/// [`PricingSnapshot`] provenance: splitting them would create a second
/// thing to keep in sync with no second source to sync it against.
///
/// Since `@1.3` the row also carries three *research facts* (see below)
/// whose source is NOT the upstream payload: the research lane vendors
/// them with a per-claim source, and a row without a sourced claim
/// simply omits them.
///
/// # Pricing axes (5 axes, Session 4a Phase E2)
///
/// - **Input/output** — the two universal axes, always populated.
/// - **Cache write** — cost to write tokens into the prompt cache (typically
///   1.25× the input rate). `None` = no prompt caching or not disclosed.
/// - **Cache read** — cost to read cached tokens (typically 0.1× input).
///   `None` = no prompt caching or not disclosed.
/// - **Image** — flat per-image rate on providers that bill images separately
///   from tokens (`OpenAI` vision, `Gemini`, `xAI` `Grok-4`). `None` = not
///   applicable or not disclosed.
/// - **Reasoning tokens** — per-million rate for thinking tokens. Used by
///   o-series, `GPT-5`, `Claude` thinking, `DeepSeek` `R1`. `None` = billed
///   at the `output_per_million` rate.
///
/// # Upstream model facts (4 fields, schema `@1.2`)
///
/// [`Self::max_output_tokens`] · [`Self::context_window_tokens`] ·
/// [`Self::open_weights`] · [`Self::status`] — carried verbatim from the
/// snapshot. Every one is `Option`: `None` means *upstream did not
/// disclose*, and MUST NOT be read as a zero, a `false`, or a default.
/// A caller that needs a number when the snapshot is silent has to supply
/// its own and say so.
///
/// # Research facts (3 fields, schema `@1.3`)
///
/// [`Self::energy`] · [`Self::tokenizer`] · [`Self::determinism`] —
/// vendored by the research lane from primary sources, never mirrored
/// from the upstream payload (which does not carry them). Every one is
/// `Option`: `None` means *no sourced fact has been vendored*, and MUST
/// NOT be read as "zero energy" / "unknown tokenizer is fine" / "not
/// replayable". A field without a source stays absent — absence over
/// guess.
///
/// Generated from `data/model-pricing.toml` at build time.
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
    /// Price per million tokens to **write** into the prompt cache in USD.
    /// Typically ~1.25× the input rate. `None` = not disclosed or no caching.
    pub cache_write_per_million: Option<f64>,
    /// Price per million tokens to **read** from the prompt cache in USD.
    /// Typically ~0.1× the input rate. `None` = not disclosed or no caching.
    pub cache_read_per_million: Option<f64>,
    /// Price per image input in USD (flat per-image rate; applies only on
    /// providers that bill images separately from tokens).
    pub image_per_million: Option<f64>,
    /// Price per million reasoning / thinking tokens in USD. `None` means
    /// reasoning tokens are billed at the `output_per_million` rate.
    pub reasoning_tokens_per_million: Option<f64>,
    /// Most tokens the model will emit in one response, as upstream
    /// declares it. `None` = not disclosed (never "unlimited", never 0).
    ///
    /// This is the ceiling a workflow's `max_tokens` has to fit under.
    ///
    /// Not comparable to [`Self::context_window_tokens`] in general — see
    /// that field. Zero is never stored: upstream writes `0` for models
    /// where the limit does not apply (image · video · speech), and the
    /// generator drops it rather than assert a false ceiling of nothing.
    pub max_output_tokens: Option<u32>,
    /// Context window in tokens, as upstream declares it. `None` = not
    /// disclosed. Zero is never stored (same reason as above).
    ///
    /// For text models this bounds input + output together, so
    /// `max_output_tokens <= context_window_tokens` normally holds — but
    /// it is NOT an invariant here and is deliberately not enforced. On
    /// non-text models the two count different units (a speech model's
    /// output tokens are audio), and 10 of the 633 rules in the vendored
    /// 2026-07-28 snapshot legitimately have output > context — count them:
    /// `max_output_tokens > context_window_tokens`. Enforcing the
    /// relationship would fail the build on a truthful snapshot. The
    /// invariant belongs to our hand-authored capability rules
    /// (`NIKA-014`), which describe models we chose to support, not to a
    /// mirror of somebody else's catalog.
    pub context_window_tokens: Option<u32>,
    /// Whether the weights are published, as upstream declares it.
    /// `None` = not disclosed.
    ///
    /// This is what separates "costs nothing to call" from "we have no
    /// price for it". An open-weights model run locally has a real cost
    /// that this table cannot know; a missing price is not a free model.
    pub open_weights: Option<bool>,
    /// Lifecycle marker from upstream — `"deprecated"` and `"beta"` are
    /// the values seen so far. `None` = upstream said nothing, which for
    /// this field is the ordinary case (5% of rows carry one).
    ///
    /// Deliberately a string, not an enum: this vocabulary belongs to
    /// upstream and grows without asking us. A closed enum would turn
    /// "they added a value" into a failed refresh. Match on
    /// [`Self::is_deprecated`] for the predicate that matters.
    pub status: Option<&'static str>,
    /// Sourced energy figure — Wh per million OUTPUT tokens with the
    /// provenance × scope axes (see [`ModelEnergy`]). `None` = no
    /// sourced measurement or estimate has been vendored — which says
    /// nothing about the model's actual draw.
    pub energy: Option<ModelEnergy>,
    /// Tokenizer family id (e.g. `"o200k_base"` · `"claude-v3"`).
    /// `None` = not vendored / not disclosed — consumers fall back to
    /// conservative character-based estimation.
    ///
    /// A string, not [`crate::types::TokenizerFamily`]: this row
    /// mirrors research on the OPEN ecosystem vocabulary (new models
    /// ship new tokenizers without asking us), while the enum is the
    /// closed set our capability rules chose to support. Shape-checked
    /// only (`[a-z0-9._-]`, ≤64).
    pub tokenizer: Option<&'static str>,
    /// Replay class — what re-running the same request can promise:
    /// `"seed"` (a seed parameter exists and the provider aims for
    /// reproducible sampling) · `"best-effort"` (temperature 0 gets
    /// close, no guarantee) · `"none"` (no replay story). Closed set,
    /// build-time validated — this is OUR vocabulary, not upstream's.
    /// `None` = not yet researched, which is NOT `"none"`: absence of
    /// the fact, not a fact of absence.
    pub determinism: Option<&'static str>,
}

impl ModelPricing {
    /// Explicit constructor — required because [`ModelPricing`] is
    /// `#[non_exhaustive]` (invariant #19).
    ///
    /// 15 args: identity (2) · the 2 base rates · 4 optional rate axes ·
    /// the 4 upstream model facts added in schema `@1.2` · the 3
    /// research facts added in schema `@1.3`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        provider: &'static str,
        model_pattern: &'static str,
        input_per_million: f64,
        output_per_million: f64,
        cache_write_per_million: Option<f64>,
        cache_read_per_million: Option<f64>,
        image_per_million: Option<f64>,
        reasoning_tokens_per_million: Option<f64>,
        max_output_tokens: Option<u32>,
        context_window_tokens: Option<u32>,
        open_weights: Option<bool>,
        status: Option<&'static str>,
        energy: Option<ModelEnergy>,
        tokenizer: Option<&'static str>,
        determinism: Option<&'static str>,
    ) -> Self {
        Self {
            provider,
            model_pattern,
            input_per_million,
            output_per_million,
            cache_write_per_million,
            cache_read_per_million,
            image_per_million,
            reasoning_tokens_per_million,
            max_output_tokens,
            context_window_tokens,
            open_weights,
            status,
            energy,
            tokenizer,
            determinism,
        }
    }

    /// Whether upstream marks this model as deprecated.
    ///
    /// The one [`Self::status`] value worth a typed predicate: teaching or
    /// defaulting to a model its own provider has retired is a live defect,
    /// and callers should not have to know the spelling to catch it.
    #[must_use]
    pub fn is_deprecated(&self) -> bool {
        matches!(self.status, Some("deprecated"))
    }
}

/// Provenance of the vendored pricing snapshot — the machine-readable
/// answer to « how old is your pricing, and where did it come from? ».
///
/// Generated from `data/model-pricing.toml` `[meta]` at build time
/// (schema `@1.1`; the date/sha lived in TOML comments before and no
/// surface could report them). Counts (rules · providers) are NEVER
/// carried here — they derive from [`crate::all_pricing`] at read time
/// (the born-stale law: an embedded count drifts, a derived one can't).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct PricingSnapshot {
    /// Upstream source URL (e.g. `https://models.dev/api.json`).
    pub source: &'static str,
    /// Snapshot date, ISO `YYYY-MM-DD` (the day the refresh ran).
    pub as_of: &'static str,
    /// First 16 hex chars of the upstream payload's sha256.
    pub source_sha256_16: &'static str,
}

impl PricingSnapshot {
    /// Explicit constructor — required because [`PricingSnapshot`] is
    /// `#[non_exhaustive]` (invariant #19) and the generated static is
    /// const-constructed.
    #[must_use]
    pub const fn new(
        source: &'static str,
        as_of: &'static str,
        source_sha256_16: &'static str,
    ) -> Self {
        Self {
            source,
            as_of,
            source_sha256_16,
        }
    }
}

#[cfg(test)]
mod pricing_snapshot_tests {
    use super::PricingSnapshot;

    #[test]
    fn new_builds_all_fields() {
        const S: PricingSnapshot = PricingSnapshot::new(
            "https://models.dev/api.json",
            "2026-07-07",
            "aabbccddeeff0011",
        );
        assert_eq!(S.source, "https://models.dev/api.json");
        assert_eq!(S.as_of, "2026-07-07");
        assert_eq!(S.source_sha256_16, "aabbccddeeff0011");
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
    use super::{ModelEnergy, ModelPricing};

    #[test]
    fn new_builds_all_8_axes() {
        let p = ModelPricing::new(
            "Anthropic",
            "sonnet-4",
            3.0,
            15.0,
            Some(3.75),
            Some(0.30),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(p.provider, "Anthropic");
        assert_eq!(p.model_pattern, "sonnet-4");
        assert!((p.input_per_million - 3.0).abs() < f64::EPSILON);
        assert!((p.output_per_million - 15.0).abs() < f64::EPSILON);
        assert_eq!(p.cache_write_per_million, Some(3.75));
        assert_eq!(p.cache_read_per_million, Some(0.30));
        assert_eq!(p.image_per_million, None);
        assert_eq!(p.reasoning_tokens_per_million, None);
    }

    #[test]
    fn new_builds_the_four_upstream_facts() {
        let p = ModelPricing::new(
            "anthropic",
            "claude-haiku-4-5",
            1.0,
            5.0,
            None,
            None,
            None,
            None,
            Some(64_000),
            Some(200_000),
            Some(false),
            Some("beta"),
            None,
            None,
            None,
        );
        assert_eq!(p.max_output_tokens, Some(64_000));
        assert_eq!(p.context_window_tokens, Some(200_000));
        assert_eq!(p.open_weights, Some(false));
        assert_eq!(p.status, Some("beta"));
    }

    #[test]
    fn absent_upstream_facts_stay_none_not_zero() {
        // The inversion this whole schema bump exists to prevent: a row
        // upstream is silent about must never read as 0 / false, which
        // would report the least-known model as the most-bounded one.
        let p = ModelPricing::new(
            "ollama", "qwen3", 0.0, 0.0, None, None, None, None, None, None, None, None, None,
            None, None,
        );
        assert_eq!(p.max_output_tokens, None);
        assert_eq!(p.context_window_tokens, None);
        assert_eq!(p.open_weights, None);
        assert_eq!(p.status, None);
        assert!(!p.is_deprecated(), "unknown status is not deprecated");
    }

    #[test]
    fn is_deprecated_only_fires_on_the_upstream_spelling() {
        let mk = |s: Option<&'static str>| {
            ModelPricing::new(
                "openai", "gpt-4", 30.0, 60.0, None, None, None, None, None, None, None, s, None,
                None, None,
            )
        };
        assert!(mk(Some("deprecated")).is_deprecated());
        assert!(!mk(Some("beta")).is_deprecated());
        assert!(!mk(None).is_deprecated());
    }

    #[test]
    fn new_builds_the_three_research_facts() {
        const E: ModelEnergy = ModelEnergy::new(
            42.0,
            "independent-measured",
            "gpu",
            "ml.energy v3.0 · B200 · vLLM 0.11.1",
            "2025-12",
        );
        let p = ModelPricing::new(
            "openai",
            "gpt-4o",
            2.5,
            10.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            Some(E),
            Some("o200k_base"),
            Some("seed"),
        );
        let e = p.energy.expect("energy vendored");
        assert!((e.wh_per_mtok_out - 42.0).abs() < f64::EPSILON);
        assert_eq!(e.provenance, "independent-measured");
        assert_eq!(e.scope, "gpu");
        assert_eq!(e.source, "ml.energy v3.0 · B200 · vLLM 0.11.1");
        assert_eq!(e.measured_at, "2025-12");
        assert_eq!(p.tokenizer, Some("o200k_base"));
        assert_eq!(p.determinism, Some("seed"));
    }

    #[test]
    fn absent_research_facts_stay_none_not_defaults() {
        // Same inversion guard as the upstream facts: a model nobody has
        // measured must never read as zero-energy or non-replayable.
        let p = ModelPricing::new(
            "mistral",
            "mistral-large",
            2.0,
            6.0,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        );
        assert_eq!(p.energy, None);
        assert_eq!(p.tokenizer, None);
        assert_eq!(p.determinism, None);
    }
}
