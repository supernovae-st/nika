// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The transport and recovery law the semantic doors share with this module's decoder: an
//! answer that is not the schema's JSON ends its round with a named cause (the sketch door never
//! repairs a syntax error), uncertainty never buys a retry, a refused graph is repaired only
//! within the policy's bound (five at most), and usage is reported as the provider reported it.
//! Through the public creation entry under the sketch door: the source door these laws were
//! first written for (its line transport, its dual representations, its syntax feedback) is
//! retired, and no product path reaches it.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::{CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
    TokenUsage,
};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

const INTENT: &str = "Write the text hello to ./out/result.txt.";

fn policy(repairs: u32) -> crate::AuthoringPolicy {
    crate::AuthoringPolicy::new("mock/authoring", 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(repairs)
}

/// A graph for the greeting; `id` names its one task (a non-snake_case id is refused by name).
fn graph(id: &str) -> String {
    json!({"name": "greeting", "tasks": [{"id": id, "verb": "invoke", "tool": "nika:write",
        "purpose": "save the greeting", "writes": ["./out/result.txt"]}],
        "questions": [], "gaps": [], "notes": "graph"})
    .to_string()
}

fn fills(id: &str) -> String {
    json!({"fills": [{"task": id, "field": "args.content", "value": "hello"}], "notes": "fills"})
        .to_string()
}

enum Reply {
    Answer(Box<InferResponse>),
    Failed,
    Pending,
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

/// A provider answering its replies in order, keeping every request it received.
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
        // The judge's closed choice is approved: these laws are the author's transport's.
        if let nika_kernel::ai::provider::ResponseFormat::JsonSchema(schema) =
            &request.response_format
            && let Some(keys) = schema["properties"]["choice"]["enum"].as_array()
            && let Some(choice) = ["faithful", "carried"]
                .into_iter()
                .find(|k| keys.iter().any(|v| v == k))
        {
            return Ok(completed(&json!({"choice": choice}).to_string()));
        }
        self.requests.lock().unwrap().push(request);
        let reply = (self.replies.lock().unwrap().pop_front()).expect("unexpected call");
        match reply {
            Reply::Answer(response) => Ok(*response),
            Reply::Failed => Err(ProviderError::Connection {
                reason: "uncertain transport after dispatch".into(),
            }),
            Reply::Pending => std::future::pending().await,
        }
    }
}

async fn authored(seat: &Seat, policy: crate::AuthoringPolicy) -> CompileOutcome {
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy);
    crate::compile_with_provider(&request, seat).await.unwrap()
}

fn rounds(out: &CompileOutcome) -> Vec<Value> {
    let native = &out.provenance.decision.as_ref().unwrap()["native"];
    native["rounds"].as_array().cloned().unwrap_or_default()
}

fn roles(out: &CompileOutcome) -> Vec<String> {
    let receipt = out.provenance.authoring.as_ref().unwrap();
    (receipt.context.iter())
        .filter_map(|call| call["call"].as_str().map(str::to_owned))
        .collect()
}

fn refused(out: &CompileOutcome) {
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
}

fn no_clarification(out: &CompileOutcome) {
    let asked: Vec<&str> = out.questions.iter().map(|q| q.key.as_str()).collect();
    assert!(!asked.contains(&"intent.clarification"), "{out:#?}");
}

#[tokio::test]
async fn the_harness_control_is_ready_in_two_calls() {
    let seat = Seat::new([reply(&graph("save")), reply(&fills("save"))]);
    let out = authored(&seat, policy(0)).await;
    assert_eq!(
        out.status,
        CompileStatus::Ready,
        "HARNESS_INVALID: {out:#?}"
    );
    assert_eq!(roles(&out), ["sketch", "fill", "judge_request"]);
}

#[tokio::test]
async fn a_syntax_error_ends_the_round_with_its_cause_whatever_the_repair_budget() {
    for repairs in [0, 5] {
        let malformed = r#"{"tasks": !}"#;
        let seat = Seat::new([
            reply(malformed),
            reply(&graph("save")),
            reply(&fills("save")),
        ]);
        let out = authored(&seat, policy(repairs)).await;
        refused(&out);
        no_clarification(&out);
        assert_eq!(seat.calls(), 1, "repairs {repairs}");
        assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 1);
        let first = &rounds(&out)[0];
        assert_eq!(first["decode_error"]["category"], "Syntax", "{first}");
        assert_eq!(
            first["response_sha256"],
            crate::cognition::knowledge::sha256(malformed)
        );
    }
}

