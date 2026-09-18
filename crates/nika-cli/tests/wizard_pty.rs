// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as bin_smoke: this suite's WHOLE JOB is to drive the real
// binary — here through a PTY, because the wizard is TTY-gated by
// construction (`is_terminal` on both ends) and therefore UNREACHABLE from
// every piped harness. Without a PTY the bootstrap conversation ships with
// zero executable coverage (the tests-never-run gate-hole class).
#![allow(clippy::disallowed_types)]

//! PTY e2e — bootstrap conversation and noninteractive Compile against the binary
//! on a REAL pseudo-terminal (expectrl · unix-only; the CI runner is
//! linux, dev machines are macOS — both covered).
//!
//! Anchors are ANSI-SAFE on purpose: styled prompts carry escapes between
//! words and brackets (the seam paints defaults dim), so every `expect`
//! needle sits on a plain substring — the lesson the first manual expect
//! proofs paid for. Deep assertions go past exit codes: the stamped file
//! must re-`check` clean through a second binary invocation, cancel must
//! leave the directory empty, `NO_COLOR` must strip every escape even on
//! a terminal. Panic safety is structural: scenario dirs are RAII
//! (`tempfile`) and `ptyprocess::Drop` force-exits a still-alive child,
//! so a failing assertion leaks neither files nor processes.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{ControlCode, Eof, Expect};

/// The session type with its transcript teed to the test's stderr —
/// invisible on success (the harness captures it), PRICELESS on a CI
/// timeout: `ExpectTimeout` carries no buffer, so without the tee a
/// headless failure is undebuggable (rust-pro finding #3).
type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

/// RAII scenario dir — cleans itself even when an assertion PANICS
/// (the manual `remove_dir_all` version provably leaked orphan dirs on
/// the suite's first red run).
fn fresh_dir(tag: &str) -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix(&format!("nika-pty-{tag}-"))
        .tempdir()
        .expect("tmp dir")
}

/// Spawn `nika <args>` on a PTY in `dir`. `plain` = `NO_COLOR` (keeps the
/// transcript byte-assertable); one test drops it to prove colour lives.
///
/// The colour env is HERMETIC (nika#675): the ambient developer shell can
/// leak `NO_COLOR` / `CLICOLOR*` / `TERM` into the PTY child and silently
/// flip the wizard's `--color auto` resolution — the two halves must pin
/// the four knobs the resolver reads (`main.rs`), never inherit them.
fn spawn_pty(dir: &std::path::Path, args: &[&str], plain: bool) -> LoggedSession {
    let mut cmd = Command::new(bin());
    cmd.args(args).current_dir(dir);
    cmd.env_remove("NO_COLOR")
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR");
    cmd.env("TERM", "xterm-256color");
    if plain {
        cmd.env("NO_COLOR", "1");
    }
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

#[test]
fn compile_on_a_terminal_exposes_questions_without_a_wizard_or_file() {
    let room = fresh_dir("compile-questions");
    let mut p = spawn_pty(room.path(), &["compile", "chain"], true);
    let output = p.expect(Eof).expect("returns without stdin");
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(
        text.contains("Compile incomplete") && text.contains("--answer"),
        "{text}"
    );
    assert!(
        !text.contains("what should it do?"),
        "no conversational fork"
    );
    assert!(!text.contains('\u{1b}'), "NO_COLOR stays plain");
    assert_eq!(exit_code(&mut p), 2);
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn unsupported_intent_on_a_terminal_never_routes_to_a_substitute() {
    let room = fresh_dir("compile-intent");
    let mut p = spawn_pty(
        room.path(),
        &["compile", "summarize every item in parallel"],
        true,
    );
    let output = p.expect(Eof).expect("returns without answering anything");
    let text = String::from_utf8_lossy(output.as_bytes());
    assert!(text.contains("no substitute workflow"), "{text}");
    assert_eq!(exit_code(&mut p), 2);
    assert_eq!(std::fs::read_dir(room.path()).expect("dir").count(), 0);
}

#[test]
fn init_founding_wizard_golden_path_lands_the_curriculum() {
    let dir = fresh_dir("init-yes");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["init", "."], true);

    // The founding wizard: every question BEFORE the first write.
    p.expect("recipe").expect("the recipe step");
    p.expect("agentic").expect("the curriculum leads the menu");
    p.send_line("").expect("Enter = agentic (the golden path)");
    p.expect("a number, or any provider/model")
        .expect("model menu (the curriculum takes a model)");
    p.send_line("").expect("Enter = the offline mock");
    p.expect("canvas").expect("the canvas step");
    p.send_line("").expect("Enter = skip");
    p.expect("agents").expect("the wire step");
    p.send_line("").expect("Enter = skip");
    p.expect("project file").expect("the project-file beat");
    p.send_line("").expect("Enter = skip");
    // Then the writes + the proof + the panel.
    p.expect("created AGENTS.md").expect("scaffold report");
    p.expect("workflows/01-hello-chain.nika.yaml")
        .expect("the curriculum scaffolds");
    p.expect("proof").expect("the audit step announces itself");
    p.expect("audited").expect("the ladder ran");
    p.expect("ready").expect("the panel hands over");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0, "init accepts its taught draft class");

    // DEEP: the briefs AND the 4-pattern curriculum landed.
    assert!(dir.join("AGENTS.md").is_file(), "scaffold written");
    assert!(
        dir.join(".vscode/settings.json").is_file(),
        "wiring written"
    );
    for rel in [
        "workflows/01-hello-chain.nika.yaml",
        "workflows/02-parallel-fanout.nika.yaml",
        "workflows/03-gated-ship.nika.yaml",
        "workflows/04-agent-loop.nika.yaml",
    ] {
        assert!(dir.join(rel).is_file(), "{rel} written");
    }
}

