// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::expect_used, clippy::panic)]
// The suite drives the real binary through a PTY: the renderer refuses
// anything that is not a terminal (the carve-out of the other PTY suites).
#![allow(clippy::disallowed_types)]
//! The typeahead law on a real PTY: `yes⏎` typed while Nika works never
//! answers the decision the turn ends on. `nika-tui-proto --demo-pace` holds
//! each scripted turn busy; the keys typed meanwhile land in the box once the
//! question, the proposal or the gate is painted, a dim notice says so, and
//! only the human's own `Enter`, after the decision is on screen, sends them.
//!
//! The state is read from what the renderer draws for it: the prompts
//! `reply`, `answer` and `nika ›`, and for a proposal its hint row's last word
//! `/show` (the prompt `apply?` shares cells with `reply ›`, which the renderer
//! skips, so that word never arrives whole). A decision answered by
//! typeahead would draw the next state within the window awaited here. The
//! notice is committed before the decision's prompt is drawn, so it is
//! awaited first. Needles are words the frame writes whole.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, Tee>>;

/// A log sink that keeps every byte the process wrote, for the dumps.
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

/// How long each scripted turn stays busy.
const PACE_MS: &str = "900";
/// The dim notice that says where the typeahead went.
const NOTICE: &str = "it is in the box, not sent";
/// How long a decision is watched for an answer it must not get.
const WATCH: Duration = Duration::from_millis(1500);

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika-tui-proto")
}

/// Spawn the paced proto inline on an 80 × 24 PTY, answering the probe and
/// the viewport's cursor report, and wait for the free prompt.
fn spawn() -> (LoggedSession, Tee) {
    let mut cmd = Command::new(bin());
    cmd.args(["--demo-pace", PACE_MS])
        .env("TERM", "xterm-256color")
        .env("NO_COLOR", "1");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let tee = Tee::default();
    let mut session = expectrl::session::log(session, tee.clone()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
        .expect("\x1b[c")
        .expect("the terminal probe asks the device attributes");
    session.send("\x1b[?62;22c").expect("answer the attributes");
    session
        .expect("\x1b[6n")
        .expect("the inline viewport asks where the cursor is");
    session.send("\x1b[24;1R").expect("answer the report");
    expect_or_dump(&mut session, &tee, "nika ›", "the free prompt");
    (session, tee)
}

/// Expect `needle`; on failure, dump everything written.
fn expect_or_dump(session: &mut LoggedSession, tee: &Tee, needle: &str, why: &str) {
    if let Err(error) = session.expect(needle) {
        let log = String::from_utf8_lossy(&tee.0.lock().expect("tee")).into_owned();
        panic!("{why}: {error:?}\n--- output so far ---\n{log}");
    }
}

/// `needle` must not be drawn within [`WATCH`].
fn never(session: &mut LoggedSession, tee: &Tee, needle: &str, why: &str) {
    session.set_expect_timeout(Some(WATCH));
    let seen = session.expect(needle);
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    if seen.is_ok() {
        let log = String::from_utf8_lossy(&tee.0.lock().expect("tee")).into_owned();
        panic!("{why}\n--- output so far ---\n{log}");
    }
}

/// Let a freshly painted decision be read before a human answer is typed:
/// the harness answers what it sees, as a human does, never ahead of it.
fn read_it() {
    std::thread::sleep(Duration::from_millis(400));
}

/// Send `line` with `Enter`, then `yes⏎` while the turn it starts is busy.
fn send_then_type_ahead(session: &mut LoggedSession, line: &str) {
    session.send(format!("{line}\r")).expect("a line");
    std::thread::sleep(Duration::from_millis(150));
    session.send("yes\r").expect("yes typed while Nika works");
}

fn leave(session: &mut LoggedSession) {
    session.send("\x03").expect("first Ctrl+C");
    std::thread::sleep(Duration::from_millis(300));
    session.send("\x03").expect("second Ctrl+C");
    session.expect(Eof).expect("the process ends");
    let status = session.get_process_mut().wait().expect("wait");
    assert!(matches!(status, WaitStatus::Exited(_, 130)), "{status:?}");
}

/// A question: the typeahead lands in the box and the question waits; the
/// human's own `Enter` answers it with that very draft.
#[test]
fn yes_typed_while_a_question_is_prepared_never_answers_it() {
    let (mut session, tee) = spawn();
    send_then_type_ahead(&mut session, "digest my notes");
    expect_or_dump(&mut session, &tee, NOTICE, "the notice says where yes went");
    expect_or_dump(&mut session, &tee, "reply", "the question's prompt");
    never(
        &mut session,
        &tee,
        "/show",
        "the typeahead answered the question",
    );
    session.send("\r").expect("the human's own Enter");
    expect_or_dump(
        &mut session,
        &tee,
        "/show",
        "the draft, sent by the human, answered the question",
    );
    leave(&mut session);
}

/// A proposal: `yes⏎` typed while it is prepared never consents; the free
/// prompt comes back only after the human's own `Enter` on the draft.
#[test]
fn yes_typed_while_a_proposal_is_prepared_never_consents() {
    let (mut session, tee) = spawn();
    session.send("digest my notes\r").expect("an intent");
    expect_or_dump(&mut session, &tee, "reply", "the question");
    read_it();
    send_then_type_ahead(&mut session, "./notes/lundi.md");
    expect_or_dump(&mut session, &tee, NOTICE, "the notice says where yes went");
    expect_or_dump(&mut session, &tee, "/show", "the proposal's prompt");
    never(
        &mut session,
        &tee,
        "nika ›",
        "the typeahead consented to the proposal",
    );
    session.send("\r").expect("the human's own Enter");
    expect_or_dump(
        &mut session,
        &tee,
        "nika ›",
        "the human's yes, sent after the proposal was on screen, consented",
    );
    leave(&mut session);
}

/// A gate: `yes⏎` typed while the run reaches its gate never answers it.
#[test]
fn yes_typed_while_a_run_reaches_its_gate_never_answers_it() {
    let (mut session, tee) = spawn();
    session.send("digest my notes\r").expect("an intent");
    expect_or_dump(&mut session, &tee, "reply", "the question");
    read_it();
    session.send("./notes/lundi.md\r").expect("an answer");
    expect_or_dump(&mut session, &tee, "/show", "the proposal");
    read_it();
    session.send("yes\r").expect("the human's consent");
    expect_or_dump(&mut session, &tee, "nika ›", "saved");
    send_then_type_ahead(&mut session, "run it");
    expect_or_dump(&mut session, &tee, NOTICE, "the notice says where yes went");
    expect_or_dump(&mut session, &tee, "answer", "the gate's prompt");
    never(
        &mut session,
        &tee,
        "nika ›",
        "the typeahead answered the gate",
    );
    session.send("\r").expect("the human's own Enter");
    expect_or_dump(
        &mut session,
        &tee,
        "nika ›",
        "the human's answer, sent after the gate was on screen, settled the run",
    );
    leave(&mut session);
}
