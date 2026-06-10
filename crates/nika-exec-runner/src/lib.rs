// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-exec-runner` — the production shell-execution effect.
//!
//! This crate sits at **L1**: it implements the L0.5 `nika_kernel::process`
//! traits (`ShellRun` + `ShellCancel` — and the blanket `ShellExecutor`) via
//! `tokio::process`. The only production site spawning subprocesses; crates
//! that run commands inject the kernel traits and receive [`TokioShell`] in
//! production, a mock in tests (Invariant #27).
//!
//! [`TokioShell`] implements the `*Dyn` trait-variant companions (`Send`
//! futures · base traits via the blanket impl), same pattern as
//! `nika-fs`/`nika-http`/`nika-blob`.
//!
//! # Security — safe by default
//!
//! Workflows run attacker-influenced commands, so the MECHANISM is safe
//! before `nika-policy` (L1.5 · step 8) adds richer gating. Unless
//! `command.pre_validated`, every command is checked against the `blocklist` module
//! (NFKC + zero-width + quote/basename bypass defenses · ~100 patterns) and,
//! for `shell: true`, the shell-mode blocklist — `ShellError::Blocked` on a
//! hit. `pre_validated` is the documented seam for the policy layer that has
//! already done intent-aware validation.
//!
//! # Process safety (kernel CANCEL SAFETY contract)
//!
//! - **`kill_on_drop(true)`** (INV-011) — dropping the `run()` future SIGKILLs
//!   the child. This IS the PRIMARY cancellation per ADR-016 (future-drop).
//! - **Concurrent stdout/stderr drain with `wait()`** via `tokio::try_join!`
//!   (INV-012) — a child writing past the OS pipe buffer would deadlock if we
//!   waited-then-read.
//! - **`cancel(id)`** is registry-backed kill-by-pid (ADR-016 · the OS kills) —
//!   `run()` registers each spawned child by its pid; `cancel(pid)` signals it;
//!   unknown/dead pid is an idempotent `Ok`.

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

mod blocklist;

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use nika_kernel::process::{ShellCancelDyn, ShellRunDyn};
use nika_kernel::{ShellCommand, ShellError, ShellResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::sync::Notify;

/// pid → cancel signal, shared across `run()`/`cancel()` calls.
type Registry = Arc<Mutex<BTreeMap<u32, Arc<Notify>>>>;

/// Production shell executor backed by `tokio::process::Command`.
///
/// Cheap to clone (the cancel registry is `Arc`-shared). The blocklist is
/// the safe-by-default floor; `kill_on_drop` + the registry give cancellation.
#[derive(Debug, Clone, Default)]
pub struct TokioShell {
    registry: Registry,
}

impl TokioShell {
    /// Create a new shell executor with an empty cancel registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a pid's cancel signal; returns the shared `Notify`.
    fn register(&self, pid: u32) -> Arc<Notify> {
        let notify = Arc::new(Notify::new());
        if let Ok(mut reg) = self.registry.lock() {
            reg.insert(pid, Arc::clone(&notify));
        }
        notify
    }

    /// Deregister a pid (on natural exit). Best-effort.
    fn deregister(&self, pid: u32) {
        if let Ok(mut reg) = self.registry.lock() {
            reg.remove(&pid);
        }
    }
}

/// What ended the wait — kept out of the `select!` so the pid is always
/// deregistered (no early `return` inside the arms).
enum Outcome {
    Done(std::io::Result<(std::process::ExitStatus, Vec<u8>, Vec<u8>)>),
    Cancelled,
    TimedOut(u64),
}

impl ShellRunDyn for TokioShell {
    /// Run a command: blocklist (unless `pre_validated`) → spawn
    /// (`kill_on_drop`) → concurrent drain + wait with timeout + cancel.
    ///
    /// CANCEL SAFETY: cancel-safe — `kill_on_drop(true)` means dropping this
    /// future SIGKILLs the child (no orphan). The PRIMARY cancellation path.
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        if !command.pre_validated {
            let full = format!("{} {}", command.program, command.args.join(" "));
            blocklist::check_command(&full)?;
            if command.shell {
                blocklist::check_shell_mode(&full)?;
            }
        }

        let start = Instant::now();
        let mut cmd = build_command(&command);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        cmd.stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        // INV-011: kill the child when its handle drops (cancel/timeout/panic).
        cmd.kill_on_drop(true);

        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ShellError::NotFound {
                    program: command.program.clone(),
                }
            } else {
                ShellError::Other {
                    reason: e.to_string(),
                }
            }
        })?;

        if let Some(data) = &command.stdin
            && let Some(mut stdin) = child.stdin.take()
        {
            let _ = stdin.write_all(data.as_bytes()).await;
            drop(stdin); // close → EOF for the child
        }

        // Register for out-of-band cancel-by-pid (ADR-016).
        let pid = child.id();
        let notify = pid.map(|p| self.register(p));

        // INV-012: drain stdout+stderr CONCURRENTLY with wait() — a child that
        // writes past the OS pipe buffer would deadlock on wait-then-read.
        let child_fut = async move {
            let out = child.stdout.take();
            let err = child.stderr.take();
            tokio::try_join!(child.wait(), drain(out), drain(err))
        };

        let timeout_fut = async {
            match command.timeout {
                Some(t) => tokio::time::sleep(t).await,
                None => std::future::pending::<()>().await,
            }
        };

        let outcome = match &notify {
            Some(n) => {
                tokio::select! {
                    biased;
                    () = n.notified() => Outcome::Cancelled,
                    () = timeout_fut => Outcome::TimedOut(
                        command.timeout.map_or(0, dur_ms)),
                    r = child_fut => Outcome::Done(r),
                }
            }
            None => {
                tokio::select! {
                    biased;
                    () = timeout_fut => Outcome::TimedOut(
                        command.timeout.map_or(0, dur_ms)),
                    r = child_fut => Outcome::Done(r),
                }
            }
        };

        if let Some(p) = pid {
            self.deregister(p);
        }

        match outcome {
            Outcome::Cancelled => Err(ShellError::Cancelled {
                id: pid.map_or_else(|| "?".to_string(), |p| p.to_string()),
            }),
            Outcome::TimedOut(ms) => Err(ShellError::Timeout { duration_ms: ms }),
            Outcome::Done(Err(e)) => Err(ShellError::Other {
                reason: e.to_string(),
            }),
            Outcome::Done(Ok((status, stdout, stderr))) => Ok(ShellResult::new(
                status.code().unwrap_or(-1),
                String::from_utf8_lossy(&stdout),
                String::from_utf8_lossy(&stderr),
                start.elapsed(),
            )),
        }
    }
}

