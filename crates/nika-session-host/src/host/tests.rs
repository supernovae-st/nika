// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The host's custody, keyless: the real Session over a temporary project, its reasoner never
//! asked (the intent reaches the deterministic compiler), the worker held at named points so
//! each race is decided, not hoped for.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::time::Duration;

use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::{KeptRun, ScriptedReasoner, SessionRuntime};
use serde_json::Value;

use super::*;
use crate::run::{LaneRunDoor, NoRunDoor, RunDoor, RunRequest, RunSink, RunStep};
use crate::wire::CONTRACT;

/// A Ready intent the compiler settles with no question and no model.
pub(crate) const COPY: &str = "Read ./notes/brief.md and write it to ./out/copy.md";
/// An intent whose model the compiler must ask.
const DRAFT: &str = "Read ./notes/brief.md, draft a 3-bullet summary of it and write the summary to ./out/summary.md";
const BRIEF: &str = "# Brief\n\nThe launch moves to October.\n";
const WAIT: Duration = Duration::from_secs(60);

pub(crate) fn world() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("root");
    std::fs::create_dir_all(dir.path().join("notes")).expect("notes");
    std::fs::write(dir.path().join("notes/brief.md"), BRIEF).expect("brief");
    dir
}

/// The Session over `root`, as the CLI's own fixture opens it: a local engine chosen, a
/// scripted reasoner that is never asked.
pub(crate) fn runtime(root: &Path) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let preference = UserIntelligencePreference::new(
        IntelligenceKind::Local {
            provider: "ollama".to_owned(),
        },
        None,
    );
    let mut runtime = SessionRuntime::open(
        root,
        ResolvedSessionIntelligence::resolve(&preference, &census),
        Box::new(ScriptedReasoner::new(Vec::new())),
    );
    runtime.enable_continuous_preparation();
    runtime
}

fn host(runtime: SessionRuntime) -> SessionHost {
    SessionHost::start(
        runtime,
        Box::new(NoRunDoor::new("no run door in this test")),
        Vec::new(),
    )
    .expect("host")
}

fn json(frame: &Frame) -> Value {
    serde_json::to_value(frame).expect("frame")
}

fn handle(host: &SessionHost) -> String {
    json(&host.snapshot())["snapshot"]["snapshot"]
        .as_str()
        .expect("handle")
        .to_owned()
}

fn submit(command: &str, snapshot: &str, line: &str) -> Command {
    Command::Submit {
        command: command.to_owned(),
        snapshot: snapshot.to_owned(),
        line: line.to_owned(),
    }
}

fn stop(command: &str) -> Command {
    Command::Stop {
        command: command.to_owned(),
    }
}

/// Submit and wait for the settled result.
fn settle(host: &SessionHost, command: &str, line: &str) -> Value {
    let snapshot = handle(host);
    match host.dispatch(submit(command, &snapshot, line)) {
        Dispatch::Accepted { command, .. } => json(&host.wait_result(&command).expect("settled")),
        other => panic!("not accepted: {other:?}"),
    }
}

fn kinds(result: &Value) -> Vec<String> {
    (result["outcomes"].as_array().expect("outcomes").iter())
        .map(|o| o["kind"].as_str().expect("kind").to_owned())
        .collect()
}

fn reply(dispatch: Dispatch) -> Value {
    match dispatch {
        Dispatch::Reply(frame) | Dispatch::Logged(frame) => json(&frame),
        other => panic!("not a reply: {other:?}"),
    }
}

fn accepted_events(host: &SessionHost) -> usize {
    let (events, _) = host.events_after(0);
    events.iter().filter(|f| f.kind() == "accepted").count()
}

