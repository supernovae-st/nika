// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Selected typed judgments at the public sketch entry; generation stays with the author.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use crate::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionError, DecisionSeat};
use crate::{
    AuthoringPolicy, Cognition, CompileOutcome, CompileRequest, CompileStatus, NativeMode,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex, time::Duration};

const INTENT: &str = "Write the text hello to ./out/result.txt.";
/// The one part of the request, as a doubted whole request asks it alone.
const PART: &str = "Write the text hello to ./out/result.txt";
const MODEL: &str = "mock/author";
const DECISION: &str = "mock/typed-judge";

struct Author(Mutex<(VecDeque<String>, Vec<InferRequest>)>);
impl Author {
    fn new(replies: impl IntoIterator<Item = String>) -> Self {
        Self(Mutex::new((replies.into_iter().collect(), Vec::new())))
    }
    fn calls(&self) -> usize {
        self.0.lock().unwrap().1.len()
    }
}
impl ProviderInferDyn for Author {
    async fn infer(&self, request: InferRequest) -> Result<InferResponse, ProviderError> {
        if let ResponseFormat::JsonSchema(schema) = &request.response_format {
            assert!(
                schema["properties"].get("choice").is_none(),
                "author became judge"
            );
        }
        let mut state = self.0.lock().unwrap();
        state.1.push(request);
        let text = state.0.pop_front().expect("unexpected author request");
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(100, 50),
            StopReason::EndTurn,
        ))
    }
}

struct Judge {
    answers: Mutex<VecDeque<Result<&'static str, &'static str>>>,
    questions: Mutex<Vec<ChoiceQuestion>>,
}
impl Judge {
    fn new(answers: impl IntoIterator<Item = Result<&'static str, &'static str>>) -> Self {
        Self {
            answers: Mutex::new(answers.into_iter().collect()),
            questions: Mutex::new(Vec::new()),
        }
    }
    fn questions(&self) -> Vec<ChoiceQuestion> {
        self.questions.lock().unwrap().clone()
    }
}
impl DecisionSeat for Judge {
    fn name(&self) -> &str {
        DECISION
    }
    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.questions.lock().unwrap().push(question.clone());
            let choice = self
                .answers
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected judgment");
            choice
                .map(|key| ChoiceAnswer::new(key, DECISION))
                .map_err(|e| DecisionError(e.into()))
        })
    }
}

fn policy() -> AuthoringPolicy {
    AuthoringPolicy::new(MODEL, 4096, Duration::from_secs(2))
        .with_native(NativeMode::Sketch)
        .with_repairs(0)
}
fn answers() -> [String; 2] {
    answers_named("greeting")
}
/// The greeting's sketch under the workflow name `name`, and its fills: another name is another
/// candidate's bytes.
fn answers_named(name: &str) -> [String; 2] {
    [
        json!({"name":name,"tasks":[{"id":"save","verb":"invoke","tool":"nika:write",
        "purpose":"save the greeting","writes":["./out/result.txt"]}],
        "questions":[],"gaps":[],"notes":"graph"})
        .to_string(),
        json!({"fills":[{"task":"save","field":"args.content","value":"hello"}],"notes":"fills"})
            .to_string(),
    ]
}
/// The semantic judge's line of the forensic summary `out` carries.
fn forensic(out: &CompileOutcome) -> &Value {
    &out.provenance.decision.as_ref().unwrap()["forensic"]["evidence"]["semantic_judge"]
}
async fn compile(request: CompileRequest, author: &Author, judge: &Judge) -> CompileOutcome {
    crate::compile_with_cognition(
        &request,
        Cognition {
            provider: Some(author),
            seat: Some(judge),
        },
    )
    .await
    .unwrap()
}
async fn greeting() -> CompileOutcome {
    compile(
        CompileRequest::create(INTENT).with_authoring_policy(policy()),
        &Author::new(answers()),
        &Judge::new([Ok("faithful")]),
    )
    .await
}
fn verification(out: &CompileOutcome) -> &[Value] {
    out.provenance.decision.as_ref().unwrap()["semantic_verification"]
        .as_array()
        .unwrap()
}
fn writes(out: &CompileOutcome, path: &str) {
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let source = out.candidate.as_ref().unwrap();
    assert!(nika_check::check(&crate::parse(source).unwrap()).is_clean());
    let doc: Value = serde_yaml_bw::from_str(source).unwrap();
    let tasks = doc["tasks"].as_object().unwrap();
    assert_eq!(tasks.len(), 1);
    let args = &tasks.values().next().unwrap()["invoke"]["args"];
    assert_eq!(args["content"], "hello");
    assert_eq!(args["path"], path);
    assert_eq!(doc["permits"]["fs"]["write"], json!([path]));
}

