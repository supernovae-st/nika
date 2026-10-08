// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `save & run` (NIK-14): one closed consent word saves the proposal shown and runs what it saves
//! once, through the run path an explicit « run it » takes. `yes` stays Save only. A stale or moved
//! proposal, a failed save, a failed check, a classifier's reading, a replay and a reopened
//! conversation never run.

use std::path::{Path, PathBuf};

use super::tests::{COPY, COPY_DEST, ready_with, tree};
use super::*;
use crate::change::{ProjectChange, ProjectChangeSet, RunRequest, Witness};
use crate::consent::ConsentRecord;
use crate::turn::{RoutingMethod, TurnAct, TurnClassifier, TurnContext, TurnDecision};
use crate::work::Waiting;

/// A workflow whose one required input the run asks before it is requested.
const NEEDS_INPUT: &str = "nika: greet\ninputs:\n  name: { type: string, required: true }\npermits:\n  fs: { write: [\"./hello.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  write:\n    invoke: { tool: \"nika:write\", args: { path: \"./hello.txt\", content: \"hi ${{ inputs.name }}\" } }\n";

/// A workflow its check refuses: an exec it grants nothing for.
const UNGRANTED: &str =
    "nika: ungranted\ntasks:\n  t:\n    exec: { command: [\"curl\", \"https://example.com\"] }\n";

/// A session at the proposal COPY makes, under a project ceiling of 0.10: the proposal, its
/// exact bytes and the waiting state a host paints for it.
fn at_the_proposal(dir: &Path) -> (SessionRuntime, ProposalId, String, Waiting) {
    at_the_proposal_kept(dir, None)
}

/// [`at_the_proposal`] with the conversation's history kept under `home`, opened first.
fn at_the_proposal_kept(
    dir: &Path,
    home: Option<&Path>,
) -> (SessionRuntime, ProposalId, String, Waiting) {
    let mut s = ready_with(dir, vec![]);
    if let Some(home) = home {
        s.enable_history(home).expect("history");
    }
    s.snapshot.ceiling = Some(0.10);
    let TurnOutcome::Proposal { id, .. } = s.turn(COPY) else {
        panic!("COPY is proposed");
    };
    let bytes = (s.pending.as_ref())
        .and_then(|set| set.changes.first())
        .map(|change| change.content().to_owned())
        .expect("the candidate's bytes");
    let shown = s.waiting();
    assert_eq!(
        shown,
        Waiting::Consent {
            proposal: id.clone()
        }
    );
    (s, id, bytes, shown)
}

/// `set` in place of the proposal waiting, its money bound as a reviewed proposal's is.
fn proposed(s: &mut SessionRuntime, set: ProjectChangeSet) -> Waiting {
    let id = s.proposal_id(&set);
    s.pending = Some(set);
    s.bind_proposal_money(&id);
    s.waiting()
}

fn read(dir: &Path, path: &str) -> String {
    std::fs::read_to_string(dir.join(path)).expect("a saved file")
}

fn last_consent_run(dir: &Path) -> Option<PathBuf> {
    let records = ConsentRecord::read_all(dir).expect("the consent journal");
    records.last().and_then(|record| record.run.clone())
}

/// The proposal shown, saved by `save & run`: its exact bytes land, and the run of the one
/// workflow it saves is requested once for those bytes and their world under the reviewed
/// proposal's money. The same answer again saves and runs nothing.
#[test]
fn save_and_run_lands_the_exact_bytes_and_requests_one_run() {
    let dir = tree();
    let (mut s, id, bytes, shown) = at_the_proposal(dir.path());
    let TurnOutcome::RunRequested { report, run } = s.submit("save & run", &shown) else {
        panic!("save & run requests the run once its save checked clean");
    };
    assert_eq!(read(dir.path(), COPY_DEST), bytes, "the exact bytes shown");
    assert!(
        report.contains("applied · wrote") && report.contains("run once · check"),
        "{report}"
    );
    assert_eq!(run.workflow, PathBuf::from(COPY_DEST));
    assert!(run.vars.is_empty());
    assert!(
        (run.max_cost_usd - 0.10).abs() < f64::EPSILON,
        "the reviewed proposal's ceiling, never another"
    );
    assert_eq!(run.bytes.as_deref(), Some(&Witness::of(bytes.as_bytes())));
    assert!(run.closure.is_some(), "bound to the world its check judged");
    // The consent record names a run only when the set carried its own request: this one did
    // not, and no amount was made up to fill one.
    assert_eq!(last_consent_run(dir.path()), None);
    assert_eq!(s.waiting(), Waiting::Free);
    let replayed = s.submit("save & run", &shown);
    assert!(matches!(replayed, TurnOutcome::Refusal(_)), "{replayed:?}");
    let consumed = s.consent_to(&id, "save & run");
    assert!(
        matches!(&consumed, TurnOutcome::Refusal(r) if r.class == RefusalClass::AlreadyConsumed),
        "{consumed:?}"
    );
    assert_eq!(read(dir.path(), COPY_DEST), bytes, "written once");
}

