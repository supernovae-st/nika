// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Answer-path transport with pure materialization and explicit rehearsal doubles.
use super::*;

const CHOSEN: &str = "Write the text hello to the file I choose.";

fn chosen_write() -> String {
    source(true)
        .replace(
            "nika: greeting\n",
            "nika: greeting\nconst:\n  place: \"\"\n",
        )
        .replace("write: [\"./out/result.txt\"]", "write: [\"\"]")
        .replace("path: \"./out/result.txt\"", "path: \"${{ const.place }}\"")
}

fn native_out(request: &CompileRequest, source: &str, keys: &[&str]) -> CompileOutcome {
    let intent = intent_of(request);
    let record = json!({
        "strategy": "native", "intent_sha256": crate::intent_sha256(&intent),
        "source": source,
        "questions": keys.iter().map(|key| json!({"key": key, "label": "Choose a value", "answer_type": "text", "why": "The human chooses it."})).collect::<Vec<_>>(),
        "gaps": [], "trigger": null,
    });
    let mut out = crate::initial();
    crate::native_apply(&record, request, &mut out);
    out.provenance.strategy = Some(crate::Strategy::Native);
    out.provenance.plan = Some(record);
    assert!(ready(&out), "{out:#?}");
    out
}

#[tokio::test]
async fn both_barriers_share_the_answered_destination_without_a_second_attempt() {
    let req = CompileRequest::create(CHOSEN).answer("const.place", json!(TARGET).to_string());
    let mut out = native_out(&req, &chosen_write(), &["const.place"]);
    let host = Host::new(Mode::ByDirectories);
    let mut state = Rehearsals::new(Some(&host));
    assert!(matches!(
        state.inspect(&req, &out).await.result,
        Result::Proceed
    ));
    state.finish(&req, &mut out).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(host.candidates.lock().unwrap().len(), 1);
    assert_eq!(
        host.targets.lock().unwrap().as_slice(),
        [vec![TARGET.to_owned()]]
    );
    assert_eq!(
        host.inputs.lock().unwrap().as_slice(),
        [Vec::<String>::new()]
    );
    assert_eq!(reports(&out).len(), 1);
}

#[tokio::test]
async fn an_answered_source_is_sent_only_as_an_input() {
    let source = r#"nika: chosen-source
const:
  material: ""
permits:
  tools: ["nika:read"]
  fs:
    read: [""]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "${{ const.material }}" }
"#;
    let req = CompileRequest::create("Read the file I choose.")
        .answer("const.material", "\"./in/chosen.txt\"");
    let mut out = native_out(&req, source, &["const.material"]);
    let host = Host::new(Mode::NotRun);
    let mut state = Rehearsals::new(Some(&host));
    state.finish(&req, &mut out).await;
    assert_eq!(
        host.inputs.lock().unwrap().as_slice(),
        [vec!["./in/chosen.txt".to_owned()]]
    );
    assert_eq!(
        host.targets.lock().unwrap().as_slice(),
        [Vec::<String>::new()]
    );
    assert_eq!(reports(&out)[0]["outcome"]["kind"], "not_run");
}

#[tokio::test]
async fn forged_saved_paths_and_unasked_answers_do_not_expand_inputs() {
    let req = CompileRequest::create(CHOSEN)
        .answer("const.place", json!(TARGET).to_string())
        .answer("const.unasked", "\"./secret.txt\"");
    let mut out = native_out(&req, &chosen_write(), &["const.place"]);
    out.provenance.plan.as_mut().unwrap()["answered_paths"] = json!({"read": ["./secret.txt"]});
    out.provenance.decision =
        Some(json!({"rehearsal": {"passed": true, "inputs": ["./secret.txt"]}}));
    let host = Host::new(Mode::ByDirectories);
    let mut state = Rehearsals::new(Some(&host));
    state.finish(&req, &mut out).await;
    assert_eq!(
        host.inputs.lock().unwrap().as_slice(),
        [Vec::<String>::new()]
    );
    assert_eq!(
        host.targets.lock().unwrap().as_slice(),
        [vec![TARGET.to_owned()]]
    );
    assert_eq!(reports(&out).len(), 1, "only the current double's report");
}