#[tokio::test]
async fn the_selected_judge_reads_final_bytes_and_never_becomes_the_author() {
    let (author, judge) = (Author::new(answers()), Judge::new([Ok("faithful")]));
    let out = compile(
        CompileRequest::create(INTENT).with_authoring_policy(policy()),
        &author,
        &judge,
    )
    .await;
    writes(&out, "./out/result.txt");
    assert_eq!(author.calls(), 2);
    let asked = judge.questions();
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].id, "verify-request");
    assert_eq!(
        asked[0].state["candidate_nika"],
        out.candidate.as_deref().unwrap()
    );
    assert_eq!(asked[0].state["request"], INTENT);
    assert_eq!(asked[0].keys(), ["faithful", "unfaithful", "none"]);
    let record = &verification(&out)[0];
    assert_eq!(
        record["judge"],
        json!({"seat":DECISION,"kind":"decision_seat"})
    );
    assert_eq!(record["attempted"], 1);
    assert_eq!(record["questions"][0]["role"], "judge_request");
    assert_eq!(
        record["usage"]["complete"], false,
        "unknown usage stays unknown"
    );
    assert_eq!(out.provenance.authoring.as_ref().unwrap().calls, 2);
}

/// The judge's answers that locate the greeting's one part: the request doubted, its part
/// missing, the task that writes it named; then the repaired candidate found faithful.
const LOCATED: [Result<&str, &str>; 4] = [
    Ok("unfaithful"),
    Ok("missing"),
    Ok("task-save"),
    Ok("faithful"),
];
/// The questions those answers settle, in order.
const LOCATED_IDS: [&str; 4] = [
    "verify-request",
    "verify-part-0",
    "verify-point-0",
    "verify-request",
];
/// The located defect as the reopened sketch reads it: the part, then the judge's reason.
const DEFECT: &str = "[semantic_verification] the judge compared the whole request with the candidate's bytes: it does not carry « Write the text hello to ./out/result.txt » · the judge's reason: the judge points to the task save";

/// A part the judge finds missing and pins to the task that fails it is a typed defect: the
/// author repairs the sketch from it, its reason beside it, and the repaired candidate (other
/// bytes: the sketch renamed) is READY on its own faithful judgment; both judgments stay on
/// record, and the forensic summary binds the last one to the final bytes.
#[tokio::test]
async fn a_typed_defect_repairs_with_the_author_and_keeps_both_judgments() {
    let repaired = answers_named("greeting-repaired");
    let author = Author::new(answers().into_iter().chain(repaired));
    let judge = Judge::new(LOCATED);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy().with_repairs(4));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    assert_eq!(author.calls(), 4);
    let asked = judge.questions();
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, LOCATED_IDS);
    assert_eq!(asked[2].keys(), ["task-save", "omitted", "no_task", "none"]);
    assert_eq!(asked[2].state["clause"], json!({"text": PART}));
    let records = verification(&out);
    assert_eq!(records.len(), 2, "first refusal must not disappear");
    assert_eq!(records[0]["attempted"], 3);
    assert_eq!(records[0]["defects"], json!([PART]));
    let note = json!([{"defect": PART, "note": "the judge points to the task save"}]);
    assert_eq!(records[0]["notes"], note);
    assert_eq!(records[0]["doubt"], json!(["unfaithful"]));
    assert_eq!(records[1]["attempted"], 1);
    assert_eq!(records[1]["settled_by"], "verify-request");
    assert_ne!(
        records[0]["candidate_sha256"],
        records[1]["candidate_sha256"]
    );
    let requests = &author.0.lock().unwrap().1;
    let repair = serde_json::to_string(&requests[2].messages).unwrap();
    assert!(repair.contains(DEFECT), "{repair}");
    assert!(repair.contains(INTENT), "{repair}");
    let summary = json!({"state": "recorded", "attempts": 2, "last_defects": 0,
        "last_unknown": 0, "last_contested": 0, "last_declined": false,
        "last_settled_by": "verify-request", "candidate_binding": "bound"});
    assert_eq!(forensic(&out), &summary);
}

