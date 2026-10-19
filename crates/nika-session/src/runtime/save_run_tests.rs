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
    assert!(report.contains("applied · wrote"), "report");
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
    assert!(preview.contains("saves no workflow to run"), "preview");
    assert!(!s.snapshot.root.join("nika.yaml").exists());
}

/// Bytes whose own check fails are never saved or run (QA gauntlet q04): `save & run` and `yes`
/// both refuse before anything lands, the saved clean file is never replaced by them, nothing is
/// requested, and the proposal keeps waiting for a change or `no`.
#[test]
fn bytes_failing_their_own_check_never_save_or_run() {
    let dir = tree();
    let (mut s, _, bytes, shown) = at_the_proposal(dir.path());
    assert!(matches!(s.submit("yes", &shown), TurnOutcome::Facts(_)));
    assert_eq!(
        read(dir.path(), COPY_DEST),
        bytes,
        "the clean file is saved"
    );
    let failing =
        ProjectChangeSet::workflow_at(dir.path(), "ungranted", COPY_DEST, UNGRANTED.to_owned())
            .expect("an update of the saved file");
    assert!(!failing.audits[0].clean, "{:?}", failing.audits);
    let shown = proposed(&mut s, failing);
    for answer in ["save & run", "yes"] {
        let refused = s.submit(answer, &shown);
        assert!(
            matches!(&refused, TurnOutcome::Refusal(r) if r.class == RefusalClass::NotAllowed
                && r.text.starts_with("the check of these bytes fails (`compiled-workflow.nika`)")
                && r.text.contains("no file was replaced")),
            "{answer}: {refused:?}"
        );
        assert_eq!(read(dir.path(), COPY_DEST), bytes, "the clean file stands");
        assert_eq!(s.waiting(), shown, "the proposal still waits");
    }
    assert!(s.work().requested.is_none(), "nothing requested");
    assert!(matches!(s.submit("no", &shown), TurnOutcome::Facts(_)));
    assert_eq!(read(dir.path(), COPY_DEST), bytes);
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
        "the save and run preview"
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
    assert!(report.contains("applied · wrote"), "report");
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
    assert!(preview.contains("saves several workflows"), "preview");
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
            "the held ceiling"
        );
        assert!(!dir.path().join("greet.nika").exists(), "nothing written");
    }
}

/// The run's own guards stay meaningful after the save: a paused gate waits, so the save stands
/// and the run is not started.
#[test]
fn a_waiting_gate_saves_but_never_runs() {
    let dir = tree();
    let (mut s, _, bytes, shown) = at_the_proposal(dir.path());
    s.pending_gate = Some(crate::change::PendingGate {
        workflow: PathBuf::from("gate.nika"),
        trace: PathBuf::from(".nika/traces/t.ndjson"),
        task: "approve".to_owned(),
        message: "Ship it?".to_owned(),
        mode: "confirm".to_owned(),
        values: None,
        withheld: None,
    });
    let TurnOutcome::Facts(report) = s.submit("save & run", &shown) else {
        panic!("a waiting gate stops the run, never the save");
    };
    assert!(
        report.contains("the run was not started: a paused gate waits"),
        "the gate report"
    );
    assert_eq!(read(dir.path(), COPY_DEST), bytes, "the save stands");
    assert!(s.work().requested.is_none());
}

/// A restored exposure leaves no saved decision to stand for a typed request's ceiling (NIK-170):
/// the save stands, nothing is requested, and the run asks its ceiling instead of refusing. « yes »
/// states no amount and the run keeps waiting; the amount picked runs it once under exactly that
/// ceiling, the amount the person gave, as « run it max $0.30 » would.
#[test]
fn an_unconfirmed_exposure_saves_then_asks_the_runs_ceiling() {
    let dir = tree();
    let (mut s, _, bytes, shown) = at_the_proposal(dir.path());
    s.money.reconfirm = true;
    let TurnOutcome::Question { key, question } = s.submit("save & run", &shown) else {
        panic!("an unconfirmed exposure asks the run's ceiling, never refuses the save");
    };
    assert_eq!(key, "run.ceiling");
    assert!(
        question.contains("applied · wrote"),
        "the save report: {question}"
    );
    assert!(
        question.ends_with("I did not start the run: what ceiling for this run? (e.g. 0.25)"),
        "{question}"
    );
    assert!(
        !question.contains("restored inference exposure"),
        "{question}"
    );
    assert_eq!(read(dir.path(), COPY_DEST), bytes, "the save stands");
    assert!(s.work().requested.is_none(), "nothing requested");
    let asked = s.waiting();
    assert_eq!(
        asked,
        Waiting::RunCeiling {
            workflow: PathBuf::from(COPY_DEST)
        }
    );
    let (_, said, offers, typed) = s.waiting_run_ceiling_choices().expect("its options");
    assert!(typed, "another amount may be typed");
    assert_eq!(said.lines().count(), 2, "{said}");
    let keys: Vec<&str> = offers.iter().map(|o| o.key.as_str()).collect();
    assert_eq!(keys, ["0.10", "0.25", "0.50", "1.00"]);
    let recommended: Vec<&str> = (offers.iter().filter(|o| o.recommended))
        .map(|o| o.label.as_str())
        .collect();
    assert_eq!(
        recommended,
        ["$0.25 · no run of this workflow observed yet"]
    );

    let yes = s.submit("yes", &asked);
    assert!(
        matches!(&yes, TurnOutcome::Refusal(r) if r.class == RefusalClass::EmptyAnswer
            && r.text.starts_with("« yes » is not an amount")),
        "{yes:?}"
    );
    assert_eq!(s.waiting(), asked, "the run still waits for its ceiling");
    assert!(s.work().requested.is_none(), "no amount applied");

    let TurnOutcome::RunRequested { run, .. } = s.submit("0.30", &asked) else {
        panic!("the amount picked runs it");
    };
    assert_eq!(run.workflow, PathBuf::from(COPY_DEST));
    assert_eq!(run.max_cost_usd.to_bits(), 0.30_f64.to_bits());
    assert_eq!(run.bytes.as_deref(), Some(&Witness::of(bytes.as_bytes())));
    let decision = s.monetary_decision().expect("the run's money");
    assert_eq!(decision.source, crate::money::MonetarySource::Explicit);
    assert_eq!(decision.effective_usd, Some(0.30));
    assert_eq!(s.waiting(), Waiting::Free);
}