/// `yes` is Save only: the same bytes land, the record asks no run, nothing is requested.
#[test]
fn yes_saves_only_and_never_runs() {
    let dir = tree();
    let (mut s, _, bytes, shown) = at_the_proposal(dir.path());
    let TurnOutcome::Facts(report) = s.submit("yes", &shown) else {
        panic!("a yes saves and is never a run");
    };
    assert!(report.contains("applied · wrote"), "{report}");
    assert_eq!(read(dir.path(), COPY_DEST), bytes);
    assert_eq!(last_consent_run(dir.path()), None);
    assert!(s.work().requested.is_none(), "no run was requested");
}

/// A required input the run needs is asked after the save, and its answer requests the run the
/// `save & run` asked for: no second consent, no « run it ».
#[test]
fn an_input_the_run_needs_keeps_the_save_and_run_request() {
    let dir = tree();
    let (mut s, _, _, _) = at_the_proposal(dir.path());
    let set = ProjectChangeSet::workflow_at(dir.path(), "greet", "greet.nika", NEEDS_INPUT.into());
    let shown = proposed(&mut s, set.expect("a workflow set"));
    let TurnOutcome::Question { key, question } = s.submit("save & run", &shown) else {
        panic!("the input is asked after the save");
    };
    assert_eq!(key, "input.name");
    assert!(question.contains("applied · wrote"), "{question}");
    assert_eq!(read(dir.path(), "greet.nika"), NEEDS_INPUT);
    let asked = s.waiting();
    assert!(matches!(&asked, Waiting::Input { name } if name == "name"));
    let TurnOutcome::RunRequested { run, .. } = s.submit("Thibaut", &asked) else {
        panic!("the answer continues the same request");
    };
    assert_eq!(run.workflow, PathBuf::from("greet.nika"));
    assert_eq!(run.vars, vec!["name=Thibaut".to_owned()]);
    assert!((run.max_cost_usd - 0.10).abs() < f64::EPSILON);
}

/// A proposal that is not the one shown, a target that moved on disk since it was proposed, and a
/// set that saves no workflow: nothing lands and nothing runs, and the one decision left is said.
#[test]
fn a_stale_or_moved_proposal_saves_nothing_and_runs_nothing() {
    let dir = tree();
    let (mut s, id, _, _) = at_the_proposal(dir.path());
    let stale = Waiting::Consent {
        proposal: ProposalId::of("an earlier preview"),
    };
    let refused = s.submit("save & run", &stale);
    assert!(
        matches!(&refused, TurnOutcome::Refusal(r) if r.class == RefusalClass::StaleRevision),
        "{refused:?}"
    );
    assert!(!dir.path().join(COPY_DEST).exists());
    assert_eq!(
        s.pending_proposal(),
        Some(id),
        "the proposal shown now waits"
    );
    std::fs::write(dir.path().join(COPY_DEST), "nika: elsewhere\n").expect("a file appeared");
    let shown = s.waiting();
    let moved = s.submit("save & run", &shown);
    assert!(matches!(moved, TurnOutcome::Refusal(_)), "{moved:?}");
    assert_eq!(
        read(dir.path(), COPY_DEST),
        "nika: elsewhere\n",
        "never overwritten"
    );
    assert!(s.work().requested.is_none());

    let other = tree();
    let (mut s, _, _, _) = at_the_proposal(other.path());
    let project = ProjectChangeSet {
        root: s.snapshot.root.clone(),
        goal: "a ceiling".to_owned(),
        changes: vec![ProjectChange::CreateProjectFile {
            content: "nika: demo\nceiling: 0.25\n".to_owned(),
        }],
        run: None,
        repairs: Vec::new(),
        audits: Vec::new(),
    };
    let shown = proposed(&mut s, project);
    let TurnOutcome::Held { preview, .. } = s.submit("save & run", &shown) else {
        panic!("nothing to run: the proposal is held");
    };
    assert!(preview.contains("saves no workflow to run"), "{preview}");
    assert!(!s.snapshot.root.join("nika.yaml").exists());
}

