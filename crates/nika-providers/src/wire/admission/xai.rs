// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Closed xAI text receipt: the charge each chat completion reports, independent from exact
//! route/tariff admission.
use nika_types::cost::Cost;
use serde_json::{Map, Value};

/// The chat completions reference names `usage.cost_in_usd_ticks` « accurate cost of this
/// request in USD ticks », 10,000,000,000 ticks to the dollar, beside the token meters. Its
/// markdown bytes, fetched twice on 2026-10-16, have this SHA-256.
const USAGE_SOURCE: &str =
    "https://docs.x.ai/developers/rest-api-reference/inference/chat-completions.md";
const USAGE_SHA256: &str = "d1fd398c52f1cbb512b251d1c13cc2c2dc8b912ec0384b91f506a86293fab22d";
const USAGE_AS_OF: &str = "2026-10-16";

/// Ticks in one nano-dollar: 10^10 ticks and 10^9 nano-dollars to the dollar.
const TICKS_PER_NANO: u64 = 10;

const USAGE_KEYS: &[&str] = &[
    "prompt_tokens",
    "completion_tokens",
    "total_tokens",
    "prompt_tokens_details",
    "completion_tokens_details",
    "num_sources_used",
    "cost_in_usd_ticks",
    "server_side_tool_usage_details",
];
const PROMPT_KEYS: &[&str] = &[
    "text_tokens",
    "cached_tokens",
    "audio_tokens",
    "image_tokens",
];
const COMPLETION_KEYS: &[&str] = &[
    "reasoning_tokens",
    "audio_tokens",
    "accepted_prediction_tokens",
    "rejected_prediction_tokens",
];

/// The charge a complete text reply reports, rounded up to the next nano-dollar so a positive
/// charge never reads as zero. Complete: the prompt, the completion and the reasoning, which
/// xAI counts beside the completion, add up to `total_tokens` (the reference's example and live
/// replies, 2026-10-16); the prompt is text, its cache read within it; no audio, image or
/// prediction token rode; no live source or server-side tool ran; every key is known. Any other
/// shape leaves the charge unknown.
pub(super) fn reported_charge(v: &Value) -> Option<Cost> {
    let u = v.get("usage").and_then(Value::as_object)?;
    let at = |k: &str| u.get(k).and_then(Value::as_u64);
    let prompt_details = u.get("prompt_tokens_details").and_then(Value::as_object)?;
    let completion_details = u
        .get("completion_tokens_details")
        .and_then(Value::as_object)?;
    let (prompt, completion, total, ticks) = (
        at("prompt_tokens")?,
        at("completion_tokens")?,
        at("total_tokens")?,
        at("cost_in_usd_ticks")?,
    );
    let reasoning = count(completion_details, "reasoning_tokens")?;
    let known = prompt > 0
        && prompt.checked_add(completion)?.checked_add(reasoning)? == total
        && count(prompt_details, "text_tokens") == Some(prompt)
        && count(prompt_details, "cached_tokens").is_some_and(|read| read <= prompt)
        && zero(prompt_details, &["audio_tokens", "image_tokens"])
        && zero(
            completion_details,
            &[
                "audio_tokens",
                "accepted_prediction_tokens",
                "rejected_prediction_tokens",
            ],
        )
        && closed(prompt_details, PROMPT_KEYS)
        && closed(completion_details, COMPLETION_KEYS)
        && closed(u, USAGE_KEYS)
        && at("num_sources_used") == Some(0)
        && no_server_tools(u.get("server_side_tool_usage_details"));
    known.then(|| Cost::new(i128::from(ticks.div_ceil(TICKS_PER_NANO))))
}

/// The output a reply generated: xAI reports its reasoning beside the completion, never inside
/// it, and bills both at the output rate.
pub(crate) fn output_with_reasoning(completion: u64, reasoning: Option<u64>) -> u64 {
    completion.saturating_add(reasoning.unwrap_or(0))
}

fn count(details: &Map<String, Value>, key: &str) -> Option<u64> {
    details.get(key).and_then(Value::as_u64)
}

