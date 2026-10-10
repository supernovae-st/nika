// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb over the provider layer's transport, which never re-sends a call: the receipt
//! carries the round-trips actually sent, a rate-limited seat ends the call at its first answer
//! and stays transient for an authored `retry:`, a seat that refuses the schema at the door
//! names the fix.

use super::*;
use nika_error::traits::NikaErrorCode;

const RATE_LIMITED: &str =
    r#"{"error":{"code":"rate_limit_exceeded","type":"requests","message":"slow down"}}"#;
const BAD_REQUEST: &str =
    r#"{"error":{"type":"invalid_request_error","message":"response_format unsupported"}}"#;

fn answer(text: &str) -> String {
    json!({"choices":[{"message":{"content":text},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":40,"completion_tokens":6}})
    .to_string()
}

fn verb(seam: &Arc<SeamHttp>, model: &str) -> InferVerb<SeamHttp> {
    let registry = Registry::new(
        Arc::clone(seam),
        ProvidersConfig::new()
            .with_key("openai", Secret::new("sk-test"))
            .with_key("deepseek", Secret::new("sk-test")),
    );
    InferVerb::new(Arc::new(registry), model)
}

fn typed() -> serde_json::Value {
    json!({"type":"object","additionalProperties":false,"required":["v"],"properties":{"v":{"type":"string"}}})
}

#[tokio::test]
async fn a_429_ends_the_call_at_its_first_answer_and_stays_transient() {
    let ok = answer("fine");
    let seam = SeamHttp::with_answers(&[(429, RATE_LIMITED), (200, &ok)]);
    let err = verb(&seam, "openai/gpt-4o-mini")
        .run(InferInput::new("q"))
        .await
        .expect_err("the first answer ends the call");
    assert!(
        matches!(err, VerbInferError::ProviderCall { .. }),
        "{err:?}"
    );
    assert_eq!(err.spec_code(), "NIKA-INFER-001");
    assert!(err.is_transient(), "an authored retry: may still fire");
    assert_eq!(seam.captured().len(), 1, "nothing is re-sent");
    let calls = &err.spend().expect("the dispatch sent").inference_calls;
    assert_eq!(calls.len(), 1, "the receipt counts the one request sent");
}

#[tokio::test]
async fn a_schema_repair_sums_the_transport_across_round_trips() {
    // Round-trip 1: an invalid reply. Round-trip 2 (the schema repair): a valid one. The
    // receipt reads the task total, exactly as usage does.
    let bad = answer("not json at all");
    let good = answer(r#"{"v":"ok"}"#);
    let seam = SeamHttp::with_answers(&[(200, &bad), (200, &good)]);
    let mut input = InferInput::new("q");
    input.schema = Some(typed());
    let out = verb(&seam, "openai/gpt-4o-mini")
        .run(input)
        .await
        .expect("repaired");
    assert!(matches!(out.output, InferValue::Structured(ref v) if v["v"] == "ok"));
    assert_eq!(out.transport.attempts, 2);
    assert!(!out.transport.retried());
    assert_eq!(
        out.usage.input_tokens, 80,
        "two answered round-trips billed"
    );
    assert_eq!(seam.captured().len(), 2);
}

#[tokio::test]
async fn a_400_on_a_native_schema_names_the_seat_and_the_fix() {
    let seam = SeamHttp::with_answers(&[(400, BAD_REQUEST)]);
    let mut input = InferInput::new("q");
    input.schema = Some(typed());
    let err = verb(&seam, "openai/gpt-4o-mini")
        .run(input)
        .await
        .expect_err("refused at the door");
    let VerbInferError::SchemaRefused { model, wire, .. } = &err else {
        panic!("{err:?}");
    };
    assert_eq!(model, "openai/gpt-4o-mini");
    assert_eq!(*wire, "json_schema");
    assert_eq!(err.spec_code(), "NIKA-INFER-001");
    assert!(!err.is_transient());
    let text = err.to_string();
    assert!(
        text.contains("`openai/gpt-4o-mini` rejected the structured request"),
        "{text}"
    );
    assert!(text.contains("native json_schema"), "{text}");
    assert!(text.contains("type=invalid_request_error"), "{text}");
    assert!(text.contains("json_mode: schema"), "{text}");
    assert_eq!(seam.captured().len(), 1);
}

#[tokio::test]
async fn an_underspecified_schema_travels_as_json_object_and_says_so() {
    // `{"type":"object"}` is underspecified → the F2 fallback sends
    // `json_object`; a 400 there names THAT wire.
    let seam = SeamHttp::with_answers(&[(400, BAD_REQUEST)]);
    let mut input = InferInput::new("q");
    input.schema = Some(json!({"type":"object"}));
    let err = verb(&seam, "openai/gpt-4o-mini")
        .run(input)
        .await
        .expect_err("refused");
    assert!(
        matches!(
            &err,
            VerbInferError::SchemaRefused {
                wire: "json_object",
                ..
            }
        ),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_400_without_a_native_schema_stays_a_plain_provider_call() {
    // No schema at all: the request carried nothing to refuse.
    let seam = SeamHttp::with_answers(&[(400, BAD_REQUEST)]);
    let err = verb(&seam, "openai/gpt-4o-mini")
        .run(InferInput::new("q"))
        .await
        .expect_err("refused");
    assert!(
        matches!(err, VerbInferError::ProviderCall { .. }),
        "{err:?}"
    );
    assert_eq!(err.spec_code(), "NIKA-INFER-001");

    // The instruction wire (DeepSeek honours no native schema): the schema
    // rode the prompt, so a 400 is the provider's own, not a schema refusal.
    let seam = SeamHttp::with_answers(&[(400, BAD_REQUEST)]);
    let mut input = InferInput::new("q");
    input.schema = Some(typed());
    let err = verb(&seam, "deepseek/deepseek-chat")
        .run(input)
        .await
        .expect_err("refused");
    assert!(
        matches!(err, VerbInferError::ProviderCall { .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_first_time_answer_reports_one_attempt_and_no_summary() {
    let ok = answer("fine");
    let seam = SeamHttp::with_json(&[&ok]);
    let out = openai_verb(&seam)
        .run(InferInput::new("q"))
        .await
        .expect("answers");
    assert_eq!(out.transport.attempts, 1);
    assert!(!out.transport.retried());
    assert_eq!(out.transport.summary(), None);
}

#[tokio::test]
async fn s80_failed_schema_repair_retains_prior_price_and_failed_dispatch() {
    let body = r#"{"model":"deepseek-v4-pro","choices":[{"message":{"content":"not-json"},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20,"total_tokens":110}}"#;
    let seam = SeamHttp::with_answers(&[(200, body), (500, BAD_REQUEST)]);
    let mut input = InferInput::new("q");
    input.schema = Some(typed());
    let err = verb(&seam, "deepseek/deepseek-v4-pro")
        .run(input)
        .await
        .expect_err("repair failed");
    let calls = &err.spend().expect("spend").inference_calls;
    assert_eq!(calls.len(), 2);
    assert_eq!(
        calls[0].known_estimate(),
        Some(nika_types::cost::Cost::new(69_520))
    );
    assert_eq!(calls[1].known_estimate(), None);
    assert!(calls[1].route.is_some());
}

#[tokio::test]
async fn s80_blank_answer_failure_keeps_observed_price_and_route() {
    let body = r#"{"model":"deepseek-v4-pro","choices":[{"message":{"content":""},"finish_reason":"stop"}],"usage":{"prompt_tokens":100,"completion_tokens":10,"prompt_cache_hit_tokens":80,"prompt_cache_miss_tokens":20,"total_tokens":110}}"#;
    let seam = SeamHttp::with_answers(&[(200, body)]);
    let err = verb(&seam, "deepseek/deepseek-v4-pro")
        .run(InferInput::new("q"))
        .await
        .expect_err("blank");
    let calls = &err.spend().expect("spend").inference_calls;
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].known_estimate(),
        Some(nika_types::cost::Cost::new(69_520))
    );
}