/// The point held, whether the worker reached it, whether the test released it.
type Held = (Option<&'static str>, bool, bool);

/// A gate the worker stops at: `hold` names the point, the test lets it pass.
#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<(Mutex<Held>, Condvar)>);

impl Gate {
    /// Hold the worker the next time it reaches `at`.
    pub(crate) fn hold(&self, at: &'static str) {
        let (state, _) = &*self.0;
        *state.lock().expect("gate") = (Some(at), false, false);
    }

    pub(crate) fn pause(&self) -> Pause {
        let gate = self.clone();
        Arc::new(move |at| {
            let (state, signal) = &*gate.0;
            let mut held = state.lock().expect("gate");
            if held.0 != Some(at) {
                return;
            }
            held.1 = true;
            signal.notify_all();
            while !held.2 {
                held = signal.wait(held).expect("gate");
            }
            *held = (None, false, false);
            signal.notify_all();
        })
    }

    pub(crate) fn reached(&self) {
        let (state, signal) = &*self.0;
        let held = state.lock().expect("gate");
        let (held, timeout) = signal
            .wait_timeout_while(held, WAIT, |held| !held.1)
            .expect("gate");
        assert!(
            !timeout.timed_out(),
            "the worker never reached {:?}",
            held.0
        );
    }

    pub(crate) fn release(&self) {
        let (state, signal) = &*self.0;
        state.lock().expect("gate").2 = true;
        signal.notify_all();
    }
}

#[test]
fn opening_is_event_one_with_the_sessions_own_work() {
    let root = world();
    let host = SessionHost::start(
        runtime(root.path()),
        Box::new(NoRunDoor::new("none")),
        vec!["banner".to_owned(), "conversation restored".to_owned()],
    )
    .expect("host");
    let opened = json(&host.opened().expect("opened"));
    assert_eq!(opened["contract"], CONTRACT);
    assert_eq!(opened["frame"], "opened");
    assert_eq!(opened["event"], 1);
    assert!(host.session().starts_with("ses_") && host.session().len() == 36);
    assert_eq!(opened["session"], host.session());
    assert_eq!(opened["notices"][1], "conversation restored");
    let snapshot = &opened["snapshot"];
    assert!(
        snapshot["snapshot"]
            .as_str()
            .is_some_and(|h| h.starts_with("snp_"))
    );
    assert_eq!(snapshot["seq"], 1);
    assert_eq!(snapshot["busy"], Value::Null);
    assert_eq!(snapshot["work"]["contract"], "nika/session-work@0");
    assert_eq!(snapshot["work"]["waiting"]["kind"], "free");
    assert_eq!(
        snapshot["work"]["root"].as_str(),
        root.path().to_str(),
        "work.root is the project the Session was opened over"
    );
}

#[test]
fn a_proposal_then_save_lands_the_previewed_bytes_and_requests_no_run() {
    let root = world();
    let host = host(runtime(root.path()));
    let proposed = settle(&host, "c-1", COPY);
    assert_eq!(kinds(&proposed), ["proposal"]);
    let work = &proposed["snapshot"]["work"];
    assert_eq!(work["waiting"]["kind"], "consent");
    let proposal = proposed["outcomes"][0]["proposal"].as_str().expect("id");
    assert_eq!(work["waiting"]["proposal"], proposal);
    assert_eq!(work["candidate"]["proposal"], proposal);
    let file = &work["candidate"]["files"][0];
    let path = file["path"].as_str().expect("path").to_owned();
    assert_eq!(file["landing"], "create");
    assert!(
        !root.path().join(&path).exists(),
        "nothing lands before the yes"
    );
    assert_eq!(proposed["snapshot"]["seq"], 2);
    let saved = settle(&host, "c-2", "yes");
    assert!(
        !kinds(&saved).iter().any(|k| k.starts_with("run")),
        "Save is never a Run: {saved}"
    );
    let bytes = std::fs::read(root.path().join(&path)).expect("saved file");
    assert_eq!(
        Value::String(nika_session::Witness::of(&bytes).0),
        file["bytes"],
        "the saved bytes are the previewed ones"
    );
    let work = &saved["snapshot"]["work"];
    assert_eq!(work["candidate"], Value::Null);
    assert_eq!(work["saved"]["workflow"], path.as_str());
    assert_eq!(work["requested"], Value::Null);
    assert_eq!(work["run"], Value::Null);
    assert!(
        !root.path().join("out/copy.md").exists(),
        "no run wrote its output"
    );
}

#[test]
fn a_line_typed_against_another_snapshot_never_reaches_the_session() {
    let root = world();
    let host = host(runtime(root.path()));
    let first = handle(&host);
    let proposed = settle(&host, "c-1", COPY);
    let path = proposed["snapshot"]["work"]["candidate"]["files"][0]["path"]
        .as_str()
        .expect("path")
        .to_owned();
    let accepted = accepted_events(&host);
    let stale = reply(host.dispatch(submit("c-2", &first, "yes")));
    assert_eq!(stale["error"], "stale_snapshot");
    assert_eq!(stale["line"], "yes", "the refused line is returned");
    assert_eq!(
        stale["snapshot"]["snapshot"],
        proposed["snapshot"]["snapshot"]
    );
    let unknown = reply(host.dispatch(submit("c-3", "snp_elsewhere", "yes")));
    assert_eq!(unknown["error"], "unknown_snapshot");
    assert_eq!(accepted_events(&host), accepted, "no turn started");
    assert_eq!(
        json(&host.snapshot())["snapshot"]["work"]["waiting"]["kind"],
        "consent"
    );
    assert!(
        !root.path().join(path).exists(),
        "no stale yes saved anything"
    );
    // A refused identity is not recorded: the same id answers once the line names the present.
    let saved = settle(&host, "c-2", "yes");
    assert_eq!(saved["replayed"], false);
}

#[test]
fn a_repeated_command_answers_before_busy_or_stale_and_runs_once() {
    let root = world();
    let host = host(runtime(root.path()));
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("returned");
    let opened = handle(&host);
    let first = host.dispatch(submit("c-1", &opened, COPY));
    assert!(matches!(
        first,
        Dispatch::Accepted {
            repeated: false,
            ..
        }
    ));
    gate.reached();
    // While it runs: the same identity and bytes is the same command, not a busy one.
    let again = host.dispatch(submit("c-1", &opened, COPY));
    assert!(matches!(again, Dispatch::Accepted { repeated: true, .. }));
    let conflict = reply(host.dispatch(submit("c-1", &opened, "other words")));
    assert_eq!(conflict["error"], "command_conflict");
    let busy = reply(host.dispatch(submit("c-2", &opened, COPY)));
    assert_eq!(busy["error"], "busy");
    assert_eq!(busy["line"], COPY);
    gate.release();
    let settled = json(&host.wait_result("c-1").expect("settled"));
    assert_eq!(kinds(&settled), ["proposal"]);
    // After it settled: it names a stale snapshot, and still answers its recorded result.
    let replay = reply(host.dispatch(submit("c-1", &opened, COPY)));
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["event"], settled["event"]);
    assert_eq!(replay["outcomes"], settled["outcomes"]);
    assert_eq!(accepted_events(&host), 1, "one turn for one identity");
}

