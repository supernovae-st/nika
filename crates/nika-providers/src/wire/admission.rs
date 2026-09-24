// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Qualified text request/receipt validation, before allowance can be released.
use crate::admission::Attempt;
use crate::registry::ResolvedProvider;
use nika_kernel::ai::provider::{ContentBlock, InferRequest, ProviderError};
use serde_json::Value;

pub(crate) fn reserve<H>(
    rp: &ResolvedProvider<H>,
    req: &InferRequest,
    bytes: usize,
) -> Result<Option<Attempt>, ProviderError> {
    let Some(a) = &rp.admission else {
        return Ok(None);
    };
    if bytes > 1_048_576
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
    a.reserve(rp.profile.id, &rp.wire_model, &rp.base_url, max)
        .map(Some)
}

/// Only the qualified `DeepSeek` shape has proved complete tariff meters here.
/// Missing/invalid optional detail, conflicting cache aliases and unknown cost
/// axes are not allowed to become a zero discount.
pub(crate) fn complete_usage(provider: &str, v: &Value) -> bool {
    if provider == "openai" {
        return complete_compat_usage(v);
    }
    if provider != "deepseek" {
        return false;
    }
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
fn complete_compat_usage(v: &Value) -> bool {
    let Some(u) = v.get("usage").and_then(Value::as_object) else {
        return false;
    };
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
    for (field, key, bound) in [
        ("prompt_tokens_details", "cached_tokens", input),
        ("completion_tokens_details", "reasoning_tokens", output),
    ] {
        if let Some(details) = u.get(field) {
            let Some(details) = details.as_object() else {
                return false;
            };
            if details
                .iter()
                .any(|(k, n)| k != key || n.as_u64().is_none_or(|n| n > bound))
            {
                return false;
            }
        }
    }
    u.keys().all(|k| {
        matches!(
            k.as_str(),
            "prompt_tokens"
                | "completion_tokens"
                | "total_tokens"
                | "prompt_tokens_details"
                | "completion_tokens_details"
        )
    })
}