impl ShellCancelDyn for TokioShell {
    /// Cancel a running command by its OS pid (string). Unknown/dead pid is
    /// an idempotent `Ok` (per the kernel contract).
    ///
    /// CANCEL SAFETY: cancel-safe and idempotent.
    async fn cancel(&self, id: &str) -> Result<(), ShellError> {
        let Ok(pid) = id.parse::<u32>() else {
            // Non-numeric id can't match a registered pid — harmless no-op.
            return Ok(());
        };
        let notify = self
            .registry
            .lock()
            .ok()
            .and_then(|reg| reg.get(&pid).map(Arc::clone));
        if let Some(n) = notify {
            n.notify_waiters(); // wakes run()'s `notified()` arm → child drop → SIGKILL
        }
        Ok(())
    }
}

/// Build the `tokio::process::Command` (program/args or `sh -c`, env, cwd).
fn build_command(command: &ShellCommand) -> Command {
    let mut cmd = if command.shell {
        let line = if command.args.is_empty() {
            command.program.clone()
        } else {
            format!("{} {}", command.program, command.args.join(" "))
        };
        let mut c = Command::new("sh");
        c.arg("-c").arg(line);
        c
    } else {
        let mut c = Command::new(&command.program);
        c.args(&command.args);
        c
    };
    for (k, v) in &command.env {
        cmd.env(k, v);
    }
    // After env so a removal wins over a set of the same key.
    for k in &command.env_remove {
        cmd.env_remove(k);
    }
    if let Some(cwd) = &command.cwd {
        cmd.current_dir(cwd);
    }
    cmd
}

/// Read an optional async handle to end (empty when absent).
async fn drain<R: tokio::io::AsyncRead + Unpin>(handle: Option<R>) -> std::io::Result<Vec<u8>> {
    match handle {
        Some(mut h) => {
            let mut buf = Vec::new();
            h.read_to_end(&mut buf).await?;
            Ok(buf)
        }
        None => Ok(Vec::new()),
    }
}

/// Saturating `Duration` → milliseconds (kernel `Timeout.duration_ms` is u64).
fn dur_ms(t: std::time::Duration) -> u64 {
    u64::try_from(t.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokioshell_is_clone_default() {
        fn assert_traits<T: Clone + Default>() {}
        assert_traits::<TokioShell>();
    }

    #[test]
    fn dur_ms_saturates() {
        assert_eq!(dur_ms(std::time::Duration::from_millis(1500)), 1500);
        // Duration::MAX has > u64::MAX milliseconds → saturates, never panics.
        assert_eq!(dur_ms(std::time::Duration::MAX), u64::MAX);
    }

    #[test]
    fn build_command_both_modes_do_not_panic() {
        let _ = build_command(&ShellCommand::new("echo").arg("hi"));
        let mut sh = ShellCommand::new("echo");
        sh.shell = true;
        sh.args = vec!["a".into(), "b".into()];
        let _ = build_command(&sh);
    }

    #[tokio::test]
    async fn cancel_registry_register_notify_deregister() {
        let shell = TokioShell::new();
        let notify = shell.register(4242);
        // cancel(pid) must wake a task awaiting the registered Notify.
        let waiter = {
            let n = Arc::clone(&notify);
            tokio::spawn(async move { n.notified().await })
        };
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        shell.cancel("4242").await.expect("cancel ok");
        tokio::time::timeout(std::time::Duration::from_secs(1), waiter)
            .await
            .expect("cancel must wake the waiter")
            .expect("waiter task ok");
        // deregister REMOVES the entry (not a no-op) — a fresh waiter on a
        // re-registered Notify must NOT be woken by the stale pid's cancel.
        shell.deregister(4242);
        assert!(
            shell.registry.lock().unwrap().get(&4242).is_none(),
            "deregister must remove the pid from the registry"
        );
        assert!(shell.cancel("4242").await.is_ok()); // now a harmless no-op
    }
}
