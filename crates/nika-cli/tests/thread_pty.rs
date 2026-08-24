// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::disallowed_types, clippy::expect_used, clippy::panic)]

//! PTY e2e — the first-wow cliquet: TTY and pipe are the SAME product
//! (the cascade screen). The thread lives at `nika thread`, hidden.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_nika-cli")
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

#[test]
fn tty_and_pipe_render_the_same_cascade() {
    let dir = tempfile::Builder::new()
        .prefix("nika-front-door-pty-")
        .tempdir()
        .expect("tmp dir");

    let piped = Command::new(bin())
        .current_dir(dir.path())
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(std::process::Stdio::null())
        .output()
        .expect("piped nika");
    assert_eq!(
        piped.status.code(),
        Some(0),
        "pipe is a greeting, not usage"
    );
    let pipe_text = output_text(&piped);
    assert!(
        pipe_text.contains("Local first"),
        "pipe shows the cascade: {pipe_text}"
    );
    assert!(
        !pipe_text.contains("nika · thread"),
        "pipe is not a thread: {pipe_text}"
    );
    assert!(
        !pipe_text.contains("xai/grok-4"),
        "grok-4 is dead as the default: {pipe_text}"
    );

    let mut cmd = Command::new(bin());
    cmd.current_dir(dir.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session
        .expect("Local first")
        .expect("TTY shows the cascade slogan");
    session
        .expect("Next:")
        .expect("TTY shows the same next as the pipe");
    session.expect(Eof).expect("TTY greeting exits");
    assert_eq!(exit_code(&mut session), 0);
}

#[test]
fn nika_thread_opens_the_hidden_thread() {
    let dir = tempfile::Builder::new()
        .prefix("nika-thread-verb-pty-")
        .tempdir()
        .expect("tmp dir");
    std::fs::write(
        dir.path().join("alpha.nika.yaml"),
        "nika: alpha\npermits: { exec: [\"echo\"] }\ntasks:\n  hello:\n    exec: { command: [\"echo\", \"hello\"] }\n",
    )
    .expect("workflow");

    let mut cmd = Command::new(bin());
    cmd.arg("thread")
        .current_dir(dir.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(30)));
    session.expect("nika · thread").expect("thread opens");
    session.expect("nika ›").expect("prompt");
    session.send_line("/list").expect("list");
    session.expect("alpha.nika.yaml").expect("workflow listed");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("thread closes");
    assert_eq!(exit_code(&mut session), 0);
}
