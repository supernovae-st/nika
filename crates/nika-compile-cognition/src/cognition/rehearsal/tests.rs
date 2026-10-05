// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scripted providers and rehearsal hosts exercise the compiler boundary without I/O.
//! These tests establish plumbing and decisions, not the real room's confinement.

use super::*;
use crate::rehearse::{
    Bounds, EffectCounts, FailureRecord, FinalReceipt, FinalState, LedgerFacts, Observation,
    RecordedCause, Refusal, RehearsalFuture, RehearsedOutput, RoomEvidence, Spent,
};
use crate::{
    AuthoringPolicy, NativeMode, compile_with_cognition, compile_with_cognition_rehearsed,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

const INTENT: &str = "Write the text hello to ./out/result.txt.";
const TARGET: &str = "./out/result.txt";

fn source(directories: bool) -> String {
    format!(
        r#"nika: greeting
permits:
  tools: ["nika:write"]
  fs:
    write: ["./out/result.txt"]
tasks:
  save:
    invoke:
      tool: "nika:write"
      args:
        path: "./out/result.txt"
        content: hello
        overwrite: true
        create_dirs: {directories}
"#
    )
}

fn answer(source: &str) -> String {
    json!({"candidate": source, "questions": [], "gaps": [], "notes": ""}).to_string()
}

/// The judge is an explicit approving double; it cannot bypass the rehearsal barrier.
struct Author {
    answers: Vec<String>,
    authored: AtomicUsize,
    seen: Mutex<Vec<String>>,
}

impl Author {
    fn new(answers: Vec<String>) -> Self {
        Self {
            answers,
            authored: AtomicUsize::new(0),
            seen: Mutex::new(Vec::new()),
        }
    }
}

impl ProviderInferDyn for Author {
    async fn infer(
        &self,
        request: InferRequest,
    ) -> std::result::Result<InferResponse, ProviderError> {
        let keys = match &request.response_format {
            ResponseFormat::JsonSchema(schema) => schema["properties"]["choice"]["enum"].as_array(),
            _ => None,
        };
        let approved = ["faithful", "carried"]
            .into_iter()
            .find(|key| keys.is_some_and(|keys| keys.iter().any(|value| value == *key)));
        let text = if let Some(choice) = approved {
            json!({"choice": choice}).to_string()
        } else {
            let n = self.authored.fetch_add(1, Ordering::SeqCst);
            let said = request
                .messages
                .iter()
                .flat_map(|message| &message.content)
                .filter_map(|block| match block {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            self.seen.lock().unwrap().push(said);
            self.answers.get(n).cloned().unwrap_or_default()
        };
        Ok(InferResponse::new(
            vec![ContentBlock::Text { text }],
            TokenUsage::new(1, 1),
            StopReason::EndTurn,
        ))
    }
}

#[derive(Clone, Copy)]
enum Mode {
    ByDirectories,
    Missing,
    NotRun,
    WrongDigest,
    DirtyRoom,
    Engine,
    StopAtBound,
}

struct Host {
    mode: Mode,
    candidates: Mutex<Vec<String>>,
    inputs: Mutex<Vec<Vec<String>>>,
    targets: Mutex<Vec<Vec<String>>>,
}

impl Host {
    fn new(mode: Mode) -> Self {
        Self {
            mode,
            candidates: Mutex::new(Vec::new()),
            inputs: Mutex::new(Vec::new()),
            targets: Mutex::new(Vec::new()),
        }
    }
}

impl Rehearse for Host {
    fn bound(&self) -> Duration {
        Duration::from_secs(10)
    }
    fn rehearse<'a>(&'a self, candidate: &'a str, inputs: &'a [String]) -> RehearsalFuture<'a> {
        self.rehearse_reading(candidate, inputs, &[])
    }
    fn rehearse_reading<'a>(
        &'a self,
        candidate: &'a str,
        inputs: &'a [String],
        targets: &'a [String],
    ) -> RehearsalFuture<'a> {
        Box::pin(async move {
            self.candidates.lock().unwrap().push(candidate.to_owned());
            self.inputs.lock().unwrap().push(inputs.to_vec());
            self.targets.lock().unwrap().push(targets.to_vec());
            report(candidate, self.mode)
        })
    }
}

