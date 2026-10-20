// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Qualified text request/receipt validation, before allowance can be released.
use crate::admission::Attempt;
use crate::registry::ResolvedProvider;
use nika_kernel::ai::provider::{ContentBlock, InferRequest, ProviderError};
use nika_types::cost::Cost;
use serde_json::{Map, Value};

mod openrouter;
mod xai;
pub(crate) use xai::output_with_reasoning;
#[cfg(test)]
mod tests;

/// `OpenAI`'s reference usage object carries these detail meters on a text reply.
/// A bounded text request is billed on none of them, so each reads only as an
/// exact 0.
/// <https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create>
const OPENAI_ZERO_AXES: &[(&str, &str)] = &[
    ("prompt_tokens_details", "audio_tokens"),
    ("completion_tokens_details", "audio_tokens"),
    ("completion_tokens_details", "accepted_prediction_tokens"),
    ("completion_tokens_details", "rejected_prediction_tokens"),
];
/// The input `OpenAI` wrote to the prompt cache, reported beside the cache read: within the
/// input. The GPT-6 pages price it (« Cache writes »); on a tariff with no write rate a call
/// that wrote stays unpriced, never a write priced as plain input.
const OPENAI_SUBSETS: &[(&str, &str)] = &[("prompt_tokens_details", "cache_write_tokens")];

pub(crate) fn reserve<H>(
    rp: &ResolvedProvider<H>,
    req: &InferRequest,
    bytes: usize,
) -> Result<Option<Attempt>, ProviderError> {
    let Some(a) = &rp.admission else {
        return Ok(None);
    };
    if bytes > crate::admission::BOUNDED_REQUEST_BYTES
        || !req.tools.is_empty()
        || req.memory.is_some()
        || req.thinking_budget.is_some()
        || !req.extra.params.is_empty()
        || req
            .messages
            .iter()
            .flat_map(|m| &m.content)
            .any(|c| !matches!(c, ContentBlock::Text { .. }))
    {
        return Err(a.refuse("bounded inference supports text-only requests up to 1 MiB with the qualified output/thinking bound"));
    }
    let max = req
        .max_tokens
        .ok_or_else(|| a.refuse("bounded inference requires maximum output tokens"))?;
    // The bound belongs to the actual wire model, even if the caller supplied
    // a different model field after registry resolution.
    if req.model != rp.wire_model && req.model != format!("{}/{}", rp.profile.id, rp.wire_model) {
        return Err(a.refuse("request model differs from the qualified resolved model"));
    }
    if let Some(bound) = tier_bound(rp)
        && u64::try_from(bytes).map_or(true, |b| b > bound)
    {
        return Err(a.refuse(&format!(
            "this {bytes}-byte request may exceed the {bound} input tokens its tariff admits (a longer prompt bills at the model's next price tier); nothing was sent"
        )));
    }
    a.reserve(rp.profile.id, &rp.wire_model, &rp.base_url, max)
        .map(Some)
}

/// The input bound of a tariff that admits fewer tokens than its model's window: a cheaper
/// prompt tier (Claude Haiku 5.5 up to 100,000 tokens, xAI below 200k), above which the
/// provider bills a rate the reservation does not cover. A text request's bytes bound its
/// tokens (each token spans at least one byte, and the JSON framing outweighs the provider's
/// few message markers), so a request within the bound stays in the tier; a reply reporting
/// more input still settles as unknown, never at the tier's rate. `OpenRouter`'s rows name the
/// smallest window of their endpoints, no price tier, and are not bounded here.
fn tier_bound<H>(rp: &ResolvedProvider<H>) -> Option<u64> {
    let tariff =
        nika_catalog::admission::InferenceTariff::new(rp.profile.id, &rp.wire_model, &rp.base_url)?;
    if tariff.billing_provider == "openrouter" {
        return None;
    }
    let window = nika_catalog::find_provider(rp.profile.id)?
        .models
        .iter()
        .find(|m| m.model == rp.wire_model)
        .map(|m| u64::from(m.context_window_tokens))?;
    (tariff.context_tokens < window).then_some(tariff.context_tokens)
}

