// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Scripted providers and rehearsal hosts exercise the compiler boundary without I/O.
//! These tests establish plumbing and decisions, not the real room's confinement.

use super::*;
use crate::decide::{ChoiceAnswer, ChoiceFuture, ChoiceQuestion, DecisionSeat};
use crate::rehearse::{
    Bounds, CopyReceipt, Digest, EffectCounts, FailureRecord, FinalReceipt, FinalState, Held,
    LedgerFacts, Observation, RecordedCause, Refusal, RehearsalFuture, RehearsedOutput,
    RoomEvidence, Spent,
};
use crate::{
    AuthoringPolicy, NativeMode, compile_with_cognition, compile_with_cognition_rehearsed,
};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, ResponseFormat,
    StopReason, TokenUsage,
};
use std::collections::VecDeque;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

const INTENT: &str = "Write the text hello to ./out/result.txt.";
const TARGET: &str = "./out/result.txt";
/// The input a request that reads names, copied into the room by the copying modes.
const SOURCE: &str = "./in/source.txt";

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
    /// The run reads [`SOURCE`] and writes its text to [`TARGET`], both held whole.
    Copied,
    /// The same run, both texts held only as a prefix cut at the preview bound.
    Prefixed,
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
    } else if matches!(mode, Mode::Copied | Mode::Prefixed) {
        let bytes = Digest::of(b"hello");
        let held = if matches!(mode, Mode::Copied) {
            Held::Whole("hello".into())
        } else {
            Held::Preview("hel".into())
        };
        let copy = CopyReceipt::new(SOURCE, bytes.clone(), Some(bytes.clone()), held.clone());
        observed.copies = vec![copy];
        let written = FinalState::File {
            digest: bytes,
            held,
        };
        observed.finals = vec![FinalReceipt::new(TARGET, written)];
        observed.ledger = LedgerFacts::clean(vec![TARGET.into()]);
        observed.spent = Spent::new(5, 5);
        Rehearsal::Passed {
            outputs: vec![RehearsedOutput::new(TARGET, "hello")],
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

/// A path the request names right after « in » (« look in ./x », « stay in ./x », « In ./x »),
/// which the reader takes for a destination, that the candidate only reads is material the
/// room copies, never an output it reads back, so the trial can run; the stated path the
/// candidate writes stays the target.
#[tokio::test]
async fn a_stated_path_the_candidate_only_reads_is_an_input_never_a_target() {
    let candidate = r#"nika: copy-source
permits:
  tools: ["nika:read", "nika:write"]
  fs:
    read: ["./in/source.txt"]
    write: ["./out/result.txt"]
tasks:
  load:
    invoke:
      tool: "nika:read"
      args: { path: "./in/source.txt" }
  save:
    with: { text: "${{ tasks.load.output }}" }
    invoke:
      tool: "nika:write"
      args: { path: "./out/result.txt", content: "${{ with.text }}" }
"#;
    for words in [
        "Look in ./in/source.txt and save its text to ./out/result.txt.",
        "For each booking stay in ./in/source.txt, copy its line to ./out/result.txt.",
        "In ./in/source.txt each row is a note: write the notes to ./out/result.txt.",
    ] {
        let host = Host::new(Mode::NotRun);
        let mut state = Rehearsals::new(Some(&host));
        let mut out = crate::initial();
        nika_compile::surface::finish(candidate.into(), &mut out);
        assert!(ready(&out), "{words}: {out:#?}");
        state.inspect(&CompileRequest::create(words), &out).await;
        let inputs = host.inputs.lock().unwrap().clone();
        let targets = host.targets.lock().unwrap().clone();
        assert_eq!(inputs, [vec![SOURCE.to_owned()]], "{words}");
        assert_eq!(targets, [vec![TARGET.to_owned()]], "{words}");
    }
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

/// What a judge may read of this call's runs: only the last run of exactly these bytes,
/// completed and vouched for by the room, bound to their digest, each text with whether it was
/// read whole. No run yet, a run of other bytes, a run the room sends back for repair (an output
/// missing), a report the room stops on (it names other bytes) and a safe refusal before any
/// attempt show nothing.
#[tokio::test]
async fn the_observation_is_the_vouched_run_of_these_exact_bytes() {
    let req = CompileRequest::create("Read ./in/source.txt and write it to ./out/result.txt.");
    let mut out = crate::initial();
    nika_compile::surface::finish(source(true), &mut out);
    assert!(ready(&out), "{out:#?}");
    for (mode, text, whole) in [
        (Mode::Copied, "hello", true),
        (Mode::Prefixed, "hel", false),
    ] {
        let host = Host::new(mode);
        let mut state = Rehearsals::new(Some(&host));
        assert_eq!(state.observed(&source(true)), None, "no run yet");
        assert!(matches!(
            state.inspect(&req, &out).await.result,
            Result::Proceed
        ));
        let observed = json!({
            "candidate_sha256": crate::cognition::knowledge::sha256(&source(true)),
            "inputs": [{"path": SOURCE, "text": text, "read_whole": whole}],
            "outputs": [{"path": TARGET, "text": text, "written": true, "read_whole": whole}],
        });
        assert_eq!(state.observed(&source(true)), Some(observed));
        assert_eq!(
            state.observed(&source(false)),
            None,
            "another candidate's run"
        );
    }
    let greeting = request(0);
    let unvouched = [
        (Mode::Missing, &greeting, "repair"),
        (Mode::WrongDigest, &greeting, "stop"),
        (Mode::NotRun, &req, "proceed"),
    ];
    for (mode, asked, decided) in unvouched {
        let host = Host::new(mode);
        let mut state = Rehearsals::new(Some(&host));
        let decision = match state.inspect(asked, &out).await.result {
            Result::Proceed => "proceed",
            Result::Repair(_) => "repair",
            Result::Stop(_) => "stop",
        };
        assert_eq!(decision, decided);
        assert_eq!(state.observed(&source(true)), None, "{decided}");
        assert_eq!(host.candidates.lock().unwrap().as_slice(), [source(true)]);
    }
}

/// The selected judge of the sketch door's greeting: it answers each question its script names,
/// in order, and keeps every question; a question the script does not expect panics.
struct Judging {
    script: Mutex<VecDeque<(&'static str, &'static str)>>,
    asked: Mutex<Vec<ChoiceQuestion>>,
}

const JUDGE: &str = "mock/typed-judge";

impl Judging {
    fn new<const N: usize>(script: [(&'static str, &'static str); N]) -> Self {
        Self {
            script: Mutex::new(script.into_iter().collect()),
            asked: Mutex::new(Vec::new()),
        }
    }

    fn asked(&self) -> Vec<ChoiceQuestion> {
        self.asked.lock().unwrap().clone()
    }

    /// The scripted answers no question asked for.
    fn left(&self) -> usize {
        self.script.lock().unwrap().len()
    }
}

impl DecisionSeat for Judging {
    fn name(&self) -> &str {
        JUDGE
    }

    fn choose<'a>(&'a self, question: &'a ChoiceQuestion) -> ChoiceFuture<'a> {
        Box::pin(async move {
            self.asked.lock().unwrap().push(question.clone());
            let (id, choice) = (self.script.lock().unwrap().pop_front())
                .unwrap_or_else(|| panic!("an unscripted question: {}", question.id));
            assert_eq!(question.id, id);
            Ok(ChoiceAnswer::new(choice, JUDGE))
        })
    }
}

/// The sketch door's greeting, judged by `judge` (`host`: the room that rehearses it).
async fn judged_greeting(judge: &Judging, host: Option<&Host>) -> CompileOutcome {
    let author = sketched();
    let cognition = crate::Cognition {
        provider: Some(&author),
        seat: Some(judge),
    };
    let host = host.map(|host| host as &dyn Rehearse);
    let out = compile_with_cognition_rehearsed(&sketch_request(0), cognition, host).await;
    assert_eq!(
        author.authored.load(Ordering::SeqCst),
        2,
        "the sketch and its fills"
    );
    out.unwrap()
}

/// A disagreement no part locates is decided by this compile's run of the same bytes (R6): the
/// judge doubts the whole request and carries its one part (the write is the output that part
/// states, so the engine's facts leave no task to ask about), then is shown the run the
/// evidence made of the candidate (nothing read, the greeting written whole) and finds it
/// consistent. The candidate is READY on that judgment, the only one asked over
/// the run, and the barrier reuses the run the judge read.
#[tokio::test]
async fn a_disagreement_is_decided_by_this_compiles_run_of_the_same_bytes() {
    let host = Host::new(Mode::ByDirectories);
    let judge = Judging::new([
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-observed", "consistent"),
    ]);
    let out = judged_greeting(&judge, Some(&host)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.left(), 0);
    let candidate = out.candidate.clone().unwrap();
    let ran = host.candidates.lock().unwrap().clone();
    assert_eq!(
        ran,
        std::slice::from_ref(&candidate),
        "one run, reused by the barrier"
    );
    let asked = judge.asked();
    let ids: Vec<&str> = asked.iter().map(|q| q.id.as_str()).collect();
    let questions = ["verify-request", "verify-part-0", "verify-observed"];
    assert_eq!(ids, questions);
    let observation = json!({
        "candidate_sha256": crate::cognition::knowledge::sha256(&candidate),
        "inputs": [],
        "outputs": [{"path": TARGET, "text": "hello", "written": true, "read_whole": true}],
    });
    assert_eq!(asked[2].state["observation"], observation);
    let over_the_run = ["consistent", "unexercised", "part-0", "none"];
    assert_eq!(asked[2].keys(), over_the_run);
    assert!(
        asked[..2]
            .iter()
            .all(|q| q.state.get("observation").is_none())
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let verified = &decision["semantic_verification"][0];
    assert_eq!(verified["doubt"], json!(["unfaithful"]));
    for list in ["defects", "unknown", "contested", "unsettled"] {
        assert_eq!(verified[list], json!([]), "{list}: {verified:#}");
    }
    let counts = (&verified["attempted"], &verified["consumed"]);
    assert_eq!(counts, (&json!(3), &json!(3)));
    assert_eq!(verified["settled_by"], "verify-observed");
    assert_eq!(verified["engine"][0]["settled"], "only_requested");
    let run = &verified["questions"][2];
    assert_eq!(run["choice"], "consistent");
    // The record keeps what the judge read by digest and size, never the texts.
    let sha256 = crate::cognition::knowledge::sha256;
    let kept = json!({
        "candidate_sha256": sha256(&candidate),
        "sha256": sha256(&observation.to_string()),
        "texts": [{"role": "output", "path": TARGET, "bytes": 5, "sha256": sha256("hello"),
            "read_whole": true, "written": true}],
    });
    assert_eq!(run["observation"], kept);
    let route = decision["route"].as_array().unwrap();
    assert!(
        route
            .iter()
            .any(|step| step == "verify: judged (decision_seat)"),
        "{route:?}"
    );
}

/// What a candidate whose whole request the judge rejected, nothing narrower standing, offers
/// (the `verify_held` finding of an unresolved doubt, as the verifier states it).
const HELD: &str = "The verifier doubted the request as a whole but located nothing: asked alone, none of its parts (1) was found missing, no task was found doing anything the request does not ask, and no run of these bytes decided it. Nothing is verified: the workflow is shown, never proposed, and nothing was written. Review it and describe a correction, or choose another verifier.";

/// Without a host there is no run of these bytes to show: the judge is asked once where its
/// doubt is, names nothing, and the same doubt stays contested, unresolved, nothing READY; no
/// question is asked over a run. The candidate the judge read is held: shown
/// as the preview, never offered, its replayable record dropped so no later round asks the same
/// judge again on these bytes, the finding naming the disagreement and what can decide it.
#[tokio::test]
async fn without_a_run_of_the_same_bytes_the_disagreement_is_held() {
    let judge = Judging::new([
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-doubt", "unlocated"),
    ]);
    let out = judged_greeting(&judge, None).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!((judge.asked().len(), judge.left()), (3, 0));
    let read = judge.asked()[0].state["candidate_nika"].clone();
    assert_eq!(out.candidate.as_deref(), read.as_str(), "{out:#?}");
    assert!(out.check_preview.is_some(), "{out:#?}");
    assert!(out.requested_boundary.is_none() && out.questions.is_empty());
    assert_eq!(
        out.provenance.plan, None,
        "no replay asks the same judge again"
    );
    let decision = out.provenance.decision.as_ref().unwrap();
    let verified = &decision["semantic_verification"][0];
    let unobserved = "no trial run of these exact bytes exists in this compile";
    assert_eq!(verified["contested"], json!([INTENT]));
    assert_eq!(verified["unsettled"], json!([unobserved]));
    assert_eq!(verified["doubt"], json!(["unfaithful"]));
    assert_eq!(
        (&verified["defects"], &verified["unknown"]),
        (&json!([]), &json!([]))
    );
    assert_eq!(verified["unresolved"], true, "{verified:#}");
    let held = (out.diagnostics.iter()).find(|d| d.target == "verify_held");
    let held = held.expect("the held candidate is named");
    assert_eq!(
        (held.kind, held.message.as_str()),
        (DiagnosticKind::Applied, HELD)
    );
    assert!(!(out.diagnostics.iter()).any(|d| d.target == "verify_resume"));
    let disagreement = format!(
        "The judge did not accept the request as carried (unfaithful) and located no defect a repair could start from; the same judge asked again decides nothing ({unobserved}). Nothing is READY on it. Next: a correction of the request, or another verifier."
    );
    let named = (out.diagnostics.iter()).filter(|d| d.target == "semantic_verification");
    let named: Vec<&str> = named.map(|d| d.message.as_str()).collect();
    assert_eq!(named, [disagreement.as_str()]);
    let route = decision["route"].as_array().unwrap();
    let held = "verify: not ready, candidate held";
    assert!(route.iter().any(|step| step == held), "{route:?}");
}

/// A faithful verdict on the bytes is weighed against this compile's run of them (A1): the one
/// part is asked again over what the run wrote, after the verdict and never in it. An output the
/// judge finds contradicting the part is a defect located in the run: nothing is READY, the
/// candidate is withdrawn past its last repair round, and the defect names the task the judge
/// points to there. The same run found carrying the part leaves the faithful verdict READY.
#[tokio::test]
async fn a_faithful_verdict_is_weighed_against_this_compiles_run_of_the_same_bytes() {
    let host = Host::new(Mode::ByDirectories);
    let judge = Judging::new([
        ("verify-request", "faithful"),
        ("verify-observed-part-0", "missing"),
        ("verify-observed-part-0-point", "task-save"),
    ]);
    let out = judged_greeting(&judge, Some(&host)).await;
    assert_eq!(out.status, CompileStatus::Incomplete, "{out:#?}");
    assert_eq!(out.candidate, None);
    assert_eq!(judge.left(), 0);
    let asked = judge.asked();
    assert!(asked[0].state.get("observation").is_none());
    let output = &asked[1].state["observation"]["outputs"][0];
    assert_eq!(
        (&output["path"], &output["text"]),
        (&json!(TARGET), &json!("hello"))
    );
    let verified = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    let part = INTENT.trim_end_matches('.');
    assert_eq!(verified["defects"], json!([part]));
    let note = "in the trial run, the judge points to the task save";
    assert_eq!(verified["notes"], json!([{"defect": part, "note": note}]));
    assert_eq!(
        (&verified["rejected"], &verified["settled"]),
        (&json!(true), &json!(false))
    );

    let host = Host::new(Mode::ByDirectories);
    let judge = Judging::new([
        ("verify-request", "faithful"),
        ("verify-observed-part-0", "carried"),
    ]);
    let out = judged_greeting(&judge, Some(&host)).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.left(), 0);
    let verified = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    assert_eq!(verified["settled_by"], "verify-request");
    assert_eq!(verified["questions"][1]["choice"], "carried");
}

/// An answer round judges the bytes it replays over a run of them (R6, A1): the creation round
/// asks the greeting and runs nothing; once it is answered, the round rehearses the bound bytes
/// before its judge reads them, so a rejection no part locates is decided by that run (here found
/// consistent: READY on `verify-observed`), and the final barrier reuses the same run. Without the
/// run, the same answers hold the candidate (`a_disagreement_without_a_run_*`).
#[tokio::test]
async fn an_answer_round_decides_a_disagreement_over_a_run_of_the_bytes_it_replays() {
    let author = sketched_open();
    let host = Host::new(Mode::ByDirectories);
    let req = open_request();
    let opened = Judging::new([]);
    let cognition = crate::Cognition {
        provider: Some(&author),
        seat: Some(&opened),
    };
    let waiting = compile_with_cognition_rehearsed(&req, cognition, Some(&host)).await;
    let waiting = waiting.unwrap();
    assert!(host.candidates.lock().unwrap().is_empty(), "{waiting:#?}");
    let mut answered = req.with_plan(waiting.provenance.plan.unwrap());
    let greeting = "\"hello\"".to_owned();
    answered.answers.insert("const.greeting".into(), greeting);
    let judge = Judging::new([
        ("verify-request", "unfaithful"),
        ("verify-part-0", "carried"),
        ("verify-observed", "consistent"),
    ]);
    let cognition = crate::Cognition {
        provider: Some(&author),
        seat: Some(&judge),
    };
    let out = compile_with_cognition_rehearsed(&answered, cognition, Some(&host)).await;
    let out = out.unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(judge.left(), 0);
    let asked = judge.asked();
    let output = &asked[2].state["observation"]["outputs"][0];
    assert_eq!(
        (&output["path"], &output["text"]),
        (&json!(TARGET), &json!("hello"))
    );
    assert!(
        asked[..2]
            .iter()
            .all(|q| q.state.get("observation").is_none())
    );
    let ran = host.candidates.lock().unwrap().clone();
    assert_eq!(
        ran,
        [out.candidate.clone().unwrap()],
        "one run, reused by the barrier"
    );
    let verified = &out.provenance.decision.as_ref().unwrap()["semantic_verification"][0];
    assert_eq!(verified["settled_by"], "verify-observed");
}

/// A path the request names after a bare verb (« Save ./out/result.txt … ») that the candidate
/// writes and never reads is its output, never an input the room must find: the host is asked to
/// read it back as a target, and nothing asks it to copy it in.
#[tokio::test]
async fn a_stated_path_the_candidate_only_writes_is_a_target_never_an_input() {
    let host = Host::new(Mode::ByDirectories);
    let request = CompileRequest::create("Save ./out/result.txt with the text hello.")
        .with_authoring_policy(sketch_request(0).authoring.unwrap());
    let out = compiled(&request, &sketched(), &host).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let inputs = host.inputs.lock().unwrap().clone();
    let targets = host.targets.lock().unwrap().clone();
    assert_eq!(inputs, [Vec::<String>::new()], "the output is no input");
    assert_eq!(targets, [vec![TARGET.to_owned()]]);
}