/// A repaired sketch that yields the very bytes the judge declined asks it nothing (R6): the
/// attempt repeats the earlier verdict with no call, names the attempt it repeats, and the
/// candidate is withdrawn at the last round, never READY on a second vote of the same judge.
#[tokio::test]
async fn a_repaired_sketch_with_the_declined_bytes_is_not_judged_again() {
    let author = Author::new(answers().into_iter().chain(answers()));
    let judge = Judge::new(LOCATED[..3].iter().copied());
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy().with_repairs(2));
    let out = compile(request, &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(author.calls(), 4);
    let ids: Vec<String> = judge.questions().into_iter().map(|q| q.id).collect();
    assert_eq!(ids, LOCATED_IDS[..3]);
    let records = verification(&out);
    assert_eq!(records.len(), 2);
    let repeated = &records[1];
    let fields = [
        "attempted",
        "returned",
        "questions",
        "same_bytes_as",
        "defects",
        "declined",
    ]
    .map(|key| repeated[key].clone());
    let expected = [
        json!(0),
        json!(0),
        json!([]),
        json!(0),
        json!([PART]),
        json!(true),
    ];
    assert_eq!(fields, expected);
    assert_eq!(repeated["candidate_sha256"], records[0]["candidate_sha256"]);
    assert_eq!(repeated["notes"], records[0]["notes"]);
    let route = verify_route(&out);
    let same = "verify: same bytes, earlier verdict stands".to_owned();
    assert_eq!(route, [same, "verify: not ready".to_owned()]);
}

/// Bytes the selected judge rejected in an earlier round of the conversation, carried by the
/// host, are never asked of it again (R6): the sketch door authors the very same bytes and the
/// attempt repeats that verdict with no call, recorded as carried. Its located defect reopens the
/// sketch (an authoring call, never a judge call on those bytes): the reopened bytes are judged,
/// and READY on their own faithful judgment. With no reopening the count allows, the candidate is
/// withdrawn, with no call.
#[tokio::test]
async fn a_rejection_carried_from_an_earlier_round_reopens_the_sketch_with_no_call() {
    let (author, judge) = (
        Author::new(answers()),
        Judge::new(LOCATED[..3].iter().copied()),
    );
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy());
    let first = compile(request, &author, &judge).await;
    assert_eq!(first.status, CompileStatus::Incomplete, "{first:#?}");
    let earlier = verification(&first)[0].clone();
    let flags = ["rejected", "settled", "carried"].map(|key| earlier[key].clone());
    assert_eq!(flags, [json!(true), json!(false), json!(false)]);
    let replies = answers().into_iter().chain(answers_named("salutation"));
    let (author, judge) = (Author::new(replies), Judge::new([Ok("faithful")]));
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy().with_repairs(2))
        .with_declined(vec![earlier.clone()]);
    let out = compile(request, &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        author.calls(),
        4,
        "the sketch and its fills, then one reopening"
    );
    let asked: Vec<String> = (judge.questions().iter()).map(|q| q.id.clone()).collect();
    assert_eq!(asked, ["verify-request"], "only the reopened bytes");
    let records = verification(&out);
    assert_eq!(records.len(), 2);
    let fields = [
        "attempted",
        "questions",
        "carried",
        "same_bytes_as",
        "defects",
        "notes",
        "judge",
        "candidate_sha256",
    ]
    .map(|key| records[0][key].clone());
    let expected = [
        json!(0),
        json!([]),
        json!(true),
        Value::Null,
        json!([PART]),
        earlier["notes"].clone(),
        json!({"seat": DECISION, "kind": "decision_seat"}),
        earlier["candidate_sha256"].clone(),
    ];
    assert_eq!(fields, expected);
    assert_ne!(records[1]["candidate_sha256"], earlier["candidate_sha256"]);
    let carried = "verify: same bytes, rejected in an earlier round".to_owned();
    let judged = "verify: judged (decision_seat)".to_owned();
    assert_eq!(verify_route(&out), [carried, judged]);
    // No reopening granted: withdrawn, with no call.
    let (author, silent) = (Author::new(answers()), Judge::new([]));
    let request = CompileRequest::create(INTENT)
        .with_authoring_policy(policy())
        .with_declined(vec![earlier.clone()]);
    let out = compile(request, &author, &silent).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(author.calls(), 2, "the sketch and its fills, no reopening");
    assert!(silent.questions().is_empty(), "no call");
}

