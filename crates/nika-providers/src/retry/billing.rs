// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Project existing tariff ownership into immutable per-dispatch observations.
use super::BillingRoute;
use crate::admission::DeclaredTariff;
use nika_kernel::ai::provider::{InferResponse, TokenUsage, UsageCompleteness};
use nika_types::cost::{Cost, InferenceCall, InferenceRoute};

pub(crate) fn record(
    slot: &mut Option<InferenceCall>,
    route: Option<&BillingRoute>,
    response: Option<&InferResponse>,
    declared: Option<&DeclaredTariff>,
) {
    let requested = slot.as_ref().and_then(|c| c.requested_endpoint.clone());
    let mut call = observe(route, response, declared);
    call.requested_endpoint = requested;
    *slot = Some(call);
}

pub(crate) fn observe(
    route: Option<&BillingRoute>,
    response: Option<&InferResponse>,
    declared: Option<&DeclaredTariff>,
) -> InferenceCall {
    let mut call = InferenceCall::new();
    if let Some(r) = response {
        call.usage = r.usage_reported.then(|| r.usage.clone());
        call.usage_complete =
            r.usage_reported && r.usage_completeness == UsageCompleteness::Complete;
        call.response_model.clone_from(&r.gen_ai.response_model);
        call.request_id.clone_from(&r.request_id);
    }
    let Some(route) = route else {
        return call;
    };
    call.route = Some(InferenceRoute::new(
        route.provider.clone(),
        route.model.clone(),
        route.endpoint.clone(),
    ));
    call.pricing = Some(route.observation().to_string());
    let declared =
        declared.filter(|d| d.matches_route(&route.provider, &route.model, &route.endpoint));
    if let Some(d) = declared {
        call.pricing = Some(d.observation().to_string());
    }
    let Some(usage) = &call.usage else {
        return call;
    };
    let Some(written) = cache_writes(usage) else {
        return call;
    };
    let (input, output, cached) = (
        usage.input_tokens,
        usage.output_tokens,
        usage.cache_read_tokens.unwrap_or(0),
    );
    if !call.usage_complete
        || call.response_model.as_deref() != Some(route.model.as_str())
        || cached
            .checked_add(written)
            .is_none_or(|subsets| subsets > input)
    {
        return call;
    }
    if let Some(d) = declared {
        // A declared tariff states no cache-write rate: a call that wrote stays unknown.
        if written == 0 {
            call.estimated_usd = d
                .price_native(input, output, cached)
                .filter(|_| d.currency() == "USD")
                .map(Cost::new);
        }
    } else if let Some(tariff) = route.tariff() {
        // Nor does an admission tariff.
        if written == 0 {
            call.estimated_usd = tariff.price(input, output, cached);
        }
    } else {
        // The vendored snapshot remains the owner of other exact first-party rows; a cache write
        // prices at the row's own write rate, the 5-minute one (the only TTL Nika asks for).
        call.estimated_usd = snapshot_estimate(route, input, output, cached, written);
        if call.estimated_usd.is_some() {
            let pin = nika_catalog::pricing_snapshot();
            call.pricing = Some(
                serde_json::json!({
                    "route": route, "kind": "catalog_estimate_not_invoice",
                    "currency": "USD", "source": pin.source, "as_of": pin.as_of,
                    "source_sha256_16": pin.source_sha256_16,
                    "table_schema": nika_catalog::PRICING_SCHEMA
                })
                .to_string(),
            );
        }
    }
    call
}

/// The tokens a call wrote to the provider's cache, in whichever meter its wire reports them
/// (Anthropic's creation, another wire's write); `None` when both are reported and disagree.
fn cache_writes(usage: &TokenUsage) -> Option<u64> {
    match (usage.cache_creation_tokens, usage.cache_write_tokens) {
        (Some(created), Some(written)) if created != written => None,
        (created, written) => Some(created.or(written).unwrap_or(0)),
    }
}

fn snapshot_estimate(
    route: &BillingRoute,
    input: u64,
    output: u64,
    cached: u64,
    written: u64,
) -> Option<Cost> {
    let official = crate::profile::seed().iter().any(|p| {
        p.id == route.provider
            && if matches!(p.wire, crate::profile::WireFormat::Gemini) {
                route.endpoint
                    == format!(
                        "{}/models/{}:generateContent",
                        p.base_url.trim_end_matches('/'),
                        route.model
                    )
            } else {
                p.base_url == route.endpoint
            }
    });
    let row = nika_catalog::find_pricing_scoped(&route.provider, &route.model)?;
    if !official
        || row.model_pattern != route.model
        || (cached > 0 && row.cache_read_per_million.is_none())
        || (written > 0 && row.cache_write_per_million.is_none())
    {
        return None;
    }
    let model = format!("{}/{}", route.provider, route.model);
    let nano = (nika_catalog::estimate_cost_usage_for(&model, input, output, cached, written)?.usd
        * 1_000_000_000.0)
        .round();
    if !nano.is_finite() || !(0.0..1e38).contains(&nano) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "finite nonnegative rounded nano-USD range checked above"
    )]
    Some(Cost::new(nano as i128))
}