/// Findings on the saved bytes stop the run: the save stands, and the run is not started.
#[test]
fn findings_on_the_saved_bytes_save_but_never_run() {
    let dir = tree();
    let (mut s, _, _, _) = at_the_proposal(dir.path());
    let ungranted = ProjectChangeSet {
        root: dir.path().to_path_buf(),
        goal: "ungranted".to_owned(),
        changes: vec![ProjectChange::CreateWorkflow {
            path: PathBuf::from("ungranted.nika"),
            content: UNGRANTED.to_owned(),
        }],
        run: None,
        repairs: Vec::new(),
        audits: Vec::new(),
    };
    let shown = proposed(&mut s, ungranted);
    let TurnOutcome::Facts(report) = s.submit("save & run", &shown) else {
        panic!("findings stop the run");
    };
    assert!(
        report.contains("findings ✖") && report.contains("the run was not started"),
        "{report}"
    );
    assert_eq!(
        read(dir.path(), "ungranted.nika"),
        UNGRANTED,
        "the save stands"
    );
    assert!(s.work().requested.is_none());
}

/// A classifier that reads every line as a run request.
struct ReadsRun;

impl TurnClassifier for ReadsRun {
    fn classify(&mut self, _context: &TurnContext, _raw: &str) -> TurnDecision {
        TurnDecision::new(TurnAct::RequestRun, RoutingMethod::Model)
    }
}

/// A run read in open words at the proposal is never this decision: the proposal is held, nothing
/// lands, nothing runs, and the one explicit command is named. With nothing waiting, the closed
/// word answers nothing either.
#[test]
fn a_run_read_in_words_never_saves_or_runs() {
    let dir = tree();
    let (mut s, id, _, shown) = at_the_proposal(dir.path());
    s.with_classifier(Box::new(ReadsRun));
    let TurnOutcome::Held { id: held, preview } = s.submit("ok so save it and run it now", &shown)
    else {
        panic!("a reading is never a consent");
    };
    assert_eq!(held, id);
    assert!(
        preview.contains("`save & run` saves this proposal and runs it once"),
        "{preview}"
    );
    assert!(!dir.path().join(COPY_DEST).exists());
    assert!(s.work().requested.is_none());

    let free = tree();
    let mut idle = ready_with(free.path(), vec![]);
    let nothing = idle.submit("save & run", &Waiting::Free);
    assert!(
        matches!(&nothing, TurnOutcome::Refusal(r) if r.class == RefusalClass::WrongState),
        "{nothing:?}"
    );
}

/// A conversation reopened after a `save & run` whose run was never observed: the uncertain
/// effect is said, nothing is replayed, the saved bytes are written once, and the old answer
/// reaches nothing.
#[test]
fn a_reopened_conversation_never_runs_an_unobserved_save_and_run_again() {
    let dir = tree();
    let home = tempfile::tempdir().expect("home");
    let (mut s, _, bytes, shown) = at_the_proposal_kept(dir.path(), Some(home.path()));
    assert!(matches!(
        s.submit("save & run", &shown),
        TurnOutcome::RunRequested { .. }
    ));
    drop(s);
    let mut reopened = ready_with(dir.path(), vec![]);
    let notice = reopened.enable_history(home.path()).expect("resumed");
    let said = notice.unwrap_or_default();
    assert!(
        said.contains("an earlier result or charge remains uncertain")
            && said.contains("nothing was replayed"),
        "{said}"
    );
    assert_eq!(reopened.waiting(), Waiting::Free);
    let again = reopened.submit("save & run", &shown);
    assert!(matches!(again, TurnOutcome::Refusal(_)), "{again:?}");
    assert_eq!(read(dir.path(), COPY_DEST), bytes, "written once");
}

/// A parent that calls its child, granting what the child uses.
const PARENT: &str = "nika: parent\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  call:\n    invoke: { workflow: \"./child.nika\" }\n";
const CHILD: &str = "nika: kid\npermits:\n  tools: [\"nika:jq\"]\ntasks:\n  value:\n    invoke:\n      tool: nika:jq\n      args: { input: 1, expression: \".\" }\n";

