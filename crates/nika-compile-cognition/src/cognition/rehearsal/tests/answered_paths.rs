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
    let waiting = compiled(&req, &author, &host).await;
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