fn report(candidate: &str, mode: Mode) -> RehearsalReport {
    let digest = crate::cognition::knowledge::sha256(candidate);
    if matches!(mode, Mode::NotRun) {
        return RehearsalReport::new(
            Rehearsal::NotRun {
                reason: "synthetic denied effect".into(),
            },
            Attempt::NeverAttempted,
            EffectCounts::none(),
            digest,
        )
        .with_observation(Observation::refused(Refusal::Effect));
    }
    let mut observed = Observation::none();
    observed.bounds = Bounds::new(10_000, 1_048_576, 65_536);
    observed.ledger = LedgerFacts::clean(Vec::new());
    observed.finals = vec![FinalReceipt::new(TARGET, FinalState::Absent)];
    let failed = candidate.contains("create_dirs: false") || matches!(mode, Mode::Engine);
    let stopped = matches!(mode, Mode::StopAtBound);
    let outcome = if matches!(mode, Mode::Missing) {
        Rehearsal::Missing {
            outputs: vec![TARGET.into()],
        }
    } else if stopped {
        Rehearsal::NotRun {
            reason: "synthetic time bound".into(),
        }
    } else if failed {
        let cause = if matches!(mode, Mode::Engine) {
            RecordedCause::Engine
        } else {
            RecordedCause::VerbError
        };
        observed.failure = Some(FailureRecord::new("save", "synthetic-write-failure", cause));
        Rehearsal::Failed {
            task: "save".into(),
            code: "synthetic-write-failure".into(),
            message: "the parent directory is absent".into(),
        }
    } else {
        let bytes = crate::rehearse::Digest::of(b"hello");
        observed.finals = vec![FinalReceipt::new(
            TARGET,
            FinalState::File {
                digest: bytes,
                held: crate::rehearse::Held::Whole("hello".into()),
            },
        )];
        observed.ledger = LedgerFacts::clean(vec![TARGET.into()]);
        observed.spent = Spent::new(0, 5);
        Rehearsal::Passed {
            outputs: vec![RehearsedOutput::new(TARGET, "hello")],
        }
    };
    let attempt = if stopped {
        Attempt::Stopped { elapsed_ms: 10_000 }
    } else {
        Attempt::Completed { elapsed_ms: 2 }
    };
    RehearsalReport::new(
        outcome,
        attempt,
        EffectCounts::none(),
        if matches!(mode, Mode::WrongDigest) {
            "wrong".to_owned()
        } else {
            digest
        },
    )
    .with_admitted_digest("synthetic-admission")
    .with_room(RoomEvidence::new(true, !matches!(mode, Mode::DirtyRoom)))
    .with_observation(observed)
}

fn request(repairs: u32) -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Only)
            .with_repairs(repairs),
    )
}

async fn compiled(request: &CompileRequest, author: &Author, host: &Host) -> CompileOutcome {
    compile_with_cognition_rehearsed(
        request,
        crate::Cognition {
            provider: Some(author),
            seat: None,
        },
        Some(host),
    )
    .await
    .unwrap()
}

/// The same greeting through the sketch door: one write the compiler emits from the graph and its
/// typed content fill.
fn sketch_request(repairs: u32) -> CompileRequest {
    CompileRequest::create(INTENT).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Sketch)
            .with_repairs(repairs),
    )
}

fn sketched() -> Author {
    let graph = json!({"name": "greeting", "tasks": [{"id": "save", "verb": "invoke",
        "tool": "nika:write", "purpose": "save the greeting", "writes": [TARGET]}],
        "questions": [], "gaps": [], "notes": "graph"});
    let fills = json!({"fills": [{"task": "save", "field": "args.content", "value": "hello"}],
        "notes": "fills"});
    Author::new(vec![graph.to_string(), fills.to_string()])
}

fn reports(out: &CompileOutcome) -> &[Value] {
    out.provenance.decision.as_ref().unwrap()["rehearsal"]["reports"]
        .as_array()
        .unwrap()
}

