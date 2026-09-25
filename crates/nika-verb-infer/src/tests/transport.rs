// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb over the provider layer's transport backoff: the receipt
//! carries the round-trips, a spent backoff names the seat and the wait,
//! a seat that refuses the schema at the door names the fix.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

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

/// A backoff seam that records every wait and never sleeps.
#[derive(Debug, Default)]
struct NoWait(std::sync::Mutex<Vec<Duration>>);

impl NoWait {
    fn waits(&self) -> Vec<Duration> {
        self.0.lock().expect("waits").clone()
    }
}

impl nika_providers::Backoff for NoWait {
    fn sleep(&self, duration: Duration) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        self.0.lock().expect("waits").push(duration);
        Box::pin(std::future::ready(()))
    }
}

fn verb(seam: &Arc<SeamHttp>, model: &str, backoff: Arc<NoWait>) -> InferVerb<SeamHttp> {
    let registry = Registry::new(
        Arc::clone(seam),
        ProvidersConfig::new()
            .with_key("openai", Secret::new("sk-test"))
            .with_key("deepseek", Secret::new("sk-test")),
    )
    .with_backoff(backoff);
    InferVerb::new(Arc::new(registry), model)
}

fn typed() -> serde_json::Value {
    json!({"type":"object","additionalProperties":false,"required":["v"],"properties":{"v":{"type":"string"}}})
}

#[tokio::test]
async fn a_429_then_200_answers_once_with_the_transport_in_the_receipt() {
    let ok = answer("fine");
    let seam = SeamHttp::with_answers(&[(429, RATE_LIMITED), (200, &ok)]);
    let waits = Arc::new(NoWait::default());
    let out = verb(&seam, "openai/gpt-4o-mini", waits.clone())
        .run(InferInput::new("q"))
        .await
        .expect("the second round-trip answers");
    assert!(matches!(&out.output, InferValue::Text(t) if t == "fine"));
    assert_eq!(out.transport.attempts, 2);
    assert_eq!(out.transport.statuses, vec![429]);
    assert_eq!(out.transport.waited, Duration::from_secs(1));
    assert_eq!(waits.waits(), vec![Duration::from_secs(1)]);
    assert_eq!(
        out.transport.summary().as_deref(),
        Some("retried 1× on HTTP 429 (waited 1.0 s)")
    );
    // Cost stays honest: the refused answer carried no usage, the one
    // that answered is billed once.
    assert_eq!(out.usage.input_tokens, 40);
    assert_eq!(out.usage.output_tokens, 6);
    assert_eq!(seam.captured().len(), 2);
}

#[tokio::test]
async fn a_schema_repair_sums_the_transport_across_round_trips() {
    // Round-trip 1: 429 → wait → 200 with an invalid reply (the transport
    // saw 2 attempts). Round-trip 2 (the schema repair): 200 valid. The
    // receipt reads the task total, exactly as usage does.
    let bad = answer("not json at all");
    let good = answer(r#"{"v":"ok"}"#);
    let seam = SeamHttp::with_answers(&[(429, RATE_LIMITED), (200, &bad), (200, &good)]);
    let waits = Arc::new(NoWait::default());
    let mut input = InferInput::new("q");
    input.schema = Some(typed());
    let out = verb(&seam, "openai/gpt-4o-mini", waits.clone())
        .run(input)
        .await
        .expect("repaired");
    assert!(matches!(out.output, InferValue::Structured(ref v) if v["v"] == "ok"));
    assert_eq!(out.transport.attempts, 3);
    assert_eq!(out.transport.statuses, vec![429]);
    assert_eq!(
        out.usage.input_tokens, 80,
        "two answered round-trips billed"
    );
    assert_eq!(seam.captured().len(), 3);
}

#[tokio::test]
async fn a_spent_backoff_names_the_seat_the_attempts_and_stays_transient() {
    let seam = SeamHttp::with_answers(&[
        (429, RATE_LIMITED),
        (429, RATE_LIMITED),
        (429, RATE_LIMITED),
        (429, RATE_LIMITED),
        (200, "never reached"),
    ]);
    let waits = Arc::new(NoWait::default());
    let err = verb(&seam, "openai/gpt-4o-mini", waits.clone())
        .run(InferInput::new("q"))
        .await
        .expect_err("the seat never answered");
    let VerbInferError::ProviderCallExhausted {
        model,
        attempts,
        waited_ms,
        ..
    } = &err
    else {
        panic!("{err:?}");
    };
    assert_eq!(model, "openai/gpt-4o-mini");
    assert_eq!(*attempts, 1 + nika_providers::MAX_RETRIES);
    assert_eq!(*waited_ms, 7000);
    assert_eq!(err.spec_code(), "NIKA-INFER-001");
    assert!(err.is_transient(), "an authored retry: may still fire");
    let calls = &err
        .spend()
        .expect("dispatches remain unknown")
        .inference_calls;
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|c| c.known_estimate().is_none()));
    let text = err.to_string();
    assert!(
        text.contains("on `openai/gpt-4o-mini` after 4 round-trips"),
        "{text}"
    );
    assert!(text.contains("7000 ms of backoff"), "{text}");
    assert!(text.contains("rate limited (HTTP 429)"), "{text}");
    assert_eq!(seam.captured().len(), 4);
}

#[tokio::test]
async fn a_400_on_a_native_schema_names_the_seat_and_the_fix() {
    let seam = SeamHttp::with_answers(&[(400, BAD_REQUEST)]);
    let waits = Arc::new(NoWait::default());
    let mut input = InferInput::new("q");
    input.schema = Some(typed());
    let err = verb(&seam, "openai/gpt-4o-mini", waits.clone())
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
    assert!(waits.waits().is_empty(), "a 400 is never re-sent");
    assert_eq!(seam.captured().len(), 1);
}

#[tokio::test]
async fn an_underspecified_schema_travels_as_json_object_and_says_so() {
    // `{"type":"object"}` is underspecified → the F2 fallback sends
    // `json_object`; a 400 there names THAT wire.
    let seam = SeamHttp::with_answers(&[(400, BAD_REQUEST)]);
    let mut input = InferInput::new("q");
    input.schema = Some(json!({"type":"object"}));
    let err = verb(&seam, "openai/gpt-4o-mini", Arc::new(NoWait::default()))
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
    let err = verb(&seam, "openai/gpt-4o-mini", Arc::new(NoWait::default()))
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
    let err = verb(&seam, "deepseek/deepseek-chat", Arc::new(NoWait::default()))
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
    let err = verb(
        &seam,
        "deepseek/deepseek-v4-pro",
        Arc::new(NoWait::default()),
    )
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
    let err = verb(
        &seam,
        "deepseek/deepseek-v4-pro",
        Arc::new(NoWait::default()),
    )
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
