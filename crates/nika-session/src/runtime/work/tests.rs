// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The one precedence, the one routing and the snapshot, over the real compiler and the real
//! check: every host reads the same waiting state with its identity, a line answers only what
//! was shown, and the work names the candidate, the saved workflow and the last run exactly.

use std::path::{Path, PathBuf};

use nika_onboard::compile::{CompileRequest, compile};

use super::{GATE_NOT_SHOWN, NOTHING_SHOWN, VALUE_NOT_SHOWN};
use crate::authoring::AuthoringRound;
use crate::intelligence::{DataLocus, IntelligenceKind, ResolvedSessionIntelligence};
use crate::outcome::{GateId, ProposalId, RefusalClass};
use crate::reasoner::NoReasoner;
use crate::runtime::tests::{COPY, COPY_DEST, ready_with, tree};
use crate::runtime::{SessionRuntime, TurnOutcome};
use crate::work::{CONTRACT, Landing, RunEnd, Stage, Waiting};
use crate::world::Reach;

/// A check-clean workflow that pauses at a human gate (the runtime suite's own shape).
const GATE: &str = "nika: gate\npermits: { fs: { read: [\"./draft.md\"], write: [\"./final.md\"] }, tools: [\"nika:read\", \"nika:prompt\", \"nika:write\"] }\ntasks:\n  read_draft:\n    invoke: { tool: \"nika:read\", args: { path: \"./draft.md\" } }\n  approve:\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Write final.md?\" } }\n  write_final:\n    after: { approve: success }\n    with: { go: \"${{ tasks.approve.output }}\", text: \"${{ tasks.read_draft.output }}\" }\n    when: \"${{ with.go == true }}\"\n    invoke: { tool: \"nika:write\", args: { path: \"./final.md\", content: \"${{ with.text }}\" } }\n";
/// The event a run paused at that gate leaves in its trace.
const PAUSED: &str = "{\"kind\":\"workflow_paused\",\"fields\":[{\"key\":\"task\",\"value\":\"approve\"},{\"key\":\"mode\",\"value\":\"confirm\"},{\"key\":\"message\",\"value\":\"Write final.md?\"}]}\n";

fn proposal(outcome: TurnOutcome) -> ProposalId {
    let TurnOutcome::Proposal { id, .. } = outcome else {
        panic!("a proposal: {outcome:?}");
    };
    id
}

#[test]
fn a_fresh_session_waits_for_nothing_and_holds_no_work() {
    let dir = tree();
    let s = ready_with(dir.path(), vec![]);
    assert_eq!(s.waiting(), Waiting::Free);
    let work = s.work();
    assert_eq!(work.contract, CONTRACT);
    assert_eq!(work.root, s.snapshot.root);
    assert_eq!(work.waiting, Waiting::Free);
    assert!(work.candidate.is_none() && work.saved.is_none() && work.run.is_none());
    assert_eq!(work.rail.draft, Stage::Pending);
}

