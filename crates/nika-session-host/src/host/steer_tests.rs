// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A line for the conversation's run under way, through the host: queued with its identity
//! while that run reads its queue, refused truthfully otherwise, its receipt logged and bound to
//! its command identity like a Stop's, and the queue shown in the busy snapshot. The real
//! Session over a temporary project; the run's reading is held open by the test, standing for
//! the conversation's run, while the worker is held inside a turn.

use std::path::Path;

use nika_session::intelligence::{
    IntelligenceCensus, IntelligenceKind, ResolvedSessionIntelligence, UserIntelligencePreference,
};
use nika_session::steer::{QueuedState, Steering};
use nika_session::{ScriptedReasoner, SessionReasoner, SessionRuntime};
use serde_json::Value;

use super::tests::{COPY, Gate, world};
use super::*;
use crate::run::NoRunDoor;

/// The Session a host door opens, its conversation driver kept (its reasoner never asked).
fn led(root: &Path) -> SessionRuntime {
    let mut census = IntelligenceCensus::empty();
    census.locals.push("ollama".to_owned());
    let local = IntelligenceKind::Local {
        provider: "ollama".to_owned(),
    };
    let preference = UserIntelligencePreference::new(local, None);
    let factory = Box::new(
        |_: &ResolvedSessionIntelligence| -> Box<dyn SessionReasoner> {
            Box::new(ScriptedReasoner::new(Vec::new()))
        },
    );
    let mut runtime = SessionRuntime::open_with(root, census, &preference, None, factory);
    runtime.enable_continuous_preparation();
    runtime
}

fn start(runtime: SessionRuntime) -> SessionHost {
    SessionHost::start(runtime, Box::new(NoRunDoor::new("none")), Vec::new()).expect("host")
}

fn line(op: &str, command: &str, line: &str) -> Command {
    let (command, line) = (command.to_owned(), line.to_owned());
    if op == "steer" {
        Command::Steer { command, line }
    } else {
        Command::FollowUp { command, line }
    }
}

fn logged(dispatch: Dispatch) -> Value {
    match dispatch {
        Dispatch::Logged(frame) | Dispatch::Reply(frame) => {
            serde_json::to_value(&frame).expect("frame")
        }
        other => panic!("not a logged receipt: {other:?}"),
    }
}

fn handle(host: &SessionHost) -> String {
    let snapshot = serde_json::to_value(host.snapshot()).expect("snapshot");
    snapshot["snapshot"]["snapshot"]
        .as_str()
        .expect("handle")
        .to_owned()
}

/// Submit `COPY` and hold the worker once the turn returned, still preparing.
fn held_turn(host: &SessionHost, gate: &Gate) {
    host.pause_at(gate.pause());
    gate.hold("returned");
    let submit = Command::Submit {
        command: "c-1".to_owned(),
        snapshot: handle(host),
        line: COPY.to_owned(),
    };
    assert!(matches!(host.dispatch(submit), Dispatch::Accepted { .. }));
    gate.reached();
}

#[test]
fn a_line_with_no_turn_under_way_is_nothing_to_steer_and_replays_its_receipt() {
    let root = world();
    let host = start(led(root.path()));
    let receipt = logged(host.dispatch(line("steer", "s-1", "use b instead")));
    assert_eq!(receipt["frame"], "result");
    assert_eq!(receipt["op"], "steer");
    assert_eq!(receipt["receipt"], "nothing_to_steer");
    assert_eq!(receipt["target"], Value::Null);
    assert_eq!(receipt["replayed"], false);
    assert!(receipt["event"].as_u64().is_some(), "the receipt is logged");
    let again = logged(host.dispatch(line("steer", "s-1", "use b instead")));
    assert_eq!(
        (again["replayed"].clone(), again["event"].clone()),
        (Value::Bool(true), receipt["event"].clone())
    );
    let other = logged(host.dispatch(line("steer", "s-1", "other words")));
    assert_eq!(other["error"], "command_conflict");
}

#[test]
fn a_line_for_a_turn_no_conversation_run_reads_is_not_reading() {
    let root = world();
    let host = start(led(root.path()));
    let gate = Gate::default();
    held_turn(&host, &gate);
    let receipt = logged(host.dispatch(line("follow_up", "f-1", "and c")));
    assert_eq!(receipt["receipt"], "not_reading");
    assert_eq!(receipt["target"], "c-1");
    assert_eq!(receipt["snapshot"]["busy"]["command"], "c-1");
    assert_eq!(receipt["snapshot"]["busy"].get("queued"), None);
    gate.release();
    assert!(host.wait_result("c-1").is_some());
}

#[test]
fn a_line_while_the_conversation_reads_is_queued_with_its_identity_and_shown_busy() {
    let root = world();
    let runtime = led(root.path());
    let steering: Steering = runtime
        .steering()
        .expect("the host door keeps the conversation");
    let host = start(runtime);
    let gate = Gate::default();
    held_turn(&host, &gate);
    // The conversation's run reads its queue while it runs; the test stands for it.
    steering.open();
    let steered = logged(host.dispatch(line("steer", "s-1", "use b instead")));
    assert_eq!(steered["receipt"], "queued");
    assert_eq!(steered["target"], "c-1");
    assert_eq!(
        steered["queued"],
        serde_json::json!({"id": "l1", "mode": "steer", "line": "use b instead",
            "state": "waiting"})
    );
    let followed = logged(host.dispatch(line("follow_up", "f-1", "and c")));
    assert_eq!(followed["queued"]["id"], "l2");
    let blank = logged(host.dispatch(line("steer", "s-2", "   ")));
    assert_eq!(blank["receipt"], "blank");
    let busy = serde_json::to_value(host.snapshot()).expect("snapshot")["snapshot"]["busy"].clone();
    let queued: Vec<&str> = (busy["queued"]
        .as_array()
        .expect("the queue under way")
        .iter())
    .filter_map(|q| q["id"].as_str())
    .collect();
    assert_eq!(queued, ["l1", "l2"]);
    steering.close();
    let records = steering.records();
    let returned = records.iter().filter(|q| q.state == QueuedState::Returned);
    assert_eq!(
        returned.count(),
        2,
        "the lines no run read come back unsent"
    );
    gate.release();
    assert!(host.wait_result("c-1").is_some());
}
