// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A provider call is never re-sent by the transport (the money admission's contract): a fake
//! transport answers a 429 · 503 · 529 and would then answer 200; the captured requests prove
//! nothing was re-sent, the typed rejection carries the delay the provider named, and the report
//! counts exactly the requests sent.

use std::time::Duration;

use nika_kernel::ai::provider::{
    InferRequest, Message, ProviderError, ProviderInferDyn, ProviderStreamDyn, Role,
};
use serde_json::json;

use crate::retry::{TransientRejection, transient_rejection};
use crate::test_support::{FakeHttp, resolved_with};

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
async fn a_429_503_or_529_ends_the_call_at_its_first_answer_and_nothing_is_re_sent() {
    let ok = ok_body();
    let named: &[(&str, &str)] = &[("retry-after", "2")];
    let cases = [
        (
            "openai/gpt-4o-mini",
            "sk-test",
            429,
            RATE_LIMITED,
            named,
            Some(Duration::from_secs(2)),
        ),
        (
            "deepseek/deepseek-chat",
            "sk-test",
            503,
            "{}",
            &[][..],
            None,
        ),
        ("anthropic", "sk-ant-test", 529, OVERLOADED, &[][..], None),
    ];
    for (model, key, status, body, headers, delay) in cases {
        let fake = FakeHttp::with_sequence(&[(status, body, headers), (200, &ok, &[])]);
        let rp = resolved_with(&fake, model, key);
        let (err, report) = (rp.infer_reported(ask()).await).expect_err("its first answer");
        assert_eq!(fake.captured().len(), 1, "{model}: nothing is re-sent");
        assert_eq!(
            report.attempts, 1,
            "{model}: the report counts the request sent"
        );
        assert!(!report.retried(), "{model}");
        assert_eq!(report.waited, Duration::ZERO, "{model}");
        assert!(err.is_transient(), "{model}: {err}");
        let rejection = Some(TransientRejection::new(status, delay));
        assert_eq!(transient_rejection(&err), rejection, "{model}");
    }
}

#[tokio::test]
async fn the_kernel_infer_ends_at_the_first_answer_and_its_error_carries_the_one_dispatch() {
    let ok = ok_body();
    let fake = FakeHttp::with_sequence(&[(503, "{}", &[]), (200, &ok, &[])]);
    let rp = resolved_with(&fake, "deepseek/deepseek-chat", "sk-test");
    let err = (ProviderInferDyn::infer(&rp, ask()).await).expect_err("never re-sent");
    assert_eq!(fake.captured().len(), 1);
    assert_eq!(
        err.inference_calls().len(),
        1,
        "a receipt reads the one dispatch sent"
    );
    assert_eq!(transient_rejection(&err).map(|r| r.status), Some(503));
}

#[tokio::test]
async fn exhausted_quota_and_a_wrong_request_end_the_call_as_no_transient_rejection() {
    for (status, body) in [(429, QUOTA), (400, RATE_LIMITED), (401, "{}"), (500, "{}")] {
        let fake = FakeHttp::with_sequence(&[(status, body, &[]), (200, "never", &[])]);
        let rp = resolved_with(&fake, "openai/gpt-4o-mini", "sk-test");
        let (err, report) = rp.infer_reported(ask()).await.expect_err("terminal");
        assert_eq!(fake.captured().len(), 1, "{status} {body}");
        assert_eq!(report.attempts, 1);
        assert_eq!(transient_rejection(&err), None, "{status} {body}");
        let ProviderError::HttpResponse { details } = &err else {
            panic!("{err:?}")
        };
        assert_eq!(details.status(), status);
    }
}

#[tokio::test]
async fn a_streaming_open_refused_with_a_429_is_never_re_opened() {
    let fake = FakeHttp::with_refusals_then_stream(
        &[(429, RATE_LIMITED, &[("retry-after", "2")])],
        200,
        "data: [DONE]\n\n",
        64,
    );
    let rp = resolved_with(&fake, "openai/gpt-4o-mini", "sk-test");
    let Err(err) = ProviderStreamDyn::infer_stream(&rp, ask()).await else {
        panic!("the refused open ends the call");
    };
    assert_eq!(fake.captured().len(), 1);
    assert_eq!(
        transient_rejection(&err),
        Some(TransientRejection::new(429, Some(Duration::from_secs(2))))
    );
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
