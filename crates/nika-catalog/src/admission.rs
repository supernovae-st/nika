// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Dated admission tariffs, distinct from the vendored models.dev snapshot.
//! Prices are conservative catalog estimates, never provider invoices.
use nika_error::cost::Cost;

/// An exact-route text tariff. Rates are nano-units of `currency` per token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InferenceTariff {
    /// Billing operator, which can differ from the API adapter namespace.
    pub billing_provider: &'static str,
    /// ISO currency. A non-USD observation cannot satisfy a USD ceiling.
    pub currency: &'static str,
    /// Exact parameter whose bound includes reasoning output on this route.
    pub output_token_param: &'static str,
    /// Exact adapter namespace; the billing operator is `billing_provider`.
    pub provider: &'static str,
    /// Qualified full request endpoints.
    pub endpoints: &'static [&'static str],
    /// Official request limits and complete usage semantics.
    pub limits_source: &'static str,
    /// Official evidence for the endpoint and selected wire model binding.
    pub route_source: &'static str,
    /// Exact wire model; aliases must be qualified separately.
    pub model: &'static str,
    /// Official price source, independently dated from models.dev.
    pub source: &'static str,
    /// SHA-256 of observed official document bytes, empty when unavailable.
    pub source_sha256: &'static str,
    /// SHA-256 of observed contract document bytes, empty when unavailable.
    pub limits_sha256: &'static str,
    /// Date of the source observation.
    pub as_of: &'static str,
    /// Input upper bound (full context, conservatively interpreting 1M).
    pub context_tokens: u64,
    /// Output upper bound including thinking.
    pub max_output_tokens: u32,
    /// Models the provider's own documentation names as served for `model`
    /// (a dated snapshot its replies report): each answers this same route.
    pub served_models: &'static [&'static str],
    input: i128,
    output: i128,
    cached: i128,
    cache_write: Option<i128>,
    /// The most tokens the route generates whatever the request names, when no request
    /// parameter bounds its reasoning; `None` when the requested output bounds it.
    generation: Option<u64>,
}
impl InferenceTariff {
    /// Explicitly declared complete-zero text tariff. The generator rejects
    /// all-zero rows without that declaration and every partly-zero tariff.
    #[must_use]
    pub fn is_declared_free(self) -> bool {
        self.input == 0 && self.output == 0 && self.cached == 0
    }

    /// Qualified `DeepSeek` chat text tariff at PEAK rates; no time discount
    /// is assumed. Other providers, legacy aliases and gateways stay unknown.
    #[must_use]
    pub fn deepseek(model: &str) -> Option<Self> {
        TARIFFS
            .iter()
            .find(|t| t.provider == "deepseek" && t.model == model)
            .copied()
    }
    /// Whether a reply that names `model` answered this route: its exact
    /// model or one the provider documents as served for it. Any other name
    /// leaves the charge unknown.
    #[must_use]
    pub fn serves(self, model: &str) -> bool {
        model == self.model || self.served_models.contains(&model)
    }

    /// Exact catalog binding; substring matches and aliases are not admitted.
    #[must_use]
    pub fn new(provider: &str, model: &str, endpoint: &str) -> Option<Self> {
        TARIFFS
            .iter()
            .find(|t| t.provider == provider && t.model == model && t.endpoints.contains(&endpoint))
            .copied()
    }

    /// Price the complete usage split at this pinned tariff with checked
    /// integer arithmetic. Thinking is INCLUDED in output, not added twice.
    #[must_use]
    pub fn price(self, input: u64, output: u64, cached: u64) -> Option<Cost> {
        if self.currency != "USD" {
            return None;
        }
        self.price_native(input, output, cached).map(Cost::new)
    }

    /// Native-currency estimate, never an invoice or a currency conversion.
    #[must_use]
    pub fn price_native(self, input: u64, output: u64, cached: u64) -> Option<i128> {
        let uncached = input.checked_sub(cached)?;
        let amount = i128::from(uncached)
            .checked_mul(self.input)?
            .checked_add(i128::from(cached).checked_mul(self.cached)?)?
            .checked_add(i128::from(output).checked_mul(self.output)?)?;
        Some(amount)
    }
    /// Price complete usage whose input includes `written` tokens written to
    /// the provider's cache, at this row's write rate. A row that states no
    /// write rate prices no write: that call's charge stays unknown.
    #[must_use]
    pub fn price_written(self, input: u64, output: u64, cached: u64, written: u64) -> Option<Cost> {
        if self.currency != "USD" {
            return None;
        }
        self.price_native_written(input, output, cached, written)
            .map(Cost::new)
    }

