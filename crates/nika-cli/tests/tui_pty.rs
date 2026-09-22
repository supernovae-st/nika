// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// Same carve-out as product_pty: this suite's WHOLE JOB is to drive the
// real binary through a PTY, because `nika --tui` takes the terminal only
// on a terminal (ADR-139) — unreachable from every piped harness.
#![allow(clippy::disallowed_types)]
//! The renderer's first five seconds (UX-2 · ADR-139): a human launches
//! `nika`, the same session opens behind the inline viewport, a
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
/// The renderer's terminal probe asks the primary device attributes
/// (`ESC[c`, crossterm's keyboard-enhancement check) before it asks where
/// the cursor is; a terminal answers at once, so does this harness — or
/// the probe waits its whole 2 s on every spawn and re-entry.
const DA_QUERY: &str = "\x1b[c";
const DA_ANSWER: &str = "\x1b[?62;22c";

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
        .expect(DA_QUERY)
        .expect("the terminal probe asks the device attributes");
    session.send(DA_ANSWER).expect("answer the attributes");
    session
        .expect(CURSOR_QUERY)
        .expect("the inline viewport asks where the cursor is");
    session.send("\x1b[24;1R").expect("answer the report");
}

/// Bare `nika` on a PTY in `project`, with `home` as the home. Returns the
/// session and the time the banner took to reach the terminal.
fn open_tui(project: &Path, home: &Path) -> (LoggedSession, Duration) {
    let mut cmd = Command::new(bin());
    cmd.current_dir(project)
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

    // « run it »: the run stays INSIDE the viewport — the door runs its
    // own machine lane as a child, each task line shows in the busy row,
    // the story and the observation are committed above the composer; the
    // terminal is never handed back (no cursor re-anchor to answer).
    session.send("run it\r").expect("the explicit run line");
    session
        .expect("$0.25")
        .expect("the ceiling is announced before the run");
    session
        .expect("write_output")
        .expect("the run's story names its tasks");
    session
        .expect("observed")
        .expect("the observation is committed into the viewport");
    // The free prompt was already there before the run and the diff
    // renderer never redraws an unchanged cell: the door is at the prompt.
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

/// The bare `nika` command on a PTY in `project` with `home`.
fn tui_command(project: &Path, home: &Path, term: &str) -> Command {
    let mut cmd = Command::new(bin());
    cmd.current_dir(project)
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
    spawn_sized_with(project, home, cols, rows, &[])
}

/// [`spawn_sized`] with extra environment (a seat's endpoint and key).
fn spawn_sized_with(
    project: &Path,
    home: &Path,
    cols: u16,
    rows: u16,
    env: &[(&str, &str)],
) -> (TeeSession, Tee) {
    let mut cmd = tui_command(project, home, "xterm-256color");
    for (key, value) in env {
        cmd.env(key, value);
    }
    let mut session = OsSession::spawn(cmd).expect("pty spawn");
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
    for _ in 0..6 {
        let found = match session.expect(expectrl::Any::boxed(vec![
            Box::new(DA_QUERY),
            Box::new(CURSOR_QUERY),
            Box::new(needle.to_owned()),
        ])) {
            Ok(found) => found,
            Err(error) => panic!(
                "{error}: neither the cursor query nor `{needle}` came; the child wrote:\n{}",
                tee.text()
            ),
        };
        if found.get(0) == Some(DA_QUERY.as_bytes()) {
            session.send(DA_ANSWER).expect("answer the attributes");
        } else if found.get(0) == Some(CURSOR_QUERY.as_bytes()) {
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
    assert!(
        tee.saw("· action required"),
        "the title says « action required » while the consent waits"
    );
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

// ── T8 · interruptions while a seat is called ───────────────────────────

/// A loopback endpoint that accepts and never answers: the seat call it
/// receives stalls until the door gives up (or the caller leaves). Returns
/// the base URL an openai-compatible route reads from `NIKA_OPENAI_BASE_URL`.
fn stall_server() -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a loopback port");
    let port = listener.local_addr().expect("addr").port();
    let _ = std::thread::Builder::new()
        .name("stall".to_owned())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = std::thread::Builder::new()
                    .name("stall-conn".to_owned())
                    .spawn(move || {
                        let mut sink = [0u8; 4096];
                        let _ = std::io::Read::read(&mut &stream, &mut sink);
                        std::thread::sleep(Duration::from_secs(120));
                        drop(stream);
                    });
            }
        });
    format!("http://127.0.0.1:{port}")
}

