// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The transport and recovery law the semantic doors share with this module's decoder: an
//! answer that is not the schema's JSON ends its round with a named cause (the sketch door never
//! repairs a syntax error), uncertainty never buys a retry, a refused graph is repaired only
//! within the policy's own count, and usage is reported as the provider reported it.
//! Through the public creation entry under the sketch door: the source door these laws were
//! first written for (its line transport, its dual representations, its syntax feedback) is
//! retired, and no product path reaches it. A typed repair count runs as typed; no count runs
//! until a failed call or findings already answered; a cut answer below the ceiling is asked
//! again once at the ceiling.
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

fn route(out: &CompileOutcome) -> Vec<String> {
    let route = &out.provenance.decision.as_ref().unwrap()["route"];
    (route.as_array().into_iter().flatten())
        .filter_map(|step| step.as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn a_refused_graph_is_repaired_exactly_as_many_times_as_typed() {
    // A caller-selected limit runs as typed, past the five the core once clamped to silently,
    // through a builder or a direct field alike.
    for typed in [9, 6] {
        let refused_graphs: Vec<Reply> = (0..=typed)
            .map(|n| reply(&graph(&format!("Bad-{n}"))))
            .collect();
        let seat = Seat::new(refused_graphs);
        let mut limited = policy(typed);
        limited.repairs = Some(typed);
        let out = authored(&seat, limited).await;
        refused(&out);
        let calls = usize::try_from(typed).unwrap() + 1;
        assert_eq!(seat.calls(), calls, "{typed}: {:?}", roles(&out));
        let repairs = roles(&out).iter().filter(|r| *r == "sketch-repair").count();
        assert_eq!(repairs, calls - 1, "{typed}");
    }
}

#[tokio::test]
async fn unbounded_repairs_end_on_a_failed_call_or_on_findings_already_answered() {
    let unbounded = || policy(0).with_unbounded_repairs();
    assert_eq!(unbounded().repair_limit(), None);
    // No count: each round with new findings is repaired, past any former cap, until the seat's
    // call fails (or the authority refuses it, or the caller stops the compile).
    let mut replies: Vec<Reply> = (0..12)
        .map(|n| reply(&graph(&format!("Bad-{n}"))))
        .collect();
    replies.push(Reply::Failed);
    let seat = Seat::new(replies);
    let out = authored(&seat, unbounded()).await;
    refused(&out);
    assert_eq!(seat.calls(), 13, "{:?}", roles(&out));
    // A set of findings met again, even not in a row, is no progress: the structured repairs
    // end there and, under no count, the stall switches to source recovery (its call fails
    // here, which ends it; nothing is READY).
    let cycle = ["Bad-0", "Bad-1", "Bad-0"].map(|id| reply(&graph(id)));
    let seat = Seat::new(cycle.into_iter().chain([Reply::Failed]));
    let out = authored(&seat, unbounded()).await;
    refused(&out);
    assert_eq!(seat.calls(), 4, "{:?}", roles(&out));
    assert_eq!(
        roles(&out).last().map(String::as_str),
        Some("source-recovery")
    );
    assert!(
        route(&out).iter().any(|s| s == "native: no progress"),
        "{out:#?}"
    );
    // A typed limit keeps its own rule: the same cycle is answered while the count lasts.
    let seat = Seat::new(["Bad-0", "Bad-1", "Bad-0", "Bad-1"].map(|id| reply(&graph(id))));
    let out = authored(&seat, policy(3)).await;
    refused(&out);
    assert_eq!(seat.calls(), 4, "{:?}", roles(&out));
    // An unbounded policy still reaches READY in its two calls when the seat is right.
    let seat = Seat::new([reply(&graph("save")), reply(&fills("save"))]);
    let out = authored(&seat, unbounded()).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(roles(&out), ["sketch", "fill", "judge_request"]);
}

/// Each authoring call records the requests it actually sent, read from the provider's own
/// record of its dispatches: a call the provider sent twice says two.
#[tokio::test]
async fn an_authoring_call_records_the_requests_it_sent() {
    let mut resent = completed(&graph("save"));
    resent.inference_calls.resize_with(2, Default::default);
    let seat = Seat::new([Reply::Answer(Box::new(resent)), reply(&fills("save"))]);
    let out = authored(&seat, policy(0)).await;
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(receipt.context[0]["requests_sent"], 2, "{receipt:#?}");
}

#[tokio::test]
async fn a_cut_answer_below_the_ceiling_is_asked_again_once_at_the_ceiling() {
    let opened = |initial| policy(0).with_initial_max_tokens(initial);
    let mut cut = completed(r#"{"name": "greeting", "tasks": ["#);
    cut.stop_reason = StopReason::MaxTokens;
    cut.usage.output_tokens = 1024;
    let seat = Seat::new([
        Reply::Answer(Box::new(cut.clone())),
        reply(&graph("save")),
        reply(&fills("save")),
    ]);
    let out = authored(&seat, opened(1024)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    // The cut call, the same call at the ceiling, then the fill opening at the ceiling: the
    // compile widened once, so it never opens below the ceiling again.
    let asked: Vec<Option<u32>> = (seat.requests.lock().unwrap().iter())
        .map(|r| r.max_tokens)
        .collect();
    assert_eq!(asked, [Some(1024), Some(4096), Some(4096)]);
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(roles(&out), ["sketch", "sketch", "fill", "judge_request"]);
    assert_eq!(receipt.context[0]["max_output_tokens"], 1024);
    assert_eq!(receipt.context[0]["widened_to"], 4096);
    assert_eq!(receipt.context[1]["max_output_tokens"], 4096);
    assert!(receipt.context[1].get("widened_to").is_none());
    // Every attempt is charged: the cut answer's output tokens stay in the totals.
    assert_eq!(receipt.output_tokens, Some(1024 + 50 + 50 + 50));
    // Cut again at the ceiling: nothing wider exists, so the round ends on the cut, named.
    let mut ceiling = cut;
    ceiling.usage.output_tokens = 4096;
    let seat = Seat::new([
        Reply::Answer(Box::new(ceiling.clone())),
        Reply::Answer(Box::new(ceiling)),
    ]);
    let out = authored(&seat, opened(1024)).await;
    refused(&out);
    assert_eq!(seat.calls(), 2);
    assert_eq!(rounds(&out)[0]["answer"], "cut at the authoring cap");
    // No initial limit: every call opens at the ceiling, and a cut there is not asked again.
    let mut capped = completed(&graph("save"));
    capped.stop_reason = StopReason::MaxTokens;
    let seat = Seat::new([Reply::Answer(Box::new(capped))]);
    let out = authored(&seat, policy(5)).await;
    refused(&out);
    assert_eq!(seat.calls(), 1);
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

#[tokio::test]
async fn each_call_reports_its_start_and_finish_and_a_dropped_compile_its_cancel() {
    use crate::observe::{CallState, observe_activity, observe_authoring};
    use std::sync::Arc;
    type Seen = Arc<Mutex<Vec<(u32, &'static str, String, CallState)>>>;
    let watch = |seen: &Seen| -> crate::observe::ActivitySink {
        let seen = Arc::clone(seen);
        Arc::new(move |call: &crate::observe::CallActivity<'_>| {
            let entry = (call.ordinal, call.role, call.model.to_owned(), call.state);
            seen.lock().unwrap().push(entry);
        })
    };
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(0));
    // Every real request, judge included, starts then finishes in call order, under the model
    // the policy requested; the existing observer, nested around it, still sees each authoring
    // answer (never a judge's: the sketch and the fill).
    let (seen, answers): (Seen, Arc<Mutex<u32>>) = (Arc::default(), Arc::default());
    let counted = Arc::clone(&answers);
    let observer: crate::observe::Sink =
        Arc::new(move |_: &crate::observe::AuthoringObservation<'_>| {
            *counted.lock().unwrap() += 1;
        });
    let seat = Seat::new([reply(&graph("save")), reply(&fills("save"))]);
    let compile = observe_activity(watch(&seen), crate::compile_with_provider(&request, &seat));
    let out = Box::pin(observe_authoring(observer, compile))
        .await
        .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let model = || "mock/authoring".to_owned();
    let (started, finished) = (CallState::Started, CallState::Finished);
    let expected: Vec<_> = (1..)
        .zip(["sketch", "fill", "judge_request"])
        .flat_map(|(n, role)| [(n, role, model(), started), (n, role, model(), finished)])
        .collect();
    assert_eq!(*seen.lock().unwrap(), expected);
    assert_eq!(*answers.lock().unwrap(), 2);
    // A compile dropped while its request is in flight reports that request cancelled.
    let seen: Seen = Arc::default();
    let seat = Seat::new([Reply::Pending]);
    let compile = observe_activity(watch(&seen), crate::compile_with_provider(&request, &seat));
    let dropped = Box::pin(tokio::time::timeout(Duration::from_millis(50), compile)).await;
    assert!(dropped.is_err(), "the compile was dropped mid-call");
    assert_eq!(
        *seen.lock().unwrap(),
        [
            (1, "sketch", model(), started),
            (1, "sketch", model(), CallState::Cancelled)
        ]
    );
}

#[tokio::test]
async fn a_qualified_route_profile_is_asked_as_stated_and_only_a_zero_limit_is_refused() {
    // A qualified route's profile (DeepSeek direct: 131072 first, 393216 at most, 600 s) is
    // asked as stated: the core invents no ceiling and clamps nothing.
    let profile = |max| {
        crate::AuthoringPolicy::new("mock/authoring", max, Duration::from_secs(600))
            .with_native(NativeMode::Sketch)
            .with_initial_max_tokens(131_072.min(max))
    };
    let seat = Seat::new([reply(&graph("save")), reply(&fills("save"))]);
    let out = authored(&seat, profile(393_216)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked: Vec<Option<u32>> = (seat.requests.lock().unwrap().iter())
        .map(|r| r.max_tokens)
        .collect();
    assert_eq!(asked, [Some(131_072), Some(131_072)]);
    // Only a limit of zero is no limit to ask under: refused before any request.
    let seat = Seat::new([]);
    let out = authored(&seat, profile(0)).await;
    refused(&out);
    assert_eq!(seat.calls(), 0);
    assert!(
        (out.diagnostics.iter()).any(|d| d.message.contains("a positive output-token limit")),
        "{out:#?}"
    );
}

#[tokio::test]
async fn a_long_request_reaches_the_seat_with_no_length_ceiling() {
    // Forty thousand bytes: past the old 32768 cap, a request no representation refuses; what
    // the route can hold is its provider's to answer (here the call fails, terminal as ever).
    let long = format!("{INTENT} {}", "Keep the greeting short. ".repeat(1_600));
    assert!(long.len() > 40_000);
    let seat = Seat::new([Reply::Failed]);
    let request = CompileRequest::create(long).with_authoring_policy(policy(0));
    let out = crate::compile_with_provider(&request, &seat).await.unwrap();
    assert_eq!(seat.calls(), 1, "the seat was asked: {out:#?}");
    assert!(
        !(out.diagnostics.iter()).any(|d| d.target == "authoring_policy"),
        "{out:#?}"
    );
}

/// The seat, behind a whole-request judge whose call fails while `down` holds.
struct JudgeDown {
    seat: Seat,
    down: std::sync::atomic::AtomicBool,
}

impl ProviderInferDyn for JudgeDown {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let judged = matches!(&request.response_format,
            nika_kernel::ai::provider::ResponseFormat::JsonSchema(schema)
                if schema["properties"]["choice"]["enum"].is_array());
        if judged && self.down.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(ProviderError::Connection {
                reason: "judge transport down".into(),
            });
        }
        self.seat.infer(request).await
    }
}

#[tokio::test]
async fn an_unjudged_candidate_keeps_its_record_and_a_later_round_judges_it_unregenerated() {
    let judge = JudgeDown {
        seat: Seat::new([reply(&graph("save")), reply(&fills("save"))]),
        down: std::sync::atomic::AtomicBool::new(true),
    };
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy(0));
    let out = crate::compile_with_provider(&request, &judge)
        .await
        .unwrap();
    // The judge's failure is no defect: nothing is READY and no candidate is offered, but the
    // record of the bytes it could not judge stays the round's continuation.
    refused(&out);
    no_clarification(&out);
    assert!(
        route(&out)
            .iter()
            .any(|s| s == "verify: unjudged, record kept"),
        "{out:#?}"
    );
    let record = out.provenance.plan.clone().expect("the record is kept");
    assert!(record.get("semantic_record").is_some(), "{record:#}");
    let resume = (out.diagnostics.iter()).find(|d| d.target == "verify_resume");
    assert!(
        resume.is_some_and(|d| d.kind == crate::DiagnosticKind::Applied),
        "the host and the human are told the round can resume: {out:#?}"
    );
    assert_eq!(judge.seat.calls(), 2, "the sketch and its fill");
    // A later round replays those bytes, with no author call, and its judge now answers.
    judge.down.store(false, std::sync::atomic::Ordering::SeqCst);
    let again = request.clone().with_plan(record.clone());
    let out = crate::compile_with_provider(&again, &judge).await.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.seat.calls(), 2, "nothing was regenerated");
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(
        record["final"]["candidate_sha256"],
        crate::cognition::knowledge::sha256(candidate),
        "the bytes judged are the bytes the first round could not judge"
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