/// A set landing `files` in order, carrying `run` when one is given.
fn landing(root: &Path, files: &[(&str, &str)], run: Option<RunRequest>) -> ProjectChangeSet {
    let changes = (files.iter())
        .map(|(path, content)| ProjectChange::CreateWorkflow {
            path: PathBuf::from(path),
            content: (*content).to_owned(),
        })
        .collect();
    let (goal, repairs, audits) = ("a set".to_owned(), Vec::new(), Vec::new());
    ProjectChangeSet {
        root: root.to_path_buf(),
        goal,
        changes,
        run,
        repairs,
        audits,
    }
}

/// A run request a set may carry: its own target, inputs and ceiling.
fn carried(workflow: &str, vars: &[&str], max_cost_usd: f64) -> RunRequest {
    RunRequest {
        workflow: PathBuf::from(workflow),
        vars: vars.iter().map(|v| (*v).to_owned()).collect(),
        max_cost_usd,
        access_pin: None,
        bytes: None,
        closure: None,
    }
}

/// A set that carries its own run request: `save & run` runs exactly that request (its target,
/// inputs and ceiling) through the shared admission; `yes` saves it and runs nothing.
#[test]
fn a_carried_run_request_runs_exactly_on_save_and_run_and_never_on_yes() {
    let dir = tree();
    let (mut s, _, _, _) = at_the_proposal(dir.path());
    let request = carried("greet.nika", &["name=Thibaut"], 0.05);
    let set = landing(dir.path(), &[("greet.nika", NEEDS_INPUT)], Some(request));
    let shown = proposed(&mut s, set);
    let TurnOutcome::RunRequested { run, .. } = s.submit("save & run", &shown) else {
        panic!("the carried request runs, its input already bound");
    };
    assert_eq!(run.workflow, PathBuf::from("greet.nika"));
    assert_eq!(run.vars, vec!["name=Thibaut".to_owned()]);
    assert!(
        (run.max_cost_usd - 0.05).abs() < f64::EPSILON,
        "its own ceiling"
    );
    assert_eq!(
        last_consent_run(dir.path()),
        Some(PathBuf::from("greet.nika"))
    );

    let other = tree();
    let (mut s, _, _, _) = at_the_proposal(other.path());
    let request = carried("greet.nika", &["name=Thibaut"], 0.05);
    let set = landing(other.path(), &[("greet.nika", NEEDS_INPUT)], Some(request));
    let shown = proposed(&mut s, set);
    let TurnOutcome::Facts(report) = s.submit("yes", &shown) else {
        panic!("a yes saves only, whatever run the set carries");
    };
    assert!(report.contains("applied · wrote"), "{report}");
    assert_eq!(last_consent_run(other.path()), None, "no run was asked");
    assert!(s.work().requested.is_none());
}

/// A composed proposal lands its child before its parent: `save & run` never picks one of them
/// by position. Without a target of its own it is held, nothing written; a carried request that
/// names the parent runs the parent.
#[test]
fn a_composed_proposal_runs_only_the_workflow_its_request_names() {
    let files = [("child.nika", CHILD), ("parent.nika", PARENT)];
    let dir = tree();
    let (mut s, _, _, _) = at_the_proposal(dir.path());
    let shown = proposed(&mut s, landing(dir.path(), &files, None));
    let TurnOutcome::Held { preview, .. } = s.submit("save & run", &shown) else {
        panic!("several workflows and no target: held");
    };
    assert!(preview.contains("saves several workflows"), "{preview}");
    assert!(!dir.path().join("child.nika").exists(), "nothing written");
    assert!(!dir.path().join("parent.nika").exists(), "nothing written");
    assert!(
        s.pending_proposal().is_some(),
        "the composed proposal still waits"
    );

    let other = tree();
    let (mut s, _, _, _) = at_the_proposal(other.path());
    let request = carried("parent.nika", &[], 0.05);
    let shown = proposed(&mut s, landing(other.path(), &files, Some(request)));
    let TurnOutcome::RunRequested { run, .. } = s.submit("save & run", &shown) else {
        panic!("the request names its root");
    };
    assert_eq!(
        run.workflow,
        PathBuf::from("parent.nika"),
        "never the first child"
    );
    assert!(
        run.closure.is_some(),
        "the parent's world, its child included"
    );
}