/// The founder's trap (NIK-170): a Save that bound no ceiling (a reopened conversation keeps no
/// draft for its later proposals) lands, then « run it » asks for the run's ceiling instead of
/// refusing that it cannot be proved. The refusal is still observed as the Run's money; the
/// amount picked runs it under exactly that ceiling, and nothing ran before it.
#[test]
fn a_save_without_a_provable_ceiling_asks_it_when_run() {
    let dir = tree();
    let (mut s, _, bytes, _) = at_the_proposal(dir.path());
    // As a reopened conversation's later proposal: no draft, so no money bound to it.
    s.money.draft = None;
    s.money.pending = None;
    let shown = s.waiting();
    let saved = s.submit("yes", &shown);
    assert!(matches!(saved, TurnOutcome::Facts(_)), "{saved:?}");
    let TurnOutcome::Question { key, question } = s.turn("run it") else {
        panic!("no provable ceiling asks it");
    };
    assert_eq!(key, "run.ceiling");
    assert_eq!(
        question,
        "I did not start the run: what ceiling for this run? (e.g. 0.25)"
    );
    let observed = s.monetary_decision().expect("the Run's money observed");
    assert_eq!(observed.effective_usd, None, "no default substituted");
    assert!(
        (observed.refusal.as_deref())
            .is_some_and(|why| why.starts_with("the saved spending constraint cannot be proved")),
        "{observed:?}"
    );
    assert!(s.work().requested.is_none(), "nothing requested");
    let asked = s.waiting();
    // « why? » beside it says why none could be proved, and the question keeps waiting.
    let why = s.submit("why?", &asked);
    assert!(
        matches!(&why, TurnOutcome::Aside(text)
            if text.contains("was not started: the saved spending constraint cannot be proved in this session")
                && text.contains("the run still waits for its ceiling")),
        "{why:?}"
    );
    assert_eq!(s.waiting(), asked);
    let command = s.submit("/bogus", &asked);
    assert!(
        matches!(&command, TurnOutcome::Refusal(r) if r.class == RefusalClass::WrongState),
        "{command:?}"
    );
    assert_eq!(s.waiting(), asked, "a command is no amount");
    let TurnOutcome::RunRequested { run, .. } = s.submit("0.25", &asked) else {
        panic!("the amount picked runs it");
    };
    assert_eq!(run.max_cost_usd.to_bits(), 0.25_f64.to_bits());
    assert_eq!(run.bytes.as_deref(), Some(&Witness::of(bytes.as_bytes())));
    assert_eq!(s.waiting(), Waiting::Free);
}

/// The ceiling stated at a run's question is that Run's money (NIK-170): typed with its currency,
/// it amends no Session inference, so the reopened conversation reads no exposure to reconfirm.
#[test]
fn a_ceiling_stated_at_its_question_is_no_session_money_after_a_reopen() {
    let dir = tree();
    let home = tempfile::tempdir().expect("home");
    let (mut s, _, _, _) = at_the_proposal_kept(dir.path(), Some(home.path()));
    s.money.draft = None;
    s.money.pending = None;
    let shown = s.waiting();
    assert!(matches!(s.submit("yes", &shown), TurnOutcome::Facts(_)));
    assert!(matches!(s.turn("run it"), TurnOutcome::Question { .. }));
    let asked = s.waiting();
    let ran = s.submit("$0.30", &asked);
    assert!(
        matches!(&ran, TurnOutcome::RunRequested { run, .. }
            if run.max_cost_usd.to_bits() == 0.30_f64.to_bits()),
        "{ran:?}"
    );
    drop(s);
    let mut reopened = ready_with(dir.path(), vec![]);
    let restored = reopened.enable_history(home.path()).expect("history");
    assert!(restored.is_some(), "the conversation is restored");
    assert!(
        !reopened.money.reconfirm,
        "a Run's ceiling is no Session exposure to reconfirm"
    );
}

