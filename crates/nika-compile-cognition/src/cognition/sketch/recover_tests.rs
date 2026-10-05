// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source recovery through the public creation entry, on a scripted seat: off by default; after a
//! structured answer that is not the sketch's, a valid source is READY and recorded as recovered;
//! an invalid source is never READY; a spent authority sends no further request; an edit keeps
//! its base and never reaches the recovery. The oracle is the candidate's own bytes, parsed and
//! checked, never the recovery's report of itself.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::authority::{Envelope, Seat as Authority};
use crate::{CompileOutcome, CompileRequest, CompileStatus, NativeMode};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const INTENT: &str = "Write the text hello to ./out/result.txt.";
const MODEL: &str = "mock/authoring";

fn policy(recovery: u32) -> crate::AuthoringPolicy {
    crate::AuthoringPolicy::new(MODEL, 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(0)
        .with_source_recovery(recovery)
}

fn completed(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text { text: text.into() }],
        TokenUsage::new(100, 50),
        StopReason::EndTurn,
    )
}

/// A seat answering its replies in order and keeping every author request; the whole-request
/// judge answers its scripted verdicts in order, then `faithful`, and locates a defect as
/// `another_part` (the judge's own laws are tested elsewhere).
struct Scripted {
    replies: Mutex<VecDeque<String>>,
    requests: Mutex<Vec<InferRequest>>,
    verdicts: Mutex<VecDeque<&'static str>>,
}

impl Scripted {
    fn new<const N: usize>(replies: [String; N]) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().collect()),
            requests: Mutex::new(Vec::new()),
            verdicts: Mutex::new(VecDeque::new()),
        }
    }

    fn judging<const N: usize>(self, verdicts: [&'static str; N]) -> Self {
        *self.verdicts.lock().unwrap() = verdicts.into_iter().collect();
        self
    }

    fn calls(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl ProviderInferDyn for Scripted {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if let ResponseFormat::JsonSchema(schema) = &request.response_format
            && let Some(keys) = schema["properties"]["choice"]["enum"].as_array()
            && let Some(offered) = ["faithful", "another_part", "carried"]
                .into_iter()
                .find(|k| keys.iter().any(|v| v == k))
        {
            let scripted =
                (offered == "faithful").then(|| self.verdicts.lock().unwrap().pop_front());
            let choice = scripted.flatten().unwrap_or(offered);
            return Ok(completed(&json!({"choice": choice}).to_string()));
        }
        self.requests.lock().unwrap().push(request);
        let reply = self.replies.lock().unwrap().pop_front();
        Ok(completed(&reply.expect("unexpected author request")))
    }
}

/// The greeting's sketch and fills, as a seat answers them.
fn greeting_answers() -> [String; 2] {
    let graph = json!({"name": "greeting", "tasks": [{"id": "save", "verb": "invoke",
        "tool": "nika:write", "purpose": "save the greeting", "writes": ["./out/result.txt"]}],
        "questions": [], "gaps": [], "notes": "graph"});
    let fills = json!({"fills": [{"task": "save", "field": "args.content", "value": "hello"}],
        "notes": "fills"});
    [graph.to_string(), fills.to_string()]
}

/// The greeting as the sketch door emits it: the source a correct seat would write.
async fn greeting() -> String {
    let seat = Scripted::new(greeting_answers());
    let out = authored(&seat, policy(0)).await;
    assert_eq!(
        out.status,
        CompileStatus::Ready,
        "HARNESS_INVALID: {out:#?}"
    );
    out.candidate.unwrap()
}

fn source(candidate: &str) -> String {
    json!({"candidate": candidate, "candidate_lines": [], "questions": [], "gaps": [],
        "notes": "source"})
    .to_string()
}

const BROKEN: &str = "I would rather describe the workflow in prose.";

async fn authored<P: ProviderInferDyn>(seat: &P, policy: crate::AuthoringPolicy) -> CompileOutcome {
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy);
    crate::compile_with_provider(&request, seat).await.unwrap()
}

