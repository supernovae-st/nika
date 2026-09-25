// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The transport backoff through the wire: a fake transport answers 429
//! then 200, a recording clock proves what was waited, the captured
//! requests prove what was re-sent.

use std::time::Duration;

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, Message, ProviderError, ProviderInferDyn, ProviderStreamDyn, Role,
};
use nika_kernel::prelude::NikaErrorCode;
use serde_json::json;

use crate::retry::{MAX_RETRIES, MAX_RETRY_AFTER};
use crate::test_support::{Answer, FakeHttp, RecordingBackoff, resolved_with_backoff};

/// `MAX_RETRIES` as a count of things.
fn max_retries() -> usize {
    usize::try_from(MAX_RETRIES).expect("a small constant")
}

fn ok_body() -> String {
    json!({"choices":[{"message":{"content":"fine"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":7,"completion_tokens":3}})
    .to_string()
}

const RATE_LIMITED: &str =
    r#"{"error":{"code":"rate_limit_exceeded","type":"requests","message":"slow down"}}"#;
const OVERLOADED: &str = r#"{"error":{"type":"overloaded_error","message":"busy"}}"#;
const QUOTA: &str = r#"{"error":{"code":"insufficient_quota","type":"insufficient_quota"}}"#;

fn ask() -> InferRequest {
    InferRequest::new("m", vec![Message::text(Role::User, "q")])
}

#[tokio::test]
async fn a_429_then_200_answers_once_after_one_exponential_wait() {
    let ok = ok_body();
    let fake = FakeHttp::with_sequence(&[(429, RATE_LIMITED, &[]), (200, &ok, &[])]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "openai/gpt-4o-mini", "sk-test", clock.clone());

    let (response, report) = rp.infer_reported(ask()).await.expect("the second answer");
    assert!(matches!(&response.content[0], ContentBlock::Text { text } if text == "fine"));
    assert_eq!(report.attempts, 2);
    assert_eq!(report.statuses, vec![429]);
    assert_eq!(report.waited, Duration::from_secs(1));
    assert_eq!(clock.waits(), vec![Duration::from_secs(1)]);
    assert_eq!(
        report.summary().as_deref(),
        Some("retried 1× on HTTP 429 (waited 1.0 s)")
    );
    // The re-send is the IDENTICAL request: same body both times.
    let sent = fake.captured();
    assert_eq!(sent.len(), 2);
    assert_eq!(sent[0].body, sent[1].body);
    assert_eq!(sent[0].url, sent[1].url);
}

#[tokio::test]
async fn retry_after_governs_the_wait_when_the_seat_names_it() {
    let ok = ok_body();
    let fake = FakeHttp::with_sequence(&[
        (429, RATE_LIMITED, &[("Retry-After", "3")]),
        (429, RATE_LIMITED, &[("retry-after", "0.5")]),
        (200, &ok, &[]),
    ]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "xai/grok-3", "xai-test", clock.clone());
    let (_, report) = rp.infer_reported(ask()).await.expect("third answer");
    assert_eq!(report.attempts, 3);
    assert_eq!(report.statuses, vec![429, 429]);
    assert_eq!(
        clock.waits(),
        vec![Duration::from_secs(3), Duration::from_millis(500)],
        "the header wins over the schedule, case-insensitively"
    );
    assert_eq!(report.waited, Duration::from_millis(3500));
}

#[tokio::test]
async fn the_backoff_is_bounded_and_the_typed_error_survives() {
    // Four 429s: the initial call + MAX_RETRIES re-sends, then the last
    // error surfaces UNCHANGED (still transient, so an authored `retry:`
    // may still fire on top of the floor) with the report beside it.
    let fake = FakeHttp::with_sequence(&[
        (429, RATE_LIMITED, &[]),
        (429, RATE_LIMITED, &[]),
        (429, RATE_LIMITED, &[]),
        (429, RATE_LIMITED, &[]),
        (200, "never reached", &[]),
    ]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "openai/gpt-4o-mini", "sk-test", clock.clone());
    let (err, report) = rp.infer_reported(ask()).await.expect_err("spent");
    assert_eq!(fake.captured().len(), 1 + max_retries());
    assert_eq!(report.attempts, 1 + MAX_RETRIES);
    assert_eq!(report.statuses, vec![429; max_retries()]);
    assert_eq!(
        clock.waits(),
        vec![
            Duration::from_secs(1),
            Duration::from_secs(2),
            Duration::from_secs(4)
        ]
    );
    assert!(matches!(err, ProviderError::HttpResponse { .. }));
    assert!(
        err.is_transient(),
        "the author's retry: still sees a transient"
    );
    assert_eq!(err.nika_code().num, 332);
    assert!(err.to_string().contains("rate limited (HTTP 429)"));
}

#[tokio::test]
async fn a_retry_after_past_the_cap_surfaces_at_once_with_the_header() {
    let fake = FakeHttp::with_sequence(&[
        (429, RATE_LIMITED, &[("retry-after", "120")]),
        (200, "never reached", &[]),
    ]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "openai/gpt-4o-mini", "sk-test", clock.clone());
    let (err, report) = rp
        .infer_reported(ask())
        .await
        .expect_err("the human decides");
    assert!(
        clock.waits().is_empty(),
        "never waits past {MAX_RETRY_AFTER:?}"
    );
    assert_eq!(report.attempts, 1);
    assert!(!report.retried());
    assert!(err.to_string().contains("Retry-After=120"), "{err}");
}

#[tokio::test]
async fn exhausted_quota_and_a_wrong_request_are_never_re_sent() {
    for (status, body) in [(429, QUOTA), (400, RATE_LIMITED), (401, "{}"), (500, "{}")] {
        let fake = FakeHttp::with_sequence(&[(status, body, &[]), (200, "never", &[])]);
        let clock = RecordingBackoff::new();
        let rp = resolved_with_backoff(&fake, "openai/gpt-4o-mini", "sk-test", clock.clone());
        let (err, report) = rp.infer_reported(ask()).await.expect_err("terminal");
        assert_eq!(fake.captured().len(), 1, "{status} {body}");
        assert!(clock.waits().is_empty());
        assert_eq!(report.attempts, 1);
        let ProviderError::HttpResponse { details } = &err else {
            panic!("{err:?}")
        };
        assert_eq!(details.status(), status);
    }
}

#[tokio::test]
async fn anthropic_overloaded_and_a_503_back_off_like_a_429() {
    let ok = json!({"id":"msg","type":"message","role":"assistant","model":"claude",
        "content":[{"type":"text","text":"fine"}],"stop_reason":"end_turn",
        "usage":{"input_tokens":7,"output_tokens":3}})
    .to_string();
    let fake =
        FakeHttp::with_sequence(&[(529, OVERLOADED, &[]), (503, "{}", &[]), (200, &ok, &[])]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "anthropic", "sk-ant-test", clock.clone());
    let (_, report) = rp.infer_reported(ask()).await.expect("third answer");
    assert_eq!(report.statuses, vec![529, 503]);
    assert_eq!(
        clock.waits(),
        vec![Duration::from_secs(1), Duration::from_secs(2)]
    );
}

#[tokio::test]
async fn the_kernel_infer_rides_the_same_floor() {
    let ok = ok_body();
    let fake = FakeHttp::with_sequence(&[(503, "{}", &[]), (200, &ok, &[])]);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "deepseek/deepseek-chat", "sk-test", clock.clone());
    let response = ProviderInferDyn::infer(&rp, ask())
        .await
        .expect("the trait form retries too");
    assert!(matches!(&response.content[0], ContentBlock::Text { text } if text == "fine"));
    assert_eq!(clock.waits(), vec![Duration::from_secs(1)]);
}

#[tokio::test]
async fn a_streaming_open_refused_with_a_429_is_reopened_after_the_wait() {
    // The open is refused once (429 with a Retry-After), then the SSE
    // stream flows: nothing was consumed, so the re-open is the identical
    // request and the events arrive as if the first open had succeeded.
    let sse = "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"},\"finish_reason\":null}]}\n\n\
               data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n\
               data: [DONE]\n\n";
    let fake = FakeHttp::with_refusals_then_stream(
        &[(429, RATE_LIMITED, &[("retry-after", "2")])],
        200,
        sse,
        64,
    );
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "openai/gpt-4o-mini", "sk-test", clock.clone());
    let stream = match ProviderStreamDyn::infer_stream(&rp, ask()).await {
        Ok(stream) => stream,
        Err(err) => panic!("the re-open must succeed: {err}"),
    };
    let events = crate::test_support::collect(stream).await;
    assert!(
        events.iter().any(|e| matches!(e, Ok(nika_kernel::ai::provider::InferEvent::Delta { text }) if text == "hi")),
        "{events:?}"
    );
    assert_eq!(clock.waits(), vec![Duration::from_secs(2)]);
    assert_eq!(fake.captured().len(), 2);
}

#[tokio::test]
async fn a_streaming_open_stops_re_opening_at_the_bound() {
    let refusals: Vec<Answer<'_>> = vec![(429, RATE_LIMITED, &[]); 1 + max_retries()];
    let fake = FakeHttp::with_refusals_then_stream(&refusals, 200, "data: [DONE]\n\n", 64);
    let clock = RecordingBackoff::new();
    let rp = resolved_with_backoff(&fake, "openai/gpt-4o-mini", "sk-test", clock.clone());
    let Err(err) = ProviderStreamDyn::infer_stream(&rp, ask()).await else {
        panic!("every open was refused");
    };
    assert_eq!(clock.waits().len(), max_retries());
    assert_eq!(fake.captured().len(), 1 + max_retries());
    assert!(err.is_transient(), "{err}");
}

#[tokio::test]
async fn the_mock_wire_never_retries_or_reports_a_physical_dispatch() {
    let rp =
        crate::registry::ProviderRegistry::without_http(crate::registry::ProvidersConfig::new())
            .resolve("mock/echo")
            .expect("mock");
    let (response, report) = rp.infer_reported(ask()).await.expect("echo");
    assert_eq!(report.attempts, 0, "mock has no physical request");
    assert!(report.inference_calls.is_empty());
    assert!(response.inference_calls.is_empty());
    assert!(!report.retried());
}
