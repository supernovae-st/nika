// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![cfg(unix)]
#![allow(clippy::disallowed_types, clippy::expect_used, clippy::panic)]

//! PTY e2e — bare `nika` opens one local thread over the released workflow
//! surfaces. This test deliberately avoids a model turn: `/list`, `/workflow`,
//! and `/quit` prove the terminal skin without network or provider authority.

use std::process::Command;
use std::time::Duration;

use expectrl::process::unix::{PtyStream, UnixProcess, WaitStatus};
use expectrl::session::{OsSession, Session};
use expectrl::stream::log::LogStream;
use expectrl::{Eof, Expect};

type LoggedSession = Session<UnixProcess, LogStream<PtyStream, std::io::Stderr>>;

const WORKFLOW: &str = "nika: alpha\npermits: { exec: [\"echo\"] }\ntasks:\n  hello:\n    exec: { command: [\"echo\", \"hello\"] }\n";

fn exit_code(session: &mut LoggedSession) -> i32 {
    match session.get_process_mut().wait().expect("wait") {
        WaitStatus::Exited(_, code) => code,
        other => panic!("process did not exit cleanly: {other:?}"),
    }
}

#[test]
fn bare_tty_lists_and_posts_a_workflow_in_one_thread() {
    let dir = tempfile::Builder::new()
        .prefix("nika-thread-pty-")
        .tempdir()
        .expect("tmp dir");
    std::fs::write(dir.path().join("alpha.nika.yaml"), WORKFLOW).expect("workflow");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_nika-cli"));
    cmd.current_dir(dir.path())
        .env("NO_COLOR", "1")
        .env("TERM", "xterm-256color");
    let session = OsSession::spawn(cmd).expect("pty spawn");
    let mut session = expectrl::session::log(session, std::io::stderr()).expect("log tee");
    session.set_expect_timeout(Some(Duration::from_secs(30)));

    session.expect("nika · thread").expect("thread opens");
    session
        .expect("model xai/grok-4 · /help")
        .expect("model shown");
    session.expect("nika ›").expect("prompt");
    session.send_line("/list").expect("list");
    session.expect("alpha.nika.yaml").expect("workflow listed");
    session.expect("nika ›").expect("same thread");
    session
        .send_line("/workflow alpha.nika.yaml")
        .expect("post workflow");
    session.expect("alpha · 1 task").expect("workflow card");
    session.expect("nika ›").expect("thread remains open");
    session.send_line("/quit").expect("quit");
    session.expect(Eof).expect("thread closes");

    assert_eq!(exit_code(&mut session), 0);
}
