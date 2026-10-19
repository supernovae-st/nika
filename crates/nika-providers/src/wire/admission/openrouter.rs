// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Closed `OpenRouter` text receipt: the account charge it reports, independent
//! from exact route/tariff admission.
use nika_types::cost::Cost;
use serde_json::{Map, Value};

/// The usage-accounting document names `cost` "the total amount charged to your
/// account" and `cost_details.upstream_inference_cost` "the actual cost charged
/// by the upstream AI provider"; only a lookup by generation id keeps that
/// upstream cost for BYOK requests (0 or null for every other request). Inline,
/// a paid non-BYOK reply carries it beside its charge (observed live on
/// 2026-10-16: `z-ai/glm-5.3-flash`, equal to `cost`). The markdown bytes of the
/// document, fetched twice on 2026-10-16, have this SHA-256 (the same bytes as
/// the 2026-09-27 observation; the 2026-10-12 one was
/// `79fe65dbbb9ca28506854d975a577271a1615f9e149b69f22d2828b995ca3724`).
const USAGE_SOURCE: &str = "https://openrouter.ai/docs/cookbook/administration/usage-accounting.md";
const USAGE_SHA256: &str = "62e9339731737dff7e35c84ffac84607be44e415a236a5458852328dd21f7cf2";
const USAGE_AS_OF: &str = "2026-10-16";

/// The documented zero axes of a text reply. A zero charge proves them all 0.
const ZERO_AXES: &[(&str, &str)] = &[
    ("prompt_tokens_details", "cache_write_tokens"),
    ("prompt_tokens_details", "audio_tokens"),
    ("prompt_tokens_details", "video_tokens"),
    ("completion_tokens_details", "audio_tokens"),
    ("completion_tokens_details", "image_tokens"),
    ("completion_tokens_details", "accepted_prediction_tokens"),
    ("completion_tokens_details", "rejected_prediction_tokens"),
];
/// A paid reply may write the cache: the write is inside its charge.
const PAID_ZERO_AXES: &[(&str, &str)] = &[
    ("prompt_tokens_details", "audio_tokens"),
    ("prompt_tokens_details", "video_tokens"),
    ("completion_tokens_details", "audio_tokens"),
    ("completion_tokens_details", "image_tokens"),
    ("completion_tokens_details", "accepted_prediction_tokens"),
    ("completion_tokens_details", "rejected_prediction_tokens"),
];
const CACHE_WRITE: &[(&str, &str)] = &[("prompt_tokens_details", "cache_write_tokens")];

/// <https://openrouter.ai/docs/cookbook/administration/usage-accounting>
/// <https://openrouter.ai/docs/api/api-reference/chat/create-a-chat-completion>
/// (response schema; bytes fetched 2026-10-12 have SHA-256
/// `4093d50b3114fe4c8d1dff383e6c28e6189196b57c19a6c784a3fae993f2970f`).
/// The reported `cost` is the account charge of this reply, never an invoice or
/// a seat qualification. It is known only with explicit non-BYOK identity (a
/// BYOK reply's upstream bill stays unknown), complete text meters and the
/// closed key set. A zero charge must also prove every documented zero axis and
/// a zero decomposition: a zero tariff does not turn a missing charge into zero.
/// A paid charge keeps the media axes at zero and may carry a cache write and a
/// nonnegative decomposition beside its total. Any other axis, fee or key
/// leaves the charge unknown.
pub(super) fn reported_charge(v: &Value) -> Option<Cost> {
    let u = v.get("usage").and_then(Value::as_object)?;
    let charge = nano_usd(u.get("cost")?)?;
    let paid = charge > 0;
    let (zero_axes, subsets) = if paid {
        (PAID_ZERO_AXES, CACHE_WRITE)
    } else {
        (ZERO_AXES, &[][..])
    };
    let known = super::complete_text_meters(u, zero_axes, subsets)
        && u.get("is_byok").and_then(Value::as_bool) == Some(false)
        && u.keys().all(|k| {
            super::is_text_meter(k)
                || matches!(
                    k.as_str(),
                    "cost"
                        | "is_byok"
                        | "cost_details"
                        | "server_tool_use"
                        | "server_tool_use_details"
                )
        })
        && no_server_tools(u)
        && decomposition_known(u.get("cost_details"), paid);
    known.then_some(Cost::new(charge))
}

/// No server tool ran: the legacy `server_tool_use` names zero web searches,
/// and the documented `server_tool_use_details` counts are zero or null.
fn no_server_tools(u: &Map<String, Value>) -> bool {
    let legacy = u.get("server_tool_use").is_none_or(|tools| {
        tools.as_object().is_some_and(|tools| {
            tools
                .iter()
                .all(|(k, n)| k == "web_search_requests" && n.as_u64() == Some(0))
        })
    });
    let details = match u.get("server_tool_use_details") {
        None | Some(Value::Null) => true,
        Some(Value::Object(tools)) => tools.iter().all(|(k, n)| {
            matches!(
                k.as_str(),
                "tool_calls_executed" | "tool_calls_requested" | "web_search_requests"
            ) && (n.is_null() || n.as_u64() == Some(0))
        }),
        Some(_) => false,
    };
    legacy && details
}

