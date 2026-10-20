// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The usage law of each provider receipt, judged on the documented shapes.
use super::{complete_usage, reported_charge};
use nika_types::cost::Cost;
use serde_json::{Value, json};

/// `OpenAI`'s reference usage object: every detail meter of a text reply.
fn openai() -> Value {
    json!({"usage": {
        "prompt_tokens": 19, "completion_tokens": 10, "total_tokens": 29,
        "prompt_tokens_details": {"cached_tokens": 0, "audio_tokens": 0},
        "completion_tokens_details": {
            "reasoning_tokens": 0, "audio_tokens": 0,
            "accepted_prediction_tokens": 0, "rejected_prediction_tokens": 0
        }
    }})
}

/// `OpenRouter`'s documented paid, non-BYOK reply (create-a-chat-completion example).
fn paid() -> Value {
    json!({"usage": {
        "prompt_tokens": 10, "completion_tokens": 15, "total_tokens": 25,
        "prompt_tokens_details": {"cached_tokens": 2},
        "completion_tokens_details": {"reasoning_tokens": 5},
        "cost": 0.0012, "is_byok": false,
        "cost_details": {
            "upstream_inference_cost": null,
            "upstream_inference_prompt_cost": 0.0008,
            "upstream_inference_completions_cost": 0.0004
        },
        "server_tool_use_details": {"tool_calls_executed": 0, "tool_calls_requested": 0}
    }})
}

#[test]
fn a_complete_openai_text_reply_is_complete_and_every_billed_axis_refuses() {
    assert!(complete_usage("openai", &openai()));
    for (field, key) in [
        ("prompt_tokens_details", "audio_tokens"),
        ("completion_tokens_details", "audio_tokens"),
        ("completion_tokens_details", "accepted_prediction_tokens"),
        ("completion_tokens_details", "rejected_prediction_tokens"),
    ] {
        let mut reply = openai();
        reply["usage"][field][key] = json!(0);
        assert!(complete_usage("openai", &reply), "{field}.{key} = 0");
        reply["usage"][field][key] = json!(1);
        assert!(!complete_usage("openai", &reply), "{field}.{key} = 1");
        reply["usage"][field][key] = json!(false);
        assert!(!complete_usage("openai", &reply), "{field}.{key} = false");
    }
    let mut unknown = openai();
    unknown["usage"]["completion_tokens_details"]["new_axis"] = json!(0);
    assert!(!complete_usage("openai", &unknown), "an unknown axis");
    let mut top = openai();
    top["usage"]["cost"] = json!(0);
    assert!(
        !complete_usage("openai", &top),
        "a key outside the text meters"
    );
    let mut cached = openai();
    cached["usage"]["prompt_tokens_details"]["cached_tokens"] = json!(20);
    assert!(!complete_usage("openai", &cached), "cached above the input");
    let mut absent = openai();
    absent["usage"]["prompt_tokens_details"] = Value::Null;
    absent["usage"]["completion_tokens_details"]["audio_tokens"] = Value::Null;
    assert!(complete_usage("openai", &absent), "null reads as absent");
    // The admission rule reaches no other compatible provider.
    for provider in ["mistral", "xai", "groq", "scaleway", "huggingface"] {
        assert!(!complete_usage(provider, &openai()), "{provider}");
    }
}

