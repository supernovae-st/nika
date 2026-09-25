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
    input: i128,
    output: i128,
    cached: i128,
}
impl InferenceTariff {
    /// Qualified `DeepSeek` chat text tariff at PEAK rates; no time discount
    /// is assumed. Other providers, legacy aliases and gateways stay unknown.
    #[must_use]
    pub fn deepseek(model: &str) -> Option<Self> {
        TARIFFS
            .iter()
            .find(|t| t.provider == "deepseek" && t.model == model)
            .copied()
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
    /// Reserve a full context at uncached rate and the exact requested
    /// maximum output. A absent/zero/unqualified output bound is not admitted.
    #[must_use]
    pub fn reserve(self, output: u32) -> Option<Cost> {
        if output == 0 || output > self.max_output_tokens {
            return None;
        }
        self.price(self.context_tokens, u64::from(output), 0)
    }
}

/// Exact dated observations used by this binary. Callers must preserve the
/// full route and currency when displaying or persisting provenance.
pub fn tariffs() -> impl Iterator<Item = &'static InferenceTariff> {
    TARIFFS.iter()
}

include!(concat!(env!("OUT_DIR"), "/inference_admission.rs"));

#[cfg(test)]
mod route_tests {
    use super::*;
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
