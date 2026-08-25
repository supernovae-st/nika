// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! #1181 leg B — `nika lsp --clientProcessId` must outlive nothing.
//!
//! The unit tests in `nika-lsp::watchdog` prove the PREDICATE (a reaped
//! child is not alive). They cannot prove the predicate is WIRED: a
//! `process_is_alive` stubbed to `false` would still pass them, and so
//! would a `main.rs` that went back to dropping the flag. This file is
//! the witness for the wiring, and it runs the real binary.
//!
//! Leg A (a clean shutdown closes stdin, the server sees EOF and exits) is
//! why this defect stayed invisible for so long, so it is pinned here too:
//! a watchdog that killed the ordinary path would be a worse bug than the
//! one it fixes.
//!
//! Held open, never closed: the test owns the write end of the server's
//! stdin for the whole of `parent_death_ends_the_server`. That is the
//! condition the flag exists for — EOF never arrives — and without it the
//! server would exit for the ordinary reason and prove nothing.

#![cfg(unix)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// The point of this file is to run the SHIPPED binary as a user's editor
// would. A `ShellExecutor` seam would prove the seam, not the wiring.
#![allow(clippy::disallowed_types)]

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Generous: the watchdog polls on a 2s tick, CI runners stall, and a
/// flaky-red here would teach the next reader to distrust the gate.
const DEADLINE: Duration = Duration::from_secs(30);

fn nika() -> Command {
    Command::new(env!("CARGO_BIN_EXE_nika-cli"))
}

/// A process that will sit still until somebody kills it — the stand-in
/// for the editor that spawned us.
fn spawn_parent() -> Child {
    Command::new("sleep")
        .arg("600")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn the stand-in parent")
}

/// Wait for `child` to exit, or return `None` at the deadline.
fn wait_for_exit(child: &mut Child, deadline: Duration) -> Option<std::process::ExitStatus> {
    let start = Instant::now();
    while start.elapsed() < deadline {
        match child.try_wait().expect("poll the server") {
            Some(status) => return Some(status),
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    None
}

#[test]
fn parent_death_ends_the_server() {
    let mut parent = spawn_parent();
    let parent_pid = parent.id();

    let mut server = nika()
        .args(["lsp", "--stdio"])
        .arg(format!("--clientProcessId={parent_pid}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the language server");

    // The write end stays in scope for the whole test. Dropping it here
    // would close the pipe, the server would exit on EOF, and this test
    // would go green against a server with no watchdog at all.
    let held_stdin = server.stdin.take().expect("the server's stdin");

    // The server is now blocked in `initialize`, which never completes:
    // no client is speaking. That is deliberate — it pins the fix for the
    // host that dies DURING the handshake, which the first shape of this
    // patch (watchdog spawned after `initialize`) would have missed.
    parent.kill().expect("kill the declared parent");
    parent.wait().expect("reap the declared parent");

    let status = wait_for_exit(&mut server, DEADLINE);
    let exited = status.is_some();
    if !exited {
        let _ = server.kill();
        let _ = server.wait();
    }
    drop(held_stdin);
    assert!(
        exited,
        "the server outlived its declared parent for {DEADLINE:?} with stdin still open \
         — the #1181 orphan"
    );
    assert_eq!(
        status.and_then(|s| s.code()),
        Some(0),
        "a host that is gone is a normal end of session, not a failure"
    );
}

#[test]
fn a_closed_pipe_still_ends_the_server() {
    // Leg A: no flag, no parent to watch — the ordinary path must be
    // exactly as it was. This is the mutation guard on the watchdog
    // itself: a watchdog that fired unconditionally would still pass the
    // test above, and would fail this one.
    let mut server = nika()
        .args(["lsp", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the language server");

    let mut stdin = server.stdin.take().expect("the server's stdin");
    let _ = stdin.flush();
    drop(stdin); // EOF — the clean client shutdown.

    let status = wait_for_exit(&mut server, DEADLINE);
    if status.is_none() {
        let _ = server.kill();
        let _ = server.wait();
    }
    assert!(
        status.is_some(),
        "a closed pipe must still end the session (leg A unchanged)"
    );
}

#[test]
fn a_live_parent_keeps_the_server_up() {
    // The other side of the verdict. Without this, a watchdog that exited
    // on its first tick regardless of the parent would pass every test
    // above — the failure mode that turns a language server into one that
    // dies two seconds into every session.
    let mut parent = spawn_parent();

    let mut server = nika()
        .args(["lsp", "--stdio"])
        .arg(format!("--clientProcessId={}", parent.id()))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn the language server");
    let held_stdin = server.stdin.take().expect("the server's stdin");

    // Several watchdog ticks with the parent alive and the pipe open.
    std::thread::sleep(Duration::from_secs(7));
    let early = server.try_wait().expect("poll the server");

    let _ = server.kill();
    let _ = server.wait();
    drop(held_stdin);
    parent.kill().expect("kill the stand-in parent");
    parent.wait().expect("reap the stand-in parent");

    assert!(
        early.is_none(),
        "the server exited while its declared parent was alive: {early:?}"
    );
}