/// Provider-specific text receipts must prove every qualified cost meter.
/// Missing/invalid optional detail, conflicting cache aliases and unknown cost
/// axes are not allowed to become a zero discount.
pub(crate) fn complete_usage(provider: &str, v: &Value) -> bool {
    match provider {
        "openai" => complete_compat_usage(v, OPENAI_ZERO_AXES, OPENAI_SUBSETS),
        "openrouter" => openrouter::reported_charge(v).is_some(),
        "xai" => xai::reported_charge(v).is_some(),
        "deepseek" => complete_deepseek_usage(v),
        "anthropic" => complete_anthropic_usage(v),
        "gemini" => complete_gemini_usage(v),
        _ => false,
    }
}

/// Anthropic's Messages `usage` object (API reference, 2026-10-15): `input_tokens` and
/// `output_tokens`, the cache read and the cache write beside the input, and detail objects.
/// Complete when both counts are numbers, every optional count is a number or null, the
/// thinking tokens stay within the output, the write is the 5-minute one (`cache_creation`
/// splits it by lifetime; a 1-hour write bills at another rate), and no other billed axis
/// rides: no server tool request, the standard service tier, global inference (a US-only
/// reply bills 1.1x), the standard speed (every reply carries `"speed": "standard"` since
/// 2026-10, measured live; a fast reply bills at another rate). Any other key is an unknown axis.
fn complete_anthropic_usage(v: &Value) -> bool {
    let Some(u) = v.get("usage").and_then(Value::as_object) else {
        return false;
    };
    let count = |k: &str| u.get(k).and_then(Value::as_u64);
    let optional = |k: &str| match u.get(k) {
        None | Some(Value::Null) => Some(0),
        Some(n) => n.as_u64(),
    };
    let (Some(input), Some(output), Some(read), Some(written)) = (
        count("input_tokens"),
        count("output_tokens"),
        optional("cache_read_input_tokens"),
        optional("cache_creation_input_tokens"),
    ) else {
        return false;
    };
    let total = input.checked_add(read).and_then(|n| n.checked_add(written));
    if total.is_none_or(|n| n == 0) {
        return false;
    }
    let details_ok = match u.get("output_tokens_details") {
        None | Some(Value::Null) => true,
        Some(Value::Object(d)) => d.iter().all(|(k, n)| {
            k == "thinking_tokens" && (n.is_null() || n.as_u64().is_some_and(|n| n <= output))
        }),
        Some(_) => false,
    };
    let lifetimes_ok = match u.get("cache_creation") {
        None | Some(Value::Null) => true,
        Some(Value::Object(c)) => {
            let lifetime = |k: &str| match c.get(k) {
                None | Some(Value::Null) => Some(0),
                Some(n) => n.as_u64(),
            };
            c.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "ephemeral_5m_input_tokens" | "ephemeral_1h_input_tokens"
                )
            }) && lifetime("ephemeral_1h_input_tokens") == Some(0)
                && lifetime("ephemeral_5m_input_tokens") == Some(written)
        }
        Some(_) => false,
    };
    let no_server_tool = match u.get("server_tool_use") {
        None | Some(Value::Null) => true,
        Some(Value::Object(s)) => s.iter().all(|(k, n)| {
            matches!(k.as_str(), "web_search_requests" | "web_fetch_requests")
                && (n.is_null() || n.as_u64() == Some(0))
        }),
        Some(_) => false,
    };
    let word = |k: &str, standard: &str| match u.get(k) {
        None | Some(Value::Null) => true,
        Some(w) => w.as_str() == Some(standard),
    };
    details_ok
        && lifetimes_ok
        && no_server_tool
        && word("service_tier", "standard")
        && word("inference_geo", "global")
        && word("speed", "standard")
        && u.keys().all(|k| {
            matches!(
                k.as_str(),
                "input_tokens"
                    | "output_tokens"
                    | "cache_read_input_tokens"
                    | "cache_creation_input_tokens"
                    | "cache_creation"
                    | "output_tokens_details"
                    | "server_tool_use"
                    | "service_tier"
                    | "inference_geo"
                    | "speed"
            )
        })
}

