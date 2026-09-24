// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Completed syntax feedback shares the native budget; uncertainty never buys a retry.
use super::*;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, Role, StopReason,
    TokenUsage,
};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::sync::Mutex;

enum Reply {
    Answer(Box<InferResponse>),
    Failed,
    Pending,
}

struct Seat {
    replies: Mutex<VecDeque<Reply>>,
    requests: Mutex<Vec<InferRequest>>,
}

impl Seat {
    fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl ProviderInferDyn for Seat {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        self.requests.lock().unwrap().push(request);
        let reply = self
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .expect("unexpected call");
        match reply {
            Reply::Answer(response) => Ok(*response),
            Reply::Failed => Err(ProviderError::Connection {
                reason: "uncertain transport after dispatch".into(),
            }),
            Reply::Pending => std::future::pending().await,
        }
    }
}

fn completed(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text { text: text.into() }],
        TokenUsage::new(100, 50),
        StopReason::EndTurn,
    )
}

fn reply(text: &str) -> Reply {
    Reply::Answer(Box::new(completed(text)))
}

fn good() -> String {
    answer(&candidate_a("./data/paiements.csv"), &json!([]))
}

async fn author(seat: &Seat, repairs: u32) -> nika_compile::CompileOutcome {
    let request =
        CompileRequest::create(CASE_A).with_authoring_policy(policy(NativeMode::Only, repairs));
    Box::pin(compile_with_provider(&request, seat))
        .await
        .unwrap()
}

fn refused(out: &nika_compile::CompileOutcome) {
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_ne!(native_record(out)["accepted"], true, "{out:#?}");
}

#[tokio::test]
async fn complete_syntax_error_is_repaired_with_same_request_contract_and_judged() {
    let malformed = r#"{"candidate": !}"#;
    let seat = Seat::new([reply(malformed), reply(&good())]);
    let out = author(&seat, 1).await;
    assert_eq!(seat.calls(), 2);
    assert_eq!(native_record(&out)["accepted"], true, "{out:#?}");
    assert_eq!(keys(&out), ["model"]); // acceptance does not fabricate a workflow model
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        (receipt.calls, receipt.input_tokens, receipt.output_tokens),
        (2, Some(200), Some(100))
    );
    assert_eq!(receipt.context[0]["call"], "native");
    assert_eq!(receipt.context[1]["call"], "native-repair");
    assert_eq!(
        receipt.context[0]["schema_sha256"],
        receipt.context[1]["schema_sha256"]
    );
    assert_eq!(
        receipt.context[0]["instruction_sha256"],
        receipt.context[1]["instruction_sha256"]
    );
    let native = native_record(&out);
    let rounds = native["rounds"].as_array().unwrap();
    assert_eq!(rounds.len(), 2);
    assert_eq!(rounds[0]["decode_error"]["category"], "Syntax");
    assert_eq!(
        rounds[0]["response_sha256"],
        format!("{:x}", Sha256::digest(malformed))
    );
    assert!(
        rounds[0]["answer"]
            .as_str()
            .unwrap()
            .contains("expected value")
    );
    assert!(rounds[0].get("candidate_sha256").is_none());
    assert!(rounds[1]["diagnostics"].as_array().unwrap().is_empty());
    let requests = seat.requests.lock().unwrap();
    for request in requests.iter() {
        assert_eq!(request.model, "mock/authoring");
        assert_eq!(request.max_tokens, Some(4096));
        assert_eq!(request.timeout, Some(Duration::from_secs(2)));
        assert!(request.tools.is_empty());
        assert!(request.extra.params.is_empty());
    }
    let messages = &requests[1].messages;
    assert_eq!(messages[messages.len() - 2].role, Role::Assistant);
    assert!(matches!(&messages[messages.len() - 2].content[..],
        [ContentBlock::Text { text }] if text == malformed));
    assert!(matches!(&messages.last().unwrap().content[..],
        [ContentBlock::Text { text }] if text.contains("answer_json_syntax")));
}

#[tokio::test]
async fn distinct_malformed_answers_exhaust_only_the_authorized_repairs() {
    let seat = Seat::new([
        reply(r#"{"candidate": !}"#),
        reply(r#"{"candidate": ?}"#),
        reply(r#"{"candidate": @}"#),
        reply(&good()),
    ]);
    let out = author(&seat, 2).await;
    refused(&out);
    assert_eq!(seat.calls(), 3);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 3);
    assert_eq!(native_record(&out)["rounds"].as_array().unwrap().len(), 3);
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("not a native answer"))
    );
}