/// Only an unprovable ceiling is asked: a saved workflow changed since its Save still refuses its
/// Run in the money law's words, and no ceiling waits.
#[test]
fn a_changed_saved_file_still_refuses_its_run_and_asks_nothing() {
    let dir = tree();
    let (mut s, _, bytes, shown) = at_the_proposal(dir.path());
    assert!(matches!(s.submit("yes", &shown), TurnOutcome::Facts(_)));
    std::fs::write(
        dir.path().join(COPY_DEST),
        format!("{bytes}# edited by hand\n"),
    )
    .expect("an edit");
    let refused = s.turn("run it");
    assert!(
        matches!(&refused, TurnOutcome::Refusal(r) if r.class == RefusalClass::NotAllowed
            && r.text.contains("changed since")),
        "{refused:?}"
    );
    assert_eq!(s.waiting(), Waiting::Free, "no ceiling is asked");
    assert!(s.waiting_run_ceiling_choices().is_none());
    assert!(s.work().requested.is_none(), "nothing requested");
}

/// A run paused at a confirm gate.
fn a_paused_gate() -> crate::change::PendingGate {
    crate::change::PendingGate {
        workflow: PathBuf::from("gate.nika"),
        trace: PathBuf::from(".nika/traces/t.ndjson"),
        task: "approve".to_owned(),
        message: "Ship it?".to_owned(),
        mode: "confirm".to_owned(),
        values: None,
        withheld: None,
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

/// A ceiling the request states for each run (QA gauntlet q09: « budget max 0,10 $ par run » in
/// the first request) is the proposal's, the Save keeps it, and every later run of the saved
/// workflow reuses it and says so; a ceiling stated for one run is that run's alone, and the
/// next run names the saved one again, never a default in silence.
#[test]
fn a_ceiling_the_request_states_is_saved_and_named_by_every_rerun() {
    let dir = tree();
    let mut s = ready_with(dir.path(), vec![]);
    let TurnOutcome::Proposal { preview, .. } =
        s.turn(&format!("{COPY}, budget max 0,10 $ par run"))
    else {
        panic!("the request is proposed");
    };
    assert!(
        preview.contains("money: $0.10 USD · the amount you gave"),
        "{preview}"
    );
    let shown = s.waiting();
    assert!(matches!(s.submit("yes", &shown), TurnOutcome::Facts(_)));
    // Each ceiling as it was set, the host's words alike (live gauntlet r09 B12: « $0.1 » here
    // against « $0.10 » there, and a $0.001 ceiling written « $0.00 »).
    for (line, amount, said, source) in [
        ("run it", 0.1_f64, "0.10", "saved beside the workflow"),
        (
            "run it with a ceiling of 0.05",
            0.05,
            "0.05",
            "stated for this run",
        ),
        (
            "run it with a ceiling of 0.001",
            0.001,
            "0.001",
            "stated for this run",
        ),
        ("run it", 0.1, "0.10", "saved beside the workflow"),
    ] {
        let TurnOutcome::RunRequested { report, run } = s.turn(line) else {
            panic!("{line}: the run is requested");
        };
        assert_eq!(run.max_cost_usd.to_bits(), amount.to_bits(), "{line}");
        assert!(
            report.ends_with(&format!(" · ceiling ${said} ({source})")),
            "{line}: {report}"
        );
        let _ = s.observe_run(0, None);
    }
}

/// The file a Save wrote, as its report names it (« applied · wrote `<file>` »).
fn wrote(saved: &str) -> &str {
    let after = saved.split_once("wrote `").map_or("", |(_, after)| after);
    after.split_once('`').map_or("", |(file, _)| file)
}

/// A Save says how to run the saved workflow from a terminal, by the program Nika runs as and
/// with the ceiling the person saved with it (NIK-171): `<program> run <file> --max-cost-usd
/// 0.1`. A workflow saved with no ceiling of the person's leaves the flag out, never a default.
#[test]
fn a_save_says_the_terminal_run_with_its_saved_ceiling() {
    let nika = super::schedule::program();
    for (request, flag) in [
        (
            format!("{COPY}, budget max 0,10 $ par run"),
            " --max-cost-usd 0.1",
        ),
        (COPY.to_owned(), ""),
    ] {
        let dir = tree();
        let mut s = ready_with(dir.path(), vec![]);
        let TurnOutcome::Proposal { .. } = s.turn(&request) else {
            panic!("{request}: the request is proposed");
        };
        let shown = s.waiting();
        let TurnOutcome::Facts(saved) = s.submit("yes", &shown) else {
            panic!("{request}: the yes saves");
        };
        let file = wrote(&saved);
        let workflow = Path::new(file).extension().is_some_and(|ext| ext == "nika");
        assert!(workflow, "{saved}");
        let line = format!("\n  from a terminal: `{nika} run {file}{flag}`");
        assert!(saved.contains(&line), "{request}: {saved}");
        assert_eq!(
            saved.matches("--max-cost-usd").count(),
            usize::from(!flag.is_empty()),
            "{saved}"
        );
    }
}