#[tokio::test]
async fn a_missing_output_is_not_a_completed_success() {
    let author = sketched();
    let out = compiled(&sketch_request(0), &author, &Host::new(Mode::Missing)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(reports(&out)[0]["outcome"]["kind"], "missing");
}

#[tokio::test]
async fn a_safe_not_run_is_explicit_and_does_not_invent_outputs() {
    let author = sketched();
    let out = compiled(&sketch_request(0), &author, &Host::new(Mode::NotRun)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(reports(&out)[0]["outcome"]["kind"], "not_run");
    assert_eq!(reports(&out)[0]["attempt"], "never_attempted");
    assert!(reports(&out)[0]["outcome"].get("outputs").is_none());
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"]["attempts"],
        0
    );
}

#[tokio::test]
async fn invalid_harness_or_engine_failure_never_buys_an_author_repair() {
    for mode in [Mode::WrongDigest, Mode::DirtyRoom, Mode::Engine] {
        let author = sketched();
        let out = compiled(&sketch_request(3), &author, &Host::new(mode)).await;
        assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
        // The sketch and its fills, and no authoring after the host's fault.
        assert_eq!(author.authored.load(Ordering::SeqCst), 2);
        assert_eq!(reports(&out)[0]["decision"]["kind"], "stop");
    }
}

#[tokio::test]
async fn absent_host_keeps_the_existing_entry_source_only() {
    let author = sketched();
    let out = compile_with_cognition(
        &sketch_request(0),
        crate::Cognition {
            provider: Some(&author),
            seat: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.provenance
            .decision
            .as_ref()
            .unwrap()
            .get("rehearsal")
            .is_none()
    );
}

#[tokio::test]
async fn the_common_barrier_checks_exact_final_bytes_and_does_not_trust_provenance() {
    let host = Host::new(Mode::Missing);
    let mut state = Rehearsals::new(Some(&host));
    let mut out = crate::initial();
    nika_compile::surface::finish(source(true), &mut out);
    out.provenance.decision =
        Some(json!({"rehearsal": {"reports": [{"outcome": {"kind": "passed"}}]}}));
    assert!(ready(&out), "{out:#?}");
    state.finish(&request(0), &mut out).await;
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(out.candidate.is_none());
    assert_eq!(host.candidates.lock().unwrap().as_slice(), [source(true)]);
    assert_eq!(reports(&out).len(), 1);
    assert_eq!(reports(&out)[0]["outcome"]["kind"], "missing");
}

#[tokio::test]
async fn no_host_call_starts_on_an_unchecked_candidate() {
    let host = Host::new(Mode::ByDirectories);
    let mut state = Rehearsals::new(Some(&host));
    let mut out = crate::initial();
    nika_compile::surface::finish("not a workflow".into(), &mut out);
    assert!(out.candidate.is_some());
    assert!(!ready(&out));
    state.finish(&request(0), &mut out).await;
    assert!(host.candidates.lock().unwrap().is_empty());
}

#[tokio::test]
async fn input_authority_comes_from_the_request_not_candidate_reads() {
    let host = Host::new(Mode::NotRun);
    let mut state = Rehearsals::new(Some(&host));
    let req = CompileRequest::create("Read ./in/source.txt and write it to ./out/result.txt.");
    let mut out = crate::initial();
    nika_compile::surface::finish(
        r#"nika: unrelated-read
permits:
  tools: ["nika:read"]
  fs:
    read: ["./private.txt"]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "./private.txt" }
"#
        .into(),
        &mut out,
    );
    assert!(ready(&out));
    state.inspect(&req, &out).await;
    assert_eq!(
        host.inputs.lock().unwrap().as_slice(),
        [vec!["./in/source.txt".to_owned()]]
    );
    assert_eq!(
        host.targets.lock().unwrap().as_slice(),
        [vec![TARGET.to_owned()]]
    );
}

#[tokio::test]
async fn a_changed_final_candidate_cannot_reuse_an_earlier_report() {
    let host = Host::new(Mode::ByDirectories);
    let mut state = Rehearsals::new(Some(&host));
    let mut first = crate::initial();
    nika_compile::surface::finish(source(true), &mut first);
    assert!(matches!(
        state.inspect(&request(0), &first).await.result,
        Result::Proceed
    ));
    let mut out = crate::initial();
    nika_compile::surface::finish(source(false), &mut out);
    assert!(ready(&out));
    state.finish(&request(0), &mut out).await;
    assert_ne!(out.status, CompileStatus::Ready);
    assert_eq!(host.candidates.lock().unwrap().len(), 2);
    assert_eq!(reports(&out)[1]["outcome"]["kind"], "failed");
}

/// A hand-written base with no semantic record keeps its bytes: no author call and no run.
#[tokio::test]
async fn a_manual_base_edit_is_kept_unrun_with_its_limitation() {
    let author = Author::new(vec![answer(&source(true))]);
    let host = Host::new(Mode::ByDirectories);
    let req = CompileRequest::edit(
        source(false),
        "Create missing parent directories before writing hello to ./out/result.txt.",
    )
    .with_original_intent(INTENT)
    .with_authoring_policy(sketch_request(0).authoring.unwrap());
    let out = compiled(&req, &author, &host).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(
        out.candidate.is_none() || out.candidate == Some(source(false)),
        "{out:#?}"
    );
    // One typed reading call (choice A); its answer is no revision, so nothing runs.
    assert_eq!(author.authored.load(Ordering::SeqCst), 1);
    assert!(host.candidates.lock().unwrap().is_empty());
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        (out.diagnostics.iter()).any(|d| d.message.contains("not a revision answer")),
        "the limitation is named: {out:#?}"
    );
}