    /// Native-currency form of [`Self::price_written`].
    #[must_use]
    pub fn price_native_written(
        self,
        input: u64,
        output: u64,
        cached: u64,
        written: u64,
    ) -> Option<i128> {
        if written == 0 {
            return self.price_native(input, output, cached);
        }
        let write = self.cache_write?;
        let uncached = input.checked_sub(cached)?.checked_sub(written)?;
        i128::from(uncached)
            .checked_mul(self.input)?
            .checked_add(i128::from(cached).checked_mul(self.cached)?)?
            .checked_add(i128::from(written).checked_mul(write)?)?
            .checked_add(i128::from(output).checked_mul(self.output)?)
    }

    /// The output tokens a reservation of `requested` covers: the requested bound, or the
    /// route's whole generation when no request parameter bounds its reasoning (xAI documents
    /// that `max_completion_tokens` does not apply to reasoning, measured: 1,973 reasoning
    /// tokens under a bound of 40). An absent/zero/unqualified bound covers nothing.
    #[must_use]
    pub fn reserved_output(self, requested: u32) -> Option<u64> {
        if requested == 0 || requested > self.max_output_tokens {
            return None;
        }
        Some(self.generation.unwrap_or(u64::from(requested)))
    }

    /// Whether no request parameter bounds this route's reasoning: it may generate, and bill as
    /// output, more than the bound a request names.
    #[must_use]
    pub fn reasons_beyond_bound(self) -> bool {
        self.generation.is_some()
    }

    /// Reserve a full context at uncached rate and the output the request may
    /// generate ([`Self::reserved_output`]). A absent/zero/unqualified output bound is not
    /// admitted.
    #[must_use]
    pub fn reserve(self, output: u32) -> Option<Cost> {
        let output = self.reserved_output(output)?;
        let written = if self.cache_write.is_some_and(|write| write > self.input) {
            self.context_tokens
        } else {
            0
        };
        self.price_written(self.context_tokens, output, 0, written)
    }
}

/// Exact dated observations used by this binary. Callers must preserve the
/// full route and currency when displaying or persisting provenance.
pub fn tariffs() -> impl Iterator<Item = &'static InferenceTariff> {
    TARIFFS.iter()
}

/// Whether a dated route of `model` (`provider/name`) reasons beyond the output bound a request
/// names, and bills that reasoning as output (xAI documents that `max_completion_tokens` bounds
/// the visible output only). A model no row holds says nothing: `false`.
#[must_use]
pub fn reasons_beyond_bound(model: &str) -> bool {
    let Some((provider, name)) = model.split_once('/') else {
        return false;
    };
    tariffs().any(|row| row.provider == provider && row.serves(name) && row.reasons_beyond_bound())
}

include!(concat!(env!("OUT_DIR"), "/inference_admission.rs"));

