// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// The suite's whole job is to drive the real binary through a PTY: the
// renderer refuses anything that is not a terminal, so no piped harness can
// reach it (the same carve-out as nika-cli's PTY suites).
#![allow(clippy::disallowed_types)]
//! The terminal lifecycle proof of ADR-139 (UX-1): `nika-tui-proto` runs on a
//! real PTY in both presentations over the same fixture, and on EVERY exit
//! path the terminal is handed back restored: the normal close, two
//! `Ctrl+C`, a panic inside the loop, `SIGTERM`. « Restored » is read from
//! the bytes the process wrote to the PTY, in order: after the last draw
//! the process must have sent the sequences that leave the alternate screen
//! (focus only), disable bracketed paste, disable focus reporting and show
//! the cursor; raw mode is the PTY's own line discipline, which crossterm
//! resets through `tcsetattr`, so its proof is the process leaving cleanly
//! with the expected exit code and the sequence set intact.
//!
//! What this suite does not prove: the pixels. The frames are judged by the
//! `TestBackend` tests of the crate; here only the contract with the
//! terminal is on trial.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, Tee>>;

/// A log sink that keeps every byte the process wrote, for the assertions.
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

const PASTE_OFF: &str = "\x1b[?2004l";
const FOCUS_OFF: &str = "\x1b[?1004l";
const CURSOR_SHOW: &str = "\x1b[?25h";
const ALT_OFF: &str = "\x1b[?1049l";
const CURSOR_QUERY: &str = "\x1b[6n";
/// The fixture's first question, by a token the diff renderer never splits
/// (a word with no space inside it).
const FIRST_QUESTION: &str = "const.source_path";

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika-tui-proto")
}

/// The inline viewport anchors itself on a cursor-position report
/// (`ESC[6n`); a real terminal answers, this harness must (row 24 of 24).
fn answer_cursor_report(session: &mut LoggedSession) {
    session
        .expect(CURSOR_QUERY)
        .expect("the inline viewport asks where the cursor is");
    session.send("\x1b[24;1R").expect("answer the report");
}