#[test]
fn a_consent_answers_the_proposal_shown_and_nothing_unseen_is_saved() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let id = proposal(s.turn(COPY));
    assert_eq!(
        s.waiting(),
        Waiting::Consent {
            proposal: id.clone()
        }
    );

    let work = s.work();
    let candidate = work.candidate.expect("the candidate under review");
    assert_eq!(candidate.proposal, id);
    assert!(!candidate.aside && !candidate.run_after_save);
    let [file] = candidate.files.as_slice() else {
        panic!("one workflow: {:?}", candidate.files);
    };
    assert_eq!(file.path, PathBuf::from(COPY_DEST));
    assert_eq!(file.landing, Landing::Create);
    let audit = file.audit.as_ref().expect("audited");
    assert!(audit.clean, "{:?}", audit.findings);
    assert_eq!(
        audit.world.reach,
        Reach::Local,
        "a copy between project files reaches no service: {:?}",
        audit.world
    );
    assert_eq!(work.rail.draft, Stage::Done);

    let TurnOutcome::Refusal(unseen) = s.submit("yes", &Waiting::Free) else {
        panic!("nothing shown, nothing consented");
    };
    assert_eq!(unseen.class, RefusalClass::WrongState);
    assert_eq!(unseen.text, NOTHING_SHOWN);
    assert!(!dir.path().join(COPY_DEST).exists());
    assert_eq!(
        s.waiting(),
        Waiting::Consent {
            proposal: id.clone()
        }
    );

    let other = Waiting::Consent {
        proposal: ProposalId::of("another preview"),
    };
    let TurnOutcome::Refusal(stale) = s.submit("yes", &other) else {
        panic!("stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert!(!dir.path().join(COPY_DEST).exists());

    let shown = s.waiting();
    assert!(
        matches!(s.submit("yes", &shown), TurnOutcome::Facts(ref t) if t.contains("applied")),
        "the consent shown lands the set"
    );
    assert!(dir.path().join(COPY_DEST).is_file());
    assert_eq!(s.waiting(), Waiting::Free);
    let work = s.work();
    let saved = work.saved.expect("saved");
    assert_eq!(saved.workflow, PathBuf::from(COPY_DEST));
    assert_eq!(saved.check_clean, Some(true));
    assert_eq!(
        (work.rail.saved, work.rail.checked),
        (Stage::Done, Stage::Done)
    );
    assert!(
        matches!(s.submit("yes", &shown), TurnOutcome::Refusal(_)),
        "a repeated line never applies twice"
    );
}

#[test]
fn a_declining_line_still_declines_when_no_proposal_was_shown() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    proposal(s.turn(COPY));
    assert!(
        matches!(s.submit("no", &Waiting::Free), TurnOutcome::Facts(ref t) if t.contains("discarded")),
        "leaving or declining applies nothing, shown or not"
    );
    assert_eq!(s.waiting(), Waiting::Free);
    assert!(!dir.path().join(COPY_DEST).exists());
}