/// Under a repair count the exact defect set the seat was just asked to repair is no progress
/// (R4 A11): a repaired sketch whose bytes the judge already declined repeats the same defect
/// with no call, and the door stops reopening there instead of spending the rest of the count
/// on the same bytes.
#[tokio::test]
async fn under_a_repair_count_the_same_judged_defect_again_stops_reopening() {
    let author = Author::new(answers().into_iter().chain(answers()).chain(answers()));
    let judge = Judge::new(LOCATED[..3].iter().copied());
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy().with_repairs(4));
    let out = compile(request, &author, &judge).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(author.calls(), 4, "the repeat ends the reopenings");
    let route = out.provenance.decision.as_ref().unwrap()["route"].to_string();
    assert!(route.contains("native: no progress"), "{route}");
}

/// What every question of a create's whole-request judgment adds, the verdict, each part asked
/// alone and the task a missing one names: a request to author this workflow (« create report.nika that … ») does not ask the
/// program to write its own file; nothing says the file is saved; a stated name is judged on the
/// bytes; the program's own writes stay judged. A revision's questions keep their own wording.
const FILE_CLAUSE: &str = "it does not ask the program to write its own file";
const NOT_SAVED: &str = "it is neither missing nor done here";
const NAME: &str = "judged against the candidate's own `nika:` name";
const SAVE_PATH: &str = "A workflow identity is not proof of a Save filename or path";
const WRITES: &str =
    "including every file the program itself writes (another `.nika` file among them)";

#[tokio::test]
async fn a_create_tells_every_whole_request_question_what_authoring_this_workflow_asks() {
    let repaired = answers_named("greeting-repaired");
    let author = Author::new(answers().into_iter().chain(repaired));
    let judge = Judge::new(LOCATED);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy().with_repairs(4));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    let asked = judge.questions();
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, LOCATED_IDS);
    for question in &asked {
        let told = &question.instructions;
        for law in [FILE_CLAUSE, NOT_SAVED, NAME, SAVE_PATH, WRITES] {
            assert!(told.contains(law), "{}: {law}: {told}", question.id);
        }
        assert!(!told.contains("REVISES"), "{}: {told}", question.id);
        assert!(question.state.get("revision").is_none(), "{}", question.id);
    }
}

/// What an unjudged candidate's kept record offers the next round (the `verify_resume` finding,
/// as the verifier states it).
const RESUME: &str = "The candidate was not judged, so it is not offered; its bytes are kept: a round that replays this record under a judge asks it on the same candidate, with no new authoring call (a replay with no judge judges nothing).";
/// What a candidate judged and rejected with no defect located offers (the `verify_held`
/// finding, as the verifier states it).
const HELD: &str = "The candidate was judged and not accepted, with no defect a repair could start from: it is shown, never offered, and nothing was written. A correction of the request or another verifier can decide it.";
/// What a candidate the verifier only abstained on offers: an abstention is never carried to a
/// later round, so a new round that authors again can decide it.
const HELD_ABSTAINED: &str = "The verifier read the candidate and abstained: it neither accepted nor rejected it, and located no defect. It is shown, never offered, and nothing was written; it is not asked again on these bytes in this compile. A correction of the request, another verifier, or a new round that authors again can decide it.";
/// The extra-operation question left without a choice.
const EXTRA_UNSETTLED: &str =
    "whether any task does something the request does not ask (the judge made no choice)";
/// Why a disagreement with no run of these bytes stays contested.
const UNOBSERVED: &str = "no trial run of these exact bytes exists in this compile";
/// A doubt no part locates: the request unfaithful, its one part carried, no task doing more.
const UNLOCATED: [Result<&str, &str>; 3] = [Ok("unfaithful"), Ok("carried"), Ok("only_requested")];