/// Drive the renderer to a seat call that stalls: the seat is `2 openai`
/// on the stalling endpoint, the line is a question only a seat answers.
fn stalled_turn(tag: &str) -> (TeeSession, Tee) {
    let (project, home) = rig(tag);
    let base = stall_server();
    let (mut session, tee) = spawn_sized_with(
        project.path(),
        home.path(),
        80,
        24,
        &[
            ("NIKA_OPENAI_BASE_URL", base.as_str()),
            ("OPENAI_API_KEY", "sk-test-stall"),
        ],
    );
    answer_until(&mut session, &tee, 24, "automate?");
    session.send("/intelligence\r").expect("the first screen");
    answer_until(&mut session, &tee, 24, "conversation");
    session
        .send("2 openai\r")
        .expect("a metered seat on the stalling endpoint");
    answer_until(&mut session, &tee, 24, "metered");
    session
        .send("Que penses-tu de ce projet ?\r")
        .expect("a line only a seat answers");
    answer_until(&mut session, &tee, 24, "through");
    std::mem::forget(project);
    std::mem::forget(home);
    (session, tee)
}

/// H · a seat call that never returns: one `Ctrl+C` warns in the busy row
/// (the call cannot be recalled), a second one leaves at once with the
/// terminal restored — never a wait for the call.
#[test]
fn ctrl_c_twice_leaves_a_stalled_seat_call_at_once() {
    let (mut session, tee) = stalled_turn("stall-ctrl-c");
    // Let the loader turn a few frames (one every 100 ms) before interrupting.
    std::thread::sleep(Duration::from_millis(350));
    let pressed = Instant::now();
    session.send("\x03").expect("Ctrl+C once");
    answer_until(&mut session, &tee, 24, "again");
    session.send("\x03").expect("Ctrl+C again");
    if let Err(error) = session.expect(Eof) {
        panic!(
            "the door did not leave ({error}); the child wrote:\n{}",
            tee.text()
        );
    }
    let left = pressed.elapsed();
    assert_eq!(exit_code(&mut session), 130, "left by an interruption");
    assert!(
        ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏']
            .iter()
            .filter(|g| tee.saw(&g.to_string()))
            .count()
            >= 2,
        "the loader turned while the call stalled:\n{}",
        tee.text()
    );
    assert!(
        left < Duration::from_secs(10),
        "the door left without waiting for the call: {left:?}"
    );
    assert!(
        tee.saw("\x1b[?2004l"),
        "the terminal is restored on the way out"
    );
}

/// I · `SIGTERM` while a seat call stalls: the door leaves at once with
/// 143, the terminal restored, the call left to die with the process.
#[test]
fn sigterm_during_a_stalled_seat_call_leaves_with_143() {
    let (mut session, tee) = stalled_turn("stall-sigterm");
    let pid = session.get_process().pid().to_string();
    let sent = Instant::now();
    let killed = Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .expect("kill");
    assert!(killed.success(), "the signal was sent");
    session.expect(Eof).expect("the door leaves");
    let left = sent.elapsed();
    assert_eq!(exit_code(&mut session), 143, "left by SIGTERM");
    assert!(
        left < Duration::from_secs(10),
        "the door left without waiting for the call: {left:?}"
    );
    assert!(
        tee.saw("\x1b[?2004l"),
        "the terminal is restored on the way out"
    );
}

/// J · a gated run inside the viewport: the run pauses headless at its
/// human gate, the gate view asks in the viewport with its own prompt,
/// the human's `y` resumes through the same lane, the effect happens
/// exactly once and the result says approved. No terminal handoff.
#[test]
fn a_gated_run_pauses_inside_the_viewport_and_the_answer_resumes_it() {
    let (project, home) = rig("gate-in-viewport");
    let draft = "the draft to publish\n";
    std::fs::write(project.path().join("draft.md"), draft).expect("draft");
    std::fs::write(
        project.path().join("approve.nika"),
        "nika: approve-then-write\npermits:\n  fs: { read: [\"./draft.md\"], write: [\"./final.md\"] }\n  tools: [\"nika:read\", \"nika:prompt\", \"nika:write\"]\ntasks:\n  read_draft:\n    invoke: { tool: \"nika:read\", args: { path: \"./draft.md\" } }\n  approve:\n    after: { read_draft: success }\n    invoke: { tool: \"nika:prompt\", args: { mode: confirm, message: \"Write final.md from the draft?\" } }\n  write_final:\n    after: { approve: success }\n    with: { go: \"${{ tasks.approve.output }}\", text: \"${{ tasks.read_draft.output }}\" }\n    when: \"${{ with.go == true }}\"\n    invoke: { tool: \"nika:write\", args: { path: \"./final.md\", content: \"${{ with.text }}\" } }\n",
    )
    .expect("gated workflow");
    let (mut session, tee) = spawn_sized(project.path(), home.path(), 100, 30);
    answer_until(&mut session, &tee, 30, "automate?");
    session.send("run approve.nika\r").expect("the run line");
    answer_until(&mut session, &tee, 30, "$0.25");
    answer_until(&mut session, &tee, 30, "Paused");
    assert!(
        !project.path().join("final.md").exists(),
        "nothing is written before the human answers"
    );
    answer_until(&mut session, &tee, 30, "answer");
    session
        .send("y\r")
        .expect("the human answers in the viewport");
    answer_until(&mut session, &tee, 30, "resuming");
    answer_until(&mut session, &tee, 30, "approved");
    answer_until(&mut session, &tee, 30, "observed");
    session.send("/quit\r").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert_eq!(
        std::fs::read_to_string(project.path().join("final.md")).expect("the effect"),
        draft
    );
    let traces: Vec<_> = std::fs::read_dir(project.path().join(".nika").join("traces"))
        .expect("traces")
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ndjson"))
        .collect();
    assert_eq!(
        traces.len(),
        2,
        "the pause and the resume each left a trace"
    );
    assert!(
        !tee.saw(CURSOR_QUERY.repeat(3).as_str()),
        "the terminal was never handed back around the run"
    );
}