#[test]
fn a_gate_answer_names_the_gate_shown_and_the_run_keeps_its_identity() {
    let dir = tree();
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(
        s.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(&trace, PAUSED).expect("trace");
    let TurnOutcome::GateAsk { id, .. } = s.observe_run(4, Some(&trace)) else {
        panic!("the gate is asked");
    };
    assert_eq!(s.waiting(), Waiting::Gate { gate: id.clone() });
    let run = s.work().run.expect("the paused run");
    assert!(run.current);
    assert_eq!(run.end, Some(RunEnd::Paused));
    assert_eq!(
        run.trace.as_deref(),
        Some(trace.display().to_string().as_str())
    );
    assert_eq!(s.work().rail.run, Stage::Paused);

    let elsewhere = Waiting::Gate {
        gate: GateId::new(Path::new("other.ndjson"), "approve"),
    };
    let TurnOutcome::Refusal(stale) = s.submit("yes", &elsewhere) else {
        panic!("another gate is stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    let shown = s.waiting();
    let TurnOutcome::ResumeRequested { answer, .. } = s.submit("yes", &shown) else {
        panic!("the shown gate resumes");
    };
    assert_eq!(answer, "approve=true");
    assert_eq!(s.waiting(), Waiting::Free);
}

#[test]
fn a_run_before_the_last_save_is_kept_but_never_the_current_result() {
    let dir = tree();
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(
        s.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    let _ = s.observe_run(1, None);
    let run = s.work().run.expect("observed");
    assert!(run.current);
    assert_eq!(run.end, Some(RunEnd::Failed));

    let id = proposal(s.turn(COPY));
    assert!(matches!(
        s.submit("yes", &Waiting::Consent { proposal: id }),
        TurnOutcome::Facts(_)
    ));
    let run = s.work().run.expect("still kept as evidence");
    assert!(
        !run.current,
        "a new Save makes the earlier run evidence, not this revision's result"
    );
    assert_eq!(run.end, Some(RunEnd::Failed));
    assert_eq!(s.work().rail.run, Stage::Pending);
}

#[test]
fn the_snapshot_serializes_with_its_contract_and_waiting_kind() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let id = proposal(s.turn(COPY));
    let json = serde_json::to_value(s.work()).expect("serializes");
    assert_eq!(json["contract"], CONTRACT);
    assert_eq!(json["waiting"]["kind"], "consent");
    assert_eq!(json["waiting"]["proposal"], id.as_str());
    assert_eq!(json["candidate"]["files"][0]["landing"], "create");
    assert_eq!(
        json["candidate"]["files"][0]["audit"]["world"]["reach"],
        "local"
    );
}

/// A session with no intelligence chosen, so a value is bound as typed and nothing is sent.
fn literal(root: &Path) -> SessionRuntime {
    let none = ResolvedSessionIntelligence {
        kind: IntelligenceKind::None,
        model: None,
        locus: DataLocus::None,
        ready: false,
        why: None,
    };
    SessionRuntime::open(root, none, Box::new(NoReasoner))
}

/// An exact skeleton's first authoring question, waiting in this session.
fn at_a_question(s: &mut SessionRuntime) -> String {
    let out = compile(&CompileRequest::create("aggregate-by-key")).expect("compiles");
    let mut round = AuthoringRound::new("aggregate-by-key");
    round.absorb(&out);
    let key = round.current().map(|q| q.key.clone()).expect("a question");
    s.authoring = Some(round);
    s.questions.ask();
    key
}

/// Whether the session bound `line` as an answer.
fn bound(s: &SessionRuntime, line: &str) -> bool {
    s.recent
        .iter()
        .any(|(said, noted)| said == line && noted.starts_with("(answered "))
}

#[test]
fn a_gate_that_appears_after_the_line_was_typed_takes_no_answer_from_it() {
    let dir = tree();
    std::fs::write(dir.path().join("draft.md"), "the draft\n").expect("draft");
    std::fs::write(dir.path().join("gate.nika"), GATE).expect("gate");
    let mut s = ready_with(dir.path(), vec![]);
    assert!(matches!(
        s.turn("run gate.nika"),
        TurnOutcome::RunRequested { .. }
    ));
    // The host showed a free prompt; the run then paused at its gate before the line arrived.
    let shown = s.waiting();
    assert_eq!(shown, Waiting::Free);
    let store = dir.path().join(".nika").join("traces");
    std::fs::create_dir_all(&store).expect("store");
    let trace = store.join("paused.ndjson");
    std::fs::write(&trace, PAUSED).expect("trace");
    let TurnOutcome::GateAsk { id, .. } = s.observe_run(4, Some(&trace)) else {
        panic!("the gate is asked");
    };

    // Before the fix this `yes` resumed the run (ResumeRequested): an answer to a gate never seen.
    let TurnOutcome::Refusal(unseen) = s.submit("yes", &shown) else {
        panic!("a gate the host did not show takes no answer");
    };
    assert_eq!(unseen.class, RefusalClass::StaleRevision, "{unseen}");
    assert_eq!(unseen.text, GATE_NOT_SHOWN);
    assert_eq!(s.waiting(), Waiting::Gate { gate: id.clone() });
    // Read-only and leaving lines still go through; the gate keeps waiting.
    assert!(
        matches!(s.submit("/status", &shown), TurnOutcome::Facts(_)),
        "a local command reads the session's facts beside the gate"
    );
    assert_eq!(s.waiting(), Waiting::Gate { gate: id.clone() });

    let shown = s.waiting();
    let TurnOutcome::ResumeRequested { answer, .. } = s.submit("yes", &shown) else {
        panic!("the gate shown takes its answer");
    };
    assert_eq!(answer, "approve=true");
}

#[test]
fn a_question_takes_only_the_answer_typed_at_its_own_identity() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = literal(dir.path());
    let key = at_a_question(&mut s);
    let shown = s.waiting();
    let Waiting::Question { key: asked, id } = &shown else {
        panic!("a question waits: {shown:?}");
    };
    assert_eq!(asked, &key);
    assert_eq!(s.pending_question_id().as_ref(), Some(id));

    // A line typed at a free prompt fills nothing the host did not show.
    let TurnOutcome::Refusal(unseen) = s.submit("\"EUR\"", &Waiting::Free) else {
        panic!("an unshown question takes no value");
    };
    assert_eq!(unseen.class, RefusalClass::StaleRevision, "{unseen}");
    assert_eq!(unseen.text, VALUE_NOT_SHOWN);
    assert!(!bound(&s, "\"EUR\""));
    assert_eq!(
        s.waiting(),
        shown,
        "the question keeps waiting, same identity"
    );
    assert!(matches!(
        s.submit("/status", &Waiting::Free),
        TurnOutcome::Facts(_)
    ));

    // The same question asked again by a reopened session is another identity: the answer
    // typed before the reopening is refused there, before anything reads it.
    let mut reopened = literal(dir.path());
    assert_eq!(at_a_question(&mut reopened), key);
    let TurnOutcome::Refusal(stale) = reopened.submit("\"EUR\"", &shown) else {
        panic!("an answer typed for another session's question is stale");
    };
    assert_eq!(stale.class, RefusalClass::StaleRevision, "{stale}");
    assert!(!bound(&reopened, "\"EUR\""));
    assert!(
        reopened.pending_question().is_some(),
        "the new question still waits"
    );

    // The question shown takes the line.
    let _ = s.submit("\"EUR\"", &shown);
    assert!(bound(&s, "\"EUR\""), "the shown question binds its answer");
    assert_ne!(s.waiting(), shown, "an answered question waits no more");
}

#[test]
fn the_question_identity_rides_the_wire_as_its_witness() {
    let dir = tempfile::tempdir().expect("root");
    let mut s = literal(dir.path());
    let key = at_a_question(&mut s);
    let id = s.pending_question_id().expect("waits");
    let json = serde_json::to_value(s.work()).expect("serializes");
    assert_eq!(json["waiting"]["kind"], "question");
    assert_eq!(json["waiting"]["key"], key);
    assert_eq!(json["waiting"]["id"], id.as_str());
}

/// A check-clean workflow that needs one declared input and posts to an exact loopback host:
/// the contract-server level of a journey, a service on this machine and never the real one.
const LOCAL_SINK: &str = "nika: stock-sink\ninputs:\n  base:\n    type: string\n    required: true\npermits: { tools: [\"nika:fetch\"], net: { http: [\"127.0.0.1\"] } }\ntasks:\n  notify:\n    invoke:\n      tool: \"nika:fetch\"\n      args:\n        url: \"http://127.0.0.1:8787/notifications/stock\"\n        method: POST\n        headers: { idempotency-key: \"stock-${{ inputs.base }}\" }\n        body: { channel: \"stock\" }\n";

#[test]
fn a_requested_run_keeps_the_reach_of_its_bytes_and_only_the_names_of_its_inputs() {
    let dir = tree();
    std::fs::write(dir.path().join("sink.nika"), LOCAL_SINK).expect("workflow");
    let mut s = ready_with(dir.path(), vec![]);
    assert!(s.work().requested.is_none(), "nothing was requested yet");
    let TurnOutcome::Question { key, .. } = s.turn("run sink.nika") else {
        panic!("the declared input is asked first");
    };
    assert_eq!(key, "input.base");
    assert!(
        s.work().requested.is_none(),
        "a run that waits on its input is not requested yet"
    );
    let shown = s.waiting();
    let TurnOutcome::RunRequested { run, .. } = s.submit("http://127.0.0.1:8787", &shown) else {
        panic!("the answer binds the input and requests the run");
    };
    assert_eq!(run.vars, ["base=http://127.0.0.1:8787"]);
    let work = s.work();
    let requested = work.requested.expect("the run just requested");
    assert_eq!(requested.workflow, PathBuf::from("sink.nika"));
    assert_eq!(requested.inputs, ["base"]);
    assert_eq!(
        requested.world.reach,
        Reach::LocalServices,
        "an exact loopback host is a local service: {:?}",
        requested.world
    );
    let json = serde_json::to_string(&s.work()).expect("serializes");
    assert!(json.contains("\"inputs\":[\"base\"]"), "{json}");
    assert!(
        !json.contains("http://127.0.0.1:8787\"]"),
        "an input value never rides the snapshot: {json}"
    );
}

#[test]
fn the_saved_reach_belongs_to_the_bytes_a_consent_saved_never_to_a_workflow_only_run() {
    let dir = tree();
    std::fs::write(dir.path().join("sink.nika"), LOCAL_SINK).expect("workflow");
    let mut s = ready_with(dir.path(), vec![]);
    let id = proposal(s.turn(COPY));
    assert!(matches!(
        s.submit("yes", &Waiting::Consent { proposal: id }),
        TurnOutcome::Facts(_)
    ));
    let saved = s.work().saved.expect("saved by the consent");
    assert_eq!(saved.workflow, PathBuf::from(COPY_DEST));
    assert_eq!(
        saved.world.map(|w| w.reach),
        Some(Reach::Local),
        "a copy between project files reaches no service"
    );
    assert!(s.work().requested.is_none(), "saving is not a run request");

    assert!(matches!(
        s.turn("run sink.nika"),
        TurnOutcome::Question { .. }
    ));
    let shown = s.waiting();
    assert!(matches!(
        s.submit("http://127.0.0.1:8787", &shown),
        TurnOutcome::RunRequested { .. }
    ));
    let work = s.work();
    let saved = work.saved.expect("the workflow named last");
    assert_eq!(saved.workflow, PathBuf::from("sink.nika"));
    assert_eq!(
        saved.world, None,
        "a workflow only run carries no saved reach, never the earlier consent's"
    );
    assert_eq!(
        work.requested.map(|r| r.world.reach),
        Some(Reach::LocalServices)
    );
}

/// The work snapshot names how the proposing compile revised the candidate's document only
/// while the record binds the candidate's exact bytes; any other bytes are never described by it.
#[test]
fn a_document_revision_describes_only_the_bytes_its_record_binds() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let _ = proposal(s.turn(COPY));
    let bytes = s
        .candidate()
        .and_then(|c| {
            c.set
                .changes
                .iter()
                .find(|c| c.is_workflow())
                .map(|c| c.content().to_owned())
        })
        .expect("the pending workflow");
    let record = |candidate: &str| {
        serde_json::json!({"mode": "operations", "base_sha256": "base",
            "candidate_sha256": candidate, "changed": ["tasks.copy.invoke.args.path"],
            "preservation": "by construction", "components": []})
    };
    s.proposed_revision = Some(record(&nika_compile::surface::sha256(&bytes)));
    let revision = (s.work().candidate)
        .and_then(|c| c.revision)
        .expect("the record binds these bytes");
    assert_eq!(revision.mode, "operations");
    assert_eq!(revision.changed, ["tasks.copy.invoke.args.path"]);
    s.proposed_revision = Some(record("another candidate's digest"));
    assert!(
        s.work().candidate.and_then(|c| c.revision).is_none(),
        "a record of other bytes describes nothing here"
    );
}

/// The rail's « Saved » is the consent's fact: a workflow only named for a run is not saved here
/// and shows no check, while naming the saved one again keeps that consent's facts.
#[test]
fn a_workflow_only_run_is_never_shown_saved_on_the_rail() {
    let dir = tree();
    std::fs::write(dir.path().join("sink.nika"), LOCAL_SINK).expect("workflow");
    let mut s = ready_with(dir.path(), vec![]);
    let id = proposal(s.turn(COPY));
    assert!(matches!(
        s.submit("yes", &Waiting::Consent { proposal: id }),
        TurnOutcome::Facts(_)
    ));
    let rail = s.lifecycle().rail();
    assert!(rail.starts_with("Draft ✓ · Saved ✓ · Checked ✓"), "{rail}");

    assert!(matches!(
        s.turn("run sink.nika"),
        TurnOutcome::Question { .. }
    ));
    let shown = s.waiting();
    assert!(matches!(
        s.submit("http://127.0.0.1:8787", &shown),
        TurnOutcome::RunRequested { .. }
    ));
    assert_eq!(
        s.lifecycle().rail(),
        "Draft ○ · Saved ○ · Checked ○ · Active ○ · Run ○",
        "only named for a run: neither saved here nor checked at a consent"
    );

    assert!(matches!(
        s.turn(&format!("run {COPY_DEST}")),
        TurnOutcome::RunRequested { .. }
    ));
    let rail = s.lifecycle().rail();
    assert!(rail.starts_with("Draft ✓ · Saved ✓ · Checked ✓"), "{rail}");
}