/// The verification steps of the route `out` records, in order.
fn verify_route(out: &CompileOutcome) -> Vec<String> {
    let decision = out.provenance.decision.as_ref().unwrap();
    (decision["route"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .filter(|step| step.starts_with("verify:"))
        .map(str::to_owned)
        .collect()
}

/// A held candidate (R6): the bytes the judge read shown as the preview, never offered (no
/// question, no boundary), its replayable record dropped so no later round asks the same judge
/// again on them, and the `verify_held` finding saying, as `why`, what can decide it.
fn assert_held(out: &CompileOutcome, judged: &ChoiceQuestion, why: &str) {
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    let read = judged.state["candidate_nika"].as_str();
    assert_eq!(out.candidate.as_deref(), read, "{out:#?}");
    assert!(out.check_preview.is_some(), "{out:#?}");
    assert!(out.requested_boundary.is_none() && out.questions.is_empty());
    assert_eq!(out.provenance.plan, None);
    let held = (out.diagnostics.iter()).find(|d| d.target == "verify_held");
    let held = held.expect("the held candidate is named");
    assert_eq!(held.kind, crate::DiagnosticKind::Applied);
    assert_eq!(held.message, why);
    assert!(!(out.diagnostics.iter()).any(|d| d.target == "verify_resume"));
    assert_eq!(verify_route(out), ["verify: not ready, candidate held"]);
}

/// Neither NONE, nor a failed call, nor a choice no option offers authorizes the candidate or
/// falls back to the author as judge. NONE is an abstention: its part and the extra question,
/// answered NONE again, locate nothing and the request stays unknown; the judge having answered,
/// the candidate is held as an abstention, its record dropped. A failure or an unoffered choice
/// judges nothing: unknown, nothing more is asked, the bytes are kept, and only an explicit
/// continuation asks the selected judge again.
#[tokio::test]
async fn none_failure_and_unoffered_choice_cannot_authorize_or_fallback() {
    let (author, judge) = (Author::new(answers()), Judge::new([Ok("none"); 3]));
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy());
    let out = compile(request.clone(), &author, &judge).await;
    assert_eq!(author.calls(), 2);
    let asked = judge.questions();
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["verify-request", "verify-part-0", "verify-extra"]);
    let verified = &verification(&out)[0];
    assert_eq!(verified["defects"], json!([]));
    assert_eq!(verified["unknown"], json!([PART, EXTRA_UNSETTLED, INTENT]));
    assert_eq!(verified["contested"], json!([]));
    assert_eq!(verified["doubt"], json!(["none"]));
    let how = ["declined", "rejected", "stopped"].map(|key| verified[key].clone());
    assert_eq!(how, [json!(true), json!(false), json!(false)]);
    assert_held(&out, &asked[0], HELD_ABSTAINED);
    for verdict in [Err("unavailable"), Ok("invented")] {
        let (author, judge) = (Author::new(answers()), Judge::new([verdict]));
        let out = compile(request.clone(), &author, &judge).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert!(out.candidate.is_none());
        assert!(out.requested_boundary.is_none());
        assert_eq!(author.calls(), 2);
        let asked: Vec<String> = judge.questions().into_iter().map(|q| q.id).collect();
        assert_eq!(asked, ["verify-request"]);
        let verified = &verification(&out)[0];
        assert_eq!(verified["defects"], json!([]));
        assert_eq!(verified["unknown"], json!([INTENT]));
        assert_eq!(verified["doubt"], json!([]));
        let record = out
            .provenance
            .plan
            .clone()
            .expect("unjudged bytes are kept");
        let kept = (out.diagnostics.iter()).find(|d| d.target == "verify_resume");
        let kept = kept.expect("the kept record is named");
        assert_eq!(kept.kind, crate::DiagnosticKind::Applied);
        assert_eq!(kept.message, RESUME);
        let unjudged = ["verify: not ready", "verify: unjudged, record kept"];
        assert_eq!(verify_route(&out), unjudged);
        // Only an explicit continuation asks the selected judge again. Its successful
        // judgment reuses the same source, without falling back to the author as judge.
        let resumed_judge = Judge::new([Ok("faithful")]);
        let resumed = request.clone().with_plan(record.clone());
        let resumed = compile(resumed, &author, &resumed_judge).await;
        writes(&resumed, "./out/result.txt");
        assert_eq!(author.calls(), 2, "the source was not regenerated");
        assert_eq!(resumed_judge.questions().len(), 1);
        assert_eq!(
            record["final"]["candidate_sha256"],
            crate::cognition::knowledge::sha256(resumed.candidate.as_deref().unwrap())
        );
    }
}