#[test]
fn a_paid_openrouter_reply_reports_its_charge_and_byok_stays_unknown() {
    assert_eq!(
        reported_charge("openrouter", &paid()),
        Some(Cost::new(1_200_000))
    );
    assert!(complete_usage("openrouter", &paid()));
    for provider in ["openai", "deepseek"] {
        assert_eq!(reported_charge(provider, &paid()), None, "{provider}");
    }
    let mut written = paid();
    written["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(8);
    assert_eq!(
        reported_charge("openrouter", &written),
        Some(Cost::new(1_200_000)),
        "a paid cache write is inside the charge"
    );
    let mut nulls = paid();
    nulls["usage"]["completion_tokens_details"] = json!({
        "reasoning_tokens": null, "audio_tokens": null,
        "accepted_prediction_tokens": null, "rejected_prediction_tokens": null
    });
    nulls["usage"]["cost_details"]["server_tool_cost"] = Value::Null;
    nulls["usage"]["server_tool_use_details"] = Value::Null;
    assert_eq!(
        reported_charge("openrouter", &nulls),
        Some(Cost::new(1_200_000)),
        "documented nulls read as absent"
    );
    let mut tiny = paid();
    tiny["usage"]["cost"] = json!(1e-12);
    assert_eq!(
        reported_charge("openrouter", &tiny),
        Some(Cost::new(1)),
        "a positive charge never reads as zero"
    );
}

/// The GPT-6 pages price the input written to the prompt cache: the write is counted within
/// the input beside the cache read, and only a write the input cannot hold refuses. Whether a
/// written call is priced belongs to its tariff's write rate, never to this law.
#[test]
fn an_openai_cache_write_within_the_input_is_complete() {
    let mut written = openai();
    written["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(12);
    assert!(
        complete_usage("openai", &written),
        "a write within the input"
    );
    written["usage"]["prompt_tokens_details"]["cached_tokens"] = json!(7);
    assert!(
        complete_usage("openai", &written),
        "a read and a write within the input"
    );
    written["usage"]["prompt_tokens_details"]["cached_tokens"] = json!(8);
    assert!(
        !complete_usage("openai", &written),
        "a read and a write beyond the input"
    );
    written["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(false);
    assert!(
        !complete_usage("openai", &written),
        "a write that is no count"
    );
}

/// A live gemini-3.1-pro-preview reply of 2026-10-16: 24 prompt tokens, 2 candidate tokens and
/// 1,568 thoughts, on the standard tier.
fn gemini() -> Value {
    json!({"usageMetadata": {
        "promptTokenCount": 24, "candidatesTokenCount": 2, "totalTokenCount": 1594,
        "promptTokensDetails": [{"modality": "TEXT", "tokenCount": 24}],
        "thoughtsTokenCount": 1568, "serviceTier": "standard"
    }})
}

/// Gemini's usage law: thoughts and candidates add up with the prompt, a cache read rides
/// within the text prompt, and an omitted zero count reads as 0; any other modality, tier,
/// tool-use prompt or key leaves the meters incomplete.
#[test]
fn a_gemini_text_reply_is_complete_and_every_other_axis_refuses() {
    assert!(complete_usage("gemini", &gemini()));
    let mut cut = gemini();
    cut["usageMetadata"] = json!({
        "promptTokenCount": 24, "totalTokenCount": 81, "thoughtsTokenCount": 57,
        "promptTokensDetails": [{"modality": "TEXT", "tokenCount": 24}]
    });
    assert!(
        complete_usage("gemini", &cut),
        "no candidate, no tier named"
    );
    let mut read = gemini();
    read["usageMetadata"]["cachedContentTokenCount"] = json!(16);
    read["usageMetadata"]["cacheTokensDetails"] = json!([{"modality": "TEXT", "tokenCount": 16}]);
    assert!(
        complete_usage("gemini", &read),
        "a cache read within the prompt"
    );
    for (name, defect) in [
        (
            "audio prompt",
            json!({"promptTokensDetails": [{"modality": "AUDIO", "tokenCount": 24}]}),
        ),
        (
            "prompt details apart",
            json!({"promptTokensDetails": [{"modality": "TEXT", "tokenCount": 20}]}),
        ),
        ("no prompt details", json!({"promptTokensDetails": null})),
        ("tool-use prompt", json!({"toolUsePromptTokenCount": 3})),
        ("flex tier", json!({"serviceTier": "flex"})),
        ("priority tier", json!({"serviceTier": "priority"})),
        ("total", json!({"totalTokenCount": 1600})),
        (
            "read above the prompt",
            json!({"cachedContentTokenCount": 25}),
        ),
        (
            "read without its details",
            json!({"cachedContentTokenCount": 16}),
        ),
        (
            "image candidates",
            json!({"candidatesTokensDetails": [{"modality": "IMAGE", "tokenCount": 2}]}),
        ),
        ("unknown key", json!({"newBillableAxis": 0})),
    ] {
        let mut reply = gemini();
        for (key, value) in defect.as_object().expect("object") {
            reply["usageMetadata"][key] = value.clone();
        }
        assert!(!complete_usage("gemini", &reply), "{name}");
    }
}

/// One way a paid receipt leaves the documented shape./// One way a paid receipt leaves the documented shape.
type Defect = (&'static str, fn(&mut Value));

#[test]
fn a_paid_reply_outside_the_documented_shape_is_no_charge() {
    let cases: [Defect; 13] = [
        ("byok", |u| u["is_byok"] = json!(true)),
        ("missing byok", |u| {
            if let Some(u) = u.as_object_mut() {
                u.remove("is_byok");
            }
        }),
        ("null cost", |u| u["cost"] = Value::Null),
        ("negative cost", |u| u["cost"] = json!(-0.0012)),
        ("string cost", |u| u["cost"] = json!("0.0012")),
        ("negative upstream cost", |u| {
            u["cost_details"]["upstream_inference_cost"] = json!(-0.001);
        }),
        ("unknown fee", |u| {
            u["cost_details"]["platform_fee"] = json!(0);
        }),
        ("negative part", |u| {
            u["cost_details"]["upstream_inference_prompt_cost"] = json!(-0.0008);
        }),
        ("tool charge", |u| {
            u["cost_details"]["server_tool_cost"] = json!(0.0001);
        }),
        ("tool ran", |u| {
            u["server_tool_use_details"]["tool_calls_executed"] = json!(2);
        }),
        ("audio", |u| {
            u["completion_tokens_details"]["audio_tokens"] = json!(1);
        }),
        ("writes above the input", |u| {
            u["prompt_tokens_details"]["cache_write_tokens"] = json!(9);
        }),
        ("unknown key", |u| u["new_billable_axis"] = json!(0)),
    ];
    for (name, defect) in cases {
        let mut reply = paid();
        defect(&mut reply["usage"]);
        assert_eq!(reported_charge("openrouter", &reply), None, "{name}");
        assert!(!complete_usage("openrouter", &reply), "{name}");
    }
}

/// A paid non-BYOK reply carries the upstream provider's cost beside its charge (live,
/// `z-ai/glm-5.3-flash`, 2026-10-16): the account is charged `cost`, the breakdown is read
/// for its shape only.
#[test]
fn a_paid_reply_beside_its_upstream_cost_is_its_charge() {
    let reply = json!({"usage": {
        "prompt_tokens": 18, "completion_tokens": 70, "total_tokens": 88,
        "cost": 1.0933e-05, "is_byok": false,
        "prompt_tokens_details": {
            "cached_tokens": 0, "cache_write_tokens": 0, "audio_tokens": 0, "video_tokens": 0
        },
        "cost_details": {
            "upstream_inference_cost": 1.0933e-05,
            "upstream_inference_prompt_cost": 7.83e-07,
            "upstream_inference_completions_cost": 1.015e-05
        },
        "completion_tokens_details": {"reasoning_tokens": 66, "image_tokens": 0, "audio_tokens": 0}
    }});
    assert_eq!(
        reported_charge("openrouter", &reply),
        Some(Cost::new(10_933))
    );
    assert!(complete_usage("openrouter", &reply));
}

#[test]
fn a_zero_charge_still_proves_every_zero_axis() {
    let mut free = paid();
    free["usage"]["cost"] = json!(0);
    free["usage"]["cost_details"] = json!({"upstream_inference_cost": null});
    assert_eq!(reported_charge("openrouter", &free), Some(Cost::zero()));
    let mut written = free.clone();
    written["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(1);
    assert_eq!(
        reported_charge("openrouter", &written),
        None,
        "a free write"
    );
    let mut decomposed = free.clone();
    decomposed["usage"]["cost_details"]["upstream_inference_prompt_cost"] = json!(0.0008);
    assert_eq!(
        reported_charge("openrouter", &decomposed),
        None,
        "a free call with an upstream cost"
    );
    let mut negative_zero = free;
    negative_zero["usage"]["cost"] = json!(-0.0);
    assert_eq!(
        reported_charge("openrouter", &negative_zero),
        Some(Cost::zero())
    );
}

/// Anthropic's documented Messages usage (API reference example, 2026-10-15), a 5-minute write.
fn anthropic() -> Value {
    json!({"usage": {
        "cache_creation": {"ephemeral_1h_input_tokens": 0, "ephemeral_5m_input_tokens": 2051},
        "cache_creation_input_tokens": 2051, "cache_read_input_tokens": 2051,
        "inference_geo": "global", "input_tokens": 2095, "output_tokens": 503,
        "output_tokens_details": {"thinking_tokens": 0},
        "server_tool_use": {"web_fetch_requests": 0, "web_search_requests": 0},
        "service_tier": "standard"
    }})
}

/// NIK-152: Anthropic's usage is complete when every billed meter is accounted for: the bare
/// counts, nulls for the optional ones and the documented example all are. A missing count, a
/// 1-hour write, a server tool request, a priority tier, US-only inference, thinking beyond the
/// output, a lifetime split that disagrees with the write, or an unknown key is not.
#[test]
fn anthropic_usage_is_complete_only_with_every_billed_meter() {
    let complete = [
        anthropic(),
        json!({"usage": {"input_tokens": 12, "output_tokens": 5}}),
        json!({"usage": {
            "input_tokens": 12, "output_tokens": 5, "cache_read_input_tokens": null,
            "cache_creation_input_tokens": null, "cache_creation": null,
            "server_tool_use": null, "service_tier": null, "inference_geo": null,
            "output_tokens_details": null
        }}),
    ];
    for v in &complete {
        assert!(complete_usage("anthropic", v), "{v}");
        assert!(!complete_usage("gemini", v), "another provider's law: {v}");
    }
    let mut not = Vec::new();
    for (pointer, value) in [
        ("/usage/input_tokens", Value::Null),
        ("/usage/output_tokens", json!("503")),
        ("/usage/cache_creation/ephemeral_1h_input_tokens", json!(1)),
        (
            "/usage/cache_creation/ephemeral_5m_input_tokens",
            json!(2050),
        ),
        ("/usage/server_tool_use/web_search_requests", json!(1)),
        ("/usage/service_tier", json!("priority")),
        ("/usage/inference_geo", json!("us")),
        ("/usage/output_tokens_details/thinking_tokens", json!(504)),
        ("/usage/speed", json!("fast")),
    ] {
        let mut v = anthropic();
        let (parent, key) = pointer.rsplit_once('/').expect("pointer");
        v.pointer_mut(parent)
            .and_then(Value::as_object_mut)
            .expect("object")
            .insert(key.to_owned(), value);
        not.push(v);
    }
    not.push(json!({"usage": null}));
    not.push(json!({"usage": {"input_tokens": 0, "output_tokens": 0}}));
    not.push(json!({}));
    for v in &not {
        assert!(!complete_usage("anthropic", v), "{v}");
    }
}