#[cfg(test)]
mod route_tests {
    use super::*;
    /// A synthetic `@1.2` row: the generated table carries no such row yet.
    fn written_row(cache_write: Option<i128>) -> InferenceTariff {
        InferenceTariff {
            provider: "vendor",
            billing_provider: "vendor",
            currency: "USD",
            output_token_param: "max_completion_tokens",
            model: "model-a",
            endpoints: &["https://api.vendor.example/v1/chat/completions"],
            limits_source: "https://vendor.example/models/model-a",
            route_source: "https://vendor.example/api",
            source: "https://vendor.example/pricing",
            source_sha256: "",
            limits_sha256: "",
            as_of: "2026-10-12",
            context_tokens: 1_000,
            max_output_tokens: 100,
            served_models: &["model-a-2026-01-01"],
            input: 100,
            output: 400,
            cached: 10,
            cache_write,
            generation: None,
        }
    }
    /// `OpenAI`'s three earlier direct standard rows read their model pages: the price per field,
    /// the window and output cap, and the dated snapshot their replies report. No cache-write
    /// rate is listed, so a call that wrote the cache stays unknown.
    #[test]
    fn openai_direct_rows_read_their_model_pages() {
        const API: &str = "https://api.openai.com/v1/chat/completions";
        for (model, snapshot, context, output, rates) in [
            (
                "gpt-5.2",
                "gpt-5.2-2025-12-11",
                400_000,
                128_000,
                [1_750, 14_000, 175],
            ),
            (
                "gpt-5-mini",
                "gpt-5-mini-2025-08-07",
                400_000,
                128_000,
                [250, 2_000, 25],
            ),
            (
                "gpt-4o-mini",
                "gpt-4o-mini-2024-07-18",
                128_000,
                16_384,
                [150, 600, 75],
            ),
        ] {
            let row = InferenceTariff::new("openai", model, API).expect(model);
            assert_eq!(
                (row.context_tokens, row.max_output_tokens),
                (context, output)
            );
            assert_eq!(
                (row.currency, row.output_token_param),
                ("USD", "max_completion_tokens")
            );
            assert_eq!(row.served_models, [snapshot], "{model}");
            assert!(row.serves(model) && row.serves(snapshot), "{model}");
            assert!(!row.serves(&format!("{model}-2099-01-01")), "{model}");
            let [input, out, cached] = rates;
            assert_eq!(row.price_native(1, 0, 0), Some(input), "{model}");
            assert_eq!(row.price_native(0, 1, 0), Some(out), "{model}");
            assert_eq!(row.price_native(1, 0, 1), Some(cached), "{model}");
            assert_eq!(
                row.price_written(1_000, 10, 0, 100),
                None,
                "{model}: no write rate"
            );
        }
        for held in ["gpt-5.6", "gpt-6", "gpt-4o"] {
            assert!(
                InferenceTariff::new("openai", held, API).is_none(),
                "{held}"
            );
        }
    }

    /// xAI's two direct rows read their model pages of 2026-10-16: the input admitted below the
    /// 200k tier, and a reservation over the whole window, since no request parameter bounds
    /// xAI's reasoning. Other Grok models and endpoints stay unpriced here.
    /// A route that bounds no reasoning is named by its model; one whose requested output bounds
    /// it, and a model no row holds, are not.
    #[test]
    fn a_route_whose_reasoning_no_bound_holds_is_named() {
        assert!(reasons_beyond_bound("xai/grok-4.7"));
        assert!(!reasons_beyond_bound("deepseek/deepseek-flash"));
        assert!(!reasons_beyond_bound("vendor/unknown-model"));
        assert!(!reasons_beyond_bound("grok-4.7"));
    }

    #[test]
    fn xai_rows_reserve_the_whole_generation() {
        const API: &str = "https://api.x.ai/v1/chat/completions";
        for (model, window, [input, output, cached]) in [
            ("grok-4.7", 500_000_u64, [2_000_i128, 6_000, 500]),
            ("grok-4.3", 1_000_000, [1_250, 2_500, 200]),
        ] {
            let row = InferenceTariff::new("xai", model, API).expect(model);
            assert_eq!(
                (row.billing_provider, row.currency, row.output_token_param),
                ("xai", "USD", "max_tokens"),
                "{model}"
            );
            assert_eq!(row.context_tokens, 199_999, "{model}: below the 200k tier");
            assert_eq!(row.reserved_output(40), Some(window), "{model}: the window");
            assert_eq!(
                row.reserved_output(0),
                None,
                "{model}: no bound, no admission"
            );
            assert_eq!(
                row.reserve(40).map(|c| c.nano_usd),
                Some(199_999 * input + i128::from(window) * output),
                "{model}"
            );
            assert_eq!(row.price_native(1, 0, 1), Some(cached), "{model}");
        }
        for held in ["grok-4.6", "grok-3"] {
            assert!(InferenceTariff::new("xai", held, API).is_none(), "{held}");
        }
        let gpt = InferenceTariff::new(
            "openai",
            "gpt-6-sol",
            "https://api.openai.com/v1/chat/completions",
        )
        .expect("gpt-6-sol");
        assert_eq!(
            gpt.reserved_output(40),
            Some(40),
            "a bounded route reserves its request"
        );
    }

