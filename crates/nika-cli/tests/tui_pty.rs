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

fn exit_code<W: std::io::Write>(
    session: &mut Session<UnixProcess, LogStream<PtyStream, W>>,
) -> i32 {
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

// ── T6 · the terminal matrix ────────────────────────────────────────────

/// Everything the child wrote, for the assertions no `expect` can make
/// (no escape sequence · the restore at the end).
#[derive(Clone, Default)]
struct Tee(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);

impl std::io::Write for Tee {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().expect("tee").extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Tee {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().expect("tee")).into_owned()
    }

    /// Did the child write `raw`? The log stream escapes control bytes
    /// (`ESC` becomes `\u{1b}`), so both spellings are looked for.
    fn saw(&self, raw: &str) -> bool {
        let text = self.text();
        text.contains(raw) || text.contains(&raw.replace('\x1b', "\\u{1b}"))
    }
}

type TeeSession = Session<UnixProcess, LogStream<PtyStream, Tee>>;

/// The bare `nika --tui` command on a PTY in `project` with `home`.
fn tui_command(project: &Path, home: &Path, term: &str) -> Command {
    let mut cmd = Command::new(bin());
    cmd.arg("--tui")
        .current_dir(project)
        .env_remove("CLICOLOR")
        .env_remove("CLICOLOR_FORCE")
        .env("NO_COLOR", "1")
        .env("TERM", term)
        .env("HOME", home)
        .env("NIKA_KEYCHAIN", "off");
    cmd
}

/// Spawn on a PTY of `cols` × `rows` and tee everything the child writes.
/// The size is set before the renderer's first paint; a `SIGWINCH` the
/// child sees only makes it ask the cursor position once more, which the
/// opener answers as a terminal would.
fn spawn_sized(project: &Path, home: &Path, cols: u16, rows: u16) -> (TeeSession, Tee) {
    let mut session =
        OsSession::spawn(tui_command(project, home, "xterm-256color")).expect("pty spawn");
    session
        .get_process_mut()
        .set_window_size(cols, rows)
        .expect("window size");
    let tee = Tee::default();
    let mut session = expectrl::session::log(session, tee.clone()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(120)));
    (session, tee)
}

/// Answer cursor reports until `needle` shows: a terminal answers every
/// `ESC[6n`, whether the viewport asked at entry or after a resize.
fn answer_until(session: &mut TeeSession, tee: &Tee, rows: u16, needle: &str) {
    for _ in 0..4 {
        let found = match session.expect(expectrl::Any::boxed(vec![
            Box::new(CURSOR_QUERY),
            Box::new(needle.to_owned()),
        ])) {
            Ok(found) => found,
            Err(error) => panic!(
                "{error}: neither the cursor query nor `{needle}` came; the child wrote:\n{}",
                tee.text()
            ),
        };
        if found.get(0) == Some(CURSOR_QUERY.as_bytes()) {
            session
                .send(format!("\x1b[{rows};1R"))
                .expect("answer the report");
        } else {
            return;
        }
    }
    panic!("the viewport kept asking where the cursor is instead of drawing `{needle}`");
}

/// D · the renderer opens, helps and closes at three terminal sizes: the
/// laptop split (60×20), the classic (80×24), the wide (120×40). The help
/// card is committed above the viewport whole; the prompt comes back; the
/// terminal is restored on the way out (bracketed paste off, cursor shown).
#[test]
fn the_renderer_opens_helps_and_closes_at_three_sizes() {
    for (cols, rows) in [(60u16, 20u16), (80, 24), (120, 40)] {
        let (project, home) = rig(&format!("size-{cols}x{rows}"));
        let (mut session, tee) = spawn_sized(project.path(), home.path(), cols, rows);
        answer_until(&mut session, &tee, rows, "automate?");
        answer_until(&mut session, &tee, rows, "nika ›");
        session.send("/help\r").expect("help");
        // The card's last line names `/quit`; the prompt cell below it is
        // unchanged, so the diff renderer does not redraw it: assert on the
        // card, then leave.
        answer_until(&mut session, &tee, rows, "/quit");
        session.send("/quit\r").expect("quit");
        session.expect(Eof).expect("closes");
        assert_eq!(exit_code(&mut session), 0, "{cols}x{rows}");
        let text = tee.text();
        assert!(
            !text.contains("panicked"),
            "{cols}x{rows}: the renderer panicked:\n{text}"
        );
        assert!(
            tee.saw("\x1b[?2004l") && tee.saw("\x1b[?25h"),
            "{cols}x{rows}: bracketed paste off and the cursor shown at the end"
        );
    }
}