/// Whether the outcome states that a recovered source passed every check (only READY may).
fn claims_passed(out: &CompileOutcome) -> bool {
    (out.diagnostics.iter()).any(|d| d.message.contains("passed the strict parser"))
}

fn decision(out: &CompileOutcome) -> &Value {
    out.provenance.decision.as_ref().unwrap()
}

fn route(out: &CompileOutcome) -> Vec<String> {
    let route = decision(out)["route"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    route
        .iter()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn roles(out: &CompileOutcome) -> Vec<String> {
    let receipt = out.provenance.authoring.as_ref().unwrap();
    (receipt.context.iter())
        .filter_map(|call| call["call"].as_str().map(str::to_owned))
        .collect()
}

/// The independent oracle: the bytes parse, the pure Check is clean, the one task writes the
/// stated text to the stated path, and nothing else is granted.
fn writes_hello(candidate: &str) {
    let workflow = crate::parse(candidate).unwrap();
    assert!(nika_check::check(&workflow).is_clean(), "{candidate}");
    let doc: Value = serde_yaml_bw::from_str(candidate).unwrap();
    let tasks = doc["tasks"].as_object().unwrap();
    assert_eq!(tasks.len(), 1, "{candidate}");
    let args = &tasks.values().next().unwrap()["invoke"]["args"];
    assert_eq!(args["path"], "./out/result.txt", "{candidate}");
    assert_eq!(args["content"], "hello", "{candidate}");
    assert_eq!(
        doc["permits"]["fs"]["write"],
        json!(["./out/result.txt"]),
        "{candidate}"
    );
    assert!(doc["permits"]["fs"].get("read").is_none(), "{candidate}");
}

#[tokio::test]
async fn without_the_policy_a_broken_structured_answer_ends_with_one_request() {
    let seat = Scripted::new([BROKEN.to_owned()]);
    let out = authored(&seat, policy(0)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(seat.calls(), 1);
    assert!(
        !route(&out).iter().any(|s| s == super::recover::ROUTE),
        "{out:#?}"
    );
}

#[tokio::test]
async fn a_broken_structured_answer_then_a_valid_source_is_ready_and_recorded() {
    let expected = greeting().await;
    let seat = Scripted::new([BROKEN.to_owned(), source(&expected)]);
    let out = authored(&seat, policy(2)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    writes_hello(out.candidate.as_deref().unwrap());
    // The same selected author, the same receipt: the sketch's charge is never reset.
    let requests = seat.requests.lock().unwrap();
    assert!(requests.iter().all(|r| r.model == MODEL));
    assert_eq!(roles(&out), ["sketch", "source-recovery", "judge_request"]);
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 3);
    // The recovery request carries the original request and the structured finding.
    let text = |r: &InferRequest| serde_json::to_string(&r.messages).unwrap();
    let asked = text(&requests[1]);
    assert!(
        asked.contains(INTENT) && asked.contains("previous_findings"),
        "{asked}"
    );
    assert!(asked.contains("not a sketch answer"), "{asked}");
    // The phase is the engine's, marked on the System message; the sketch instruction is kept.
    let system = |r: &InferRequest| serde_json::to_string(&r.messages[0]).unwrap();
    let (before, during) = (system(&requests[0]), system(&requests[1]));
    assert!(!before.contains("ENGINE-CONTROLLED PHASE"), "{before}");
    assert!(during.contains("ENGINE-CONTROLLED PHASE"), "{during}");
    assert!(
        during.starts_with(&before[..before.len() - 4]),
        "the sketch System text is kept"
    );
    // The fallback is recorded, never presented as the structured path.
    assert!(
        route(&out).iter().any(|s| s == super::recover::ROUTE),
        "{out:#?}"
    );
    let recovery = &decision(&out)["native"]["recovery"];
    assert_eq!(recovery["accepted"], true, "{recovery}");
    assert_eq!(
        (recovery["rounds"].as_u64(), recovery["spent"].as_u64()),
        (Some(2), Some(1))
    );
    assert!(
        recovery["after"]
            .to_string()
            .contains("not a sketch answer"),
        "{recovery}"
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_recovery"),
        "{out:#?}"
    );
    assert!(claims_passed(&out), "{out:#?}");
    // No semantic record is fabricated: the native record carries the recovered source.
    let plan = out.provenance.plan.as_ref().unwrap();
    assert_eq!(plan["strategy"], "native");
    assert!(plan.get("semantic_record").is_none(), "{plan}");
}

#[tokio::test]
async fn an_invalid_source_is_never_ready_and_its_repair_is_bounded() {
    let expected = greeting().await;
    // A source that writes elsewhere than the request states, then one that does not parse.
    let elsewhere = expected.replace("./out/result.txt", "./out/other.txt");
    let unparsable = "nika: greeting\ntasks:\n  - id: save\n";
    let seat = Scripted::new([BROKEN.to_owned(), source(&elsewhere), source(unparsable)]);
    let out = authored(&seat, policy(2)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(
        seat.calls(),
        3,
        "the sketch, then the two recovery rounds the policy states"
    );
    let rounds = decision(&out)["native"]["rounds"]
        .as_array()
        .cloned()
        .unwrap();
    let judged: Vec<&Value> = rounds.iter().filter(|r| r["phase"] == "recovery").collect();
    assert_eq!(judged.len(), 2, "{rounds:#?}");
    assert!(
        judged
            .iter()
            .all(|r| !r["diagnostics"].as_array().unwrap().is_empty())
    );
    assert_eq!(decision(&out)["native"]["recovery"]["accepted"], false);
    assert!(
        route(&out).iter().any(|s| s == "native: exhausted"),
        "{out:#?}"
    );
    assert!(!claims_passed(&out), "{out:#?}");
    // The structured finding it could not recover from stays on the outcome.
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("not a sketch answer"))
    );
}

#[tokio::test]
async fn a_repeated_refusal_is_no_progress_and_buys_no_further_round() {
    let expected = greeting().await;
    let elsewhere = source(&expected.replace("./out/result.txt", "./out/other.txt"));
    let seat = Scripted::new([BROKEN.to_owned(), elsewhere.clone(), elsewhere]);
    let out = authored(&seat, policy(3)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(seat.calls(), 3, "the third round is never asked");
    assert!(
        route(&out).iter().any(|s| s == "native: no progress"),
        "{out:#?}"
    );
}

#[tokio::test]
async fn under_no_repair_count_a_stalled_structured_door_switches_to_source_recovery() {
    let expected = greeting().await;
    let unbounded = || {
        crate::AuthoringPolicy::new(MODEL, 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
    };
    assert_eq!(
        (unbounded().repairs, unbounded().source_recovery),
        (None, 0)
    );
    // No operator count, yet the stall is no final barrier: the recovery opens and its valid
    // source is READY on the same independent oracle.
    let seat = Scripted::new([BROKEN.to_owned(), source(&expected)]);
    let out = authored(&seat, unbounded()).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    writes_hello(out.candidate.as_deref().unwrap());
    assert_eq!(roles(&out), ["sketch", "source-recovery", "judge_request"]);
    let recovery = &decision(&out)["native"]["recovery"];
    assert_eq!(recovery["rounds"], Value::Null, "no count: {recovery}");
    assert_eq!(recovery["spent"], 1, "{recovery}");
    let opened = "Source recovery opened: the structured doors made no further progress";
    assert!(
        (out.diagnostics.iter()).any(|d| d.message.starts_with(opened)),
        "{out:#?}"
    );
    // With no count of its own, the recovery still ends on a refusal it already answered.
    let elsewhere = source(&expected.replace("./out/result.txt", "./out/other.txt"));
    let seat = Scripted::new([BROKEN.to_owned(), elsewhere.clone(), elsewhere]);
    let out = authored(&seat, unbounded()).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(seat.calls(), 3);
    assert!(
        route(&out).iter().any(|s| s == "native: no progress"),
        "{out:#?}"
    );
    // A typed repair limit without the operator's recovery count keeps the old law.
    let seat = Scripted::new([BROKEN.to_owned()]);
    let out = authored(&seat, policy(0)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(seat.calls(), 1);
}

#[tokio::test]
async fn an_exhausted_total_authority_sends_no_further_request() {
    let expected = greeting().await;
    let inner = Scripted::new([BROKEN.to_owned(), source(&expected)]);
    let remedy = "authorize more requests";
    let seat = Authority::new(inner, Arc::new(Envelope::new(1, remedy)));
    let out = authored(&seat, policy(3)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(
        seat.inner().calls(),
        1,
        "the recovery request is refused before any byte leaves"
    );
    let receipt = out.provenance.authoring.as_ref().unwrap();
    assert_eq!(
        receipt.context.last().unwrap()["result"]["failure_kind"],
        "admission_refused"
    );
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_provider"),
        "{out:#?}"
    );
}

#[tokio::test]
async fn a_failed_edit_keeps_its_base_and_never_reaches_the_recovery() {
    let base = greeting().await;
    let seat = Scripted::new([BROKEN.to_owned()]);
    let request = CompileRequest::edit(base.clone(), "Also write the text bye to ./out/bye.txt.")
        .with_original_intent(INTENT)
        .with_authoring_policy(policy(3));
    let out = crate::compile_with_provider(&request, &seat).await.unwrap();
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.candidate.as_deref().is_none_or(|c| c == base),
        "{out:#?}"
    );
    assert_eq!(seat.calls(), 1, "no source request on an edit");
    assert!(
        !out.diagnostics
            .iter()
            .any(|d| d.target == "authoring_recovery"),
        "{out:#?}"
    );
    let route = out
        .provenance
        .decision
        .as_ref()
        .map(|d| d["route"].to_string());
    assert!(
        !route.unwrap_or_default().contains(super::recover::ROUTE),
        "{out:#?}"
    );
}

/// The faithful-negative whole-request case: the sketch door's candidate is found unfaithful at
/// its last round and withdrawn; without the policy nothing follows and nothing is READY.
#[tokio::test]
async fn a_whole_request_withdrawal_without_the_policy_stays_not_ready() {
    let seat = Scripted::new(greeting_answers()).judging(["unfaithful"]);
    let out = authored(&seat, policy(0)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(
        seat.calls(),
        2,
        "the sketch and its fills, no source request"
    );
    assert_eq!(
        roles(&out),
        ["sketch", "fill", "judge_request", "judge_locate"]
    );
    assert!(
        route(&out).iter().any(|s| s == "verify: not ready"),
        "{out:#?}"
    );
}

/// The same withdrawal under the policy reaches the recovery, carrying the judge's defect; the
/// recovered source is READY only on its own faithful judgment.
#[tokio::test]
async fn a_whole_request_withdrawal_at_the_last_round_is_recovered() {
    let expected = greeting().await;
    let [graph, fills] = greeting_answers();
    let seat = Scripted::new([graph, fills, source(&expected)]).judging(["unfaithful", "faithful"]);
    let out = authored(&seat, policy(1)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    writes_hello(out.candidate.as_deref().unwrap());
    let judged = [
        "judge_request",
        "judge_locate",
        "source-recovery",
        "judge_request",
    ];
    assert_eq!(roles(&out)[2..], judged);
    let asked = serde_json::to_string(&seat.requests.lock().unwrap()[2].messages).unwrap();
    assert!(
        asked.contains("evidence_defects") && asked.contains("does not carry"),
        "{asked}"
    );
    let recovery = &decision(&out)["native"]["recovery"];
    assert_eq!(recovery["accepted"], true, "{recovery}");
    assert!(
        !recovery["after"].as_array().unwrap().is_empty(),
        "{recovery}"
    );
}

/// A recovered source the whole-request judgment also finds unfaithful is withdrawn: never READY.
#[tokio::test]
async fn a_recovered_source_found_unfaithful_is_never_ready() {
    let expected = greeting().await;
    let [graph, fills] = greeting_answers();
    let seat = Scripted::new([graph, fills, source(&expected)]).judging(["unfaithful"; 2]);
    let out = authored(&seat, policy(1)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(seat.calls(), 3, "one recovery round, as the policy states");
    assert_eq!(roles(&out).last().map(String::as_str), Some("judge_locate"));
    assert!(
        route(&out).iter().any(|s| s == "verify: not ready"),
        "{out:#?}"
    );
    assert!(!claims_passed(&out), "{out:#?}");
}

/// The refusal a stopped door leaves, and whether any finding says an allowance was spent.
fn stop(out: &CompileOutcome, lead: &str) -> (String, bool) {
    let told = (out.diagnostics.iter()).find(|d| d.message.starts_with(lead));
    let spent = (out.diagnostics.iter()).any(|d| d.message.contains(" spent: "));
    (told.map(|d| d.message.clone()).unwrap_or_default(), spent)
}

/// A recovered source the judge refuses with the finding the seat was already asked to repair
/// stops the recovery while the operator's count still allows rounds: the refusal states that
/// observed repeat and names the finding, never that the recovery rounds are spent. The third
/// round the count allows is never asked.
#[tokio::test]
async fn a_recovery_judged_twice_on_the_same_part_states_the_repeat_not_spent() {
    let expected = greeting().await;
    let [graph, fills] = greeting_answers();
    let replies = [graph, fills, source(&expected), source(&expected)];
    let seat = Scripted::new(replies).judging(["unfaithful"; 3]);
    let out = authored(&seat, policy(3)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(seat.calls(), 4, "two recovery rounds of three: {out:#?}");
    let tail = ["source-recovery-repair", "judge_request", "judge_locate"];
    assert_eq!(roles(&out)[roles(&out).len() - 3..], tail, "{out:#?}");
    let lead = "The evidence refused the recovered source with findings the seat had already";
    let (told, spent) = stop(&out, lead);
    assert!(!spent, "a round was left: {out:#?}");
    assert!(told.contains("Nika stopped the recovery"), "{told}");
    assert!(
        told.contains("Same findings: ") && told.contains(INTENT),
        "{told}"
    );
}

/// With no repair count, a second sketch judged on the same finding stops reopening and
/// says so. The continuous door then tries one whole source: the same finding stops that too.
/// The repeat is of findings, despite the second graph differing; neither stop spends a limit.
#[tokio::test]
async fn a_sketch_judged_twice_on_the_same_part_states_the_repeat_not_spent() {
    let expected = greeting().await;
    let [graph, fills] = greeting_answers();
    let again = graph.replace("save the greeting", "save the greeting text");
    let replies = [graph, fills.clone(), again, fills, source(&expected)];
    let seat = Scripted::new(replies).judging(["unfaithful"; 3]);
    let continuous = crate::AuthoringPolicy::new(MODEL, 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch);
    assert_eq!((continuous.repairs, continuous.source_recovery), (None, 0));
    let out = authored(&seat, continuous).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(seat.calls(), 5, "two sketches, one recovery: {out:#?}");
    assert_eq!(
        roles(&out),
        [
            "sketch",
            "fill",
            "judge_request",
            "judge_locate",
            "sketch-repair",
            "fill",
            "judge_request",
            "judge_locate",
            "source-recovery",
            "judge_request",
            "judge_locate",
        ],
        "{out:#?}"
    );
    let lead = "The evidence refused this candidate with findings the seat had already";
    let (told, spent) = stop(&out, lead);
    assert!(!spent, "no repair count was selected: {out:#?}");
    assert!(told.contains("Nika stopped reopening it"), "{told}");
    assert!(
        told.contains("Same findings: ") && told.contains(INTENT),
        "{told}"
    );
    let lead = "The evidence refused the recovered source with findings the seat had already";
    let (told, _) = stop(&out, lead);
    assert!(told.contains("Nika stopped the recovery"), "{told}");
    assert!(told.contains(INTENT), "{told}");
    assert!(!claims_passed(&out), "{out:#?}");
}

/// A spent authority refuses the recovery request: the notice says the recovery opened, never
/// that a source passed.
#[tokio::test]
async fn an_admission_refused_recovery_claims_no_pass() {
    let inner = Scripted::new([BROKEN.to_owned()]);
    let seat = Authority::new(inner, Arc::new(Envelope::new(1, "authorize more requests")));
    let out = authored(&seat, policy(1)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let opened = (out.diagnostics.iter()).any(|d| d.target == "authoring_recovery");
    assert!(opened, "{out:#?}");
    assert!(!claims_passed(&out), "{out:#?}");
}

/// A recovered source that asks a business value stays a preview with its question: not READY,
/// and its rehearsal and whole-request judgment wait for the answer round, so it claims no pass.
#[tokio::test]
async fn a_recovered_source_with_an_open_question_claims_no_pass() {
    let asking = "nika: greeting\nconst:\n  greeting: \"\"\npermits:\n  fs:\n    write: [\"./out/result.txt\"]\n  tools: [\"nika:write\"]\ntasks:\n  save:\n    invoke:\n      tool: \"nika:write\"\n      args:\n        path: \"./out/result.txt\"\n        content: \"${{ const.greeting }}\"\n";
    let question = json!({"key": "const.greeting", "label": "Greeting text",
        "answer_type": "text", "why": "the text to write"});
    let answer = json!({"candidate": asking, "candidate_lines": [], "questions": [question],
        "gaps": [], "notes": "asks"});
    let seat = Scripted::new([BROKEN.to_owned(), answer.to_string()]);
    let out = authored(&seat, policy(1)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    let asked = (out.questions.iter()).any(|q| q.key == "const.greeting");
    assert!(
        asked,
        "the laws admitted the source and its question: {out:#?}"
    );
    assert!(!claims_passed(&out), "{out:#?}");
}

struct ThinkingScript(Scripted);

impl ProviderInferDyn for ThinkingScript {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        let mut answer = self.0.infer(request).await?;
        answer.content.insert(
            0,
            ContentBlock::Thinking {
                text: "private-recovery-thinking-canary; not the source".into(),
            },
        );
        Ok(answer)
    }
}

#[tokio::test]
async fn source_recovery_and_its_judge_read_only_final_text_beside_thinking() {
    let expected = greeting().await;
    let replies = || [BROKEN.to_owned(), source(&expected)];
    let baseline = authored(&Scripted::new(replies()), policy(1)).await;
    let seat = ThinkingScript(Scripted::new(replies()));
    let out = authored(&seat, policy(1)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(out.candidate, baseline.candidate);
    writes_hello(out.candidate.as_deref().unwrap());
    assert_eq!(roles(&out), ["sketch", "source-recovery", "judge_request"]);
    assert_eq!(
        seat.0.calls(),
        2,
        "thinking consumes no extra recovery round"
    );
    assert_eq!(
        decision(&out)["native"]["recovery"],
        decision(&baseline)["native"]["recovery"]
    );
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
    assert!(
        seat.0
            .requests
            .lock()
            .unwrap()
            .iter()
            .all(|r| r.model == MODEL)
    );
    assert!(!format!("{out:?}").contains("private-recovery-thinking-canary"));
}