    /// Gemini's ten text rows read the pricing page and their model pages of 2026-10-16 at peak
    /// Standard rates, admitted on the API stem every qualification names and on the model's
    /// `:generateContent` URL the transport answers from; Pro models below their 200k tier.
    #[test]
    fn gemini_rows_read_the_pricing_page_at_peak_rates() {
        const STEM: &str = "https://generativelanguage.googleapis.com/v1beta";
        for (model, context, [input, output, cached]) in [
            ("gemini-4-argon", 1_048_576, [4_000, 20_000, 200]),
            ("gemini-3.8-flash", 1_048_576, [1_500, 7_500, 150]),
            ("gemini-3.6-flash", 1_048_576, [1_500, 7_500, 150]),
            ("gemini-3.5-flash-lite", 1_048_576, [300, 2_500, 30]),
            ("gemini-3.1-pro-preview", 200_000, [2_000, 12_000, 200]),
            ("gemini-3.1-flash-lite", 1_048_576, [250, 1_500, 25]),
            ("gemini-3-flash-preview", 1_048_576, [500, 3_000, 50]),
            ("gemini-2.5-pro", 200_000, [1_250, 10_000, 125]),
            ("gemini-2.5-flash", 1_048_576, [300, 2_500, 30]),
            ("gemini-2.5-flash-lite", 1_048_576, [100, 400, 10]),
        ] {
            let full = format!("{STEM}/models/{model}:generateContent");
            for endpoint in [STEM, full.as_str()] {
                let row = InferenceTariff::new("gemini", model, endpoint).expect(model);
                assert_eq!(
                    (row.context_tokens, row.max_output_tokens),
                    (context, 65_536),
                    "{model}"
                );
                assert_eq!(row.price_native(1, 0, 0), Some(input), "{model}");
                assert_eq!(row.price_native(0, 1, 0), Some(output), "{model}");
                assert_eq!(row.price_native(1, 0, 1), Some(cached), "{model}");
                assert_eq!(
                    row.reserved_output(512),
                    Some(512),
                    "{model}: bounded thinking"
                );
            }
            let other = format!("{STEM}/models/{model}:streamGenerateContent");
            assert!(
                InferenceTariff::new("gemini", model, &other).is_none(),
                "{model}"
            );
        }
        assert!(InferenceTariff::new("gemini", "gemini-2.0-flash", STEM).is_none());
    }