fn spawn(args: &[&str]) -> (LoggedSession, Tee) {
    let mut cmd = Command::new(bin());
    cmd.args(args)
        .env("TERM", "xterm-256color")
        .env("NO_COLOR", "1");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let tee = Tee::default();
    let mut session = expectrl::session::log(session, tee.clone()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    if !args.contains(&"--focus") {
        answer_cursor_report(&mut session);
    }
    session
        .expect("nika · session")
        .expect("the banner is the first thing drawn");
    session.expect("nika ›").expect("the free prompt");
    (session, tee)
}

fn wait(session: &mut LoggedSession) -> WaitStatus {
    session.expect(Eof).expect("the process ends");
    session.get_process_mut().wait().expect("wait")
}

/// Expect `needle` on the raw stream; on failure, dump everything the
/// process wrote so the miss can be read.
fn expect_or_dump(session: &mut LoggedSession, tee: &Tee, needle: &str, why: &str) {
    if let Err(error) = session.expect(needle) {
        panic!(
            "{why}: {error:?}\n--- output so far ---\n{:?}",
            written(tee)
        );
    }
}

/// Everything the process wrote, as raw bytes. The tee holds expectrl's
/// log, one line per chunk: `read: "…"` with the bytes Rust-escaped when
/// the chunk is valid UTF-8, `read:(bytes): [27, 91, …]` when a chunk
/// boundary split a multi-byte glyph (`›`, `·`, `─`). This undoes both
/// framings so the assertions can search exact escape sequences across
/// chunk boundaries.
fn written(tee: &Tee) -> String {
    let log_bytes = tee.0.lock().expect("tee").clone();
    let log = String::from_utf8_lossy(&log_bytes);
    let mut raw: Vec<u8> = Vec::new();
    for line in log.lines() {
        if let Some(rest) = line.strip_prefix("read: \"") {
            let inner = rest.strip_suffix('"').unwrap_or(rest);
            raw.extend_from_slice(unescape(inner).as_bytes());
        } else if let Some(rest) = line.strip_prefix("read:(bytes): [") {
            let inner = rest.strip_suffix(']').unwrap_or(rest);
            raw.extend(inner.split(',').filter_map(|n| n.trim().parse::<u8>().ok()));
        }
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('0') => out.push('\0'),
            Some('u') => {
                let mut hex = String::new();
                if chars.next() == Some('{') {
                    for h in chars.by_ref() {
                        if h == '}' {
                            break;
                        }
                        hex.push(h);
                    }
                }
                if let Some(ch) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                    out.push(ch);
                }
            }
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}

/// The bytes written after the last occurrence of `marker`.
fn tail_after(tee: &Tee, marker: &str) -> String {
    let text = written(tee);
    match text.rfind(marker) {
        Some(i) => text[i..].to_owned(),
        None => panic!("marker {marker:?} never drawn; output:\n{text:?}"),
    }
}

fn assert_restored(tail: &str, focus: bool) {
    assert!(
        tail.contains(PASTE_OFF),
        "bracketed paste left on:\n{tail:?}"
    );
    assert!(
        tail.contains(FOCUS_OFF),
        "focus reporting left on:\n{tail:?}"
    );
    assert!(tail.contains(CURSOR_SHOW), "cursor left hidden:\n{tail:?}");
    if focus {
        assert!(
            tail.contains(ALT_OFF),
            "alternate screen left on:\n{tail:?}"
        );
        let alt = tail.rfind(ALT_OFF).expect("alt off");
        let paste = tail.rfind(PASTE_OFF).expect("paste off");
        assert!(
            alt < paste,
            "the screen is left before the modes are dropped"
        );
    }
}

#[test]
fn the_normal_close_restores_the_inline_terminal() {
    let (mut session, tee) = spawn(&["--exit-after", "1"]);
    session
        .send("read ./notes and digest them\r")
        .expect("send");
    expect_or_dump(
        &mut session,
        &tee,
        FIRST_QUESTION,
        "the fixture's first question is committed above the viewport",
    );
    let status = wait(&mut session);
    assert!(matches!(status, WaitStatus::Exited(_, 0)), "{status:?}");
    let tail = tail_after(&tee, FIRST_QUESTION);
    assert_restored(&tail, false);
    assert!(tail.contains("left cleanly"), "{tail:?}");
}

#[test]
fn the_normal_close_restores_the_focus_terminal() {
    let (mut session, tee) = spawn(&["--focus", "--exit-after", "1"]);
    session
        .send("read ./notes and digest them\r")
        .expect("send");
    expect_or_dump(
        &mut session,
        &tee,
        FIRST_QUESTION,
        "the transcript on the alternate screen",
    );
    let status = wait(&mut session);
    assert!(matches!(status, WaitStatus::Exited(_, 0)), "{status:?}");
    let tail = tail_after(&tee, FIRST_QUESTION);
    assert_restored(&tail, true);
}

#[test]
fn two_control_c_while_idle_leave_and_restore() {
    let (mut session, tee) = spawn(&[]);
    session.send("\x03").expect("first Ctrl+C");
    expect_or_dump(
        &mut session,
        &tee,
        "Ctrl+C again leaves",
        "the first press arms and says so",
    );
    session.send("\x03").expect("second Ctrl+C");
    let status = wait(&mut session);
    assert!(matches!(status, WaitStatus::Exited(_, 130)), "{status:?}");
    assert_restored(&tail_after(&tee, "Ctrl+C again leaves"), false);
}

#[test]
fn a_panic_inside_the_loop_restores_before_the_message() {
    let (mut session, tee) = spawn(&["--panic-after", "1"]);
    session.send("boom\r").expect("send");
    expect_or_dump(
        &mut session,
        &tee,
        "panic requested after 1 line(s)",
        "the panic message reaches the terminal",
    );
    let status = wait(&mut session);
    assert!(matches!(status, WaitStatus::Exited(_, 101)), "{status:?}");
    let text = written(&tee);
    let message = text.rfind("panic requested").expect("message");
    let before = &text[..message];
    let paste_off = before.rfind(PASTE_OFF);
    let cursor = before.rfind(CURSOR_SHOW);
    assert!(
        paste_off.is_some() && cursor.is_some(),
        "the hook restores BEFORE the panic prints (paste_off {paste_off:?} · cursor {cursor:?} · message {message})"
    );
}

#[test]
fn sigterm_restores_and_leaves_with_143() {
    let (mut session, tee) = spawn(&["--focus"]);
    session.send("read ./notes\r").expect("send");
    expect_or_dump(&mut session, &tee, FIRST_QUESTION, "a turn happened");
    let pid = session.get_process().pid().as_raw();
    let sent = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .expect("kill runs");
    assert!(sent.success(), "kill -TERM {pid}: {sent}");
    let status = wait(&mut session);
    assert!(matches!(status, WaitStatus::Exited(_, 143)), "{status:?}");
    assert_restored(&tail_after(&tee, FIRST_QUESTION), true);
}

#[test]
fn a_pasted_yes_is_data_and_the_focus_switch_keeps_the_draft() {
    let (mut session, tee) = spawn(&[]);
    // Bracketed paste: the terminal wraps the text; the shell must insert
    // it, never act on the `yes` inside.
    session.send("\x1b[200~yes\n/quit\x1b[201~").expect("paste");
    session.send("\x14").expect("Ctrl+T · focus view");
    expect_or_dump(
        &mut session,
        &tee,
        "focus · Esc returns inline",
        "the focus status line",
    );
    session.send("\x1b").expect("Esc · back inline");
    answer_cursor_report(&mut session);
    session.expect("nika ›").expect("the inline prompt is back");
    session.send("\x03\x03").expect("leave");
    let status = wait(&mut session);
    assert!(matches!(status, WaitStatus::Exited(_, 130)), "{status:?}");
    let text = written(&tee);
    assert!(
        text.contains("/quit"),
        "the pasted text was drawn in the composer"
    );
    assert!(
        !text.contains("nika-tui-proto: left cleanly"),
        "a pasted yes never closed anything"
    );
    assert!(text.contains(ALT_OFF), "the focus view was left:\n{text:?}");
}

#[test]
fn a_pipe_is_refused_with_exit_2_and_no_escape_sequence() {
    let out = Command::new(bin())
        .env("TERM", "xterm-256color")
        .output()
        .expect("run on a pipe");
    assert_eq!(out.status.code(), Some(2));
    assert!(
        out.stdout.is_empty(),
        "{:?}",
        String::from_utf8_lossy(&out.stdout)
    );
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("not a terminal"), "{err}");
    assert!(
        !err.contains('\x1b'),
        "no escape sequence on a pipe: {err:?}"
    );
}