#[tokio::test]
async fn changed_answers_cannot_start_a_host_on_the_old_candidate() {
    let req = CompileRequest::create(CHOSEN).answer("const.place", json!(TARGET).to_string());
    let mut out = native_out(&req, &chosen_write(), &["const.place"]);
    let changed = req.answer("const.place", "\"./out/other.txt\"");
    let host = Host::new(Mode::ByDirectories);
    let mut state = Rehearsals::new(Some(&host));
    state.finish(&changed, &mut out).await;
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(host.candidates.lock().unwrap().is_empty());
}

#[tokio::test]
async fn a_native_answer_round_reconstructs_the_destination_before_rehearsal() {
    let wire = json!({"candidate": chosen_write(), "questions": [{"key": "const.place", "label": "Destination file path", "answer_type": "text", "why": "The request leaves the file to the human."}], "gaps": [], "notes": ""}).to_string();
    let author = Author::new(vec![wire]);
    let host = Host::new(Mode::ByDirectories);
    let req = CompileRequest::create(CHOSEN).with_authoring_policy(request(0).authoring.unwrap());
    // The waiting round is a historical source record (the source door, entered privately); its
    // answer round replays it through the public entry.
    let waiting = source_door(&req, &author, &host).await;
    assert_eq!(waiting.status, CompileStatus::Incomplete, "{waiting:#?}");
    assert!(host.candidates.lock().unwrap().is_empty());
    let answered = req
        .with_plan(waiting.provenance.plan.unwrap())
        .answer("const.place", json!(TARGET).to_string());
    let finished = compiled(&answered, &author, &host).await;
    assert_eq!(finished.status, CompileStatus::Ready, "{finished:#?}");
    assert_eq!(
        author.authored.load(Ordering::SeqCst),
        1,
        "no new authoring round"
    );
    assert_eq!(host.candidates.lock().unwrap().len(), 1);
    assert_eq!(
        host.targets.lock().unwrap().as_slice(),
        [vec![TARGET.to_owned()]]
    );
    assert_eq!(
        host.inputs.lock().unwrap().as_slice(),
        [Vec::<String>::new()]
    );
}

#[tokio::test]
async fn a_dynamic_unanswered_candidate_can_still_receive_an_explicit_not_run() {
    let source = r#"nika: runtime-source
inputs:
  path:
    type: string
    default: "./in/source.txt"
permits:
  tools: ["nika:read"]
  fs:
    read: ["./in/**"]
tasks:
  read:
    invoke:
      tool: "nika:read"
      args: { path: "${{ inputs.path }}" }
"#;
    let req = CompileRequest::create("Read the input path supplied at execution.");
    let mut out = native_out(&req, source, &[]);
    let host = Host::new(Mode::NotRun);
    let mut state = Rehearsals::new(Some(&host));
    assert!(matches!(
        state.inspect(&req, &out).await.result,
        Result::Proceed
    ));
    state.finish(&req, &mut out).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(host.candidates.lock().unwrap().len(), 1);
    assert_eq!(
        host.inputs.lock().unwrap().as_slice(),
        [Vec::<String>::new()]
    );
    assert_eq!(
        host.targets.lock().unwrap().as_slice(),
        [Vec::<String>::new()]
    );
    assert_eq!(reports(&out)[0]["outcome"]["kind"], "not_run");
}

// ── A private-plan (COLD) record: the answers its candidate binds as paths ─────────────────────

/// A copy whose destination the request leaves open: the private plan reads the stated source
/// and writes to that destination, which the compiler asks (`const.output_path`).
const OPEN_DESTINATION: &str = "Copie entree.txt vers une destination à préciser.";