/// E · a resize while a proposal waits: the viewport re-anchors (it asks
/// the cursor position again), the consent prompt is drawn again at the
/// new size, a `non` discards, the prompt comes back, the terminal is
/// restored. Down to the laptop split, then up to the wide terminal.
#[test]
fn a_resize_while_a_proposal_waits_redraws_the_consent_prompt() {
    let (project, home) = rig("resize");
    std::fs::create_dir_all(project.path().join("notes")).expect("notes");
    std::fs::write(project.path().join("notes/brief.md"), "brief\n").expect("brief");
    let (mut session, tee) = spawn_sized(project.path(), home.path(), 80, 24);
    answer_until(&mut session, &tee, 24, "automate?");
    answer_until(&mut session, &tee, 24, "nika ›");
    session
        .send("Lis ./notes/brief.md et écris-le dans ./out/copie.md\r")
        .expect("the intent");
    answer_until(&mut session, &tee, 24, "compiled-workflow.nika");
    answer_until(&mut session, &tee, 24, "apply?");
    for (cols, rows) in [(60u16, 20u16), (120, 40)] {
        session
            .get_process_mut()
            .set_window_size(cols, rows)
            .expect("resize");
        // The inline viewport re-anchors on a fresh cursor report, then
        // draws the same waiting state at the new size.
        answer_until(&mut session, &tee, rows, "apply?");
    }
    session.send("non\r").expect("discard");
    answer_until(&mut session, &tee, 40, "nika ›");
    assert!(
        !project.path().join("compiled-workflow.nika").exists(),
        "a non writes nothing"
    );
    session.send("/quit\r").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    let text = tee.text();
    assert!(!text.contains("panicked"), "{text}");
    assert!(tee.saw("\x1b[?2004l"), "the terminal is restored");
}

/// F · `TERM=dumb` on a real terminal: the renderer refuses the terminal,
/// says so once, and the SAME session opens as the plain loop — no
/// cursor query, no CSI sequence, the human's question, the prompt.
#[test]
fn term_dumb_opens_the_plain_session_without_escape_sequences() {
    let (project, home) = rig("dumb");
    let session =
        OsSession::spawn(tui_command(project.path(), home.path(), "dumb")).expect("pty spawn");
    let tee = Tee::default();
    let mut session = expectrl::session::log(session, tee.clone()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(120)));
    session
        .expect("the plain session opens instead")
        .expect("the refusal is said once, with its reason");
    session
        .expect("What do you want to automate?")
        .expect("the same session, plain");
    session.expect("nika ›").expect("the plain prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    let text = tee.text();
    assert!(
        text.contains("TERM=dumb"),
        "the reason names the terminal: {text}"
    );
    assert!(
        !tee.saw(CURSOR_QUERY) && !tee.saw("\x1b["),
        "no cursor query and no CSI sequence on a dumb terminal: {text:?}"
    );
}

/// G · a terminal that never answers the cursor-position report (a
/// multiplexer pane in the wrong mode · a recorder · a serial console):
/// the renderer gives up after its wait, restores what it enabled, says
/// so once, and the plain session opens — never a frozen door.
#[test]
fn a_mute_cursor_report_falls_back_to_the_plain_session() {
    let (project, home) = rig("mute");
    let session = OsSession::spawn(tui_command(project.path(), home.path(), "xterm-256color"))
        .expect("pty spawn");
    let tee = Tee::default();
    let mut session = expectrl::session::log(session, tee.clone()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(120)));
    session
        .expect(CURSOR_QUERY)
        .expect("the inline viewport asks where the cursor is");
    // Nobody answers.
    let asked = Instant::now();
    session
        .expect("the plain session opens instead")
        .expect("the renderer gives up and says so");
    let waited = asked.elapsed();
    let _ = writeln!(
        std::io::stderr(),
        "[tui_pty] the mute cursor report was given up after {waited:?}"
    );
    assert!(
        waited < Duration::from_secs(15),
        "the wait for a cursor report is bounded: {waited:?}"
    );
    session
        .expect("What do you want to automate?")
        .expect("the same session, plain");
    session.expect("nika ›").expect("the plain prompt");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    let text = tee.text();
    assert!(
        tee.saw("\x1b[?2004l"),
        "what the renderer enabled before giving up is restored: {text:?}"
    );
}
