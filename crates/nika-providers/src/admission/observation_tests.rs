// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! The reading law of the account's own observation, on what the serializer
//! writes: every observation an account produces reads as its own, and each
//! contradiction is named with the words the cost journal has always used.
use super::*;
use nika_kernel::ai::provider::{ContentBlock, StopReason, UsageCompleteness};
use serde_json::{Value, json};

const ENDPOINT: &str = "https://api.deepseek.com/v1/chat/completions";

fn complete() -> InferResponse {
    let mut usage = TokenUsage::new(100, 20);
    usage.cache_read_tokens = Some(0);
    let mut r = InferResponse::new(
        vec![ContentBlock::Text { text: "ok".into() }],
        usage,
        StopReason::EndTurn,
    );
    r.usage_completeness = UsageCompleteness::Complete;
    r.gen_ai.response_model = Some("deepseek-v4-pro".into());
    r
}

/// An account's observation after `steps`, exactly as the serializer wrote it.
fn written(steps: impl FnOnce(&InferenceAdmission)) -> Value {
    let account = InferenceAdmission::new(Cost::new(2_000_000_000)).expect("account");
    steps(&account);
    account.snapshot().expect("receipt").observation()
}

fn settled() -> Value {
    written(|a| {
        let mut call = a
            .reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
            .expect("reservation");
        call.sent().expect("dispatch");
        call.settle(&complete()).expect("settles");
    })
}

#[test]
fn every_observation_the_serializer_writes_reads_as_its_own() {
    let dropped = written(|a| {
        let mut call = a
            .reserve("deepseek", "deepseek-v4-pro", ENDPOINT, 8192)
            .expect("reservation");
        call.sent().expect("dispatch");
        drop(call);
    });
    let closed = written(|a| a.close("run ended").expect("close"));
    for (name, observation, state, moved) in [
        ("fresh", written(|_| {}), AdmissionState::Open, false),
        ("settled", settled(), AdmissionState::Open, true),
        ("dropped", dropped, AdmissionState::Uncertain, true),
        ("closed", closed, AdmissionState::Closed, false),
    ] {
        assert!(observation_readable(&observation), "{name}: {observation}");
        assert_eq!(
            observation_consistent(&observation),
            Ok((state, moved)),
            "{name}: {observation}"
        );
    }
}

#[test]
fn each_contradiction_is_named_as_the_cost_journal_names_it() {
    type Break = fn(&mut Value);
    let cases: [(&str, Break); 6] = [
        ("records its account without both attempt lists", |o| {
            o["attempts"] = Value::Null;
        }),
        ("records an attempt its account cannot write", |o| {
            o["attempts"][0]["sent"] = Value::Null;
        }),
        ("records an attempt its account cannot write", |o| {
            o["attempts"][0]["estimated_nano_usd"] = json!("-1");
        }),
        ("reports a negative known subtotal", |o| {
            o["known_subtotal_nano_usd"] = json!("-5");
        }),
        (
            "reports a known subtotal its attempts do not add up to",
            |o| {
                o["known_subtotal_nano_usd"] = json!("1");
            },
        ),
        (
            "counts unknown calls its sent attempts do not record",
            |o| {
                o["unknown_calls"] = json!(7);
            },
        ),
    ];
    let base = settled();
    assert_eq!(
        observation_consistent(&base),
        Ok((AdmissionState::Open, true))
    );
    for (reason, break_it) in cases {
        let mut observation = base.clone();
        break_it(&mut observation);
        assert_eq!(
            observation_consistent(&observation),
            Err(reason),
            "{observation}"
        );
    }
}

#[test]
fn only_what_the_serializer_writes_is_readable() {
    type Break = fn(&mut Value);
    let cases: [(&str, Break); 5] = [
        ("another schema", |o| {
            o["schema"] = json!("nika/inference-cost-observation@9");
        }),
        ("a subtotal that does not parse", |o| {
            o["known_subtotal_nano_usd"] = json!("12.5");
        }),
        ("a subtotal written as a number", |o| {
            o["known_subtotal_nano_usd"] = json!(0);
        }),
        ("an unknown-call count written as text", |o| {
            o["unknown_calls"] = json!("0");
        }),
        ("a state the account has no word for", |o| {
            o["state"] = json!("Paused");
        }),
    ];
    let base = written(|_| {});
    assert!(observation_readable(&base));
    for (name, break_it) in cases {
        let mut observation = base.clone();
        break_it(&mut observation);
        assert!(!observation_readable(&observation), "{name}");
    }
    assert!(!observation_readable(&Value::Null));
}

