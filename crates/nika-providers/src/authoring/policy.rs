// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Route-qualified completion limits, separate from price and total-request authority.
use crate::{ProviderRegistry, ProvidersConfig};
use std::time::Duration;

/// The legacy bounded Session output ceiling (the compiler's own maximum: a
/// reasoning seat spends part of it on its reasoning, and a complete candidate needs the rest).
pub const AUTHORING_MAX_TOKENS: u32 = 32_768;
/// The first native generation's output limit: a REPORTED truncation spends one of the native
/// repairs to widen it, up to [`AUTHORING_MAX_TOKENS`] — never a transport retry.
pub const AUTHORING_INITIAL_TOKENS: u32 = 16_384;
/// Wall time one Session authoring call may take.
pub const AUTHORING_TIMEOUT: Duration = Duration::from_secs(180);
/// Legacy explicitly bounded Session repair rounds; continuous preparation does not use it.
pub const AUTHORING_REPAIRS: u32 = 3;
/// Historical bounded-review compatibility; not the continuous compiler's worst case.
pub const AUTHORING_CALLS_PER_COMPILE: u32 = 2 + 1 + AUTHORING_REPAIRS;

/// One completion's technical output bound and transport deadline. Not a monetary ceiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompletionBounds {
    /// Output requested before a reported truncation permits widening.
    pub initial_tokens: u32,
    /// Qualified technical ceiling, or the existing default on an unknown route.
    pub max_tokens: u32,
    /// The existing transport's deadline for one completion, not a total Session timeout.
    pub timeout: Duration,
}
/// The compatibility policy for callers that have not selected continuous preparation.
#[must_use]
pub fn legacy_completion_bounds(harness: bool) -> CompletionBounds {
    CompletionBounds {
        initial_tokens: AUTHORING_INITIAL_TOKENS,
        max_tokens: AUTHORING_MAX_TOKENS,
        timeout: if harness {
            Duration::from_secs(300)
        } else {
            AUTHORING_TIMEOUT
        },
    }
}
/// Resolve against the effective route, not a provider nickname or an arbitrary gateway.
/// Exact admission rows own their technical limits independently of currency. Other native
/// catalog routes use their published output cap; unknown routes retain the existing wire default.
#[must_use]
pub fn completion_bounds(model: &str, harness: bool, config: ProvidersConfig) -> CompletionBounds {
    let mut bounds = CompletionBounds {
        initial_tokens: AUTHORING_INITIAL_TOKENS,
        max_tokens: AUTHORING_MAX_TOKENS,
        timeout: Duration::from_secs(600),
    };
    if harness {
        return bounds;
    }
    let Some((provider, name)) = model.split_once('/') else {
        return bounds;
    };
    let registry = ProviderRegistry::without_http(config);
    let provider = crate::canonical_provider(provider);
    let Some(profile) = registry.profiles().iter().find(|p| p.id == provider) else {
        return bounds;
    };
    let Some(endpoint) = registry.effective_base_url(provider) else {
        return bounds;
    };
    let wire = profile.resolve_model(name);
    let tariff = nika_catalog::admission::InferenceTariff::new(provider, wire, endpoint);
    if let Some(tariff) = tariff {
        bounds.max_tokens = tariff.max_output_tokens;
        if tariff.billing_provider == "deepseek" {
            bounds.initial_tokens = 131_072;
        }
    } else if endpoint == profile.base_url
        && let Some(cap) =
            nika_catalog::find_pricing_scoped(provider, wire).and_then(|p| p.max_output_tokens)
    {
        bounds.max_tokens = cap;
    }
    bounds.initial_tokens = bounds.initial_tokens.min(bounds.max_tokens);
    bounds
}

/// Ordinary labels retain their existing finite ceiling.
const LABEL_CEILING_TOKENS: u32 = 1024;
/// Catalog-known reasoning shares output tokens with the visible label.
/// This finite first-call ceiling matches the compiler's reasoning draft
/// floor; it does not guarantee an answer and never triggers a larger retry.
const REASONING_LABEL_CEILING_TOKENS: u32 = 4096;

/// Select the label default only; caller-supplied infer limits stay intact.
#[must_use]
pub fn label_ceiling(model: &str) -> u32 {
    let reasoning = model.split_once('/').is_some_and(|(provider, model)| {
        // The mock catalog row claims every capability for fixtures.
        !provider.eq_ignore_ascii_case("mock")
            && nika_catalog::model_capabilities(provider, model).reasoning
    });
    if reasoning {
        REASONING_LABEL_CEILING_TOKENS
    } else {
        LABEL_CEILING_TOKENS
    }
}

#[cfg(test)]
mod tests;
