// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as product_pty: this suite's WHOLE JOB is to drive the
// real binary through a PTY, because `nika --tui` takes the terminal only
// on a terminal (ADR-139) — unreachable from every piped harness.
#![allow(clippy::disallowed_types)]
//! The renderer's first five seconds (UX-2 · ADR-139): a human launches
//! `nika --tui`, the same session opens behind the inline viewport, a
//! sentence becomes a proposal committed above the composer, `oui` lands the
//! exact bytes and the real check runs, « run it » hands the terminal back
//! to the plain run path and the observation returns into the viewport,
//! `/quit` closes the door with the terminal restored. Keyless: the kept
//! choice is « no AI in this conversation », the compiler is the frozen
//! product base. The plain loop's own suites stay the law of bare `nika`.
//!
//! Two facts a real terminal supplies and this harness must: the inline
//! viewport anchors on a cursor-position report (`ESC[6n`, answered here
//! with row 24), and raw mode reads Enter as `\r`.

use std::io::Write as _;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

const CURSOR_QUERY: &str = "\x1b[6n";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

/// A project root and a home whose kept choice is « no AI in this
/// conversation »: authoring stays deterministic, no model is contacted.
fn rig(tag: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let project = tempfile::Builder::new()
        .prefix(&format!("nika-tui-{tag}-"))
        .tempdir()
        .expect("project dir");
    let home = tempfile::Builder::new()
        .prefix(&format!("nika-tui-home-{tag}-"))
        .tempdir()
        .expect("home dir");
    std::fs::create_dir_all(home.path().join(".nika")).expect("home .nika");
    std::fs::write(
        home.path().join(".nika").join("session-intelligence.json"),
        "{\"kind\":{\"kind\":\"none\"},\"model\":null,\"chosen_at\":\"2026-09-20T00:00:00Z\"}",
    )
    .expect("kept choice");
    (project, home)
}

/// The inline viewport asks where the cursor is; a terminal answers, so
/// does this harness (row 24 of 24).
fn answer_cursor_report(session: &mut LoggedSession) {
    session
        .expect(CURSOR_QUERY)
        .expect("the inline viewport asks where the cursor is");
    session.send("\x1b[24;1R").expect("answer the report");
}

/// `nika --tui` on a PTY in `project`, with `home` as the home. Returns the
/// session and the time the banner took to reach the terminal.
fn open_tui(project: &Path, home: &Path) -> (LoggedSession, Duration) {
    let mut cmd = Command::new(bin());
    cmd.arg("--tui")
        .current_dir(project)
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off");
    let started = Instant::now();
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(120)));
    answer_cursor_report(&mut session);
    session
        .expect("automate?")
        .expect("the banner (the human's question) is committed above the viewport");
    let first_paint = started.elapsed();
    session
        .expect("nika ›")
        .expect("the free prompt in the viewport");
    (session, first_paint)
}

fn exit_code(session: &mut LoggedSession) -> i32 {
    match session.get_process_mut().wait().expect("wait") {
        WaitStatus::Exited(_, code) => code,
        other => panic!("unexpected wait status: {other:?}"),
    }
}

/// The candidate the compiler returns for this intent through its own
/// machine document — the parity oracle for the bytes the session landed.
fn compiled_candidate(project: &Path, intent: &str) -> String {
    let out = Command::new(bin())
        .current_dir(project)
        .env("NIKA_KEYCHAIN", "off")
        .args(["compile", intent, "--json", "--fresh"])
        .output()
        .expect("nika compile");
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).expect("compile document");
    assert_eq!(doc["status"], "ready", "{doc}");
    doc["candidate"].as_str().expect("candidate").to_owned()
}

/// A · the product path behind the renderer: words → proposal → exact
/// bytes → check → run (the terminal handed back) → observation → quit.
#[test]
fn the_renderer_takes_a_sentence_to_a_file_and_gives_the_terminal_back() {
    let (project, home) = rig("copy-fr");
    let brief = "# Brief\n\nLe lancement passe en octobre. Budget : 12k.\n";
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), brief).expect("brief");
    let intent = "Lis ./notes/brief.md et écris-le dans ./out/copie.md";

    let (mut session, first_paint) = open_tui(project.path(), home.path());
    let _ = writeln!(
        std::io::stderr(),
        "[tui_pty] first paint after spawn: {first_paint:?}"
    );
    assert!(
        first_paint < Duration::from_secs(10),
        "the banner reached the terminal after {first_paint:?}"
    );

    session.send(format!("{intent}\r")).expect("the intent");
    session
        .expect("through")
        .expect("the busy state (« working through your words ») is drawn before the turn runs");
    session
        .expect("compiled-workflow.nika")
        .expect("the proposal names the file it would write");
    // The diff renderer skips the cell the two prompts share: assert on the
    // word, never on the whole prompt.
    session.expect("apply?").expect("the consent prompt");
    session.send("oui\r").expect("consent");
    session
        .expect("compiled-workflow.nika")
        .expect("the applied line names the file");
    session
        .expect("nika ›")
        .expect("back at the prompt: consent is never a run");

    // « run it »: the shell hands the terminal back, the plain run path
    // prints below the viewport, the renderer takes the terminal again (a
    // fresh inline viewport asks the cursor position) and the observation
    // is committed above the composer.
    session.send("run it\r").expect("the explicit run line");
    session
        .expect("$0.25")
        .expect("the ceiling is announced before the terminal is handed back");
    answer_cursor_report(&mut session);
    session
        .expect("observed")
        .expect("the observation returns into the viewport");
    session.expect("nika ›").expect("the prompt again");

    session.send("/quit\r").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);

    assert_eq!(
        std::fs::read_to_string(project.path().join("out/copie.md")).expect("the artefact"),
        brief,
        "the expected final artefact"
    );
    let landed = std::fs::read_to_string(project.path().join("compiled-workflow.nika"))
        .expect("the accepted workflow");
    assert_eq!(
        landed,
        compiled_candidate(project.path(), intent),
        "the bytes the session landed are the compiler's own candidate"
    );
}

/// C · `/help` answers from the engine, committed above the composer, and
/// the prompt comes back free; no busy state is drawn for a slash command.
#[test]
fn help_is_answered_by_the_engine_inside_the_viewport() {
    let (project, home) = rig("help");
    let (mut session, _first_paint) = open_tui(project.path(), home.path());
    session.send("/help\r").expect("help");
    session
        .expect("/intelligence")
        .expect("the help card names the intelligence door");
    // The prompt was `nika ›` before and after: the diff renderer sends
    // nothing for it, so the next door proves the session is still free.
    session.send("/quit\r").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
}

/// B · a pipe never reaches the renderer: `--tui` on a pipe is the
/// concierge, not a terminal owner, and prints no escape sequence.
#[test]
fn tui_on_a_pipe_keeps_the_concierge_and_writes_no_escape_sequence() {
    let (project, home) = rig("pipe");
    let out = Command::new(bin())
        .arg("--tui")
        .current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home.path())
        .env("NIKA_KEYCHAIN", "off")
        .output()
        .expect("run on a pipe");
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        !text.contains('\x1b'),
        "no escape sequence on a pipe: {text:?}"
    );
    assert!(!text.is_empty(), "the concierge still speaks on a pipe");
}
