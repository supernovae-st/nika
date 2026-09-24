// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Dated admission tariffs, distinct from the vendored models.dev snapshot.
//! Prices are conservative catalog estimates, never provider invoices.
use nika_error::cost::Cost;

/// A qualified text tariff. All rates are integral nano-USD per token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InferenceTariff {
    /// Exact provider namespace, never an arbitrary compatible gateway.
    pub provider: &'static str,
    /// Qualified full request endpoints.
    pub endpoints: &'static [&'static str],
    /// Official request limits and complete usage semantics.
    pub limits_source: &'static str,
    /// Exact wire model; aliases must be qualified separately.
    pub model: &'static str,
    /// Official price source, independently dated from models.dev.
    pub source: &'static str,
    /// SHA-256 of the observed official pricing document bytes.
    pub source_sha256: &'static str,
    /// SHA-256 of the observed request/usage contract document bytes.
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
        let uncached = input.checked_sub(cached)?;
        let amount = i128::from(uncached)
            .checked_mul(self.input)?
            .checked_add(i128::from(cached).checked_mul(self.cached)?)?
            .checked_add(i128::from(output).checked_mul(self.output)?)?;
        Some(Cost::new(amount))
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

include!(concat!(env!("OUT_DIR"), "/inference_admission.rs"));
