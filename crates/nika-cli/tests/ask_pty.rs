// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as wizard_pty: this suite's WHOLE JOB is to drive the
// real binary through a PTY, because the terminal ask is TTY-gated by
// construction (`is_terminal` on stdin + stderr) and therefore
// UNREACHABLE from every piped harness.
#![allow(clippy::disallowed_types)]

//! PTY e2e — the terminal ask at a paused human gate (ADR-099
//! "interactively it asks") against the REAL binary on a REAL
//! pseudo-terminal (expectrl · unix-only; the CI runner is linux, dev
//! machines are macOS — both covered).
//!
//! The storyline this pins is the first-run gate (2026-07-31): a
//! default-less `nika:prompt` used to kill the run in milliseconds —
//! now the run pauses, teaches its resume line, asks the human, binds
//! the answer through the attested resume path, and completes. Walking
//! away (^D) leaves the durable pause (exit 4) instead of a failure.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{ControlCode, Eof, Expect};

/// The session type with its transcript teed to the test's stderr —
/// invisible on success, priceless on a CI timeout (the `wizard_pty`
/// lesson: `ExpectTimeout` carries no buffer).
type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

/// RAII scenario dir (panic-safe cleanup — the `wizard_pty` precedent).
fn fresh_dir(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("nika-ask-pty-{tag}-"))
        .tempdir()
        .expect("tmp dir")
}

/// One default-less confirm gate feeding one downstream task — the
/// smallest workflow whose first run crosses a human gate. The route is
/// AFFIRMATIVE (bound answer + `when: == true` · NIKA-SEC-014): a `no`
/// fires exactly zero effects, so the fixture practices the law the
/// check teaches.
const GATED: &str = "nika: gate-ask\npermits:\n  exec: [\"echo\"]\n  tools: [\"nika:prompt\"]\ntasks:\n  ask:\n    invoke:\n      tool: \"nika:prompt\"\n      args: { mode: confirm, message: \"deploy the thing?\" }\n  after:\n    with:\n      approved: ${{ tasks.ask.output }}\n    when: ${{ with.approved == true }}\n    exec: { command: [\"echo\", \"went\", \"${{ with.approved }}\"] }\n";

/// Spawn `nika run gate.nika.yaml` on a PTY in `dir` — colour env
/// hermetic (the nika#675 lesson), `NO_COLOR` pinned so every `expect`
/// needle sits on plain bytes.
fn spawn_gate_run(dir: &std::path::Path) -> LoggedSession {
    std::fs::write(dir.join("gate.nika.yaml"), GATED).expect("fixture written");
    let mut cmd = Command::new(bin());
    cmd.args(["run", "gate.nika.yaml"]).current_dir(dir);
    cmd.env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR");
    cmd.env("TERM", "xterm-256color");
    cmd.env("NO_COLOR", "1");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
}

/// Exit code of the finished session (unix wait status).
fn exit_code(session: &mut LoggedSession) -> i32 {
    match session.get_process_mut().wait().expect("wait") {
        WaitStatus::Exited(_, code) => code,
        other => panic!("process did not exit cleanly: {other:?}"),
    }
}

/// Every trace journal the scenario wrote, as text. An ask leg writes
/// its OWN trace (pause journal · then the resumed leg's) — two births
/// in the same second differ only by the random name tail, so picking
/// "the newest by name" elected the WRONG file (the first red run's
/// lesson). Assertions scan ALL of them instead.
fn trace_journals(dir: &std::path::Path) -> Vec<String> {
    let traces = dir.join(".nika/traces");
    let mut bodies: Vec<String> = std::fs::read_dir(&traces)
        .expect("trace dir")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ndjson"))
        .map(|p| std::fs::read_to_string(&p).expect("trace read"))
        .collect();
    bodies.sort();
    assert!(!bodies.is_empty(), "a trace was written");
    bodies
}

#[test]
fn the_gate_asks_on_a_terminal_and_the_answer_completes_the_run() {
    let dir = fresh_dir("answered");
    let dir = dir.path();
    let mut p = spawn_gate_run(dir);

    // The run parks first — durable pause, taught resume line — THEN asks.
    p.expect("paused").expect("the paused card speaks");
    p.expect("resume: nika run gate.nika.yaml")
        .expect("the durable escape is taught before the ask");
    p.expect("deploy the thing?")
        .expect("the gate's own message");
    p.expect("[y/N]").expect("confirm speaks its answer shape");
    p.send_line("y").expect("the human answers");
    // The in-process resume: the gate binds, downstream sees the value.
    p.expect("ran live")
        .expect("the resume summary closes the loop");
    p.expect(Eof).expect("conversation ends");
    assert_eq!(exit_code(&mut p), 0, "answered gate completes the run");

    // DEEP: the answered leg's journal exists — the bound answer reached
    // downstream and the run completed (the pause leg's journal sits
    // beside it; both are honest).
    let journals = trace_journals(dir);
    assert!(
        journals
            .iter()
            .any(|j| j.contains("\"kind\":\"workflow_completed\"") && j.contains("went true")),
        "one journal carries the completed run + the bound answer:\n{}",
        journals.join("\n────\n")
    );
}

#[test]
fn walking_away_leaves_the_durable_pause() {
    let dir = fresh_dir("escape");
    let dir = dir.path();
    let mut p = spawn_gate_run(dir);

    p.expect("[y/N]").expect("the ask fired");
    // ^D = the human left. The pause stands — never a failure, never a loop.
    p.send(ControlCode::EndOfTransmission).expect("^D");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 4, "run state paused · exit 4");

    // DEEP: the journal carries the pause, and NO journal carries a
    // failure — the walk-away is resumable later.
    let journals = trace_journals(dir);
    assert!(
        journals
            .iter()
            .any(|j| j.contains("\"kind\":\"workflow_paused\"")),
        "{}",
        journals.join("\n────\n")
    );
    assert!(
        journals
            .iter()
            .all(|j| !j.contains("\"kind\":\"workflow_failed\"")),
        "a pause is never a failure:\n{}",
        journals.join("\n────\n")
    );
}