/// Two required inputs whose values may hold spaces and JSON, in a file whose name has a space.
const NEEDS_TWO: &str = "nika: greet-two\ninputs:\n  name: { type: string, required: true }\n  payload: { type: string, required: true }\npermits:\n  fs: { write: [\"./hello.txt\"] }\n  tools: [\"nika:write\"]\ntasks:\n  write:\n    invoke: { tool: \"nika:write\", args: { path: \"./hello.txt\", content: \"${{ inputs.name }} ${{ inputs.payload }}\" } }\n";

/// A carried request reaches the run admission typed: a value with spaces, a JSON value and a
/// workflow whose name has a space arrive exactly, under its own ceiling.
#[test]
fn a_carried_request_keeps_spaces_and_json_exactly() {
    let dir = tree();
    let (mut s, _, _, _) = at_the_proposal(dir.path());
    let vars = [
        "name=Thibaut Melen",
        r#"payload={"items": [1, 2], "note": "a, b; c"}"#,
    ];
    let request = carried("greet two.nika", &vars, 0.05);
    let set = landing(dir.path(), &[("greet two.nika", NEEDS_TWO)], Some(request));
    let shown = proposed(&mut s, set);
    let TurnOutcome::RunRequested { run, .. } = s.submit("save & run", &shown) else {
        panic!("every input is bound: the run is requested");
    };
    assert_eq!(run.workflow, PathBuf::from("greet two.nika"));
    let exact: Vec<String> = vars.iter().map(|v| (*v).to_owned()).collect();
    assert_eq!(run.vars, exact, "never cut at a space or a comma");
    assert!(
        (run.max_cost_usd - 0.05).abs() < f64::EPSILON,
        "its own ceiling"
    );
    assert_eq!(read(dir.path(), "greet two.nika"), NEEDS_TWO);
}

/// A workflow whose name has a space, saved and run with no request of its own: the admission
/// takes the path as typed.
#[test]
fn a_workflow_name_with_a_space_saves_and_runs() {
    let dir = tree();
    let (mut s, _, _, _) = at_the_proposal(dir.path());
    let set = landing(dir.path(), &[("my flow.nika", NEEDS_INPUT)], None);
    let shown = proposed(&mut s, set);
    let TurnOutcome::Question { key, .. } = s.submit("save & run", &shown) else {
        panic!("its input is asked after the save");
    };
    assert_eq!(key, "input.name");
    let asked = s.waiting();
    let TurnOutcome::RunRequested { run, .. } = s.submit("Thibaut Melen", &asked) else {
        panic!("the answer continues the same request");
    };
    assert_eq!(run.workflow, PathBuf::from("my flow.nika"));
    assert_eq!(run.vars, vec!["name=Thibaut Melen".to_owned()]);
    assert!(
        (run.max_cost_usd - 0.10).abs() < f64::EPSILON,
        "the reviewed decision"
    );
}

/// A carried ceiling that is not a finite nonnegative amount is a validation error: held,
/// nothing written.
#[test]
fn a_carried_request_with_an_invalid_ceiling_is_held() {
    for ceiling in [f64::NAN, -1.0, f64::INFINITY] {
        let dir = tree();
        let (mut s, _, _, _) = at_the_proposal(dir.path());
        let request = carried("greet.nika", &["name=Thibaut"], ceiling);
        let set = landing(dir.path(), &[("greet.nika", NEEDS_INPUT)], Some(request));
        let shown = proposed(&mut s, set);
        let TurnOutcome::Held { preview, .. } = s.submit("save & run", &shown) else {
            panic!("{ceiling}: an invalid ceiling is held");
        };
        assert!(
            preview.contains("not a finite nonnegative amount"),
            "{preview}"
        );
        assert!(!dir.path().join("greet.nika").exists(), "nothing written");
    }
}