#[test]
fn the_route_and_request_ids_read_by_field() {
    assert_eq!(
        observation_route(&settled()),
        Value::Null,
        "no choice, no route"
    );
    let observation = json!({
        "unknown_cost": {"provider": "deepseek", "model": "deepseek-chat",
            "endpoint": "https://gateway.example/v1/chat/completions", "max_requests": 2},
        "attempts": [{"request_id": "catalog-1"}, {"id": 1}],
        "unknown_attempts": [{"request_id": "unknown-1"}, {"request_id": null}],
    });
    assert_eq!(
        observation_route(&observation),
        json!({"provider": "deepseek", "model": "deepseek-chat",
            "endpoint": "https://gateway.example/v1/chat/completions"})
    );
    assert_eq!(
        observation_request_ids(&observation),
        ["catalog-1", "unknown-1"],
        "catalog attempts first, in the order written"
    );
}

/// A durable observation (`@2`: W9 with amendments A1 and A8-A10), written by
/// hand from the wire contract, never by the projection: one settled
/// unknown-cost request to a gateway whose path it does not name.
const DURABLE: &str = r#"{
  "schema": "nika/inference-cost-observation@2",
  "known_subtotal_nano_usd": "0", "unknown_calls": 1,
  "unknown_cost": {"candidate": "c", "invocation": "i", "provider": "deepseek",
    "model": "deepseek-chat", "origin": "https://gateway.example:443", "max_requests": 1,
    "max_output_tokens": 64, "timeout_ms": 1000, "declared_tariff": null},
  "unknown_attempts": [{"id": 0,
    "choice": {"candidate": "c", "invocation": "i", "provider": "deepseek",
      "model": "deepseek-chat", "origin": "https://gateway.example:443", "max_requests": 1,
      "max_output_tokens": 64, "timeout_ms": 1000, "declared_tariff": null},
    "pricing": {"kind": "unknown", "table_schema": "nika/inference-admission@1.1",
      "route": {"provider": "deepseek", "model": "deepseek-chat",
        "origin": "https://gateway.example:443"},
      "billing_provider": null, "currency": null, "source": null, "route_source": null,
      "limits_source": null, "as_of": null, "source_sha256": null,
      "unit": "nano_currency_per_token", "input_rate": null, "output_rate": null,
      "cached_rate": null, "usd_conversion": null, "withheld": []},
    "sent": true, "usage": {"input_tokens": 10, "output_tokens": 2},
    "estimated_nano_usd": null, "native_estimated_nano": null, "currency": null,
    "response_model": "deepseek-chat", "request_id": "req-1",
    "note": "completed; USD cost unknown"}],
  "overridden_defaults": [null, null], "limit_nano_usd": null, "billed_nano_usd": null,
  "state": "Closed", "refusal": "Run ended", "attempts": [], "withheld": []
}"#;

#[test]
fn a_durable_observation_reads_by_the_same_law_and_names_its_origin() {
    let durable: Value = serde_json::from_str(DURABLE).expect("the frozen @2");
    assert!(observation_readable(&durable), "a well-formed @2 reads");
    assert_eq!(
        observation_consistent(&durable),
        Ok((AdmissionState::Closed, true))
    );
    assert_eq!(
        observation_route(&durable),
        json!({"provider": "deepseek", "model": "deepseek-chat",
            "origin": "https://gateway.example:443"})
    );
    assert_eq!(observation_request_ids(&durable), ["req-1"]);
}

#[test]
fn a_malformed_durable_observation_does_not_read() {
    type Break = fn(&mut Value);
    let cases: [(&str, Break); 8] = [
        ("an unknown key", |o| {
            o["extra"] = json!(1);
        }),
        ("a leftover endpoint", |o| {
            o["unknown_cost"]["endpoint"] = json!("https://gateway.example/p/v1");
        }),
        ("a path inside the origin", |o| {
            o["unknown_cost"]["origin"] = json!("https://gateway.example:443/p");
        }),
        ("a subtotal written as a number", |o| {
            o["known_subtotal_nano_usd"] = json!(0);
        }),
        ("a state the account has no word for", |o| {
            o["state"] = json!("Paused");
        }),
        ("a counter written as text", |o| {
            o["unknown_attempts"][0]["usage"]["input_tokens"] = json!("10");
        }),
        ("a withheld entry naming a kept field", |o| {
            o["withheld"] = json!([{"field": "/refusal", "reason": "endpoint_material"}]);
        }),
        ("no withheld list", |o| {
            o.as_object_mut().map(|fields| fields.remove("withheld"));
        }),
    ];
    let base: Value = serde_json::from_str(DURABLE).expect("the frozen @2");
    for (name, break_it) in cases {
        let mut observation = base.clone();
        break_it(&mut observation);
        assert!(!observation_readable(&observation), "{name}");
    }
}