/// A doubt no part locates and no task explains, with no run of these bytes, is held (R6): the
/// candidate the judge read is shown, never offered; its record is dropped so no later round
/// asks the same judge again on these bytes; the finding names the disagreement and what can
/// decide it, never a defect.
#[tokio::test]
async fn a_doubted_sketch_candidate_is_held_never_offered() {
    let (author, judge) = (Author::new(answers()), Judge::new(UNLOCATED));
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy());
    let out = compile(request, &author, &judge).await;
    assert_eq!(author.calls(), 2);
    let asked = judge.questions();
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["verify-request", "verify-part-0", "verify-extra"]);
    let verified = &verification(&out)[0];
    let lists = ["defects", "unknown", "contested", "doubt", "unsettled"].map(|k| &verified[k]);
    let expected = [
        json!([]),
        json!([]),
        json!([INTENT]),
        json!(["unfaithful"]),
        json!([UNOBSERVED]),
    ];
    assert_eq!(lists, expected.each_ref());
    let how = ["declined", "rejected", "whole_asked", "request"].map(|key| verified[key].clone());
    assert_eq!(how, [json!(true), json!(true), json!(true), json!(INTENT)]);
    assert_held(&out, &asked[0], HELD);
    let contested = format!(
        "The judge did not accept the request as carried (unfaithful) and located no defect a repair could start from; the same judge asked again decides nothing ({UNOBSERVED}). Nothing is READY on it. Next: a correction of the request, or another verifier."
    );
    let named: Vec<&str> = (out.diagnostics.iter())
        .filter(|d| d.target == "semantic_verification")
        .map(|d| d.message.as_str())
        .collect();
    assert_eq!(named, [contested.as_str()]);
    // The held bytes are the last verdict's: it judged them, declined them and settled nothing.
    let summary = json!({"state": "recorded", "attempts": 1, "last_defects": 0,
        "last_unknown": 0, "last_contested": 1, "last_declined": true,
        "last_settled_by": null, "candidate_binding": "bound"});
    assert_eq!(forensic(&out), &summary);
}

/// The answer round of a kept record (slice C) replays its bytes with no author call and asks
/// the selected judge: one that doubts them, locating nothing, leaves the round not READY and
/// drops the record, so no later round asks that judge again on these bytes (R6); one that
/// answers nothing keeps it.
#[tokio::test]
async fn a_replayed_record_the_judge_doubts_is_never_replayed_again() {
    let (author, judge) = (Author::new(answers()), Judge::new([Err("unavailable")]));
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy());
    let out = compile(request.clone(), &author, &judge).await;
    let record = out.provenance.plan.expect("unjudged bytes are kept");
    assert!(record.get("semantic_record").is_some(), "{record:#}");
    let doubting = Judge::new(UNLOCATED);
    let replayed = compile(
        request.clone().with_plan(record.clone()),
        &author,
        &doubting,
    )
    .await;
    assert_eq!(author.calls(), 2, "the record replays with no author call");
    let ids: Vec<String> = doubting.questions().into_iter().map(|q| q.id).collect();
    assert_eq!(ids, ["verify-request", "verify-part-0", "verify-extra"]);
    assert_eq!(replayed.status, CompileStatus::Incomplete, "{replayed:#?}");
    assert_eq!(replayed.provenance.plan, None);
    let doubted = ["verify: not ready", "verify: doubted, not replayable"];
    assert_eq!(verify_route(&replayed), doubted);
    let verified = verification(&replayed).last().unwrap();
    assert_eq!(verified["contested"], json!([INTENT]));
    assert_eq!(verified["doubt"], json!(["unfaithful"]));
    let held = (replayed.diagnostics.iter()).find(|d| d.target == "verify_held");
    assert_eq!(
        held.map(|d| d.message.as_str()),
        Some(HELD),
        "{replayed:#?}"
    );
    let silent = Judge::new([Err("unavailable")]);
    let kept = compile(request.with_plan(record), &author, &silent).await;
    assert_eq!(kept.status, CompileStatus::Incomplete, "{kept:#?}");
    assert!(kept.provenance.plan.is_some(), "{kept:#?}");
    assert_eq!(verify_route(&kept), ["verify: not ready"]);
    assert_eq!(author.calls(), 2);
}

