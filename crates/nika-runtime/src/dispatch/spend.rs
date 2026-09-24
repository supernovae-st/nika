// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE model-spend computation and its honest-absence WHY — split
//! from `dispatch.rs` at the 1500-line file cap when the usage
//! split joined the seam; the bodies moved verbatim.

use nika_types::cost::{InferenceCall, UnpricedReason};

/// Fold qualified per-dispatch estimates. Never apply one route to aggregate usage.
pub(super) fn spend_for_calls(calls: &[InferenceCall]) -> (Option<f64>, Option<UnpricedReason>) {
    let mut known = None;
    let mut unknown = None;
    for call in calls {
        if let Some(cost) = call.known_estimate() {
            known = Some(known.unwrap_or(0.0) + cost.to_usd_f64());
        } else {
            unknown = Some(if call.usage.is_none() || !call.usage_complete {
                UnpricedReason::ProviderDidNotReportUsage
            } else {
                UnpricedReason::MissingCatalogPrice
            });
        }
    }
    (known, unknown)
}

/// the split of what a FAILED verb had already burned — the same
/// numbers `price_failed_spend` turns into dollars, so `task_failed`
/// explains its own `cost_usd`.
pub(super) fn failed_usage_split(
    spend: Option<&nika_types::cost::SpendOnFailure>,
) -> Option<Box<crate::usage::UsageSplit>> {
    let spend = spend?;
    crate::usage::UsageSplit::of(&spend.usage)
        .with_calls(&spend.inference_calls)
        .carried()
}

pub(super) fn price_failed_spend(
    spend: Option<&nika_types::cost::SpendOnFailure>,
) -> (Option<f64>, Option<String>, Option<UnpricedReason>) {
    let Some(incurred) = spend else {
        return (None, None, None);
    };
    let (llm, unpriced) = if incurred.inference_calls.is_empty() {
        match incurred.model_resolved.as_deref() {
            Some(model) if usage_has_signal(&incurred.usage) => {
                spend_for_model(model, &incurred.usage)
            }
            _ => (None, None),
        }
    } else {
        spend_for_calls(&incurred.inference_calls)
    };
    let cost_usd = match (llm, incurred.tools_cost_usd) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    };
    (cost_usd, incurred.model_resolved.clone(), unpriced)
}

/// Older verb/failure DTOs carry a model but no billing route. Their usage
/// survives, but it cannot justify a publisher's tariff at an arbitrary gateway.
pub(super) fn spend_for_model(
    model: &str,
    usage: &nika_kernel::provider::TokenUsage,
) -> (Option<f64>, Option<UnpricedReason>) {
    let reason = if usage_has_signal(usage) || nika_catalog::find_pricing_for(model).is_none() {
        unpriced_reason_for(model)
    } else {
        UnpricedReason::ProviderDidNotReportUsage
    };
    (None, Some(reason))
}

/// Whether the provider reported ANY billable meter — zero-everything is
/// « did not report », never a $0.00.
fn usage_has_signal(usage: &nika_kernel::provider::TokenUsage) -> bool {
    usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.cache_read_tokens.is_some_and(|n| n > 0)
        || usage.cache_write_tokens.is_some_and(|n| n > 0)
        || usage.cache_creation_tokens.is_some_and(|n| n > 0)
}

/// Why a model string has no catalog price (`mock` · local · missing).
fn unpriced_reason_for(model: &str) -> UnpricedReason {
    match model.split_once('/').map(|(provider, _)| provider) {
        Some("mock") => UnpricedReason::MockProvider,
        Some(prefix) => match nika_catalog::find_provider(prefix) {
            Some(row) if !row.requires_key => UnpricedReason::LocalModel,
            _ => UnpricedReason::MissingCatalogPrice,
        },
        None => UnpricedReason::MissingCatalogPrice,
    }
}

#[cfg(test)]
mod billing_tests;
