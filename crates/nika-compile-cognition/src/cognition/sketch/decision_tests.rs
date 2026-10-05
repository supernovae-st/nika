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
    [
        json!({"name":"greeting","tasks":[{"id":"save","verb":"invoke","tool":"nika:write",
        "purpose":"save the greeting","writes":["./out/result.txt"]}],
        "questions":[],"gaps":[],"notes":"graph"})
        .to_string(),
        json!({"fills":[{"task":"save","field":"args.content","value":"hello"}],"notes":"fills"})
            .to_string(),
    ]
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

#[tokio::test]
async fn a_typed_defect_repairs_with_the_author_and_keeps_both_judgments() {
    let author = Author::new(answers().into_iter().chain(answers()));
    let judge = Judge::new([Ok("unfaithful"), Ok("another_part"), Ok("faithful")]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy().with_repairs(4));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    assert_eq!(author.calls(), 4);
    let ids: Vec<_> = judge.questions().into_iter().map(|q| q.id).collect();
    assert_eq!(ids, ["verify-request", "verify-locate", "verify-request"]);
    let records = verification(&out);
    assert_eq!(records.len(), 2, "first refusal must not disappear");
    assert_eq!(records[0]["attempted"], 2);
    assert_eq!(records[0]["defects"], json!([INTENT]));
    assert_eq!(records[1]["attempted"], 1);
    let requests = &author.0.lock().unwrap().1;
    let repair = serde_json::to_string(&requests[2].messages).unwrap();
    assert!(repair.contains("semantic_verification"), "{repair}");
    assert!(repair.contains(INTENT), "{repair}");
}

/// What every whole-request question of a create adds: a request to author this workflow
/// (« create report.nika that … ») does not ask the program to write its own file; nothing says
/// the file is saved; a stated name is judged on the bytes; the program's own writes stay judged.
/// A revision's questions keep their own wording.
const FILE_CLAUSE: &str = "it does not ask the program to write its own file";
const NOT_SAVED: &str = "it is neither missing nor done here";
const NAME: &str = "judged against the candidate's own `nika:` name";
const SAVE_PATH: &str = "A workflow identity is not proof of a Save filename or path";
const WRITES: &str =
    "including every file the program itself writes (another `.nika` file among them)";

#[tokio::test]
async fn a_create_tells_every_whole_request_question_what_authoring_this_workflow_asks() {
    let author = Author::new(answers().into_iter().chain(answers()));
    let judge = Judge::new([Ok("unfaithful"), Ok("another_part"), Ok("faithful")]);
    let request = CompileRequest::create(INTENT).with_authoring_policy(policy().with_repairs(4));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    let asked = judge.questions();
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    assert_eq!(ids, ["verify-request", "verify-locate", "verify-request"]);
    for question in &asked {
        let told = &question.instructions;
        for law in [FILE_CLAUSE, NOT_SAVED, NAME, SAVE_PATH, WRITES] {
            assert!(told.contains(law), "{}: {law}: {told}", question.id);
        }
        assert!(!told.contains("REVISES"), "{}: {told}", question.id);
        assert!(question.state.get("revision").is_none(), "{}", question.id);
    }
}

#[tokio::test]
async fn none_failure_and_unoffered_choice_cannot_authorize_or_fallback() {
    for answer in [Ok("none"), Err("unavailable"), Ok("invented")] {
        let (author, judge) = (Author::new(answers()), Judge::new([answer]));
        let request = CompileRequest::create(INTENT).with_authoring_policy(policy());
        let out = compile(request.clone(), &author, &judge).await;
        assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
        assert!(out.candidate.is_none());
        assert!(out.requested_boundary.is_none());
        assert_eq!(author.calls(), 2);
        assert_eq!(judge.questions().len(), 1);
        assert_eq!(verification(&out)[0]["unknown"], json!([INTENT]));
        let record = out.provenance.plan.expect("unjudged bytes are kept");
        assert!(
            out.diagnostics.iter().any(|d| {
                d.kind == crate::DiagnosticKind::Applied && d.target == "verify_resume"
            })
        );
        // Only an explicit continuation asks the selected judge again. Its successful
        // judgment reuses the same source, without falling back to the author as judge.
        let resumed_judge = Judge::new([Ok("faithful")]);
        let resumed = compile(request.with_plan(record.clone()), &author, &resumed_judge).await;
        writes(&resumed, "./out/result.txt");
        assert_eq!(author.calls(), 2, "the source was not regenerated");
        assert_eq!(resumed_judge.questions().len(), 1);
        assert_eq!(
            record["final"]["candidate_sha256"],
            crate::cognition::knowledge::sha256(resumed.candidate.as_deref().unwrap())
        );
    }
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

#[tokio::test]
async fn recovery_retains_the_refusal_that_opened_it() {
    let base = greeting().await.candidate.unwrap();
    let source =
        json!({"candidate":base,"candidate_lines":[],"questions":[],"gaps":[],"notes":"recovery"})
            .to_string();
    let author = Author::new(answers().into_iter().chain([source]));
    let judge = Judge::new([Ok("unfaithful"), Ok("another_part"), Ok("faithful")]);
    let request =
        CompileRequest::create(INTENT).with_authoring_policy(policy().with_source_recovery(1));
    let out = compile(request, &author, &judge).await;
    writes(&out, "./out/result.txt");
    assert_eq!(author.calls(), 3);
    assert_eq!(judge.questions().len(), 3);
    let records = verification(&out);
    assert_eq!(records.len(), 2);
    assert_eq!(records[0]["defects"], json!([INTENT]));
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