/// A request of two parts, each a write a task of its own does.
const TWO: &str = "Write the text hello to ./out/a.txt, write the text bye to ./out/b.txt.";
/// Its parts, as a doubted whole request asks them alone.
const HELLO: &str = "Write the text hello to ./out/a.txt";
const BYE: &str = "write the text bye to ./out/b.txt";

/// The two writes' sketch under the workflow name `name` (another name: other bytes), and its
/// fills.
fn two_writes(name: &str) -> [String; 2] {
    [
        json!({"name":name,"tasks":[
            {"id":"hello","verb":"invoke","tool":"nika:write","purpose":"save hello",
                "writes":["./out/a.txt"]},
            {"id":"bye","verb":"invoke","tool":"nika:write","purpose":"save bye",
                "writes":["./out/b.txt"]}],
        "questions":[],"gaps":[],"notes":"graph"})
        .to_string(),
        json!({"fills":[{"task":"hello","field":"args.content","value":"hello"},
            {"task":"bye","field":"args.content","value":"bye"}],"notes":"fills"})
        .to_string(),
    ]
}

/// One verdict of the two writes naming, of its two parts, those `missing` (each pinned to the
/// task that writes it), the others carried. The seat is asked both parts together (A1), then
/// each missing part's task question in its turn.
fn naming(missing: [bool; 2]) -> Vec<Result<&'static str, &'static str>> {
    let mut answers = vec![Ok("unfaithful")];
    for missed in missing {
        answers.push(Ok(if missed { "missing" } else { "carried" }));
    }
    for (missed, task) in missing.into_iter().zip(["task-hello", "task-bye"]) {
        if missed {
            answers.push(Ok(task));
        }
    }
    answers
}

/// The two writes authored by the sketch door with no repair count, renamed at each round, the
/// judge naming the parts `rounds` lists in turn; the source recovery the stall opens gets an
/// answer that is no source.
async fn two_writes_judged(rounds: &[[bool; 2]]) -> (CompileOutcome, Author, Judge) {
    let mut replies: Vec<String> = Vec::new();
    for round in 0..rounds.len() {
        replies.extend(two_writes(&format!("two-writes-{round}")));
    }
    replies.push("I would rather describe the workflow in prose.".to_owned());
    let author = Author::new(replies);
    let judge = Judge::new(rounds.iter().flat_map(|missing| naming(*missing)));
    let continuous =
        AuthoringPolicy::new(MODEL, 4096, Duration::from_secs(2)).with_native(NativeMode::Sketch);
    let request = CompileRequest::create(TWO).with_authoring_policy(continuous);
    let out = compile(request, &author, &judge).await;
    (out, author, judge)
}

/// The defect sets the attempts of `out` recorded, in order.
fn defect_sets(out: &CompileOutcome) -> Vec<Value> {
    (verification(out).iter())
        .map(|attempt| attempt["defects"].clone())
        .collect()
}

/// Under no repair count, a judged defect set naming a part never named before is progress
/// (the first write, then the second): the door reopens; the first write named again after the
/// second is no progress (A, B, A), so the door stops reopening and the stall opens the source
/// recovery.
#[tokio::test]
async fn under_no_repair_count_a_part_named_again_after_another_stops_reopening() {
    let (out, author, judge) =
        two_writes_judged(&[[true, false], [false, true], [true, false]]).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert_eq!(
        author.calls(),
        7,
        "three sketches and their fills, then the recovery"
    );
    assert_eq!(judge.questions().len(), 12);
    let sets = [json!([HELLO]), json!([BYE]), json!([HELLO])];
    assert_eq!(defect_sets(&out), sets);
    let route = out.provenance.decision.as_ref().unwrap()["route"].to_string();
    assert!(route.contains("native: no progress"), "{route}");
}