#[tokio::test]
async fn a_refused_graph_is_repaired_only_within_the_bound_of_five() {
    let refused_graphs: Vec<Reply> = (0..10)
        .map(|n| reply(&graph(&format!("Bad-{n}"))))
        .collect();
    let seat = Seat::new(refused_graphs);
    let mut generous = policy(5);
    generous.repairs = 9; // a direct mutation cannot buy more than five repairs
    let out = authored(&seat, generous).await;
    refused(&out);
    assert_eq!(seat.calls(), 6, "{:?}", roles(&out));
    let repairs = roles(&out).iter().filter(|r| *r == "sketch-repair").count();
    assert_eq!(repairs, 5);
}

#[tokio::test]
async fn cut_incomplete_unknown_or_unmetered_answers_end_after_one_call_with_honest_usage() {
    let mut capped = completed(&graph("save"));
    capped.stop_reason = StopReason::MaxTokens;
    capped.usage.output_tokens = 4096;
    let mut no_usage = completed(r#"{"tasks": !}"#);
    no_usage.usage_reported = false;
    let mut multi = completed(&graph("save"));
    multi.content.push(ContentBlock::Text {
        text: "extra".into(),
    });
    let mut unknown_stop = completed(&graph("save"));
    unknown_stop.stop_reason = StopReason::Unknown("unrecognized".into());
    for response in [capped, no_usage, multi, unknown_stop] {
        let (metered, cut) = (
            response.usage_reported,
            response.stop_reason == StopReason::MaxTokens,
        );
        let seat = Seat::new([Reply::Answer(Box::new(response)), reply(&fills("save"))]);
        let out = authored(&seat, policy(5)).await;
        refused(&out);
        assert_eq!(seat.calls(), 1);
        let receipt = out.provenance.authoring.as_ref().unwrap();
        assert_eq!(receipt.calls, 1);
        if !metered {
            assert_eq!((receipt.input_tokens, receipt.output_tokens), (None, None));
        }
        if cut {
            assert_eq!(rounds(&out)[0]["answer"], "cut at the authoring cap");
            assert_eq!(receipt.output_tokens, Some(4096));
        }
    }
}

#[tokio::test]
async fn a_failed_or_timed_out_opening_call_is_terminal_and_names_its_cause() {
    for failure in [Reply::Failed, Reply::Pending] {
        let seat = Seat::new([failure]);
        let mut bounded = policy(5);
        bounded.timeout = Duration::from_millis(20);
        let out = authored(&seat, bounded).await;
        refused(&out);
        no_clarification(&out);
        assert_eq!(seat.calls(), 1);
        let receipt = out.provenance.authoring.as_ref().unwrap();
        assert_eq!(
            (receipt.calls, receipt.input_tokens, receipt.output_tokens),
            (1, None, None)
        );
        assert!(
            out.diagnostics
                .iter()
                .any(|d| d.target == "authoring_provider"),
            "the cause is reported: {out:#?}"
        );
    }
}

#[tokio::test]
async fn a_failed_repair_call_keeps_the_partial_usage_and_is_never_retried() {
    let seat = Seat::new([reply(&graph("Bad-0")), Reply::Failed, reply(&graph("save"))]);
    let out = authored(&seat, policy(5)).await;
    refused(&out);
    assert_eq!(seat.calls(), 2);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        (receipt.calls, receipt.input_tokens, receipt.output_tokens),
        (2, Some(100), Some(50))
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("uncertain transport")),
        "{out:#?}"
    );
}

/// Every author and judge returns distinct reasoning beside its sole final answer.
struct ThinkingSeat(Seat);

impl ProviderInferDyn for ThinkingSeat {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let mut answer = self.0.infer(request).await?;
        answer.content.insert(
            0,
            ContentBlock::Thinking {
                text: "private-thinking-canary; a different candidate must not be parsed".into(),
            },
        );
        Ok(answer)
    }
}

#[tokio::test]
async fn sketch_fill_and_whole_judge_accept_separate_thinking_with_unchanged_usage() {
    let replies = || [reply(&graph("save")), reply(&fills("save"))];
    let baseline = authored(&Seat::new(replies()), policy(0)).await;
    let seat = ThinkingSeat(Seat::new(replies()));
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(0));
    let out = crate::compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate, baseline.candidate);
    assert_eq!(roles(&out), ["sketch", "fill", "judge_request"]);
    assert_eq!(seat.0.calls(), 2, "separate thinking buys no repair");
    let receipt = out.provenance.authoring.as_ref().unwrap();
    let original = baseline.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        (receipt.calls, receipt.input_tokens, receipt.output_tokens),
        (
            original.calls,
            original.input_tokens,
            original.output_tokens
        )
    );
    for (call, before) in receipt.context.iter().zip(&original.context) {
        assert_eq!(
            call["response"]["blocks"], 2,
            "raw shape remains observable"
        );
        assert_eq!(call["response"]["sha256"], before["response"]["sha256"]);
    }
    assert!(!format!("{out:?}").contains("private-thinking-canary"));
}