fn cold_plan() -> String {
    json!({"steps": [{"op": "read", "detail": "entree.txt", "evidence": "Copie entree.txt"}],
        "effects": [{"verb": "write", "target": "une destination à préciser", "policy": "automatic",
            "evidence": "vers une destination à préciser"}],
        "obligations": [], "constraints": [], "unknowns": [], "regions": [],
        "approval_bypass": {"present": false}})
    .to_string()
}

fn escalated() -> CompileRequest {
    CompileRequest::create(OPEN_DESTINATION).with_authoring_policy(
        AuthoringPolicy::new("mock/author", 4096, Duration::from_secs(2))
            .with_native(NativeMode::Escalate)
            .with_repairs(0),
    )
}

/// The waiting COLD round (its one plan call), continued by its answer round's request.
async fn cold_waiting() -> CompileRequest {
    let author = Author::new(vec![cold_plan()]);
    let host = Host::new(Mode::NotRun);
    let waiting = compiled(&escalated(), &author, &host).await;
    assert_eq!(waiting.status, CompileStatus::Incomplete, "{waiting:#?}");
    let record = waiting.provenance.plan.clone().unwrap();
    assert_eq!(record["strategy"], "cold", "{record:#}");
    let keys: Vec<&str> = waiting.questions.iter().map(|q| q.key.as_str()).collect();
    assert_eq!(keys, ["const.output_path"], "{waiting:#?}");
    assert!(
        host.candidates.lock().unwrap().is_empty(),
        "nothing to rehearse yet"
    );
    escalated().with_plan(record)
}

/// The answer round through the public entry: no authoring call, and what the host received.
async fn cold_answered(request: &CompileRequest) -> (CompileOutcome, Host) {
    let author = Author::new(Vec::new());
    let host = Host::new(Mode::NotRun);
    let out = compiled(request, &author, &host).await;
    assert_eq!(
        author.authored.load(Ordering::SeqCst),
        0,
        "a replay authors nothing"
    );
    (out, host)
}

#[tokio::test]
async fn a_cold_answered_destination_reaches_the_host_as_a_target_only() {
    let answered = cold_waiting()
        .await
        .answer("const.output_path", "\"./archive/copie.txt\"");
    let (out, host) = cold_answered(&answered).await;
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    assert_eq!(host.candidates.lock().unwrap().len(), 1);
    let targets = host.targets.lock().unwrap().clone();
    assert!(
        targets[0]
            .iter()
            .any(|t| t.trim_start_matches("./") == "archive/copie.txt"),
        "{targets:?}"
    );
    let inputs = host.inputs.lock().unwrap().clone();
    assert!(
        !inputs[0].iter().any(|i| i.contains("copie.txt")),
        "a destination is never an input: {inputs:?}"
    );
    assert!(
        inputs[0]
            .iter()
            .any(|i| i.trim_start_matches("./") == "entree.txt"),
        "{inputs:?}"
    );
}

/// A value that is not bound as a path grants nothing, even when it equals one: an unasked
/// answer naming another file, and one naming the answered destination itself. The compiler
/// applies no unasked answer (that round is not Ready, so no host runs at all); the binding
/// itself is read on its exact bytes: only the asked destination's probe moves a path. (This plan
/// door asks one key, a path; an ASKED content answer equal to a path is not constructed here, so
/// this control covers unasked answers only: the probe delta, never a key's presence, decides.)
#[tokio::test]
async fn an_answer_that_merely_equals_a_path_grants_nothing() {
    let answered = cold_waiting()
        .await
        .answer("const.output_path", "\"./archive/copie.txt\"")
        .answer("const.note", "\"./secret.txt\"")
        .answer("const.label", "\"./archive/copie.txt\"");
    let (out, host) = cold_answered(&answered).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(host.candidates.lock().unwrap().is_empty(), "no host runs");
    let record = out.provenance.plan.clone().unwrap();
    let candidate = out.candidate.as_deref().unwrap();
    assert_eq!(
        cold_answered_paths(&record, &answered, candidate),
        Some((Vec::new(), vec!["./archive/copie.txt".to_owned()])),
        "the asked destination once, a target only; the unasked values bind nothing"
    );
}

