// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as bin_smoke: this suite's WHOLE JOB is to drive the real
// binary — here through a PTY, because the wizard is TTY-gated by
// construction (`is_terminal` on both ends) and therefore UNREACHABLE from
// every piped harness. Without a PTY the guided conversation ships with
// zero executable coverage (the tests-never-run gate-hole class).
#![allow(clippy::disallowed_types)]

//! PTY e2e — the guided onboarding conversation against the REAL binary
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
    env!("CARGO_BIN_EXE_nika-cli")
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
fn golden_path_lands_a_checked_workflow() {
    let dir = fresh_dir("golden");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["new"], true);

    p.expect("your first workflow").expect("header");
    p.expect("what should it do?").expect("q1");
    p.send_line("").expect("enter");
    p.expect("template `chain`").expect("routed to the default");
    p.expect("my-first.nika.yaml]").expect("file default shown");
    p.send_line("").expect("enter");
    p.expect("a number, or any provider/model")
        .expect("model menu");
    p.send_line("").expect("enter");
    p.expect("model `mock/echo`")
        .expect("offline default echoed");
    p.expect("stamped workflow `my-first`").expect("summary");
    // The wow contract: the audit ladder runs INSIDE the wizard.
    p.expect("audited").expect("embedded ladder");
    p.expect("scriptable form").expect("teaches its flags form");
    let m = p.expect(Eof).expect("conversation ends");
    assert!(
        !String::from_utf8_lossy(m.as_bytes()).contains("error"),
        "clean TAIL after the last anchor (the tee holds the full transcript)"
    );
    assert_eq!(exit_code(&mut p), 0, "spec §4 · success");

    // DEEP: the artifact stands on its own — a second binary run
    // re-audits the stamped file clean (own-corpus law end to end).
    let check = Command::new(bin())
        .args(["check", "my-first.nika.yaml"])
        .current_dir(dir)
        .output()
        .expect("check runs");
    assert_eq!(
        check.status.code(),
        Some(0),
        "the wizard's file re-checks clean: {}",
        String::from_utf8_lossy(&check.stdout)
    );
}

#[test]
fn intent_routes_and_the_file_is_stamped() {
    let dir = fresh_dir("intent");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["new"], true);

    p.expect("what should it do?").expect("q1");
    p.send_line("summarize every item in parallel")
        .expect("intent");
    p.expect("template `fanout`").expect("BM25 routes");
    p.expect("file ").expect("q2");
    p.send_line("review-batch").expect("custom name");
    p.expect("a number, or any provider/model").expect("q3");
    p.send_line("2").expect("menu pick");
    p.expect("routed intent → template `fanout`")
        .expect("summary says the routing");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0);

    // DEEP: the three stamps landed in the file itself.
    let written =
        std::fs::read_to_string(dir.join("review-batch.nika.yaml")).expect("file written");
    assert!(
        written.contains("nika: review-batch"),
        "the id is stamped on the envelope's one identity key"
    );
    assert!(written.contains("model: mock/echo"), "menu pick stamped");
}

#[test]
fn no_model_skeleton_completes_in_two_answers() {
    let dir = fresh_dir("permodel");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["new"], true);

    p.expect("what should it do?").expect("q1");
    p.send_line("gate-and-act").expect("exact template name");
    p.expect("template `gate-and-act`").expect("rung 1");
    p.expect("file ").expect("q2");
    p.send_line("").expect("enter");
    // The whole point: NO model question — the per-task truth instead,
    // then straight to the summary + ladder. Everything to Eof in one
    // capture so the absence is assertable.
    let m = p.expect(Eof).expect("ends");
    let transcript = String::from_utf8_lossy(m.as_bytes()).into_owned();
    assert!(
        transcript.contains("models are per-task in this skeleton"),
        "{transcript}"
    );
    assert!(
        !transcript.contains("a number, or any provider/model"),
        "the model question must not fire: {transcript}"
    );
    assert!(transcript.contains("models per-task"), "honest summary");
    assert!(transcript.contains("audited"), "ladder still runs");
    assert_eq!(exit_code(&mut p), 0);
}

#[test]
fn dest_hint_door_honors_the_given_name() {
    // The third door (V5 grammar): `nika new some-name.nika.yaml` bare on
    // a terminal — the extension marks a DESTINATION, not an intent. The
    // wizard runs with the GIVEN name as the file default, Enter keeps it.
    let dir = fresh_dir("hint");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["new", "team-standup.nika.yaml"], true);

    p.expect("what should it do?").expect("q1");
    p.send_line("").expect("enter");
    p.expect("[team-standup.nika.yaml]")
        .expect("the hint IS the default");
    p.send_line("").expect("enter");
    p.expect("a number, or any provider/model").expect("q3");
    p.send_line("").expect("enter");
    p.expect("stamped workflow `team-standup`")
        .expect("id from the hint");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0);
    assert!(dir.join("team-standup.nika.yaml").is_file());
}

