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
    spawn_workflow(dir, GATED, &[])
}

fn spawn_workflow(dir: &std::path::Path, source: &str, args: &[&str]) -> LoggedSession {
    std::fs::write(dir.join("gate.nika.yaml"), source).expect("fixture written");
    let home = dir.join("home");
    std::fs::create_dir(&home).expect("isolated approval claim store");
    let mut cmd = Command::new(bin());
    cmd.args(["run", "gate.nika.yaml", "--no-gc"])
        .args(args)
        .current_dir(dir);
    cmd.env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR");
    cmd.env("TERM", "xterm-256color");
    cmd.env("NO_COLOR", "1");
    cmd.env("HOME", home);
    cmd.env_remove("NIKA_NO_TRACE_FILE");
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
/// its OWN execution/trace identity. Filename ordering does not establish
/// leg order, so assertions inspect all of them.
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

/// Every leg closes one verified journal under its own engine identity.
fn assert_leg_journals(journals: &[String], expected: usize) -> Vec<Vec<nika_event::Event>> {
    assert_eq!(journals.len(), expected, "one journal per execution leg");
    let mut identities = std::collections::HashSet::new();
    let mut legs = Vec::new();
    for journal in journals {
        assert!(matches!(
            nika_dap::chain::walk(journal),
            nika_dap::chain::Verdict::Intact { .. }
        ));
        let recovered = nika_dap::recover::recover_events(journal, "interactive leg")
            .expect("readable journal");
        assert!(recovered.truncated_note.is_none());
        let execution = recovered.events[0].execution.expect("engine execution ID");
        assert!(
            identities.insert(execution),
            "each leg has a fresh identity"
        );
        assert!(
            recovered
                .events
                .iter()
                .all(|event| event.execution == Some(execution)),
            "every projection in the journal names the same execution"
        );
        assert_eq!(
            recovered
                .events
                .iter()
                .filter(|event| event.is_terminal())
                .count(),
            1,
            "one terminal settlement per leg"
        );
        assert!(nika_event::settlement::RunSettlement::from_events(&recovered.events).is_some());
        legs.push(recovered.events);
    }
    legs
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
    let _ = assert_leg_journals(&journals, 2);
    assert!(
        journals
            .iter()
            .any(|j| j.contains("\"kind\":\"workflow_completed\"") && j.contains("went true")),
        "one journal carries the completed run + the bound answer:\n{}",
        journals.join("\n────\n")
    );
}

#[test]
fn two_sequential_gates_have_three_identities_and_journals_at_fixed_time() {
    let dir = fresh_dir("two-gates");
    let dir = dir.path();
    let source = r#"nika: sequential-gates
model: openai/gpt-5.2
run: { entropy: none, clock: virtual }
permits:
  tools: ["nika:prompt"]
tasks:
  first:
    invoke:
      tool: nika:prompt
      args: { mode: confirm, message: "first gate?" }
  second:
    with:
      approved: ${{ tasks.first.output }}
    when: ${{ with.approved == true }}
    invoke:
      tool: nika:prompt
      args: { mode: confirm, message: "second gate?" }
  after:
    with:
      approved: ${{ tasks.second.output }}
    when: ${{ with.approved == true }}
    infer: { prompt: "captured answer", max_tokens: 256 }
outputs:
  answer: ${{ tasks.after.output }}
"#;
    let mut p = spawn_workflow(dir, source, &["--model", "mock/echo", "--access", "mock"]);
    p.expect("first gate?").expect("first gate asks");
    p.expect("[y/N]").expect("first answer shape");
    std::fs::write(dir.join("gate.nika.yaml"), "not: the admitted workflow\n")
        .expect("replace the visible root between legs");
    p.send_line("y").expect("first answer");
    p.expect("second gate?").expect("second gate asks");
    p.expect("[y/N]").expect("second answer shape");
    std::fs::remove_file(dir.join("gate.nika.yaml")).expect("remove the visible root");
    p.send_line("y").expect("second answer");
    p.expect(Eof).expect("conversation ends");
    assert_eq!(exit_code(&mut p), 0, "both captured gates complete");

    let journals = trace_journals(dir);
    let legs = assert_leg_journals(&journals, 3);
    assert!(
        legs.iter()
            .all(|events| events[0].timestamp == legs[0][0].timestamp)
    );
    let mut paused = 0;
    let mut completed = 0;
    for events in &legs {
        paused += events
            .iter()
            .filter(|e| e.kind == nika_event::EventKind::WorkflowPaused)
            .count();
        completed += events
            .iter()
            .filter(|e| e.kind == nika_event::EventKind::WorkflowCompleted)
            .count();
        assert!(
            events
                .iter()
                .any(|e| e.str_field("model_override") == Some("mock/echo"))
        );
        assert!(
            events
                .iter()
                .any(|e| e.str_field("access_pin") == Some("mock"))
        );
        assert_eq!(
            nika_dap::resume::trace_access_lanes(events)
                .expect("recorded access")
                .get("mock/echo"),
            Some(&("mock".to_owned(), "mock".to_owned()))
        );
    }
    assert_eq!((paused, completed), (2, 1));
    assert!(journals.iter().any(|j| {
        j.contains("captured answer") && j.contains("\"kind\":\"workflow_completed\"")
    }));
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