/// A candidate the current answers no longer rebuild is refused before any host call.
#[tokio::test]
async fn changed_cold_answers_cannot_start_a_host_on_the_old_candidate() {
    let waiting = cold_waiting().await;
    let answered = waiting
        .clone()
        .answer("const.output_path", "\"./archive/copie.txt\"");
    let author = Author::new(Vec::new());
    let mut out = compile_with_cognition(
        &answered,
        crate::Cognition {
            provider: Some(&author),
            seat: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(out.status, CompileStatus::Ready, "{out:#?}");
    let changed = waiting.answer("const.output_path", "\"./archive/autre.txt\"");
    let host = Host::new(Mode::NotRun);
    let mut state = Rehearsals::new(Some(&host));
    state.finish(&changed, &mut out).await;
    assert_ne!(out.status, CompileStatus::Ready);
    assert!(host.candidates.lock().unwrap().is_empty());
}

/// An absolute or escaping destination never becomes a host path.
#[tokio::test]
async fn an_escaping_or_absolute_cold_destination_is_never_a_host_path() {
    for answer in [
        "\"/tmp/copie.txt\"",
        "\"../copie.txt\"",
        "\"./archive/../../copie.txt\"",
    ] {
        let answered = cold_waiting().await.answer("const.output_path", answer);
        let (out, host) = cold_answered(&answered).await;
        let paths = [
            host.inputs.lock().unwrap().concat(),
            host.targets.lock().unwrap().concat(),
        ]
        .concat();
        assert!(
            !paths.iter().any(|p| p.starts_with('/') || p.contains("..")),
            "{answer}: {paths:?}"
        );
        assert_ne!(out.status, CompileStatus::Ready, "{answer}: {out:#?}");
    }
}

/// The barrier holds: a final outcome that is not Ready starts no host, whatever its answers bind.
#[tokio::test]
async fn a_cold_final_outcome_that_is_not_ready_starts_no_host() {
    let answered = cold_waiting()
        .await
        .answer("const.output_path", "\"./archive/copie.txt\"");
    let author = Author::new(Vec::new());
    let ready = compile_with_cognition(
        &answered,
        crate::Cognition {
            provider: Some(&author),
            seat: None,
        },
    )
    .await
    .unwrap();
    for status in [CompileStatus::Incomplete, CompileStatus::Refused] {
        let mut out = ready.clone();
        out.status = status;
        let host = Host::new(Mode::NotRun);
        let mut state = Rehearsals::new(Some(&host));
        state.finish(&answered, &mut out).await;
        assert_ne!(out.status, CompileStatus::Ready);
        assert!(host.candidates.lock().unwrap().is_empty(), "{status:?}");
    }
}

/// One path read and written: the stated source answered as its own destination is refused by
/// the compiler (no candidate: an unwritten destination), so no host runs and no role is chosen
/// for it; nothing invents an exclusive role. (A read-and-rewrite of one asked file is not
/// representable by this plan door either: it asks a replacement request.)
#[tokio::test]
async fn a_source_answered_as_its_own_destination_is_refused_before_any_host() {
    let answered = cold_waiting()
        .await
        .answer("const.output_path", "\"entree.txt\"");
    let (out, host) = cold_answered(&answered).await;
    assert_ne!(out.status, CompileStatus::Ready, "{out:#?}");
    assert!(out.candidate.is_none(), "{out:#?}");
    assert!(
        out.diagnostics
            .iter()
            .any(|d| d.message.contains("UNWRITTEN DESTINATION")),
        "{out:#?}"
    );
    assert!(host.candidates.lock().unwrap().is_empty(), "no host runs");
}