    /// The newer `OpenAI` models read the markdown form of their model pages of 2026-10-16: a
    /// 1,050,000-token window admitted up to the 272,000-token lower tier, 128K output, the
    /// GPT-6 family's cache-write rate and the snapshot each reply names.
    #[test]
    fn newer_openai_direct_rows_read_their_model_pages() {
        const API: &str = "https://api.openai.com/v1/chat/completions";
        for (model, served, [input, output, cached], write) in [
            (
                "gpt-5.5",
                Some("gpt-5.5-2026-04-23"),
                [5_000, 30_000, 500],
                None,
            ),
            ("gpt-5.6-sol", None, [4_000, 20_000, 400], None),
            ("gpt-6-astra", None, [10_000, 50_000, 1_000], Some(12_500)),
            ("gpt-6-sol", None, [2_000, 10_000, 200], Some(2_500)),
            ("gpt-6.1-sol", None, [2_000, 10_000, 100], Some(2_500)),
            ("gpt-6-luna", None, [100, 500, 10], Some(125)),
        ] {
            let row = InferenceTariff::new("openai", model, API).expect(model);
            assert_eq!(
                (row.context_tokens, row.max_output_tokens),
                (272_000, 128_000),
                "{model}"
            );
            assert_eq!(
                (row.currency, row.output_token_param),
                ("USD", "max_completion_tokens"),
                "{model}"
            );
            assert_eq!(row.served_models, served.as_slice(), "{model}");
            assert_eq!(row.price_native(1, 0, 0), Some(input), "{model}");
            assert_eq!(row.price_native(0, 1, 0), Some(output), "{model}");
            assert_eq!(row.price_native(1, 0, 1), Some(cached), "{model}");
            assert_eq!(
                row.price_written(2, 0, 0, 1).map(|c| c.nano_usd),
                write.map(|w| input + w),
                "{model}: the write rate, or a written call left unpriced"
            );
        }
    }
    /// Anthropic's four current Claude models read the official pricing page of 2026-10-15:
    /// input, output, cache hit and 5-minute cache write per token, 128K output, and a 1M
    /// window, except Claude Haiku 5.5, admitted up to its 100,000-token lower tier. Each reply
    /// names the model itself; earlier models and other endpoints stay unpriced here.
    #[test]
    fn anthropic_direct_rows_read_the_pricing_page() {
        const API: &str = "https://api.anthropic.com/v1/messages";
        for (model, context, [input, output, cached, write]) in [
            ("claude-sonnet-5-5", 1_048_576, [2_000, 10_000, 100, 2_500]),
            ("claude-opus-5-5", 1_048_576, [4_000, 20_000, 200, 5_000]),
            ("claude-haiku-5-5", 100_000, [100, 500, 10, 125]),
            ("claude-fable-5-1", 1_048_576, [10_000, 50_000, 250, 12_500]),
        ] {
            let row = InferenceTariff::new("anthropic", model, API).expect(model);
            assert_eq!(
                (row.context_tokens, row.max_output_tokens),
                (context, 128_000),
                "{model}"
            );
            assert_eq!(
                (row.currency, row.output_token_param, row.billing_provider),
                ("USD", "max_tokens", "anthropic"),
                "{model}"
            );
            assert_eq!(row.as_of, "2026-10-15", "{model}");
            assert!(row.served_models.is_empty(), "{model}");
            assert!(
                row.serves(model) && !row.serves("claude-sonnet-4-6"),
                "{model}"
            );
            assert_eq!(row.price_native(1, 0, 0), Some(input), "{model}");
            assert_eq!(row.price_native(0, 1, 0), Some(output), "{model}");
            assert_eq!(row.price_native(1, 0, 1), Some(cached), "{model}");
            assert_eq!(row.price_native_written(1, 0, 0, 1), Some(write), "{model}");
        }
        // The legacy models the catalogue still names read the pricing page of 2026-10-16 (live
        // gauntlet r09: the author offered them as « prix non connu ici »).
        for (model, (context, output_cap), [input, output, cached, write]) in [
            (
                "claude-sonnet-4-6",
                (1_048_576, 128_000),
                [3_000, 15_000, 300, 3_750],
            ),
            (
                "claude-haiku-4-5-20251001",
                (200_000, 64_000),
                [1_000, 5_000, 100, 1_250],
            ),
        ] {
            let row = InferenceTariff::new("anthropic", model, API).expect(model);
            assert_eq!(
                (row.context_tokens, row.max_output_tokens),
                (context, output_cap),
                "{model}"
            );
            assert_eq!(row.as_of, "2026-10-16", "{model}");
            assert_eq!(row.price_native(1, 0, 0), Some(input), "{model}");
            assert_eq!(row.price_native(0, 1, 0), Some(output), "{model}");
            assert_eq!(row.price_native(1, 0, 1), Some(cached), "{model}");
            assert_eq!(row.price_native_written(1, 0, 0, 1), Some(write), "{model}");
        }
        assert!(InferenceTariff::new("anthropic", "claude-opus-5", API).is_none());
        let gateway = "https://gateway.example/v1/messages";
        assert!(InferenceTariff::new("anthropic", "claude-sonnet-5-5", gateway).is_none());
    }
    /// Schema `@1.2`: a documented snapshot answers the route, nothing else
    /// does; a write is priced only at a stated write rate, and a write rate
    /// above input makes the reservation a whole written context.
    #[test]
    fn a_served_snapshot_answers_and_a_write_needs_its_rate() {
        let row = written_row(Some(125));
        assert!(row.serves("model-a") && row.serves("model-a-2026-01-01"));
        for other in ["model-a-2026-02-01", "model-b", "", "MODEL-A"] {
            assert!(!row.serves(other), "{other}");
        }
        // 1000 input = 600 uncached + 300 read + 100 written · 10 output.
        assert_eq!(
            row.price_native_written(1_000, 10, 300, 100),
            Some(600 * 100 + 300 * 10 + 100 * 125 + 10 * 400)
        );
        assert_eq!(
            row.price_written(1_000, 10, 300, 0),
            row.price(1_000, 10, 300)
        );
        assert_eq!(
            row.price_written(100, 0, 60, 41),
            None,
            "subsets above input"
        );
        assert_eq!(written_row(None).price_written(1_000, 10, 300, 100), None);
        assert_eq!(
            row.reserve(100).map(|c| c.nano_usd),
            Some(1_000 * 125 + 100 * 400)
        );
        assert_eq!(written_row(Some(90)).reserve(100), row.price(1_000, 100, 0));
        assert_eq!(written_row(None).reserve(100), row.price(1_000, 100, 0));
        assert_eq!(row.reserve(101), None);
    }
    #[test]
    fn a_declared_free_tariff_is_exact_and_still_bounds_output() {
        let endpoint = "https://openrouter.ai/api/v1/chat/completions";
        let model = "nvidia/nemotron-3-super-120b-a12b:free";
        let tariff = InferenceTariff::new("openrouter", model, endpoint).expect("curated zero");
        assert_eq!(tariff.reserve(4096), Some(Cost::zero()));
        assert_eq!(tariff.price(10, 10, 0), Some(Cost::zero()));
        assert_eq!(tariff.price(10, 10, 11), None, "contradictory usage");
        assert_eq!(tariff.reserve(0), None);
        assert_eq!(tariff.reserve(tariff.max_output_tokens + 1), None);
        for other in [
            "nvidia/nemotron-3-super-120b-a12b:free-extra",
            "nvidia/nemotron-3-super-120b-a12b",
            "unknown:free",
            "openrouter/free",
            "google/lyria-3-clip-preview",
        ] {
            assert!(InferenceTariff::new("openrouter", other, endpoint).is_none());
        }
        for other in [
            "https://gateway.example/v1/chat/completions",
            "https://openrouter.ai.evil.test/api/v1/chat/completions",
            "https://openrouter.ai/api/v1/chat/completions?free=true",
            "http://openrouter.ai/api/v1/chat/completions",
        ] {
            assert!(InferenceTariff::new("openrouter", model, other).is_none());
        }
        assert!(InferenceTariff::new("openai", model, endpoint).is_none());
    }
    #[test]
    fn deepseek_peak_and_cache_have_one_owning_source() {
        let t = InferenceTariff::new(
            "deepseek",
            "deepseek-v4-pro",
            "https://api.deepseek.com/v1/chat/completions",
        )
        .expect("qualified");
        assert_eq!(t.price(162, 178, 0).expect("USD").nano_usd, 918_720);
        assert_eq!(
            t.price(1_000_000, 0, 1_000_000).expect("cache").nano_usd,
            44_000_000
        );
        assert!(t.price(1, 0, 2).is_none());
        let p = crate::find_pricing_for("deepseek/deepseek-v4-pro").expect("projected");
        assert!((p.input_per_million - 1.32).abs() < f64::EPSILON);
        assert!((p.output_per_million - 3.96).abs() < f64::EPSILON);
        assert_eq!(p.cache_read_per_million, Some(0.044));
    }
    /// The five paid `OpenRouter` routes are admitted at their peak
    /// tariffs, bounded by the smallest limits of every endpoint, with a dated and hashed
    /// capacity source. The dated canonical slug is not an alias of the public id.
    #[test]
    fn paid_openrouter_routes_carry_peak_tariffs_and_dated_capacity() {
        let endpoint = "https://openrouter.ai/api/v1/chat/completions";
        for (model, context, output, (input, out, cached)) in [
            ("z-ai/glm-5.3-flash", 262_144, 128_000, (225, 1600, 99)),
            ("xiaomi/mimo-v2.6-flash", 1_000_000, 131_072, (190, 350, 50)),
            (
                "stepfun/step-5-preview",
                1_000_000,
                64_000,
                (1000, 2700, 50),
            ),
            ("tencent/hy4-preview", 1_000_000, 64_000, (834, 2501, 42)),
            // One token below the 272K long-prompt override; input at the write peak.
            ("openai/gpt-6-luna", 271_999, 128_000, (250, 1000, 20)),
        ] {
            let t = InferenceTariff::new("openrouter", model, endpoint).expect("admitted");
            assert_eq!((t.billing_provider, t.currency), ("openrouter", "USD"));
            assert!(!t.is_declared_free(), "{model}");
            assert_eq!(
                (t.context_tokens, t.max_output_tokens),
                (context, output),
                "{model}"
            );
            assert_eq!(
                [
                    t.price_native(1, 0, 0),
                    t.price_native(0, 1, 0),
                    t.price_native(1, 0, 1)
                ],
                [Some(input), Some(out), Some(cached)],
                "{model}"
            );
            assert_eq!(t.as_of, "2026-10-12");
            assert_eq!(t.limits_source, t.source);
            assert_eq!(t.limits_sha256.len(), 64);
            let dated = t
                .limits_source
                .strip_prefix("https://openrouter.ai/api/v1/models/")
                .and_then(|path| path.strip_suffix("/endpoints"))
                .expect("an endpoints listing");
            assert_ne!(dated, model, "the listing names the canonical slug");
            assert!(InferenceTariff::new("openrouter", dated, endpoint).is_none());
        }
        let p = crate::find_pricing_for("openrouter/z-ai/glm-5.3-flash").expect("projected");
        assert!((p.input_per_million - 0.225).abs() < f64::EPSILON);
        assert!((p.output_per_million - 1.6).abs() < f64::EPSILON);
        assert_eq!(p.cache_read_per_million, Some(0.099));
    }
    /// Every admitted route reads a dated window and output cap from
    /// its tariff, and the curated rows added for the admitted routes state
    /// where and when their own limits were read. Capacity is read in that
    /// order (tariff, curated row, the snapshot's exact row), never by
    /// `contains()`.
    #[cfg(feature = "providers")]
    #[test]
    fn every_admitted_route_carries_a_dated_window_and_output_cap() {
        let dated = |day: &str| {
            let b = day.as_bytes();
            b.len() == 10 && b[4] == b'-' && b[7] == b'-'
        };
        for t in tariffs() {
            assert!(
                t.context_tokens > 0
                    && t.max_output_tokens > 0
                    && t.limits_source.starts_with("https://")
                    && dated(t.as_of),
                "{}/{}",
                t.provider,
                t.model
            );
        }
        for (provider, model) in [
            ("openrouter", "z-ai/glm-5.3-flash"),
            ("openrouter", "xiaomi/mimo-v2.6-flash"),
            ("openrouter", "stepfun/step-5-preview"),
            ("openrouter", "tencent/hy4-preview"),
            ("openrouter", "openai/gpt-6-luna"),
            ("deepseek", "deepseek-flash"),
            ("deepseek", "deepseek-v4-pro"),
        ] {
            let row = crate::find_provider(provider)
                .and_then(|p| p.models.iter().find(|m| m.model == model));
            assert!(
                row.is_some_and(|m| m.context_window_tokens > 0 && m.max_output_tokens > 0),
                "{provider}/{model}: no curated row"
            );
            let facts = crate::model_facts(provider, model);
            assert!(
                facts.is_some_and(|f| {
                    f.limits_source
                        .is_some_and(|source| source.starts_with("https://"))
                        && f.limits_as_of.is_some_and(dated)
                }),
                "{provider}/{model}: no sourced limits"
            );
        }
    }
    #[test]
    fn scaleway_route_is_eur_and_never_a_usd_allowance() {
        let url = "https://api.scaleway.ai/v1/chat/completions";
        let t = InferenceTariff::new("openai", "gpt-oss-120b", url).expect("observed exact route");
        assert_eq!((t.billing_provider, t.currency), ("scaleway", "EUR"));
        assert_eq!(t.price_native(1_000_000, 1_000_000, 0), Some(750_000_000));
        assert!(t.price(1_000_000, 1_000_000, 0).is_none());
        assert!(t.reserve(512).is_none());
        for endpoint in [
            "https://api.openai.com/v1/chat/completions",
            "https://api.scaleway.ai/project/v1/chat/completions",
            "https://api.scaleway.ai/v1/chat/completions?free=true",
            "https://api.scaleway.ai/v1/responses",
            "http://127.0.0.1/v1/chat/completions",
        ] {
            assert!(InferenceTariff::new("openai", "gpt-oss-120b", endpoint).is_none());
        }
        for model in ["gpt-oss-120b-new", "openai/gpt-oss-120b:fp4", "gpt-oss-20b"] {
            assert!(InferenceTariff::new("openai", model, url).is_none());
        }
        assert!(InferenceTariff::new("scaleway", "gpt-oss-120b", url).is_none());
    }
}
