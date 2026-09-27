// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Closed `OpenRouter` text receipt, independent from exact route/tariff admission.
use serde_json::Value;

/// <https://openrouter.ai/docs/cookbook/administration/usage-accounting>
/// <https://openrouter.ai/docs/api_reference/overview>
/// A zero tariff does not turn a missing charge into zero. Require the reported
/// account charge and explicit non-BYOK identity; other cost axes remain unknown.
/// This is a meter-completeness proof, never an invoice or seat qualification.
/// The dated usage document identifies `cost` as the total account charge.
/// Its observed 2026-09-27 bytes have SHA-256
/// `62e9339731737dff7e35c84ffac84607be44e415a236a5458852328dd21f7cf2`.
pub(super) fn complete_zero_usage(v: &Value) -> bool {
    let Some(u) = v.get("usage").and_then(Value::as_object) else {
        return false;
    };
    if !super::complete_text_meters(
        u,
        &[
            ("prompt_tokens_details", "cache_write_tokens"),
            ("prompt_tokens_details", "audio_tokens"),
            ("prompt_tokens_details", "video_tokens"),
            ("completion_tokens_details", "audio_tokens"),
            ("completion_tokens_details", "image_tokens"),
        ],
    ) || u.get("cost").is_none_or(|n| !zero(n))
        || u.get("is_byok").and_then(Value::as_bool) != Some(false)
        || !u.keys().all(|k| {
            super::is_text_meter(k)
                || matches!(
                    k.as_str(),
                    "cost" | "is_byok" | "cost_details" | "server_tool_use"
                )
        })
    {
        return false;
    }
    if let Some(tools) = u.get("server_tool_use") {
        let Some(tools) = tools.as_object() else {
            return false;
        };
        if tools
            .iter()
            .any(|(k, n)| k != "web_search_requests" || n.as_u64() != Some(0))
        {
            return false;
        }
    }
    // The required aggregate charge is known. Missing/null decomposition is
    // not a missing total; a present conflicting or unknown fee remains unknown.
    if let Some(details) = u.get("cost_details").filter(|v| !v.is_null()) {
        let Some(details) = details.as_object() else {
            return false;
        };
        if !details.iter().all(|(key, value)| match key.as_str() {
            // Null means not applicable for this explicitly non-BYOK request.
            // It is not evidence of a measured zero upstream invoice.
            "upstream_inference_cost" => value.is_null() || zero(value),
            "upstream_inference_prompt_cost"
            | "upstream_inference_completions_cost"
            | "server_tool_cost" => zero(value),
            _ => false,
        }) {
            return false;
        }
    }
    true
}

fn zero(value: &Value) -> bool {
    value.as_f64() == Some(0.0)
}