/// Each meter absent, `null` or an exact 0.
fn zero(details: &Map<String, Value>, keys: &[&str]) -> bool {
    keys.iter().all(|key| match details.get(*key) {
        None | Some(Value::Null) => true,
        Some(n) => n.as_u64() == Some(0),
    })
}

fn closed(object: &Map<String, Value>, keys: &[&str]) -> bool {
    object.keys().all(|k| keys.contains(&k.as_str()))
}

/// No server-side tool ran: the details are absent, `null`, or every count is 0.
fn no_server_tools(details: Option<&Value>) -> bool {
    match details {
        None | Some(Value::Null) => true,
        Some(Value::Object(calls)) => calls.values().all(|n| n.as_u64() == Some(0)),
        Some(_) => false,
    }
}

/// Provenance of a settled reported charge: the document that defines it and the route.
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
    use super::*;
    use serde_json::json;

    /// A live grok-4.7 reply of 2026-10-16: 1,248 prompt tokens (1,152 read from the cache),
    /// 2 completion and 53 reasoning tokens, $0.001098.
    fn live() -> Value {
        json!({"usage": {
            "prompt_tokens": 1248, "completion_tokens": 2, "total_tokens": 1303,
            "prompt_tokens_details": {
                "text_tokens": 1248, "audio_tokens": 0, "image_tokens": 0, "cached_tokens": 1152
            },
            "completion_tokens_details": {
                "reasoning_tokens": 53, "audio_tokens": 0,
                "accepted_prediction_tokens": 0, "rejected_prediction_tokens": 0
            },
            "num_sources_used": 0,
            "cost_in_usd_ticks": 10_980_000
        }})
    }

    #[test]
    fn a_live_reply_reports_its_charge_in_nano_dollars() {
        // 96 plain input at 2,000 + 1,152 read at 500 + (2 + 53) output at 6,000 nano-dollars.
        assert_eq!(reported_charge(&live()), Some(Cost::new(1_098_000)));
        let mut tiny = live();
        tiny["usage"]["cost_in_usd_ticks"] = json!(5);
        assert_eq!(
            reported_charge(&tiny),
            Some(Cost::new(1)),
            "half a nano-dollar is charged, never zero"
        );
    }

    /// One way a reply leaves the documented text shape.
    type Defect = (&'static str, fn(&mut Value));

    #[test]
    fn a_reply_outside_the_text_shape_reports_no_charge() {
        let cases: [Defect; 11] = [
            ("no ticks", |u| {
                if let Some(u) = u.as_object_mut() {
                    u.remove("cost_in_usd_ticks");
                }
            }),
            ("fractional ticks", |u| u["cost_in_usd_ticks"] = json!(1.5)),
            ("negative ticks", |u| u["cost_in_usd_ticks"] = json!(-1)),
            ("total", |u| u["total_tokens"] = json!(1250)),
            ("a live source", |u| u["num_sources_used"] = json!(1)),
            ("a server tool", |u| {
                u["server_side_tool_usage_details"] = json!({"web_search_calls": 1});
            }),
            ("an image", |u| {
                u["prompt_tokens_details"]["image_tokens"] = json!(4);
            }),
            ("text apart from the prompt", |u| {
                u["prompt_tokens_details"]["text_tokens"] = json!(1000);
            }),
            ("a read above the prompt", |u| {
                u["prompt_tokens_details"]["cached_tokens"] = json!(1249);
            }),
            ("a prediction", |u| {
                u["completion_tokens_details"]["accepted_prediction_tokens"] = json!(1);
            }),
            ("unknown key", |u| u["new_billable_axis"] = json!(0)),
        ];
        for (name, defect) in cases {
            let mut reply = live();
            defect(&mut reply["usage"]);
            assert_eq!(reported_charge(&reply), None, "{name}");
        }
        let mut idle = live();
        idle["usage"]["server_side_tool_usage_details"] =
            json!({"web_search_calls": 0, "x_search_calls": 0});
        assert!(reported_charge(&idle).is_some(), "tools that never ran");
    }

    #[test]
    fn the_reasoning_counts_in_the_output() {
        assert_eq!(output_with_reasoning(2, Some(1_973)), 1_975);
        assert_eq!(output_with_reasoning(9, None), 9);
    }
}
