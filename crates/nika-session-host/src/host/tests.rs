// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The host's custody, keyless: the real Session over a temporary project, its reasoner never
//! asked (the intent reaches the deterministic compiler), the worker held at named points so
//! each race is decided, not hoped for.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::{ScriptedReasoner, SessionRuntime};
use serde_json::Value;

use super::*;
use crate::run::{NoRunDoor, RunDoor, RunRequest, RunSink, RunStep};
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