/// The provider's own account charge for this reply, when its complete receipt
/// reports one (`OpenRouter`'s `usage.cost`, xAI's `usage.cost_in_usd_ticks`). Other
/// providers report none: their replies are priced at a catalog tariff.
pub(crate) fn reported_charge(provider: &str, v: &Value) -> Option<Cost> {
    match provider {
        "openrouter" => openrouter::reported_charge(v),
        "xai" => xai::reported_charge(v),
        _ => None,
    }
}

/// Provenance of a settled reported charge on `route`: the document of its biller that
/// defines the charge.
pub(crate) fn reported_charge_observation(route: &crate::retry::BillingRoute) -> Value {
    if route.provider == "xai" {
        xai::reported_charge_observation(route)
    } else {
        openrouter::reported_charge_observation(route)
    }
}

/// Gemini's `usageMetadata` (API reference, its markdown bytes of 2026-10-16): the prompt with
/// its cached part, the candidates and the thoughts, which bill at the output rate, and
/// `totalTokenCount`, the prompt, tool-use prompt, thoughts and candidates together. A count of
/// 0 may be omitted. Complete when the prompt is text (`promptTokensDetails`), its cache read
/// within it and text too, the candidates text, no tool-use prompt rode, the counts add up, the
/// standard tier served it (flex and priority bill other rates) and every key is known.
fn complete_gemini_usage(v: &Value) -> bool {
    const KEYS: &[&str] = &[
        "promptTokenCount",
        "cachedContentTokenCount",
        "candidatesTokenCount",
        "toolUsePromptTokenCount",
        "thoughtsTokenCount",
        "totalTokenCount",
        "promptTokensDetails",
        "cacheTokensDetails",
        "candidatesTokensDetails",
        "serviceTier",
    ];
    let Some(u) = v.get("usageMetadata").and_then(Value::as_object) else {
        return false;
    };
    let optional = |k: &str| match u.get(k) {
        None | Some(Value::Null) => Some(0),
        Some(n) => n.as_u64(),
    };
    let (Some(prompt), Some(total)) = (
        u.get("promptTokenCount").and_then(Value::as_u64),
        u.get("totalTokenCount").and_then(Value::as_u64),
    ) else {
        return false;
    };
    let (Some(read), Some(candidates), Some(thoughts), Some(tool_use)) = (
        optional("cachedContentTokenCount"),
        optional("candidatesTokenCount"),
        optional("thoughtsTokenCount"),
        optional("toolUsePromptTokenCount"),
    ) else {
        return false;
    };
    prompt > 0
        && tool_use == 0
        && read <= prompt
        && prompt
            .checked_add(candidates)
            .and_then(|n| n.checked_add(thoughts))
            == Some(total)
        && text_modalities(u.get("promptTokensDetails"), Some(prompt))
        && text_modalities(u.get("cacheTokensDetails"), (read > 0).then_some(read))
        && text_modalities(u.get("candidatesTokensDetails"), None)
        && matches!(
            u.get("serviceTier").and_then(Value::as_str),
            None | Some("standard" | "unspecified")
        )
        && u.keys().all(|k| KEYS.contains(&k.as_str()))
}

/// A Gemini `ModalityTokenCount` list: every entry `TEXT`, summing to `total` when one is owed.
/// An absent or `null` list counts nothing, which only an unowed total accepts.
fn text_modalities(list: Option<&Value>, total: Option<u64>) -> bool {
    let entries = match list {
        None | Some(Value::Null) => return total.is_none(),
        Some(Value::Array(entries)) => entries,
        Some(_) => return false,
    };
    let mut sum = 0_u64;
    for entry in entries {
        let Some(entry) = entry.as_object() else {
            return false;
        };
        let (Some("TEXT"), Some(count)) = (
            entry.get("modality").and_then(Value::as_str),
            entry.get("tokenCount").and_then(Value::as_u64),
        ) else {
            return false;
        };
        if entry.keys().any(|k| k != "modality" && k != "tokenCount") {
            return false;
        }
        let Some(next) = sum.checked_add(count) else {
            return false;
        };
        sum = next;
    }
    total.is_none_or(|total| sum == total)
}

