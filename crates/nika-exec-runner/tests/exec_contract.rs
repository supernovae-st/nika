// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Subprocess-driven contract tests — Unix `echo`/`sh`/`cat`/`sleep`.
#![cfg(unix)]

//! Contract tests for `nika-exec-runner` — the production `TokioShell`.
//!
//! Real subprocess execution: exit codes, stdout/stderr, stdin, env, cwd,
//! timeout, the blocklist (+ `pre_validated` bypass), idempotent cancel, and
//! the large-output no-deadlock invariant (INV-012). Cancel-by-pid plumbing
//! is unit-tested in `src/lib.rs` (the registry is not publicly observable).

use std::time::Duration;

use nika_exec_runner::TokioShell;
use nika_kernel::process::{ShellCancelDyn, ShellRunDyn};
use nika_kernel::{ShellCommand, ShellError, ShellExecutor};

fn shell() -> TokioShell {
    TokioShell::new()
}

fn cmd(program: &str, args: &[&str]) -> ShellCommand {
    let mut c = ShellCommand::new(program);
    c.args = args.iter().map(|s| (*s).to_string()).collect();
    c.timeout = Some(Duration::from_secs(10));
    c
}

// ── type-level ───────────────────────────────────────────────────────

#[test]
fn tokioshell_satisfies_blanket_executor_and_dyn() {
    fn takes_executor<T: ShellExecutor>(_: &T) {}
    fn takes_dyn<T: ShellRunDyn + ShellCancelDyn>(_: &T) {}
    let s = shell();
    takes_executor(&s);
    takes_dyn(&s);
}

// ── run · basics ──────────────────────────────────────────────────────

#[tokio::test]
async fn echo_captures_stdout_and_succeeds() {
    let r = shell().run(cmd("echo", &["hello"])).await.unwrap();
    assert!(r.success());
    assert_eq!(r.status, 0);
    assert_eq!(r.stdout.trim(), "hello");
}

#[tokio::test]
async fn nonzero_exit_status_captured() {
    let r = shell().run(cmd("false", &[])).await.unwrap();
    assert!(!r.success());
    assert_ne!(r.status, 0);
}

#[tokio::test]
async fn stderr_captured() {
    let mut c = ShellCommand::new("echo oops 1>&2");
    c.shell = true;
    c.pre_validated = true; // shell-mode `sh -c` is a blocklist hit; policy vouched
    c.timeout = Some(Duration::from_secs(10));
    let r = shell().run(c).await.unwrap();
    assert!(r.stderr.contains("oops"));
}

#[tokio::test]
async fn program_not_found_errs() {
    let err = shell()
        .run(cmd("nika-no-such-binary-xyz", &[]))
        .await
        .unwrap_err();
    assert!(matches!(err, ShellError::NotFound { .. }), "got {err:?}");
}

#[tokio::test]
async fn stdin_is_piped_to_child() {
    let mut c = cmd("cat", &[]);
    c.stdin = Some("piped-input".to_string());
    let r = shell().run(c).await.unwrap();
    assert_eq!(r.stdout.trim(), "piped-input");
}

#[tokio::test]
async fn env_var_is_set() {
    let mut c = ShellCommand::new("printenv");
    c.args = vec!["NIKA_TEST_VAR".to_string()];
    c.env
        .insert("NIKA_TEST_VAR".to_string(), "exec-runner".to_string());
    c.timeout = Some(Duration::from_secs(10));
    // `printenv` substring-trips the `env ` wrapper pattern (a deliberate
    // over-block — see blocklist::tests::printenv_is_over_blocked). This test
    // exercises env-PASSING (run mechanics), so it vouches via pre_validated.
    c.pre_validated = true;
    let r = shell().run(c).await.unwrap();
    assert_eq!(r.stdout.trim(), "exec-runner");
}

#[tokio::test]
async fn cwd_is_respected() {
    let dir = std::env::temp_dir();
    let mut c = cmd("pwd", &[]);
    c.cwd = Some(dir.clone());
    let r = shell().run(c).await.unwrap();
    let got = std::fs::canonicalize(r.stdout.trim()).unwrap();
    let want = std::fs::canonicalize(&dir).unwrap();
    assert_eq!(got, want, "pwd must reflect the requested cwd");
}

// ── timeout ───────────────────────────────────────────────────────────

#[tokio::test]
async fn timeout_kills_long_command() {
    let mut c = cmd("sleep", &["30"]);
    c.timeout = Some(Duration::from_millis(150));
    let start = std::time::Instant::now();
    let err = shell().run(c).await.unwrap_err();
    assert!(
        matches!(err, ShellError::Timeout { duration_ms: 150 }),
        "got {err:?}"
    );
    assert!(
        start.elapsed() < Duration::from_secs(5),
        "must not wait the full 30s"
    );
}

// ── blocklist (security) ──────────────────────────────────────────────

#[tokio::test]
async fn blocklist_blocks_dangerous_command_before_spawn() {
    let err = shell().run(cmd("rm", &["-rf", "/"])).await.unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn blocklist_blocks_absolute_path_priv_esc() {
    let err = shell()
        .run(cmd("/usr/bin/sudo", &["rm", "-rf", "/"]))
        .await
        .unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn pre_validated_bypasses_the_blocklist() {
    // `sudo ` is a blocklisted substring; echoing it would be blocked WITHOUT
    // pre_validated. With the policy-vouched flag, the gate is skipped.
    let mut c = cmd("echo", &["sudo", "trust-me"]);
    c.pre_validated = true;
    let r = shell().run(c).await.unwrap();
    assert!(r.success());
    assert!(r.stdout.contains("sudo"));
}

// ── INV-012: large output must NOT deadlock ──────────────────────────

#[tokio::test]
async fn large_output_does_not_deadlock() {
    // ~512 KB to stdout — far past the OS pipe buffer (~64KB Linux/16KB macOS).
    // Wait-then-read would deadlock; concurrent drain (try_join!) must not.
    let mut c = ShellCommand::new("yes hello | head -c 524288");
    c.shell = true;
    c.pre_validated = true;
    c.timeout = Some(Duration::from_secs(10));
    let r = shell().run(c).await.unwrap();
    assert_eq!(r.stdout.len(), 524_288);
}

// ── cancel ────────────────────────────────────────────────────────────

#[tokio::test]
async fn cancel_unknown_id_is_idempotent_ok() {
    let s = shell();
    assert!(s.cancel("999999999").await.is_ok());
    assert!(s.cancel("not-a-pid").await.is_ok());
}