/// The example lane end-to-end: pick 6 → slug → NO model question (a
/// lesson carries its own) → canvas/wire skips → the verbatim file +
/// generated index land, the proof ladder runs, the panel hands over.
#[test]
fn init_example_lane_founds_around_one_lesson() {
    let dir = fresh_dir("init-example");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["init", "."], true);

    p.expect("recipe").expect("the blueprint step");
    p.expect("start from one example")
        .expect("the example lane is offered");
    p.send_line("6").expect("6 = the example lane");
    p.expect("example slug").expect("the slug beat");
    p.expect("[01-hello]").expect("the Enter default is named");
    p.send_line("").expect("Enter = 01-hello");
    p.expect("example `01-hello`")
        .expect("the confirmation rail");
    // NO model question — straight to canvas.
    p.expect("canvas").expect("a lesson carries its own model");
    p.send_line("").expect("skip");
    p.expect("agents").expect("the wire step");
    p.send_line("").expect("skip");
    p.expect("project file").expect("the project-file beat");
    p.send_line("").expect("Enter = lay it (#1283)");
    p.expect("created AGENTS.md").expect("briefs land");
    p.expect("workflows/01-hello.nika.yaml")
        .expect("the lesson lands verbatim");
    p.expect("proof").expect("the audit step");
    p.expect("audited").expect("the ladder ran");
    p.expect("ready").expect("the panel hands over");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0, "init accepts its taught draft class");

    let body =
        std::fs::read_to_string(dir.join("workflows/01-hello.nika.yaml")).expect("lesson written");
    assert!(
        body.contains("nika: hello"),
        "verbatim example body: {body}"
    );
    assert!(
        dir.join("workflows/README.md").is_file(),
        "generated index written"
    );
    // The INTERACTIVE half of the project-file beat: #1283 flipped the
    // default — Enter lays the starter (the team file is part of a team
    // scaffold), `n` skips it (unit-tested in `nika-onboard::wizard`).
    assert!(
        dir.join("nika.yaml").is_file(),
        "Enter kept the project-file default — the starter is laid"
    );
}

#[test]
fn init_starter_recipe_hands_over_to_explicit_compile() {
    let dir = fresh_dir("init-starter");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["init", "."], true);

    p.expect("recipe").expect("the recipe step");
    p.send_line("2").expect("2 = starter");
    p.expect("canvas").expect("no model question for starter");
    p.send_line("").expect("skip");
    p.expect("agents").expect("the wire step");
    p.send_line("").expect("skip");
    p.expect("project file").expect("the project-file beat");
    p.send_line("").expect("Enter = skip");
    p.expect("created AGENTS.md").expect("scaffold report");
    p.expect("nika compile")
        .expect("explicit authoring handoff");
    p.expect(Eof).expect("ends without a second wizard");
    assert_eq!(exit_code(&mut p), 0);
    assert!(dir.join("AGENTS.md").is_file());
    assert!(!dir.join("my-first.nika.yaml").exists());
}

#[test]
fn init_cancel_at_the_first_question_writes_nothing() {
    let dir = fresh_dir("init-cancel");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["init", "."], true);

    p.expect("recipe").expect("the recipe step");
    // ^D = EOF at the FIRST question: the founding wizard must leave the
    // directory untouched (every question rides before the first write).
    p.send(ControlCode::EndOfTransmission).expect("^D");
    p.expect("cancelled — nothing written")
        .expect("honest cancel");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 3, "spec §4 · environment (cancelled)");
    assert!(!dir.join("AGENTS.md").exists(), "no partial scaffold");
    assert!(!dir.join(".vscode").exists(), "no partial wiring");
}
