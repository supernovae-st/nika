// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The parent watchdog — LSP `initialize` §`processId`.
//!
//! > *"If the parent process is not alive then the server should exit its
//! > process."*
//!
//! Hosts declare their PID two ways: on argv (`--clientProcessId`, the
//! vscode-languageclient / nvim / helix convention) and in the
//! `initialize` params. Both were accepted and neither was read (#1181):
//! `Command::Lsp { .. }` discarded the flag and `run_stdio()` took no
//! argument.
//!
//! The ordinary shutdown does not need this. A client that closes the
//! pipe sends EOF and the transport ends the loop — which is why the
//! gap stayed invisible. The case the flag exists for is the client that
//! dies **without** closing stdio (a crash, a `SIGKILL`): EOF never
//! arrives and the server outlives its host forever.
//!
//! ## Why this exits the process rather than unwinding
//!
//! Returning cleanly from the message loop cannot work here. The reader
//! IO thread is parked on a `read()` that will never return — that is
//! the defining condition of this failure — so `io_threads.join()` would
//! block exactly as [`crate::server::run_stdio`] documents. Exiting the
//! process is both the only thing that ends a blocked read and the
//! literal instruction of the specification.

use std::time::Duration;

/// How often the parent is polled. Small enough that an editor restart
/// does not leave a stale server holding a workspace, large enough to be
/// free: one syscall per tick.
pub(crate) const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Is `pid` still a live process?
///
/// Unix: `kill(pid, 0)` — the existence probe, no signal delivered.
/// `EPERM` counts as **alive**: the process exists, it is merely owned by
/// somebody else. Only `ESRCH` (no such process) means gone.
///
/// Non-unix targets have no portable equivalent here, so the parent is
/// reported alive and the watchdog never fires. Every shipped target is
/// unix (`release.yml` builds macOS + Linux only); this arm exists so the
/// crate still compiles for `wasm32`.
#[must_use]
pub(crate) fn process_is_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        let Ok(raw) = i32::try_from(pid) else {
            // Not a PID this platform can express — nothing to watch.
            return true;
        };
        !matches!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(raw), None),
            Err(nix::errno::Errno::ESRCH)
        )
    }
    #[cfg(not(unix))]
    {
        let _ = pid;
        true
    }
}

/// Watch `pid` on a detached thread and end this process when it dies.
///
/// A PID of 0 is ignored: it is the "no parent" value hosts send when
/// they do not want to be watched, and on unix `kill(0, …)` addresses the
/// caller's own process group — the one probe that must never be made.
pub(crate) fn spawn(pid: u32, interval: Duration) {
    if pid == 0 {
        return;
    }
    // Detached on purpose: the thread must outlive nothing and be joined
    // by nobody — the server's lifetime is the process's lifetime.
    //
    // `tokio::spawn` is the workspace default and cannot apply here: this
    // crate is deliberately sync (`server.rs` — "No async, no tokio").
    // Pulling a runtime in to own one sleeping poll loop would be a far
    // larger change than the defect it serves.
    #[allow(clippy::disallowed_methods)] // sync crate by design · no tokio runtime to spawn onto
    std::thread::spawn(move || {
        while process_is_alive(pid) {
            std::thread::sleep(interval);
        }
        // A host that is gone is a normal end of session, not a failure.
        std::process::exit(0);
    });
}

/// The PID this server should watch, given the flag and the `initialize`
/// params, in that order of precedence.
///
/// argv wins because a host that bothered to pass `--clientProcessId`
/// named the process it wants watched; `processId` in the payload is the
/// protocol's own channel and covers every host that sends only that.
/// `processId: null` is the spec's "no parent" and yields `None`.
#[must_use]
pub(crate) fn declared_parent(flag: Option<u32>, init_params: &serde_json::Value) -> Option<u32> {
    flag.or_else(|| {
        init_params
            .get("processId")
            .and_then(serde_json::Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    })
    .filter(|pid| *pid != 0)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::disallowed_types)] // a real child process is the only honest fixture for a liveness probe

    use super::*;
    use serde_json::json;

    #[test]
    fn this_process_is_alive() {
        assert!(process_is_alive(std::process::id()));
    }

    #[cfg(unix)]
    #[test]
    fn a_reaped_child_is_not_alive() {
        // Spawn, kill, WAIT — the wait is what makes this deterministic:
        // an unreaped child is a zombie and a zombie still answers
        // `kill(pid, 0)`. Without the reap this test would flake green
        // against a broken predicate.
        let mut child = std::process::Command::new("sleep")
            .arg("30")
            .spawn()
            .expect("spawn a child to kill");
        let pid = child.id();
        assert!(process_is_alive(pid), "alive before the kill");
        child.kill().expect("kill the child");
        child.wait().expect("reap the child");
        assert!(!process_is_alive(pid), "dead and reaped after the kill");
    }

    #[test]
    fn pid_zero_is_never_watched() {
        assert_eq!(declared_parent(Some(0), &json!({})), None);
        assert_eq!(declared_parent(None, &json!({ "processId": 0 })), None);
    }

    #[test]
    fn the_flag_wins_over_the_payload() {
        let params = json!({ "processId": 222 });
        assert_eq!(declared_parent(Some(111), &params), Some(111));
    }

    #[test]
    fn the_payload_covers_a_host_that_sends_no_flag() {
        let params = json!({ "processId": 222 });
        assert_eq!(declared_parent(None, &params), Some(222));
    }

    #[test]
    fn a_null_process_id_is_no_parent() {
        assert_eq!(declared_parent(None, &json!({ "processId": null })), None);
        assert_eq!(declared_parent(None, &json!({})), None);
    }
}