fn complete_deepseek_usage(v: &Value) -> bool {
    let Some(u) = v.get("usage").and_then(Value::as_object) else {
        return false;
    };
    let at = |k: &str| u.get(k).and_then(Value::as_u64);
    let (Some(input), Some(output), Some(hit), Some(miss), Some(total)) = (
        at("prompt_tokens"),
        at("completion_tokens"),
        at("prompt_cache_hit_tokens"),
        at("prompt_cache_miss_tokens"),
        at("total_tokens"),
    ) else {
        return false;
    };
    if input == 0
        || hit.checked_add(miss) != Some(input)
        || input.checked_add(output) != Some(total)
    {
        return false;
    }
    if let Some(details) = u.get("completion_tokens_details") {
        let Some(details) = details.as_object() else {
            return false;
        };
        if details
            .iter()
            .any(|(k, v)| k != "reasoning_tokens" || v.as_u64().is_none_or(|n| n > output))
        {
            return false;
        }
    }
    if let Some(details) = u.get("prompt_tokens_details") {
        let Some(details) = details.as_object() else {
            return false;
        };
        if details
            .iter()
            .any(|(k, v)| k != "cached_tokens" || v.as_u64() != Some(hit))
        {
            return false;
        }
    }
    u.keys().all(|k| {
        matches!(
            k.as_str(),
            "prompt_tokens"
                | "completion_tokens"
                | "total_tokens"
                | "prompt_cache_hit_tokens"
                | "prompt_cache_miss_tokens"
                | "completion_tokens_details"
                | "prompt_tokens_details"
        )
    })
}

// Complete standard text meters, independent from price/currency qualification.
// Additional billable axes are unknown until explicitly accounted for.
fn complete_compat_usage(v: &Value, zero_axes: &[(&str, &str)], subsets: &[(&str, &str)]) -> bool {
    let Some(u) = v.get("usage").and_then(Value::as_object) else {
        return false;
    };
    complete_text_meters(u, zero_axes, subsets) && u.keys().all(|k| is_text_meter(k))
}

fn is_text_meter(key: &str) -> bool {
    matches!(
        key,
        "prompt_tokens"
            | "completion_tokens"
            | "total_tokens"
            | "prompt_tokens_details"
            | "completion_tokens_details"
    )
}

/// Input and output reported with their exact total, and every detail meter a
/// subset of its own count. A known detail object or meter written `null` reads
/// as absent, the documented "not applicable". `zero_axes` names the other
/// meters a text reply may carry, each only as an exact 0; `subsets` names the
/// further reported subsets that stay, with the cache read, within the input
/// (a cache write). Any other detail key is an unknown axis.
fn complete_text_meters(
    u: &Map<String, Value>,
    zero_axes: &[(&str, &str)],
    subsets: &[(&str, &str)],
) -> bool {
    let at = |k: &str| u.get(k).and_then(Value::as_u64);
    let (Some(input), Some(output), Some(total)) = (
        at("prompt_tokens"),
        at("completion_tokens"),
        at("total_tokens"),
    ) else {
        return false;
    };
    if input == 0 || input.checked_add(output) != Some(total) {
        return false;
    }
    [
        ("prompt_tokens_details", "cached_tokens", input),
        ("completion_tokens_details", "reasoning_tokens", output),
    ]
    .into_iter()
    .all(|(field, key, bound)| match u.get(field) {
        None | Some(Value::Null) => true,
        Some(Value::Object(details)) => {
            details_within(details, field, key, bound, zero_axes, subsets)
        }
        Some(_) => false,
    })
}

/// One details object: its counted meters (`key` and `subsets`) sum within
/// `bound`, its zero axes read 0, and it names no other key.
fn details_within(
    details: &Map<String, Value>,
    field: &str,
    key: &str,
    bound: u64,
    zero_axes: &[(&str, &str)],
    subsets: &[(&str, &str)],
) -> bool {
    let mut within = 0_u64;
    for (k, n) in details {
        let axis = (field, k.as_str());
        let counted = k == key || subsets.contains(&axis);
        if !counted && !zero_axes.contains(&axis) {
            return false;
        }
        if n.is_null() {
            continue;
        }
        let Some(n) = n.as_u64() else {
            return false;
        };
        if counted {
            let Some(sum) = within.checked_add(n) else {
                return false;
            };
            within = sum;
        } else if n != 0 {
            return false;
        }
    }
    within <= bound
}
