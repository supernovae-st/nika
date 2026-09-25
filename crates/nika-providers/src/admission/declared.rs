// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Operator-declared estimate, never a provider-guaranteed price or a grant.
use super::{ProviderError, UnknownCostChoice, denied};
use serde::Serialize;

/// Explicit price denominator. No implicit per-token/per-million conversion.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TariffUnit {
    /// One million tokens of the named meter.
    PerMillionTokens,
}
/// Finite nonnegative declared rates with an exact route and dated/versioned
/// provenance. This object is an estimate and cannot bypass monetary policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct DeclaredTariff {
    provider: String,
    model: String,
    endpoint: String,
    billing_provider: String,
    currency: String,
    unit: TariffUnit,
    nano_per_token: [u64; 3],
    provenance: String,
    version: String,
}
impl DeclaredTariff {
    /// Rates are input/output/cache-read per million tokens, rounded UP to
    /// integral nano-currency per token. Zero must be explicitly declared.
    /// # Errors
    /// NaN, infinity, negative/overflow rates, missing currency/biller/source/version.
    #[allow(
        clippy::too_many_arguments,
        reason = "each argument is an independently required tariff provenance axis"
    )]
    pub fn new(
        scope: &UnknownCostChoice,
        billing_provider: String,
        currency: String,
        unit: TariffUnit,
        rates: [f64; 3],
        provenance: String,
        version: String,
    ) -> Result<Self, ProviderError> {
        if currency.len() != 3
            || !currency.bytes().all(|b| b.is_ascii_uppercase())
            || [&billing_provider, &provenance, &version]
                .iter()
                .any(|v| v.trim().is_empty())
        {
            return Err(denied(
                "declared tariff requires explicit billing provider/currency/unit/provenance/version",
            ));
        }
        let mut nano_per_token = [0; 3];
        for (i, rate) in rates.into_iter().enumerate() {
            let nano = (rate * 1000.0).ceil();
            // Well below u64 and f64 integer limits. Overlarge rates are invalid,
            // never saturating/zero and never transformed into credit.
            if !rate.is_finite() || rate < 0.0 || !nano.is_finite() || nano > 1_000_000_000_000.0 {
                return Err(denied(
                    "declared tariff must be finite, nonnegative and representable",
                ));
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "finite nonnegative integer and range checked above"
            )]
            {
                nano_per_token[i] = nano as u64;
            }
        }
        Ok(Self {
            provider: scope.provider().into(),
            model: scope.model().into(),
            endpoint: scope.endpoint().into(),
            billing_provider,
            currency,
            unit,
            nano_per_token,
            provenance,
            version,
        })
    }
    /// Check exact scope before attaching a declared price to a choice.
    #[must_use]
    pub fn matches(&self, scope: &UnknownCostChoice) -> bool {
        self.provider == scope.provider()
            && self.model == scope.model()
            && self.endpoint == scope.endpoint()
    }
    /// Exact selected route; a declared tariff never follows a redirect.
    #[must_use]
    pub fn matches_route(&self, provider: &str, model: &str, endpoint: &str) -> bool {
        self.provider == provider && self.model == model && self.endpoint == endpoint
    }
    /// Provenance of a user-declared estimate, explicitly distinct from catalog data.
    #[must_use]
    pub fn observation(&self) -> serde_json::Value {
        serde_json::json!({
            "kind": "user_declared_estimate_not_invoice",
            "route": { "provider": self.provider, "model": self.model, "endpoint": self.endpoint },
            "billing_provider": self.billing_provider, "currency": self.currency,
            "unit": self.unit, "nano_per_token": self.nano_per_token,
            "provenance": self.provenance, "version": self.version,
            "usd_conversion": null
        })
    }

    /// Currency of the operator's estimate.
    #[must_use]
    pub fn currency(&self) -> &str {
        &self.currency
    }
    /// Checked native-currency estimate. Input includes its cache subset.
    #[must_use]
    pub fn price_native(&self, input: u64, output: u64, cached: u64) -> Option<i128> {
        let uncached = input.checked_sub(cached)?;
        i128::from(uncached)
            .checked_mul(i128::from(self.nano_per_token[0]))?
            .checked_add(i128::from(output).checked_mul(i128::from(self.nano_per_token[1]))?)?
            .checked_add(i128::from(cached).checked_mul(i128::from(self.nano_per_token[2]))?)
    }
}