/// K · `Tab` completes a slash command from the session's own list and
/// the completed line runs; the title names the project. T05 · a line
/// with an accent, an emoji and CJK goes through the composer and comes
/// back whole in the committed echo.
#[test]
fn tab_completes_a_slash_command_and_unicode_goes_through_whole() {
    let (project, home) = rig("polish");
    let (mut session, tee) = spawn_sized(project.path(), home.path(), 100, 30);
    answer_until(&mut session, &tee, 30, "automate?");
    assert!(
        tee.saw("\x1b]0;nika · "),
        "the terminal title names the project while the door is open"
    );
    session
        .send("/pro\t")
        .expect("a partial slash command and Tab");
    session.send("\r").expect("run the completed command");
    answer_until(&mut session, &tee, 30, "observed");
    session
        .send("héllo 🦋 日本語 ünïcödé\r")
        .expect("a line with wide and combined characters");
    // the committed echo: a wide glyph is its own cell run (the diff moves
    // the cursor after each), so the needle is the last word, then every
    // glyph of the line is looked for in what the child wrote.
    answer_until(&mut session, &tee, 30, "ünïcödé");
    for glyph in ["héllo", "🦋", "日", "本", "語", "ünïcödé"] {
        assert!(tee.saw(glyph), "`{glyph}` never came back:\n{}", tee.text());
    }
    answer_until(&mut session, &tee, 30, "conversational");
    session.send("/quit\r").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(!tee.text().contains("panicked"), "{}", tee.text());
    // The previous title was pushed before ours and popped at the close:
    // the shell's own title returns.
    // (the log stream escapes ESC as `\u{1b}`: both spellings are looked for)
    let text = tee.text();
    let find = |raw: &str| {
        text.find(raw)
            .or_else(|| text.find(&raw.replace('\x1b', "\\u{1b}")))
    };
    let rfind = |raw: &str| {
        text.rfind(raw)
            .or_else(|| text.rfind(&raw.replace('\x1b', "\\u{1b}")))
    };
    let pushed = find("\x1b[22;0t").expect("the title stack is pushed");
    let set = find("\x1b]0;nika · ").expect("the title is set");
    let popped = rfind("\x1b[23;0t").expect("the title stack is popped");
    assert!(
        pushed < set && set < popped,
        "push, set, pop in that order: {pushed} {set} {popped}"
    );
}

/// A run the door refuses before its first frame — here the cost floor
/// (`NIKA-1709`: a priced model under a ceiling the check on disk cannot
/// judge) — shows its reason inside the viewport, never a silent « run
/// observed · exit 2 », and the door stays. (A finding the session's own
/// check sees is named by the session before any lane starts.)
#[test]
fn a_refused_run_names_its_reason_inside_the_viewport() {
    let (project, home) = rig("refused-run");
    std::fs::write(
        project.path().join("floor.nika"),
        "nika: floor\nmodel: openai/gpt-5-mini\npermits: {}\ntasks:\n  t:\n    infer: { prompt: \"hi\", max_tokens: 5000 }\n",
    )
    .expect("a priced workflow");
    let (mut session, tee) = spawn_sized(project.path(), home.path(), 100, 30);
    answer_until(&mut session, &tee, 30, "automate?");
    session
        .send("run floor.nika with a ceiling of 0.000001\r")
        .expect("the run line with a ceiling under the floor");
    answer_until(&mut session, &tee, 30, "refused");
    answer_until(&mut session, &tee, 30, "NIKA-1709");
    answer_until(&mut session, &tee, 30, "observed");
    session.send("/quit\r").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(!tee.text().contains("panicked"), "{}", tee.text());
}