#[test]
fn collision_walk_defaults_to_my_second() {
    let dir = fresh_dir("collide");
    let dir = dir.path();
    std::fs::write(dir.join("my-first.nika.yaml"), "taken").expect("seed");
    let mut p = spawn_pty(dir, &["new"], true);

    p.expect("what should it do?").expect("q1");
    p.send_line("").expect("enter");
    // The default walked past the taken name BEFORE asking.
    p.expect("my-second.nika.yaml]")
        .expect("collision-aware default");
    p.send_line("").expect("enter");
    p.expect("a number, or any provider/model").expect("q3");
    p.send_line("").expect("enter");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0);

    assert!(
        dir.join("my-second.nika.yaml").is_file(),
        "written next door"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("my-first.nika.yaml")).expect("read"),
        "taken",
        "the taken file is untouched"
    );
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
    // Then the writes + the proof + the panel.
    p.expect("created AGENTS.md").expect("scaffold report");
    p.expect("workflows/01-hello-chain.nika.yaml")
        .expect("the curriculum scaffolds");
    p.expect("proof").expect("the audit step announces itself");
    p.expect("audited").expect("the ladder ran");
    p.expect("ready").expect("the panel hands over");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0);

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
    p.expect("created AGENTS.md").expect("briefs land");
    p.expect("workflows/01-hello.nika.yaml")
        .expect("the lesson lands verbatim");
    p.expect("proof").expect("the audit step");
    p.expect("audited").expect("the ladder ran");
    p.expect("ready").expect("the panel hands over");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0);

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
}

#[test]
fn init_starter_recipe_hands_over_to_the_guided_flow() {
    let dir = fresh_dir("init-starter");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["init", "."], true);

    p.expect("recipe").expect("the recipe step");
    p.send_line("2").expect("2 = starter");
    p.expect("canvas").expect("no model question for starter");
    p.send_line("").expect("skip");
    p.expect("agents").expect("the wire step");
    p.send_line("").expect("skip");
    p.expect("created AGENTS.md").expect("scaffold report");
    // The hand-off: the SAME three-question flow `nika new` speaks.
    p.expect("your first workflow")
        .expect("the guided flow took over");
    p.send_line("").expect("intent Enter (chain)");
    p.expect("my-first.nika.yaml]").expect("file default");
    p.send_line("").expect("enter");
    p.expect("a number, or any provider/model")
        .expect("model menu");
    p.send_line("").expect("enter");
    p.expect("audited").expect("its ladder ran");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 0);

    assert!(dir.join("AGENTS.md").is_file(), "scaffold written");
    assert!(dir.join("my-first.nika.yaml").is_file(), "workflow written");
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

#[test]
fn eof_cancels_without_writing() {
    let dir = fresh_dir("cancel");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["new"], true);

    p.expect("what should it do?").expect("q1");
    // ^D = EOF mid-conversation: the human left. Cancel, never loop.
    p.send(ControlCode::EndOfTransmission).expect("^D");
    p.expect("cancelled — nothing written")
        .expect("honest cancel");
    p.expect(Eof).expect("ends");
    assert_eq!(exit_code(&mut p), 3, "spec §4 · environment (cancelled)");

    let leftovers: Vec<_> = std::fs::read_dir(dir)
        .expect("dir")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".nika.yaml"))
        .collect();
    assert!(leftovers.is_empty(), "nothing written: {leftovers:?}");
}

#[test]
fn colour_lives_on_a_terminal_and_no_color_kills_every_escape() {
    // Half 1 · a real PTY without NO_COLOR: the semantic accents exist
    // (the wizard resolves --color auto → TTY → on).
    let dir = fresh_dir("colour");
    let dir = dir.path();
    let mut p = spawn_pty(dir, &["new"], false);
    p.expect("what should it do?").expect("q1");
    p.send_line("").expect("enter");
    p.send_line("").expect("enter");
    p.send_line("").expect("enter");
    let m = p.expect(Eof).expect("ends");
    let coloured = String::from_utf8_lossy(m.as_bytes()).into_owned();
    assert!(
        coloured.contains("\u{1b}[36m"),
        "the single accent paints on a TTY"
    );
    assert!(coloured.contains("\u{1b}[2m"), "dim metadata paints");
    assert_eq!(exit_code(&mut p), 0);

    // Half 2 · NO_COLOR on the SAME terminal: zero escapes — the sober
    // register is a promise even where colour is possible.
    let dir2 = fresh_dir("nocolour");
    let dir2 = dir2.path();
    let mut q = spawn_pty(dir2, &["new"], true);
    q.expect("what should it do?").expect("q1");
    q.send_line("").expect("enter");
    q.send_line("").expect("enter");
    q.send_line("").expect("enter");
    let m2 = q.expect(Eof).expect("ends");
    let plain = String::from_utf8_lossy(m2.as_bytes()).into_owned();
    assert!(
        !plain.contains('\u{1b}'),
        "NO_COLOR strips every escape, even on a terminal"
    );
    assert_eq!(exit_code(&mut q), 0);
}