fn forensic(out: &CompileOutcome) -> Value {
    out.provenance.decision.as_ref().unwrap()["forensic"].clone()
}

/// The forensic record binds a rehearsal to the exact final bytes and keeps every attempt's
/// identity; an observed run, a judge's verdict and a READY are never called satisfaction.
#[tokio::test]
async fn the_forensic_record_binds_rehearsal_to_the_final_bytes_and_proves_no_satisfaction() {
    // The sketch door's final bytes are bound to its rehearsal, and the compiler wrote them.
    let out = compiled(
        &sketch_request(0),
        &sketched(),
        &Host::new(Mode::ByDirectories),
    )
    .await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let summary = forensic(&out);
    let final_sha = crate::cognition::knowledge::sha256(out.candidate.as_deref().unwrap());
    assert_eq!(summary["evidence"]["candidate_sha256"], final_sha);
    assert_eq!(summary["evidence"]["rehearsal"]["bound_to_candidate"], true);
    assert_eq!(summary["evidence"]["satisfaction"], "UNKNOWN");
    assert_eq!(summary["door"]["reason"], "policy_sketch_before_hot");
    assert_eq!(
        summary["door"]["source_owner"],
        "compiler_from_model_sketch_and_fills"
    );
    // READY over a run the host never attempted: visible as such, never as an observation.
    let out = compiled(&sketch_request(0), &sketched(), &Host::new(Mode::NotRun)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let evidence = forensic(&out)["evidence"].clone();
    assert_eq!(evidence["rehearsal"]["state"], "not_run", "{evidence:#}");
    assert_eq!(evidence["rehearsal"]["outcome"], "not_run");
    assert_eq!(evidence["satisfaction"], "UNKNOWN");
    // A report for other bytes is not evidence about this candidate.
    let out = compiled(
        &sketch_request(0),
        &sketched(),
        &Host::new(Mode::WrongDigest),
    )
    .await;
    let evidence = forensic(&out)["evidence"].clone();
    assert_eq!(
        evidence["rehearsal"]["bound_to_candidate"], false,
        "{evidence:#}"
    );
}

fn sketched_open() -> Author {
    let graph = json!({"name": "greeting", "tasks": [{"id": "save", "verb": "invoke",
        "tool": "nika:write", "purpose": "save the greeting", "writes": [TARGET]}],
        "questions": [{"key": "const.greeting", "label": "What greeting?", "answer_type": "text",
                       "why": "the human chooses it"}],
        "gaps": [], "notes": "graph"});
    let fills = json!({"fills": [{"task": "save", "field": "args.content",
        "value": "${{ const.greeting }}"}], "notes": "fills"});
    Author::new(vec![graph.to_string(), fills.to_string()])
}

fn open_request() -> CompileRequest {
    CompileRequest::create("Write the greeting I choose to ./out/result.txt.")
        .with_authoring_policy(sketch_request(0).authoring.unwrap())
}

#[tokio::test]
async fn a_semantic_open_question_starts_no_rehearsal_and_its_answer_replays_afresh() {
    let author = sketched_open();
    let host = Host::new(Mode::ByDirectories);
    let req = open_request();
    let waiting = compiled(&req, &author, &host).await;
    assert_eq!(waiting.status, CompileStatus::Incomplete, "{waiting:#?}");
    assert!(host.candidates.lock().unwrap().is_empty());
    assert!(
        waiting.questions.iter().any(|q| q.key == "const.greeting"),
        "{waiting:#?}"
    );
    // The creation round keeps its replayable record and spends no rehearsal admission.
    assert!(waiting.provenance.plan.is_some(), "{waiting:#?}");
    assert!(reports(&waiting).is_empty(), "{waiting:#?}");
    assert!(
        !(waiting.diagnostics.iter()).any(|d| d.target == "rehearsal"),
        "{waiting:#?}"
    );
    assert_eq!(author.authored.load(Ordering::SeqCst), 2);
    let mut answered = req.with_plan(waiting.provenance.plan.unwrap());
    answered
        .answers
        .insert("const.greeting".into(), "\"hello\"".into());
    let finished = compiled(&answered, &author, &host).await;
    assert_eq!(finished.status, CompileStatus::Ready, "{finished:#?}");
    assert_eq!(
        author.authored.load(Ordering::SeqCst),
        2,
        "replay does not regenerate the sketch or its fills"
    );
    let candidates = host.candidates.lock().unwrap();
    assert_eq!(candidates.len(), 1, "the answered candidate runs afresh");
    assert_eq!(Some(&candidates[0]), finished.candidate.as_ref());
    // The answer round rehearses the bound bytes once, under the original budget.
    assert_eq!(reports(&finished).len(), 1, "{finished:#?}");
    assert_eq!(
        reports(&finished)[0]["candidate_sha256"],
        crate::cognition::knowledge::sha256(&candidates[0])
    );
    assert_eq!(
        finished.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"]["attempts"],
        1
    );
}

#[path = "tests/answered_paths.rs"]
mod answered_paths;

#[tokio::test]
async fn a_stopped_rehearsal_with_no_repair_budget_is_not_ready_and_counts_its_attempt() {
    let author = sketched();
    let out = compiled(&sketch_request(0), &author, &Host::new(Mode::StopAtBound)).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(
        author.authored.load(Ordering::SeqCst),
        2,
        "the sketch and its fills"
    );
    assert_eq!(reports(&out).len(), 1);
    assert_eq!(reports(&out)[0]["outcome"]["kind"], "not_run");
    assert_eq!(reports(&out)[0]["attempt"], "stopped");
    assert_eq!(reports(&out)[0]["decision"]["code"], "rehearsal_time_bound");
    assert_eq!(
        out.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"]["attempts"],
        1
    );
}

#[tokio::test]
async fn answers_are_baked_before_the_host_sees_the_candidate() {
    let author = sketched_open();
    let host = Host::new(Mode::ByDirectories);
    let mut req = open_request();
    req.answers
        .insert("const.greeting".into(), "\"hello\"".into());
    let out = compiled(&req, &author, &host).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let candidates = host.candidates.lock().unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(
        candidates[0].contains("greeting: \"hello\""),
        "{}",
        candidates[0]
    );
    assert_eq!(Some(&candidates[0]), out.candidate.as_ref());
    assert_eq!(
        reports(&out)[0]["candidate_sha256"],
        crate::cognition::knowledge::sha256(&candidates[0])
    );
}

#[tokio::test]
async fn an_open_question_starts_no_rehearsal_and_its_answer_replays_afresh() {
    let author = sketched_open();
    let host = Host::new(Mode::ByDirectories);
    let req = open_request();
    let waiting = compiled(&req, &author, &host).await;
    assert_eq!(waiting.status, CompileStatus::Incomplete, "{waiting:#?}");
    assert!(host.candidates.lock().unwrap().is_empty());
    assert!(
        waiting.questions.iter().any(|q| q.key == "const.greeting"),
        "{waiting:#?}"
    );
    // The creation round keeps its replayable record and spends no rehearsal admission.
    assert!(waiting.provenance.plan.is_some(), "{waiting:#?}");
    assert!(reports(&waiting).is_empty(), "{waiting:#?}");
    assert!(
        !(waiting.diagnostics.iter()).any(|d| d.target == "rehearsal"),
        "{waiting:#?}"
    );
    assert_eq!(author.authored.load(Ordering::SeqCst), 2);
    let mut answered = req.with_plan(waiting.provenance.plan.unwrap());
    answered
        .answers
        .insert("const.greeting".into(), "\"hello\"".into());
    let finished = compiled(&answered, &author, &host).await;
    assert_eq!(finished.status, CompileStatus::Ready, "{finished:#?}");
    assert_eq!(
        author.authored.load(Ordering::SeqCst),
        2,
        "replay does not regenerate the sketch or its fills"
    );
    let candidates = host.candidates.lock().unwrap();
    assert_eq!(candidates.len(), 1, "the answered candidate runs afresh");
    assert_eq!(Some(&candidates[0]), finished.candidate.as_ref());
    // The answer round rehearses the bound bytes once, under the original budget.
    assert_eq!(reports(&finished).len(), 1, "{finished:#?}");
    assert_eq!(
        reports(&finished)[0]["candidate_sha256"],
        crate::cognition::knowledge::sha256(&candidates[0])
    );
    assert_eq!(
        finished.provenance.decision.as_ref().unwrap()["rehearsal"]["usage"]["attempts"],
        1
    );
}