#[test]
fn reads_and_details_answer_while_the_turn_prepares_and_stop_nothing() {
    let root = world();
    let host = host(runtime(root.path()));
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("returned");
    let before = json(&host.details());
    assert_eq!(before["frame"], "details");
    assert!(
        before["text"]
            .as_str()
            .is_some_and(|t| t.starts_with("Details"))
    );
    assert!(matches!(
        host.dispatch(submit("c-1", &handle(&host), COPY)),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    let during = json(&host.snapshot());
    assert_eq!(during["snapshot"]["busy"]["command"], "c-1");
    assert_eq!(during["snapshot"]["busy"]["phase"], "preparing");
    assert_eq!(during["snapshot"]["busy"]["stop_requested"], false);
    assert_eq!(json(&host.details())["text"], before["text"]);
    gate.release();
    let settled = json(&host.wait_result("c-1").expect("settled"));
    assert_eq!(kinds(&settled), ["proposal"], "reading stopped nothing");
}

#[test]
fn a_stop_that_wins_withdraws_the_late_proposal_before_anything_is_published() {
    let root = world();
    let host = host(runtime(root.path()));
    let gate = Gate::default();
    host.pause_at(gate.pause());
    // The Session already returned its proposal; the Stop arrives before the settlement.
    gate.hold("returned");
    assert!(matches!(
        host.dispatch(submit("c-1", &handle(&host), COPY)),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    let receipt = reply(host.dispatch(stop("s-1")));
    assert_eq!(receipt["frame"], "result");
    assert_eq!(receipt["receipt"], "stop_requested");
    assert_eq!(receipt["target"], "c-1");
    assert_eq!(receipt["snapshot"]["busy"]["stop_requested"], true);
    gate.release();
    let settled = json(&host.wait_result("c-1").expect("settled"));
    assert_eq!(kinds(&settled), ["cancelled"], "{settled}");
    let withdrawn = &settled["outcomes"][0]["withdrawn"];
    assert_eq!(
        withdrawn[0]["kind"], "proposal",
        "kept only as withdrawn history"
    );
    let work = &settled["snapshot"]["work"];
    assert_eq!(work["candidate"], Value::Null);
    assert_eq!(work["waiting"]["kind"], "free");
    // No published snapshot ever held the withdrawn candidate.
    let (events, _) = host.events_after(0);
    for frame in &events {
        let value = json(frame);
        let candidate = &value["snapshot"]["work"]["candidate"];
        assert!(candidate.is_null(), "published: {value}");
    }
}

/// A run door that counts every run, resume and review answer it is asked for, and starts none.
#[derive(Clone, Default)]
struct Counted(Arc<AtomicUsize>);

impl Counted {
    fn asked(&self) -> usize {
        self.0.load(Ordering::SeqCst)
    }

    fn count(&self) -> RunStep {
        self.0.fetch_add(1, Ordering::SeqCst);
        RunStep::NotStarted {
            why: "counted, not started".to_owned(),
        }
    }
}

impl RunDoor for Counted {
    fn run(&mut self, _root: &Path, _run: &RunRequest, _sink: &dyn RunSink) -> RunStep {
        self.count()
    }

    fn resume(&mut self, _: &Path, _: &Path, _: &Path, _: &str, _: &dyn RunSink) -> RunStep {
        self.count()
    }

    fn answer_review(&mut self, _approve: bool, _sink: &dyn RunSink) -> RunStep {
        self.count()
    }
}

/// A saved workflow, then « run it » held after the Session returned its run request.
fn run_requested_and_held(at: &'static str) -> (tempfile::TempDir, SessionHost, Counted, Gate) {
    let root = world();
    let door = Counted::default();
    let host =
        SessionHost::start(runtime(root.path()), Box::new(door.clone()), Vec::new()).expect("host");
    assert_eq!(kinds(&settle(&host, "c-1", COPY)), ["proposal"]);
    settle(&host, "c-2", "yes");
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold(at);
    assert!(matches!(
        host.dispatch(submit("c-3", &handle(&host), "run it")),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    (root, host, door, gate)
}

#[test]
fn a_stop_accepted_before_its_run_is_admitted_starts_no_run() {
    let (_root, host, door, gate) = run_requested_and_held("returned");
    let receipt = reply(host.dispatch(stop("s-1")));
    assert_eq!(
        (receipt["receipt"].clone(), receipt["target"].clone()),
        ("stop_requested".into(), "c-3".into())
    );
    gate.release();
    let settled = json(&host.wait_result("c-3").expect("settled"));
    assert_eq!(
        kinds(&settled),
        ["run_requested", "run_not_started"],
        "{settled}"
    );
    let why = settled["outcomes"][1]["text"].as_str().expect("why");
    assert!(
        why.starts_with("the Stop accepted while this turn prepared"),
        "{why}"
    );
    assert_eq!(
        door.asked(),
        0,
        "no run door was asked after an accepted Stop"
    );
    // A Stop that comes once the turn settles stops nothing: the next run is asked for.
    let (_root, host, door, gate) = run_requested_and_held("settling");
    assert_eq!(
        reply(host.dispatch(stop("s-1")))["receipt"],
        "nothing_to_stop"
    );
    gate.release();
    let settled = json(&host.wait_result("c-3").expect("settled"));
    assert_eq!(
        kinds(&settled),
        ["run_requested", "run_not_started"],
        "{settled}"
    );
    assert_eq!(settled["outcomes"][1]["text"], "counted, not started");
    assert_eq!(door.asked(), 1, "the run door was asked once");
}

#[test]
fn a_settlement_that_wins_leaves_nothing_to_stop_and_keeps_its_proposal() {
    let root = world();
    let host = host(runtime(root.path()));
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("settling");
    assert!(matches!(
        host.dispatch(submit("c-1", &handle(&host), COPY)),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    let receipt = reply(host.dispatch(stop("s-1")));
    assert_eq!(receipt["receipt"], "nothing_to_stop");
    assert_eq!(receipt["target"], "c-1");
    gate.release();
    let settled = json(&host.wait_result("c-1").expect("settled"));
    assert_eq!(kinds(&settled), ["proposal"]);
    assert!(settled["snapshot"]["work"]["candidate"].is_object());
}

#[test]
fn a_replayed_stop_never_touches_a_later_turn() {
    let root = world();
    let host = host(runtime(root.path()));
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("returned");
    assert!(matches!(
        host.dispatch(submit("c-1", &handle(&host), COPY)),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    let first = reply(host.dispatch(stop("s-1")));
    gate.release();
    let _ = host.wait_result("c-1").expect("settled");
    gate.hold("returned");
    assert!(matches!(
        host.dispatch(submit("c-2", &handle(&host), COPY)),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    let again = reply(host.dispatch(stop("s-1")));
    assert_eq!(again["replayed"], true);
    assert_eq!(again["target"], "c-1");
    assert_eq!(again["event"], first["event"]);
    assert_eq!(
        json(&host.snapshot())["snapshot"]["busy"]["stop_requested"],
        false
    );
    gate.release();
    let settled = json(&host.wait_result("c-2").expect("settled"));
    assert_eq!(
        kinds(&settled),
        ["proposal"],
        "the later turn was not stopped"
    );
}

#[test]
fn closing_while_preparing_stops_it_and_closed_ends_the_log() {
    let root = world();
    let host = host(runtime(root.path()));
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("returned");
    let opened = handle(&host);
    assert!(matches!(
        host.dispatch(submit("c-1", &opened, COPY)),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    assert!(matches!(host.dispatch(Command::Close), Dispatch::Closing));
    let after = reply(host.dispatch(submit("c-2", &opened, "hello")));
    assert_eq!(
        after["error"], "session_not_found",
        "nothing is admitted while closing"
    );
    gate.release();
    let closed = json(&host.wait_closed().expect("closed"));
    assert_eq!(closed["frame"], "closed");
    let settled = json(&host.wait_result("c-1").expect("settled"));
    assert_eq!(kinds(&settled), ["cancelled"]);
    let (events, complete) = host.events_after(0);
    assert!(complete);
    assert_eq!(events.last().map(Frame::kind), Some("closed"));
    assert_eq!(events.iter().filter(|f| f.kind() == "closed").count(), 1);
    assert!(matches!(host.dispatch(Command::Close), Dispatch::Closing));
}

#[test]
fn quitting_closes_the_log_after_its_result() {
    let root = world();
    let host = host(runtime(root.path()));
    let result = settle(&host, "c-1", "/quit");
    assert_eq!(kinds(&result), ["quit"]);
    let closed = json(&host.wait_closed().expect("closed"));
    assert_eq!(
        closed["event"].as_u64(),
        result["event"].as_u64().map(|n| n + 1)
    );
}

#[test]
fn the_same_question_in_another_session_is_another_identity() {
    let (one, two) = (world(), world());
    let (first, second) = (host(runtime(one.path())), host(runtime(two.path())));
    let asked = settle(&first, "c-1", DRAFT);
    let other = settle(&second, "c-1", DRAFT);
    assert_eq!(kinds(&asked), ["question"]);
    let waiting = &asked["snapshot"]["work"]["waiting"];
    assert_eq!(waiting["kind"], "question");
    assert_eq!(
        waiting["id"], other["snapshot"]["work"]["waiting"]["id"],
        "the witness text is shared; the identity is not"
    );
    // The question itself travels beside its identity, as the compiler asks it: the model, a
    // text answer the candidate cannot be Ready without.
    let question = &asked["snapshot"]["work"]["question"];
    assert_eq!(
        (&question["key"], &question["type"], &question["mandatory"]),
        (
            &waiting["key"],
            &serde_json::json!("text"),
            &serde_json::json!(true)
        ),
        "{question}"
    );
    assert_eq!(waiting["key"], "model");
    assert!(question.get("options").is_none(), "{question}");
    let foreign = handle(&second);
    let refused = reply(first.dispatch(submit("c-2", &foreign, "mistral/mistral-small")));
    assert_eq!(refused["error"], "unknown_snapshot");
    assert_eq!(
        json(&first.snapshot())["snapshot"]["work"]["waiting"],
        *waiting
    );
}

#[test]
fn a_restarted_session_is_another_incarnation_and_restores_without_authority() {
    let root = world();
    let home = tempfile::tempdir().expect("home");
    let open = || {
        let mut runtime = runtime(root.path());
        let recovered = runtime.enable_history(home.path()).expect("history");
        let restored = runtime.restore_state();
        let notices = recovered.into_iter().chain(restored).collect();
        SessionHost::start(runtime, Box::new(NoRunDoor::new("none")), notices).expect("host")
    };
    let first = open();
    let proposed = settle(&first, "c-1", COPY);
    let old = proposed["snapshot"]["snapshot"]
        .as_str()
        .expect("handle")
        .to_owned();
    assert!(matches!(first.dispatch(Command::Close), Dispatch::Closing));
    let _ = first.wait_closed();
    let second = open();
    assert_ne!(second.session(), first.session());
    let opened = json(&second.opened().expect("opened"));
    assert!(
        opened["notices"].to_string().contains("restored"),
        "the recovery is said: {opened}"
    );
    assert_eq!(opened["snapshot"]["work"]["candidate"], Value::Null);
    let refused = reply(second.dispatch(submit("c-2", &old, "yes")));
    assert_eq!(refused["error"], "unknown_snapshot");
    let replay = reply(second.dispatch(submit("c-1", &old, COPY)));
    assert_eq!(
        replay["error"], "unknown_snapshot",
        "a ledger lives with its incarnation"
    );
}

/// A run door that keeps every run request it is asked for and observes each one ending at 0.
#[derive(Clone, Default)]
struct Kept(Arc<Mutex<Vec<RunRequest>>>);

impl Kept {
    fn runs(&self) -> Vec<RunRequest> {
        self.0.lock().expect("runs").clone()
    }
}

impl RunDoor for Kept {
    fn run(&mut self, _root: &Path, run: &RunRequest, _sink: &dyn RunSink) -> RunStep {
        self.0.lock().expect("runs").push(run.clone());
        RunStep::Observed {
            exit: 0,
            trace: None,
            leg: None,
        }
    }

    fn resume(&mut self, _: &Path, _: &Path, _: &Path, _: &str, _: &dyn RunSink) -> RunStep {
        RunStep::NotStarted {
            why: "no resume here".to_owned(),
        }
    }

    fn answer_review(&mut self, _approve: bool, _sink: &dyn RunSink) -> RunStep {
        RunStep::NotStarted {
            why: "no review here".to_owned(),
        }
    }
}

/// `save & run` through the host every door shares: the proposal shown lands, and its one run
/// reaches the run door once, bound to the very bytes it saved and their world. The command
/// replayed answers from the record and asks no second run; `yes` asks none.
#[test]
fn save_and_run_reaches_the_run_door_once_for_the_bytes_it_saved() {
    use nika_session::change::Witness;
    let root = world();
    let door = Kept::default();
    let host =
        SessionHost::start(runtime(root.path()), Box::new(door.clone()), Vec::new()).expect("host");
    assert_eq!(kinds(&settle(&host, "c-1", COPY)), ["proposal"]);
    let shown = handle(&host);
    let settled = match host.dispatch(submit("c-2", &shown, "save & run")) {
        Dispatch::Accepted { command, .. } => json(&host.wait_result(&command).expect("settled")),
        other => panic!("not accepted: {other:?}"),
    };
    assert_eq!(kinds(&settled), ["run_requested", "facts"], "{settled}");
    let runs = door.runs();
    assert_eq!(runs.len(), 1, "one run");
    let saved = std::fs::read(root.path().join(&runs[0].workflow)).expect("saved bytes");
    assert_eq!(runs[0].bytes.as_deref(), Some(&Witness::of(&saved)));
    assert!(
        runs[0].closure.is_some(),
        "bound to the world its check judged"
    );
    let replay = reply(host.dispatch(submit("c-2", &shown, "save & run")));
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["outcomes"], settled["outcomes"]);
    assert_eq!(door.runs().len(), 1, "a replay never runs again");

    let other = world();
    let door = Kept::default();
    let host = SessionHost::start(runtime(other.path()), Box::new(door.clone()), Vec::new())
        .expect("host");
    assert_eq!(kinds(&settle(&host, "c-1", COPY)), ["proposal"]);
    assert_eq!(kinds(&settle(&host, "c-2", "yes")), ["facts"]);
    assert!(door.runs().is_empty(), "Save only asks no run");
}

/// What a line did to the question it was typed for rides the snapshot frame as the session
/// recorded it, under the identity the host was shown; a resync reads the same act, and a new
/// request is no answer.
#[test]
fn the_answer_act_rides_the_snapshot_under_the_identity_shown() {
    let root = world();
    let host = host(runtime(root.path()));
    let asked = settle(&host, "c-1", DRAFT);
    let shown = asked["snapshot"]["work"]["waiting"]["id"].clone();
    assert!(asked["snapshot"]["work"].get("answered").is_none());
    let dropped = settle(&host, "c-2", "cancel");
    assert_eq!(
        dropped["snapshot"]["work"]["answered"],
        serde_json::json!({"question": shown, "act": "dropped", "key": "model"})
    );
    let again = settle(&host, "c-3", DRAFT);
    let current = again["snapshot"]["work"]["waiting"]["id"].clone();
    assert_ne!(current, shown, "asked again is another question");
    assert!(again["snapshot"]["work"].get("answered").is_none());
    let bound = settle(&host, "c-4", "mock/echo");
    let act = &bound["snapshot"]["work"]["answered"];
    assert_eq!(
        *act,
        serde_json::json!({"question": current, "act": "bound", "key": "model",
            "value": "mock/echo", "reading": "as_typed"})
    );
    assert_eq!(
        json(&host.snapshot())["snapshot"]["work"]["answered"],
        *act,
        "a resync reads the same act"
    );
}

/// A Stop handle that counts what it is asked and says the run took its signal.
struct Counting(Arc<AtomicUsize>);

impl RunStop for Counting {
    fn stop(&self) -> Stopping {
        self.0.fetch_add(1, Ordering::SeqCst);
        Stopping::Signalled
    }
}

/// A run door whose run holds until the test lets it end, then observes it interrupted: its
/// settlement sealed or not. It can stop its runs only when it counts its Stops.
struct Holding {
    entered: mpsc::Sender<()>,
    release: mpsc::Receiver<()>,
    stops: Option<Arc<AtomicUsize>>,
    sealed: bool,
}

impl RunDoor for Holding {
    fn run(&mut self, _root: &Path, _run: &RunRequest, _sink: &dyn RunSink) -> RunStep {
        self.entered.send(()).expect("the test waits for the run");
        self.release
            .recv_timeout(WAIT)
            .expect("the test lets the run end");
        let mut leg = KeptRun::new();
        leg.execution = Some("01a0ef11-0212-70de-a8b3-99de9427fccc".to_owned());
        leg.chain_head = self.sealed.then(|| "c0ffee".to_owned());
        RunStep::Observed {
            exit: 130,
            trace: None,
            leg: Some(leg),
        }
    }

    fn resume(&mut self, _: &Path, _: &Path, _: &Path, _: &str, _: &dyn RunSink) -> RunStep {
        RunStep::NotStarted {
            why: "no resume here".to_owned(),
        }
    }

    fn answer_review(&mut self, _approve: bool, _sink: &dyn RunSink) -> RunStep {
        RunStep::NotStarted {
            why: "no review here".to_owned(),
        }
    }

    fn stopper(&mut self) -> Option<Arc<dyn RunStop>> {
        let stops = Arc::clone(self.stops.as_ref()?);
        Some(Arc::new(Counting(stops)))
    }
}

/// The project, its host, the door's stop count and the release that lets the run end.
type HeldRun = (
    tempfile::TempDir,
    SessionHost,
    Arc<AtomicUsize>,
    mpsc::Sender<()>,
);

/// A saved workflow whose « run it » is under way in a [`Holding`] door.
fn held_run(can_stop: bool, sealed: bool) -> HeldRun {
    let root = world();
    let (entered, held) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let stops = Arc::new(AtomicUsize::new(0));
    let door = Holding {
        entered,
        release: released,
        stops: can_stop.then(|| Arc::clone(&stops)),
        sealed,
    };
    let host = SessionHost::start(runtime(root.path()), Box::new(door), Vec::new()).expect("host");
    assert_eq!(kinds(&settle(&host, "c-1", COPY)), ["proposal"]);
    settle(&host, "c-2", "yes");
    assert!(matches!(
        host.dispatch(submit("c-3", &handle(&host), "run it")),
        Dispatch::Accepted { .. }
    ));
    held.recv_timeout(WAIT).expect("the run is under way");
    (root, host, stops, release)
}

/// A Stop while the turn's run executes reaches its door exactly once: the run is stopping, the
/// snapshot says a Stop was taken, the same Stop replayed answers its record, and a second Stop
/// sends nothing more (it never escalates). The run then sealed its trace as cancelled: its
/// observation says stopped, never aborted.
#[test]
fn a_stop_reaches_the_run_under_way_once_and_its_settlement_reads_stopped() {
    let (_root, host, stops, release) = held_run(true, true);
    let receipt = reply(host.dispatch(stop("s-1")));
    assert_eq!(receipt["receipt"], "run_stopping");
    assert_eq!(receipt["target"], "c-3");
    assert_eq!(receipt["snapshot"]["busy"]["phase"], "stopping");
    assert_eq!(receipt["snapshot"]["busy"]["stop_requested"], true);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    let replay = reply(host.dispatch(stop("s-1")));
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["event"], receipt["event"]);
    let second = reply(host.dispatch(stop("s-2")));
    assert_eq!(second["receipt"], "run_stopping");
    assert_eq!(
        stops.load(Ordering::SeqCst),
        1,
        "one door stop: a replay and a second Stop signal nothing"
    );
    release.send(()).expect("release");
    let settled = json(&host.wait_result("c-3").expect("settled"));
    assert_eq!(
        kinds(&settled),
        ["run_requested", "facts", "run_stopped"],
        "{settled}"
    );
    let run = &settled["snapshot"]["work"]["run"];
    assert_eq!(run["end"]["end"], "interrupted", "{run}");
    assert_eq!(run["sealed"], true, "{run}");
    assert_eq!(settled["snapshot"]["busy"], Value::Null);
}

/// The same Stop, but the run ended without sealing its trace: its observation says aborted,
/// never stopped, though both exits read 130.
#[test]
fn a_run_cut_after_its_stop_reads_aborted_not_stopped() {
    let (_root, host, stops, release) = held_run(true, false);
    assert_eq!(reply(host.dispatch(stop("s-1")))["receipt"], "run_stopping");
    release.send(()).expect("release");
    let settled = json(&host.wait_result("c-3").expect("settled"));
    assert_eq!(
        kinds(&settled),
        ["run_requested", "facts", "run_aborted"],
        "{settled}"
    );
    assert_eq!(settled["snapshot"]["work"]["run"]["sealed"], false);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

/// A Stop that reached the run reads stopped only when the run ended interrupted with its trace
/// sealed, aborted when it ended cut or crashed without sealing it, and nothing more when the run
/// reached an end of its own: its observation says that end, sealed or not.
#[test]
fn a_reached_stop_reads_by_the_runs_end_and_its_seal() {
    let read = |exit, sealed| match super::worker::stopped(exit, sealed) {
        Some(Outcome::RunStopped { .. }) => "stopped",
        Some(Outcome::RunAborted { .. }) => "aborted",
        Some(other) => panic!("not a Stop's outcome: {other:?}"),
        None => "its own end",
    };
    assert_eq!(read(130, true), "stopped");
    assert_eq!(read(130, false), "aborted");
    // A lane child killed by a signal, or one that panicked: cut without a seal.
    assert_eq!(read(3, false), "aborted");
    assert_eq!(read(101, false), "aborted");
    assert_eq!(read(3, true), "its own end");
    for exit in [0, 1, 4] {
        assert_eq!(read(exit, true), "its own end", "exit {exit}");
        assert_eq!(read(exit, false), "its own end", "exit {exit}");
    }
}

/// A door that cannot stop its run keeps the answer it always gave: the run is under way and no
/// Stop was taken for it.
#[test]
fn a_door_that_cannot_stop_its_run_answers_run_underway() {
    let (_root, host, stops, release) = held_run(false, true);
    let receipt = reply(host.dispatch(stop("s-1")));
    assert_eq!(receipt["receipt"], "run_underway");
    assert_eq!(receipt["snapshot"]["busy"]["phase"], "running");
    assert_eq!(receipt["snapshot"]["busy"]["stop_requested"], false);
    release.send(()).expect("release");
    let settled = json(&host.wait_result("c-3").expect("settled"));
    assert_eq!(kinds(&settled), ["run_requested", "facts"], "{settled}");
    assert_eq!(stops.load(Ordering::SeqCst), 0);
}

/// A Stop taken once the run is armed but before the native door spawns its child is applied at
/// the spawn: the door starts nothing (its binary does not even exist).
#[test]
fn a_stop_taken_before_the_native_door_spawns_is_applied_at_the_spawn() {
    let root = world();
    let door = LaneRunDoor::new(PathBuf::from("/nonexistent/nika-run-lane"));
    let host = SessionHost::start(runtime(root.path()), Box::new(door), Vec::new()).expect("host");
    assert_eq!(kinds(&settle(&host, "c-1", COPY)), ["proposal"]);
    settle(&host, "c-2", "yes");
    let gate = Gate::default();
    host.pause_at(gate.pause());
    gate.hold("running");
    assert!(matches!(
        host.dispatch(submit("c-3", &handle(&host), "run it")),
        Dispatch::Accepted { .. }
    ));
    gate.reached();
    let receipt = reply(host.dispatch(stop("s-1")));
    assert_eq!(receipt["receipt"], "stop_requested");
    assert_eq!(receipt["snapshot"]["busy"]["phase"], "running");
    assert_eq!(receipt["snapshot"]["busy"]["stop_requested"], true);
    gate.release();
    let settled = json(&host.wait_result("c-3").expect("settled"));
    assert_eq!(
        kinds(&settled),
        ["run_requested", "run_not_started"],
        "{settled}"
    );
    assert_eq!(
        settled["outcomes"][1]["text"],
        "a Stop arrived before this run started · nothing ran"
    );
}