/// The required aggregate charge is known. Missing/null decomposition is not a
/// missing total; a present unknown fee or a server-tool charge remains unknown.
/// A zero charge has a zero decomposition. A paid decomposition, the upstream
/// inference cost included, is the provider's own breakdown beside the settled
/// total, read for its shape only: the account is charged `cost`.
fn decomposition_known(details: Option<&Value>, paid: bool) -> bool {
    let Some(details) = details.filter(|v| !v.is_null()) else {
        return true;
    };
    let Some(details) = details.as_object() else {
        return false;
    };
    details.iter().all(|(key, value)| match key.as_str() {
        "upstream_inference_cost"
        | "upstream_inference_prompt_cost"
        | "upstream_inference_completions_cost"
            if paid =>
        {
            value.is_null() || nano_usd(value).is_some()
        }
        // Null means not applicable for this explicitly non-BYOK request.
        // It is not evidence of a measured zero upstream invoice.
        "upstream_inference_cost" => value.is_null() || zero(value),
        "server_tool_cost" if paid => value.is_null() || zero(value),
        "upstream_inference_prompt_cost"
        | "upstream_inference_completions_cost"
        | "server_tool_cost" => zero(value),
        _ => false,
    })
}

fn zero(value: &Value) -> bool {
    value.as_f64() == Some(0.0)
}

/// A reported USD amount as nano-USD, rounded UP to the next nano so a positive
/// charge never reads as zero. Only a nonnegative JSON number reads (a JSON
/// number is always finite); its decimal form is converted exactly, never
/// through a float product.
fn nano_usd(value: &Value) -> Option<i128> {
    let Value::Number(number) = value else {
        return None;
    };
    let text = number.to_string();
    match text.strip_prefix('-') {
        // A negative zero is zero; any other negative amount is no charge.
        Some(magnitude) => decimal_nano(magnitude).filter(|nano| *nano == 0),
        None => decimal_nano(&text),
    }
}

/// `digits[.digits][e[+-]digits]` USD as nano-USD, rounded up.
fn decimal_nano(text: &str) -> Option<i128> {
    let (mantissa, exponent) = match text.split_once(['e', 'E']) {
        Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
        None => (text, 0),
    };
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    let mut digits = 0_i128;
    for byte in whole.bytes().chain(fraction.bytes()) {
        if !byte.is_ascii_digit() {
            return None;
        }
        digits = digits
            .checked_mul(10)?
            .checked_add(i128::from(byte - b'0'))?;
    }
    // amount = digits * 10^(exponent - fraction digits) USD = digits * 10^scale nano.
    let scale = exponent
        .checked_add(9)?
        .checked_sub(i32::try_from(fraction.len()).ok()?)?;
    if scale >= 0 {
        return digits.checked_mul(10_i128.checked_pow(scale.unsigned_abs())?);
    }
    let Some(divisor) = 10_i128.checked_pow(scale.unsigned_abs()) else {
        // Below one nano, yet positive: the smallest charge, never zero.
        return Some(i128::from(digits != 0));
    };
    Some(digits / divisor + i128::from(digits % divisor != 0))
}

/// Provenance of a settled reported charge: the document that defines it and
/// the route it was reported for. A charge, never a catalog estimate.
pub(super) fn reported_charge_observation(route: &crate::retry::BillingRoute) -> Value {
    serde_json::json!({
        "route": route,
        "kind": "provider_reported_charge",
        "currency": "USD",
        "source": USAGE_SOURCE,
        "source_sha256": USAGE_SHA256,
        "as_of": USAGE_AS_OF,
    })
}

#[cfg(test)]
mod tests {
    use super::decimal_nano;

    #[test]
    fn decimal_amounts_convert_exactly_and_round_up_below_a_nano() {
        for (text, nano) in [
            ("0.0012", 1_200_000),
            ("0.0008", 800_000),
            ("1", 1_000_000_000),
            ("1.5e-5", 15_000),
            ("1.2E-3", 1_200_000),
            ("0.0000000001", 1),
            ("1e-40", 1),
            ("1e-60", 1),
            ("2.0000000001e-9", 3),
            ("12.25", 12_250_000_000),
        ] {
            assert_eq!(decimal_nano(text), Some(nano), "{text}");
        }
        for text in ["-1", "1e", "", "0x1", "1.2.3", "1e99999999999"] {
            assert_eq!(decimal_nano(text), None, "{text}");
        }
    }
}