/// Under no repair count, a judged defect set that narrows the last one (both writes, then the
/// second alone) is progress (R4 A11): the door reopens from it; the first write named again
/// after it is no progress, so the door stops reopening there.
#[tokio::test]
async fn under_no_repair_count_a_narrowed_judged_set_reopens_the_sketch() {
    let (out, author, judge) =
        two_writes_judged(&[[true, true], [false, true], [true, false]]).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(
        author.calls(),
        7,
        "three sketches and their fills, then the recovery"
    );
    assert_eq!(judge.questions().len(), 13);
    let sets = [json!([HELLO, BYE]), json!([BYE]), json!([HELLO])];
    assert_eq!(defect_sets(&out), sets);
}

#[tokio::test]
async fn source_recovery_keeps_the_selected_judge() {
    let base = greeting().await.candidate.unwrap();
    let source =
        json!({"candidate":base,"candidate_lines":[],"questions":[],"gaps":[],"notes":"recovery"})
            .to_string();
    let author = Author::new(["not a sketch".to_owned(), source]);
    let judge = Judge::new([Ok("faithful")]);
    let request =
        CompileRequest::create(INTENT).with_authoring_policy(policy().with_source_recovery(1));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    assert_eq!(author.calls(), 2);
    assert_eq!(judge.questions().len(), 1);
    assert_eq!(verification(&out)[0]["judge"]["kind"], "decision_seat");
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["native"]["recovery"]["accepted"],
        true
    );
}

/// A recovery the judge's refusal opened keeps that refusal on record beside the recovered
/// source's own judgment: the recovered source (other bytes: the workflow renamed) is READY only
/// on that faithful judgment.
#[tokio::test]
async fn recovery_retains_the_refusal_that_opened_it() {
    let base = greeting().await.candidate.unwrap();
    let recovered = base.replacen("nika: greeting\n", "nika: greeting-recovered\n", 1);
    assert_ne!(recovered, base);
    let source = json!({"candidate":recovered,"candidate_lines":[],"questions":[],"gaps":[],
        "notes":"recovery"})
    .to_string();
    let author = Author::new(answers().into_iter().chain([source]));
    let judge = Judge::new(LOCATED);
    let request =
        CompileRequest::create(INTENT).with_authoring_policy(policy().with_source_recovery(1));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    assert_eq!(author.calls(), 3);
    let ids: Vec<_> = judge.questions().into_iter().map(|q| q.id).collect();
    assert_eq!(ids, LOCATED_IDS);
    let records = verification(&out);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["defects"], json!([PART]));
    assert_eq!(records[1]["questions"][0]["choice"], "faithful");
}

#[tokio::test]
async fn source_and_semantic_destination_edits_keep_the_selected_judge() {
    let base = greeting().await;
    let source = base.candidate.clone().unwrap();
    let change = "Instead write the text hello to ./out/other.txt.";
    let links =
        json!({"supersedes":[{"replaces":INTENT.trim_end_matches('.'),"by":change.trim_end_matches('.')}],"adds":[],"notes":"destination"})
            .to_string();
    for semantic in [false, true] {
        let (author, judge) = (Author::new([links.clone()]), Judge::new([Ok("faithful")]));
        let mut request = CompileRequest::edit(source.clone(), change)
            .with_original_intent(INTENT)
            .with_authoring_policy(policy());
        if semantic {
            request = request.with_plan(base.provenance.plan.clone().unwrap());
        }
        let out = compile(request, &author, &judge).await;
        writes(&out, "./out/other.txt");
        assert_eq!(author.calls(), 1);
        assert_eq!(judge.questions().len(), 1);
        assert_eq!(verification(&out)[0]["judge"]["kind"], "decision_seat");
        assert!(
            judge.questions()[0].state["request"]
                .as_str()
                .unwrap()
                .contains("./out/other.txt")
        );
        let told = &judge.questions()[0].instructions;
        assert!(
            told.contains("REVISES") && !told.contains(FILE_CLAUSE) && !told.contains(SAVE_PATH),
            "{told}"
        );
    }
}

#[tokio::test]
async fn deterministic_refusal_never_consults_a_seat() {
    let (author, judge) = (Author::new([]), Judge::new([]));
    let request =
        CompileRequest::create("Write hello to ./out/result.txt. Do not write any files.")
            .with_authoring_policy(policy());
    let out = compile(request, &author, &judge).await;
    assert_ne!(out.status, CompileStatus::Ready);
    assert_eq!(author.calls(), 0);
    assert!(judge.questions().is_empty());
}