/// The run's own guards stay meaningful after the save: a paused gate waits, or a restored exposure
/// has no stated ceiling to reconfirm it, so the save stands and the run is not started.
#[test]
fn a_waiting_gate_or_an_unconfirmed_exposure_saves_but_never_runs() {
    let dir = tree();
    let (mut s, _, bytes, shown) = at_the_proposal(dir.path());
    s.pending_gate = Some(crate::change::PendingGate {
        workflow: PathBuf::from("gate.nika"),
        trace: PathBuf::from(".nika/traces/t.ndjson"),
        task: "approve".to_owned(),
        message: "Ship it?".to_owned(),
        mode: "confirm".to_owned(),
    });
    let TurnOutcome::Facts(report) = s.submit("save & run", &shown) else {
        panic!("a waiting gate stops the run, never the save");
    };
    assert!(
        report.contains("the run was not started: a paused gate waits"),
        "{report}"
    );
    assert_eq!(read(dir.path(), COPY_DEST), bytes, "the save stands");
    assert!(s.work().requested.is_none());

    let other = tree();
    let (mut s, _, _, shown) = at_the_proposal(other.path());
    s.money.reconfirm = true;
    let TurnOutcome::Facts(report) = s.submit("save & run", &shown) else {
        panic!("an unconfirmed exposure stops the run, never the save");
    };
    assert!(report.contains("the run was not started"), "{report}");
    assert!(other.path().join(COPY_DEST).exists(), "the save stands");
    assert!(s.work().requested.is_none());
}

/// A run paused at a confirm gate.
fn a_paused_gate() -> crate::change::PendingGate {
    crate::change::PendingGate {
        workflow: PathBuf::from("gate.nika"),
        trace: PathBuf::from(".nika/traces/t.ndjson"),
        task: "approve".to_owned(),
        message: "Ship it?".to_owned(),
        mode: "confirm".to_owned(),
    }
}

/// Whether `outcome` is the stale refusal a waiting run review gives every other answer.
fn refused_for_the_review(outcome: &TurnOutcome) -> bool {
    matches!(outcome, TurnOutcome::Refusal(r) if r.class == RefusalClass::StaleRevision)
}

/// A run's cost review waits first ([`SessionRuntime::waiting`]): a consent or a gate answer
/// naming its own identity is refused as `submit` refuses it, before anything is saved,
/// requested or resumed, and the review, the proposal and the gate all keep waiting.
#[test]
fn a_waiting_run_review_takes_no_consent_or_gate_answer_by_identity() {
    let dir = tree();
    let (mut s, id, _, _) = at_the_proposal(dir.path());
    s.run_review_asked("Run `other.nika` once for $0.40?", "the evidence");
    let refused = s.consent_to(&id, "save & run");
    assert!(refused_for_the_review(&refused), "{refused:?}");
    assert!(!dir.path().join(COPY_DEST).exists(), "nothing was saved");
    assert!(s.work().requested.is_none(), "no run was requested");
    assert_eq!(s.pending_proposal(), Some(id), "the proposal still waits");
    s.pending_gate = Some(a_paused_gate());
    let gate = s.waiting_gate().expect("the gate");
    let refused = s.answer_gate_for(&gate, "yes");
    assert!(refused_for_the_review(&refused), "{refused:?}");
    assert_eq!(s.waiting_gate(), Some(gate), "the gate still waits");
    assert!(matches!(s.waiting(), Waiting::RunReview { .. }));
}

/// The same through the public durable helpers a host may call with no identity: `consent` and
/// `answer_gate` answer nothing past a waiting run review, and leaving stays one line away.
#[test]
fn a_waiting_run_review_takes_no_direct_consent_or_gate_answer() {
    let dir = tree();
    let (mut s, id, _, _) = at_the_proposal(dir.path());
    s.run_review_asked("Run `other.nika` once for $0.40?", "the evidence");
    let refused = s.consent("save & run");
    assert!(refused_for_the_review(&refused), "{refused:?}");
    assert!(!dir.path().join(COPY_DEST).exists(), "nothing was saved");
    assert!(s.work().requested.is_none(), "no run was requested");
    assert_eq!(s.pending_proposal(), Some(id), "the proposal still waits");
    s.pending_gate = Some(a_paused_gate());
    let gate = s.waiting_gate().expect("the gate");
    let refused = s.answer_gate("yes");
    assert!(refused_for_the_review(&refused), "{refused:?}");
    assert_eq!(s.waiting_gate(), Some(gate.clone()), "the gate still waits");
    assert!(matches!(s.waiting(), Waiting::RunReview { .. }));
    assert!(matches!(s.answer_gate("/quit"), TurnOutcome::Quit));
    assert_eq!(s.waiting_gate(), Some(gate), "leaving answers no gate");
}
