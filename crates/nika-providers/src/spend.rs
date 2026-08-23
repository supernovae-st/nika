// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE model-spend computation — catalog price × the FULL usage
//! split, with the honest WHY when no number can exist.
//!
//! Lived in `nika-runtime` dispatch until the 15k wall; infer (HTTP and
//! harness) must price through this function so they cannot disagree.

use nika_kernel::ai::provider::TokenUsage;
use nika_types::cost::UnpricedReason;

/// Catalog price × the FULL usage split, with the honest WHY when no
/// number can exist.
///
/// Order matters: an unpriced model class (mock · local · uncataloged)
/// outranks a silent provider — « local compute · not priced » is the
/// actionable truth for a local model even when it also reported no
/// usage. A PRICED model with a degenerate split (all meters zero —
/// e.g. a stream that never carried usage) must NOT price to $0.00:
/// the spend is real but unknowable, so it stays absent + named
/// (`provider_did_not_report_usage`) — the fake-zero gate.
#[must_use]
pub fn spend_for_model(model: &str, usage: &TokenUsage) -> (Option<f64>, Option<UnpricedReason>) {
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
#[must_use]
pub fn usage_has_signal(usage: &TokenUsage) -> bool {
    usage.input_tokens > 0
        || usage.output_tokens > 0
        || usage.cache_read_tokens.is_some_and(|n| n > 0)
        || usage.cache_write_tokens.is_some_and(|n| n > 0)
        || usage.cache_creation_tokens.is_some_and(|n| n > 0)
}

/// Classify WHY a model string has no catalog price. The provider
/// prefix is the discriminator: `mock` = the test backend · a keyless
/// catalog provider = a local server (sovereign path — not priced,
/// never « free ») · anything else = not in the vendored catalog.
#[must_use]
pub fn unpriced_reason_for(model: &str) -> UnpricedReason {
    match model.split_once('/').map(|(provider, _)| provider) {
        Some("mock") => UnpricedReason::MockProvider,
        Some(prefix) => match nika_catalog::find_provider(prefix) {
            Some(row) if !row.requires_key => UnpricedReason::LocalModel,
            _ => UnpricedReason::MissingCatalogPrice,
        },
        None => UnpricedReason::MissingCatalogPrice,
    }
}
