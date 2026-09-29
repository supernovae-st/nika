// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE model-spend computation over the per-dispatch evidence this crate
//! produces, and its honest-absence WHY. Descended verbatim from
//! `nika-runtime`'s `dispatch/spend.rs` (2026-09-28, the runtime's 15k wall):
//! the provider already owns the `InferenceCall` evidence, its tariffs and
//! its settlement refusals.

use nika_types::cost::{InferenceCall, UnpricedReason};

/// Fold qualified per-dispatch estimates. Never apply one route to aggregate usage.
#[must_use]
pub fn spend_for_calls(calls: &[InferenceCall]) -> (Option<f64>, Option<UnpricedReason>) {
    let mut known = None;
    let mut unknown = None;
    for call in calls {
        if let Some(cost) = call.known_estimate() {
            known = Some(known.unwrap_or(0.0) + cost.to_usd_f64());
        } else {
            unknown = Some(unpriced_call_reason(call));
        }
    }
    (known, unknown)
}

/// Why one dispatch carries no known estimate. Missing or partial meters are
/// the provider's silence. Complete meters under a recorded USD tariff were
/// refused (the admitted output bound or context, the response model, the
/// meters themselves): the price exists and the usage was rejected (E17-F4).
/// Anything else has no USD price for its route.
fn unpriced_call_reason(call: &InferenceCall) -> UnpricedReason {
    if call.usage.is_none() || !call.usage_complete {
        UnpricedReason::ProviderDidNotReportUsage
    } else if call.pricing.as_deref().is_some_and(names_usd_tariff) {
        UnpricedReason::UsageRejected
    } else {
        UnpricedReason::MissingCatalogPrice
    }
}

/// Whether the pricing provenance recorded at dispatch names a USD tariff for
/// its route, catalog or operator-declared. An `unknown` kind or another
/// currency does not. Read for the reason only, never to price.
fn names_usd_tariff(pricing: &str) -> bool {
    let Ok(provenance) = serde_json::from_str::<serde_json::Value>(pricing) else {
        return false;
    };
    let text = |key: &str| provenance.get(key).and_then(serde_json::Value::as_str);
    matches!(
        text("kind"),
        Some("catalog_estimate_not_invoice" | "user_declared_estimate_not_invoice")
    ) && text("currency") == Some("USD")
}

/// The spend a FAILED verb had already incurred: the known USD of its calls
/// plus its tools, the model it resolved, and why part of it is unpriced.
#[must_use]
pub fn price_failed_spend(
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
#[must_use]
pub fn spend_for_model(
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
mod tests {
    use super::*;
    use crate::admission::{DeclaredTariff, TariffUnit, UnknownCostChoice};
    use crate::retry::BillingRoute;
    use nika_kernel::ai::provider::{InferResponse, StopReason, TokenUsage, UsageCompleteness};

    const DEEPSEEK: &str = "https://api.deepseek.com/v1/chat/completions";
    const PRO: &str = "deepseek-v4-pro";

    fn answered(model: &str) -> InferResponse {
        let mut r = InferResponse::new(vec![], TokenUsage::new(10, 5), StopReason::EndTurn);
        r.usage_completeness = UsageCompleteness::Complete;
        r.gen_ai.response_model = Some(model.into());
        r
    }

    /// The call the wires record for `response` on this route.
    fn observed(
        route: (&str, &str, &str),
        response: &InferResponse,
        declared: Option<&DeclaredTariff>,
    ) -> InferenceCall {
        let route = BillingRoute::new(route.0.into(), route.1.into(), route.2.into());
        let mut call = None;
        crate::retry::record(&mut call, route.as_ref(), Some(response), declared);
        call.expect("recorded")
    }

    fn reason(call: InferenceCall) -> (Option<f64>, Option<UnpricedReason>) {
        spend_for_calls(&[call])
    }

    fn declared(currency: &str) -> DeclaredTariff {
        let choice = UnknownCostChoice::new(
            "candidate".into(),
            "invocation".into(),
            "deepseek".into(),
            PRO.into(),
            DEEPSEEK.into(),
            1,
            64,
            std::time::Duration::from_secs(1),
        )
        .expect("choice");
        DeclaredTariff::new(
            &choice,
            "operator-contract".into(),
            currency.into(),
            TariffUnit::PerMillionTokens,
            [2.0, 3.0, 1.0],
            "contract-local".into(),
            "v1".into(),
        )
        .expect("tariff")
    }

    /// E17-F4: the wire clears the estimate when the admission refuses a
    /// settlement (over the output bound or the context); the tariff stays.
    #[test]
    fn a_refused_settlement_under_a_known_tariff_is_usage_rejected() {
        let mut call = observed(("deepseek", PRO, DEEPSEEK), &answered(PRO), None);
        assert!(call.known_estimate().is_some(), "this usage has a price");
        call.estimated_usd = None;
        assert_eq!(reason(call), (None, Some(UnpricedReason::UsageRejected)));
    }

    #[test]
    fn another_response_model_or_contradictory_meters_are_usage_rejected() {
        let mut cached = answered(PRO);
        cached.usage.cache_read_tokens = Some(11);
        let usd = declared("USD");
        for (response, tariff) in [
            (answered("deepseek-v4-flash"), None),
            (cached, None),
            (answered("deepseek-v4-flash"), Some(&usd)),
        ] {
            let call = observed(("deepseek", PRO, DEEPSEEK), &response, tariff);
            assert_eq!(reason(call), (None, Some(UnpricedReason::UsageRejected)));
        }
    }

    #[test]
    fn missing_and_partial_usage_stay_the_providers_silence() {
        let mut silent = answered(PRO);
        silent.usage_reported = false;
        let mut partial = answered(PRO);
        partial.usage_completeness = UsageCompleteness::Unknown;
        for response in [silent, partial] {
            let call = observed(("deepseek", PRO, DEEPSEEK), &response, None);
            let silence = Some(UnpricedReason::ProviderDidNotReportUsage);
            assert_eq!(reason(call), (None, silence));
        }
        let unanswered = InferenceCall::new();
        let silence = Some(UnpricedReason::ProviderDidNotReportUsage);
        assert_eq!(reason(unanswered), (None, silence));
    }

    #[test]
    fn a_route_without_a_usd_tariff_is_a_missing_price() {
        let gateway = (
            "deepseek",
            PRO,
            "https://gateway.example/v1/chat/completions",
        );
        let eur = (
            "openai",
            "gpt-oss-120b",
            "https://api.scaleway.ai/v1/chat/completions",
        );
        let euro = declared("EUR");
        let mut unrouted = observed(gateway, &answered(PRO), None);
        (unrouted.route, unrouted.pricing) = (None, None);
        for call in [
            observed(gateway, &answered(PRO), None),
            observed(eur, &answered("gpt-oss-120b"), None),
            observed(("deepseek", PRO, DEEPSEEK), &answered(PRO), Some(&euro)),
            unrouted,
        ] {
            let missing = Some(UnpricedReason::MissingCatalogPrice);
            assert_eq!(reason(call), (None, missing));
        }
    }

    #[test]
    fn a_priced_call_beside_a_rejected_one_keeps_its_known_subtotal() {
        let priced = observed(("deepseek", PRO, DEEPSEEK), &answered(PRO), None);
        let known = priced.known_estimate().expect("priced").to_usd_f64();
        let rejected = observed(("deepseek", PRO, DEEPSEEK), &answered("other"), None);
        assert_eq!(
            spend_for_calls(&[priced, rejected]),
            (Some(known), Some(UnpricedReason::UsageRejected))
        );
    }
}
