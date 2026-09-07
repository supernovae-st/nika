// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE model-spend computation and its honest-absence WHY — split
//! from `dispatch.rs` at the 1500-line file cap when the usage
//! split joined the seam; the bodies moved verbatim.

use nika_types::cost::UnpricedReason;

/// the split of what a FAILED verb had already burned — the same
/// numbers `price_failed_spend` turns into dollars, so `task_failed`
/// explains its own `cost_usd`.
pub(super) fn failed_usage_split(
    spend: Option<&nika_types::cost::SpendOnFailure>,
) -> Option<Box<crate::usage::UsageSplit>> {
    crate::usage::UsageSplit::of(&spend?.usage).carried()
}

pub(super) fn price_failed_spend(
    spend: Option<&nika_types::cost::SpendOnFailure>,
) -> (Option<f64>, Option<String>, Option<UnpricedReason>) {
    let Some(incurred) = spend else {
        return (None, None, None);
    };
    let (llm, unpriced) = match incurred.model_resolved.as_deref() {
        Some(model) if usage_has_signal(&incurred.usage) => spend_for_model(model, &incurred.usage),
        _ => (None, None),
    };
    let cost_usd = match (llm, incurred.tools_cost_usd) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(0.0) + b.unwrap_or(0.0)),
    };
    (cost_usd, incurred.model_resolved.clone(), unpriced)
}

/// The ONE model-spend computation — catalog price × the FULL usage
/// split, with the honest WHY when no number can exist.
///
/// Order matters: an unpriced model class (mock · local · uncataloged)
/// outranks a silent provider — « local compute · not priced » is the
/// actionable truth for a local model even when it also reported no
/// usage. A PRICED model with a degenerate split (all meters zero —
/// e.g. a stream that never carried usage) must NOT price to $0.00:
/// the spend is real but unknowable, so it stays absent + named
/// (`provider_did_not_report_usage`) — the fake-zero gate.
pub(super) fn spend_for_model(
    model: &str,
    usage: &nika_kernel::provider::TokenUsage,
) -> (Option<f64>, Option<UnpricedReason>) {
    if nika_catalog::find_pricing_for(model).is_none() {
        return (None, Some(unpriced_reason_for(model)));
    }
    if !usage_has_signal(usage) {
        return (None, Some(UnpricedReason::ProviderDidNotReportUsage));
    }
    let cache_write = usage
        .cache_write_tokens
        .unwrap_or(0)
        .saturating_add(usage.cache_creation_tokens.unwrap_or(0));
    let cost = nika_catalog::estimate_cost_usage_for(
        model,
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens.unwrap_or(0),
        cache_write,
    )
    .map(|e| e.usd);
    (cost, None)
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