#[tokio::test]
async fn zero_repairs_keeps_the_first_syntax_error_terminal() {
    let seat = Seat::new([reply(r#"{"candidate": !}"#), reply(&good())]);
    let out = author(&seat, 0).await;
    refused(&out);
    assert_eq!(seat.calls(), 1);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 1);
}

#[tokio::test]
async fn direct_policy_mutation_cannot_buy_more_than_five_repairs() {
    let replies = (0..7).map(|n| reply(&format!(r#"{{"candidate": !{n}}}"#)));
    let seat = Seat::new(replies);
    let mut bounded = policy(NativeMode::Only, 0);
    bounded.repairs = u32::MAX;
    let request = CompileRequest::create(CASE_A).with_authoring_policy(bounded);
    let out = Box::pin(compile_with_provider(&request, &seat))
        .await
        .unwrap();
    refused(&out);
    assert_eq!(seat.calls(), 6);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 6);
}

#[tokio::test]
async fn identical_malformed_answer_stalls_before_unused_repairs() {
    let malformed = r#"{"candidate": !}"#;
    let seat = Seat::new([reply(malformed), reply(malformed), reply(&good())]);
    let out = author(&seat, 5).await;
    refused(&out);
    assert_eq!(seat.calls(), 2);
    assert!(
        out.provenance.decision.as_ref().unwrap()["route"]
            .to_string()
            .contains("no progress")
    );
}

#[tokio::test]
async fn cap_incomplete_usage_and_stop_uncertainty_never_authorize_syntax_feedback() {
    let mut capped = completed(r#"{"candidate": !}"#);
    capped.stop_reason = StopReason::MaxTokens;
    capped.usage.output_tokens = 4096;
    let mut no_usage = completed(r#"{"candidate": !}"#);
    no_usage.usage_reported = false;
    let mut multi = completed(r#"{"candidate": !}"#);
    multi.content.push(ContentBlock::Text {
        text: "extra".into(),
    });
    let mut unknown_stop = completed(r#"{"candidate": !}"#);
    unknown_stop.stop_reason = StopReason::Unknown("unrecognized".into());
    for response in [
        capped,
        completed(r#"{"candidate":"#),
        no_usage,
        multi,
        unknown_stop,
    ] {
        let usage_reported = response.usage_reported;
        let capped = response.stop_reason == StopReason::MaxTokens;
        let seat = Seat::new([Reply::Answer(Box::new(response)), reply(&good())]);
        let out = author(&seat, 5).await;
        refused(&out);
        assert_eq!(seat.calls(), 1);
        let receipt = out.provenance.authoring.as_ref().unwrap();
        assert_eq!(receipt.calls, 1);
        if !usage_reported {
            assert_eq!((receipt.input_tokens, receipt.output_tokens), (None, None));
        }
        if capped {
            assert_eq!(
                native_record(&out)["rounds"][0]["answer"],
                "cut at the authoring cap"
            );
            assert_eq!(receipt.output_tokens, Some(4096));
        }
    }
}

#[tokio::test]
async fn failed_transport_after_syntax_feedback_preserves_partial_usage_without_retry() {
    let seat = Seat::new([reply(r#"{"candidate": !}"#), Reply::Failed, reply(&good())]);
    let out = author(&seat, 5).await;
    refused(&out);
    assert_eq!(seat.calls(), 2);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        (receipt.calls, receipt.input_tokens, receipt.output_tokens),
        (2, Some(100), Some(50))
    );
    assert_eq!(native_record(&out)["rounds"][1]["call"], "failed");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("uncertain transport"))
    );
}

#[tokio::test]
async fn unknown_transport_and_timeout_remain_terminal_on_the_opening_call() {
    for reply in [Reply::Failed, Reply::Pending] {
        let seat = Seat::new([reply]);
        let mut bounded = policy(NativeMode::Only, 5);
        bounded.timeout = Duration::from_millis(20);
        let request = CompileRequest::create(CASE_A).with_authoring_policy(bounded);
        let out = Box::pin(compile_with_provider(&request, &seat))
            .await
            .unwrap();
        refused(&out);
        assert_eq!(seat.calls(), 1);
        let receipt = out.provenance.authoring.as_ref().unwrap();
        assert_eq!(
            (receipt.calls, receipt.input_tokens, receipt.output_tokens),
            (1, None, None)
        );
    }
}

#[tokio::test]
async fn repaired_json_still_fails_the_original_intent_judge() {
    let unsafe_answer = answer(&candidate_a("./data/payments.csv"), &json!([]));
    let seat = Seat::new([
        reply(r#"{"candidate": !}"#),
        reply(&unsafe_answer),
        reply(&good()),
    ]);
    let out = author(&seat, 1).await;
    refused(&out);
    assert_eq!(seat.calls(), 2);
    let native = native_record(&out);
    assert!(
        native["rounds"][1]["diagnostics"]
            .to_string()
            .contains("INVENTED LITERAL")
    );
    assert!(native["rounds"][0].get("decode_error").is_some());
}

#[tokio::test]
async fn sketch_syntax_failure_keeps_its_existing_terminal_contract() {
    let seat = Seat::new([reply(r#"{"tasks": !}"#), reply(&good())]);
    let request =
        CompileRequest::create(CASE_A).with_authoring_policy(policy(NativeMode::Sketch, 3));
    let out = Box::pin(compile_with_provider(&request, &seat))
        .await
        .unwrap();
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(out.candidate.is_none());
    assert_eq!(seat.calls(), 1);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 1);
}
