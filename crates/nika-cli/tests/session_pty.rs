// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::disallowed_types, clippy::expect_used, clippy::panic)]

//! PTY e2e — ADR-125 · the one door: bare `nika` on an interactive
//! terminal opens the native session; a pipe keeps the deterministic
//! concierge (exit 0); `nika thread` is gone (no alias). The session
//! answers a Nika fact without any model, writes nothing into the
//! project, and closes on `/quit`.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika")
}

fn exit_code(session: &mut LoggedSession) -> i32 {
    match session.get_process_mut().wait().expect("wait") {
        WaitStatus::Exited(_, code) => code,
        other => panic!("process did not exit cleanly: {other:?}"),
    }
}

fn output_text(out: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// A rig: a project with one workflow and a HOME of its own.
fn rig(name: &str) -> (tempfile::TempDir, tempfile::TempDir) {
    let project = tempfile::Builder::new()
        .prefix(&format!("nika-session-pty-{name}-"))
        .tempdir()
        .expect("project dir");
    std::fs::write(
        project.path().join("alpha.nika.yaml"),
        "nika: alpha\nmodel: mock/echo\ntasks:\n  hello:\n    infer: { prompt: hi, max_tokens: 10 }\n",
    )
    .expect("workflow");
    let home = tempfile::Builder::new()
        .prefix(&format!("nika-session-home-{name}-"))
        .tempdir()
        .expect("home dir");
    (project, home)
}

/// A pipe is the concierge, never the session; the TTY is the session.
#[test]
fn a_pipe_is_the_concierge_and_the_tty_is_the_session() {
    let (project, home) = rig("door");
    let piped = Command::new(bin())
        .current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("HOME", home.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("piped nika");
    assert_eq!(
        piped.status.code(),
        Some(0),
        "a pipe is a greeting, not usage"
    );
    let pipe_text = output_text(&piped);
    assert!(
        pipe_text.contains("Local first"),
        "the pipe shows the cascade: {pipe_text}"
    );
    assert!(
        !pipe_text.contains("nika · session"),
        "the pipe never opens a session: {pipe_text}"
    );
    assert!(
        !pipe_text.contains("Choose how Nika"),
        "the pipe never asks: {pipe_text}"
    );

    // The TTY, first run: the first screen, the choice « 4 » (no
    // conversational AI), the banner, a fact, the close.
    let mut cmd = Command::new(bin());
    cmd.current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home.path());
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
        .expect("Choose which AI answers your questions here")
        .expect("the first run asks");
    session
        .expect("No AI in this conversation")
        .expect("the fourth path");
    session.send_line("4").expect("choose");
    session.expect("nika · session").expect("the session opens");
    session
        .expect("no conversational AI")
        .expect("the banner names the path");
    session.expect("nika ›").expect("the prompt");
    session
        .send_line("what workflows are here?")
        .expect("a fact");
    session
        .expect("alpha.nika.yaml")
        .expect("the workflow listed, no model asked");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("the session closes");
    assert_eq!(exit_code(&mut session), 0);
    assert!(
        home.path()
            .join(".nika")
            .join("session-intelligence.json")
            .exists(),
        "the choice is kept under the home"
    );
    let entries: Vec<_> = std::fs::read_dir(project.path())
        .expect("project")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        entries,
        vec!["alpha.nika.yaml"],
        "nothing written into the project: {entries:?}"
    );
}

/// The second run never asks again: the kept choice opens the session.
#[test]
fn the_kept_choice_opens_the_session_without_asking() {
    let (project, home) = rig("kept");
    std::fs::create_dir_all(home.path().join(".nika")).expect("home dir");
    std::fs::write(
        home.path().join(".nika").join("session-intelligence.json"),
        "{\"kind\":{\"kind\":\"none\"},\"model\":null,\"chosen_at\":\"2026-09-03T00:00:00Z\"}",
    )
    .expect("preference");
    let mut cmd = Command::new(bin());
    cmd.current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color")
        .env("HOME", home.path());
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(60)));
    session
        .expect("nika · session")
        .expect("the session opens at once");
    session.send_line("/help").expect("help");
    session.expect("/intelligence").expect("the card");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("closes");
    assert_eq!(exit_code(&mut session), 0);
}

/// `nika thread` is gone: no alias, the parser's own refusal.
#[test]
fn nika_thread_is_an_unrecognized_subcommand() {
    let (project, home) = rig("thread");
    let out = Command::new(bin())
        .arg("thread")
        .current_dir(project.path())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .env("HOME", home.path())
        .stdin(std::process::Stdio::null())
        .output()
        .expect("nika thread");
    assert_eq!(
        out.status.code(),
        Some(2),
        "a mistyped verb is the parser's usage error"
    );
    let text = output_text(&out);
    assert!(
        text.contains("unrecognized subcommand") || text.contains("unexpected argument"),
        "{text}"
    );
    assert!(
        !text.contains("nika · thread") && !text.contains("nika · session"),
        "{text}"
    );
}
