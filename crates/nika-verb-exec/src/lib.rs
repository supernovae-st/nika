// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! # nika-verb-exec — the `exec` verb executor (L2)
//!
//! Shell command execution per `nika-spec spec/02-verbs.md §exec` — the
//! second of the 4 verbs (`infer · exec · invoke · agent` · locked forever
//! per D-2026-05-22-N18).
//!
//! ## Shape
//!
//! - **Effect injected, never owned** — the verb rides the kernel
//!   `ShellRunDyn` seam: production wiring hands it
//!   `nika-exec-runner::TokioShell` (s7 · blocklist + hard-kill timeout +
//!   concurrent drain), tests hand it `nika-kernel-mock::MockShell`.
//!   `pre_validated` is NEVER set — the runner's blocklist stays the floor.
//! - **The capture one-obvious-way split (spec §exec conformance)** —
//!   default modes (`stdout` · `stderr` · `combined`) FAIL the task on a
//!   non-zero exit (NIKA-440 / spec NIKA-EXEC-001); `capture: structured`
//!   returns `{ stdout, stderr, exit_code }` as DATA — the workflow
//!   branches on it, the task succeeds.
//!
//! ## Fences (what this crate is NOT)
//!
//! The blocklist (s7 runner floor + future `nika-policy`) · `${{ }}`
//! resolution (upstream binding) · retry/timeout POLICY (task-level —
//! the engine resolves them; the resolved timeout passes through) ·
//! output truncation (runner caps apply) · cancel-by-id (engine surface).
//!
//! ## Example (mock · zero subprocess)
//!
//! ```
//! use std::sync::Arc;
//! use nika_kernel_mock::MockShell;
//! use nika_verb_exec::{ExecInput, ExecVerb};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let verb = ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("hello\n")));
//! let out = verb.run(ExecInput::new("echo hello")).await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

mod errors;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nika_kernel::process::{ShellCommand, ShellResult, ShellRunDyn};

pub use errors::VerbExecError;

/// The `exec` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExecInput {
    /// The command to run (required · spec §exec · CEL-resolved upstream).
    pub command: String,
    /// Working directory (default: engine cwd).
    pub cwd: Option<PathBuf>,
    /// OS environment for THIS subprocess (NOT the envelope `env:`).
    pub env: BTreeMap<String, String>,
    /// Stdin data.
    pub stdin: Option<String>,
    /// Output capture mode (default `Stdout`).
    pub capture: CaptureMode,
    /// Task-level timeout, resolved by the engine (03-dag) — the runner
    /// enforces the hard kill.
    pub timeout: Option<Duration>,
}

impl ExecInput {
    /// A default-capture run with every optional field unset.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            cwd: None,
            env: BTreeMap::new(),
            stdin: None,
            capture: CaptureMode::Stdout,
            timeout: None,
        }
    }
}

/// Output capture mode (spec `capture:` enum).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum CaptureMode {
    /// Stdout only (default).
    #[default]
    Stdout,
    /// Stderr only.
    Stderr,
    /// Stdout then stderr, concatenated.
    Combined,
    /// `{ stdout, stderr, exit_code }` — non-zero exit is data, not failure.
    Structured,
}

/// The verb's result value per capture mode.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecValue {
    /// Captured text (`stdout` · `stderr` · `combined` modes).
    Text(String),
    /// The structured outcome (`structured` mode) — exit code included.
    Structured {
        /// Captured standard output.
        stdout: String,
        /// Captured standard error.
        stderr: String,
        /// Process exit code (the workflow's to branch on).
        exit_code: i32,
    },
}

/// The `exec` verb output.
#[derive(Debug)]
#[non_exhaustive]
pub struct ExecOutput {
    /// The shaped output value (`.output` in spec terms).
    pub output: ExecValue,
    /// Wall-clock duration of the subprocess.
    pub duration: Duration,
}

impl ExecOutput {
    /// Construct from the shaped value and the runner-reported duration.
    #[must_use]
    pub fn new(output: ExecValue, duration: Duration) -> Self {
        Self { output, duration }
    }
}

/// Shell command execution — the `exec` verb executor.
#[derive(Debug)]
pub struct ExecVerb<S> {
    shell: Arc<S>,
}

impl<S> ExecVerb<S> {
    /// Create the verb over an injected shell effect.
    #[must_use]
    pub fn new(shell: Arc<S>) -> Self {
        Self { shell }
    }
}

impl<S> ExecVerb<S>
where
    S: ShellRunDyn + Sync,
{
    /// Execute the `exec` task.
    ///
    /// CANCEL SAFETY: cancel-safe via the runner's `kill_on_drop`
    /// contract (INV-011) — dropping the future SIGKILLs the child.
    ///
    /// # Errors
    ///
    /// [`VerbExecError::InvalidParam`] on an empty command, a NUL byte in
    /// the command/stdin, or a malformed env key ·
    /// [`VerbExecError::Shell`] when the runner fails (spawn · blocklist ·
    /// timeout) · [`VerbExecError::NonZeroExit`] on a non-zero exit in the
    /// default capture modes (`structured` returns it as data instead).
    pub async fn run(&self, input: ExecInput) -> Result<ExecOutput, VerbExecError> {
        validate_params(&input)?;
        let capture = input.capture;

        let result = self
            .shell
            .run(build_command(input))
            .await
            .map_err(|source| VerbExecError::Shell { source })?;

        // The capture one-obvious-way split: default modes fail on a
        // non-zero exit; `structured` carries the exit code as data.
        if capture != CaptureMode::Structured && result.status != 0 {
            return Err(VerbExecError::non_zero(result.status, &result.stderr));
        }

        let duration = result.duration;
        Ok(ExecOutput::new(shape_output(capture, result), duration))
    }
}

/// Verb-boundary validation BEFORE the runner call (NIKA-442).
///
/// The runner's blocklist (s7) is the security floor, but two corruption
/// classes must be refused at the verb boundary because they would
/// silently change what the OS executes (review lens 3 · P1+P2):
/// - a **NUL byte** in `command`/`stdin` truncates the C-string the shell
///   receives, so the runner's string-based blocklist checks a different
///   command than the one that runs;
/// - a **malformed env key** (`=`, NUL, or empty) corrupts the child's
///   `KEY=VALUE` environment assembly.
fn validate_params(input: &ExecInput) -> Result<(), VerbExecError> {
    if input.command.trim().is_empty() {
        return Err(VerbExecError::InvalidParam {
            param: "command",
            detail: "command must be a non-empty string".to_owned(),
        });
    }
    if input.command.contains('\0') {
        return Err(VerbExecError::InvalidParam {
            param: "command",
            detail: "command must not contain a NUL byte".to_owned(),
        });
    }
    if input.stdin.as_deref().is_some_and(|s| s.contains('\0')) {
        return Err(VerbExecError::InvalidParam {
            param: "stdin",
            detail: "stdin must not contain a NUL byte".to_owned(),
        });
    }
    for key in input.env.keys() {
        if key.is_empty() || key.contains('=') || key.contains('\0') {
            return Err(VerbExecError::InvalidParam {
                param: "env",
                detail: format!("env key {key:?} is invalid (empty, or contains '=' or NUL)"),
            });
        }
    }
    Ok(())
}

/// Map the spec task surface onto the kernel `ShellCommand`.
///
/// `shell = true` (spec: « run via the OS shell ») · `pre_validated` stays
/// `false` FOREVER — this crate never bypasses the runner blocklist.
/// Takes `input` by value so the (potentially large) env map and strings
/// move into the command instead of cloning (review lens 1 · P2-3).
fn build_command(input: ExecInput) -> ShellCommand {
    let mut cmd = ShellCommand::new(input.command);
    cmd.shell = true;
    cmd.cwd = input.cwd;
    cmd.env = input.env;
    cmd.stdin = input.stdin;
    cmd.timeout = input.timeout;
    cmd
}

/// The capture one-obvious-way split (spec §exec conformance).
///
/// Total over `CaptureMode` — no panic arm: `structured` returns the exit
/// code as DATA; the default modes fail on a non-zero exit (review lens
/// 1+2 · P1 · the prior `unreachable!` is gone).
fn shape_output(capture: CaptureMode, result: ShellResult) -> ExecValue {
    match capture {
        CaptureMode::Structured => ExecValue::Structured {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.status,
        },
        CaptureMode::Stdout => ExecValue::Text(result.stdout),
        CaptureMode::Stderr => ExecValue::Text(result.stderr),
        CaptureMode::Combined => ExecValue::Text(format!("{}{}", result.stdout, result.stderr)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::process::ShellError;
    use nika_kernel_mock::MockShell;

    fn verb(mock: MockShell) -> ExecVerb<MockShell> {
        ExecVerb::new(Arc::new(mock))
    }

    #[tokio::test]
    async fn stdout_capture_happy_path() {
        let mock = MockShell::new().enqueue_ok("built\n");
        let v = verb(mock.clone());
        let out = v
            .run(ExecInput::new("cargo build"))
            .await
            .expect("exit 0 succeeds");
        assert_eq!(out.output, ExecValue::Text("built\n".to_owned()));
        // The command reached the runner with the shell flag and WITHOUT
        // the blocklist bypass.
        let sent = mock.executed_commands();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].program, "cargo build");
        assert!(sent[0].shell, "spec: run via the OS shell");
        assert!(!sent[0].pre_validated, "blocklist floor stays armed");
    }

    #[tokio::test]
    async fn non_zero_exit_fails_default_modes() {
        let mock = MockShell::new().enqueue_fail(101, "compile error");
        let err = verb(mock)
            .run(ExecInput::new("cargo build"))
            .await
            .expect_err("non-zero fails stdout mode");
        match err {
            VerbExecError::NonZeroExit {
                status,
                stderr_tail,
            } => {
                assert_eq!(status, 101);
                assert_eq!(stderr_tail, "compile error");
            }
            other => panic!("expected NonZeroExit, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn structured_mode_returns_non_zero_as_data() {
        // THE load-bearing split: same failing command, structured capture —
        // the task SUCCEEDS and the workflow branches on exit_code.
        let mock = MockShell::new().enqueue_fail(1, "tests failed");
        let mut input = ExecInput::new("cargo test");
        input.capture = CaptureMode::Structured;
        let out = verb(mock).run(input).await.expect("structured succeeds");
        assert_eq!(
            out.output,
            ExecValue::Structured {
                stdout: String::new(),
                stderr: "tests failed".to_owned(),
                exit_code: 1,
            }
        );
    }

    #[tokio::test]
    async fn stderr_and_combined_capture_shapes() {
        let both = || {
            MockShell::new().enqueue_result(nika_kernel::process::ShellResult::new(
                0,
                "out",
                "err",
                Duration::from_millis(3),
            ))
        };

        let mut input = ExecInput::new("c");
        input.capture = CaptureMode::Stderr;
        let out = verb(both()).run(input).await.expect("exit 0");
        assert_eq!(out.output, ExecValue::Text("err".to_owned()));

        let mut input = ExecInput::new("c");
        input.capture = CaptureMode::Combined;
        let out = verb(both()).run(input).await.expect("exit 0");
        assert_eq!(out.output, ExecValue::Text("outerr".to_owned()));
    }

    #[tokio::test]
    async fn shell_error_is_wrapped_not_swallowed() {
        let mock = MockShell::new().enqueue_err(ShellError::Blocked {
            reason: "rm -rf /".to_owned(),
        });
        let err = verb(mock)
            .run(ExecInput::new("rm -rf /"))
            .await
            .expect_err("blocked propagates");
        assert!(matches!(
            err,
            VerbExecError::Shell {
                source: ShellError::Blocked { .. }
            }
        ));
    }

    #[tokio::test]
    async fn empty_command_is_rejected_before_any_call() {
        let mock = MockShell::new();
        let recorder = mock.clone();
        let err = verb(mock)
            .run(ExecInput::new("   "))
            .await
            .expect_err("empty command rejected");
        assert!(matches!(
            err,
            VerbExecError::InvalidParam {
                param: "command",
                ..
            }
        ));
        assert!(recorder.executed_commands().is_empty(), "zero runner calls");
    }

    #[tokio::test]
    async fn env_cwd_stdin_timeout_land_on_the_command() {
        let mock = MockShell::new().enqueue_ok("");
        let recorder = mock.clone();
        let mut input = ExecInput::new("printenv");
        input.cwd = Some(PathBuf::from("/tmp"));
        input.env.insert("RUST_LOG".to_owned(), "debug".to_owned());
        input.stdin = Some("data".to_owned());
        input.timeout = Some(Duration::from_secs(60));
        verb(mock).run(input).await.expect("exit 0");
        let sent = recorder.executed_commands();
        assert_eq!(sent[0].cwd, Some(PathBuf::from("/tmp")));
        assert_eq!(
            sent[0].env.get("RUST_LOG").map(String::as_str),
            Some("debug")
        );
        assert_eq!(sent[0].stdin.as_deref(), Some("data"));
        assert_eq!(sent[0].timeout, Some(Duration::from_secs(60)));
    }

    /// Gate 10 PARITY — pins the behaviors the brouillon verb established
    /// (`git show brouillon:tools/nika-verb-exec/src/lib.rs` · read-only
    /// reference · CRAFT rewrite): delegate to the kernel shell trait,
    /// stdout is the default capture, the structured shape carries
    /// `stdout`/`stderr`/`exit_code`, and the runner-reported duration surfaces.
    #[tokio::test]
    async fn gate10_parity_delegation_vs_brouillon() {
        // Pin that run() reads result.duration verbatim (not a mock default):
        // a known, unusual duration must surface unchanged.
        let known = Duration::from_millis(4242);
        let mock = MockShell::new().enqueue_result(nika_kernel::process::ShellResult::new(
            0,
            "v0.80.0\n",
            "",
            known,
        ));
        let out = verb(mock)
            .run(ExecInput::new("nika --version"))
            .await
            .expect("delegates and succeeds");
        assert_eq!(out.output, ExecValue::Text("v0.80.0\n".to_owned()));
        assert_eq!(out.duration, known, "runner duration surfaces verbatim");
    }

    #[tokio::test]
    async fn nul_byte_in_command_is_rejected_before_any_call() {
        let mock = MockShell::new();
        let recorder = mock.clone();
        let err = verb(mock)
            .run(ExecInput::new("echo ok\0; rm -rf /"))
            .await
            .expect_err("NUL truncation refused");
        assert!(matches!(
            err,
            VerbExecError::InvalidParam {
                param: "command",
                ..
            }
        ));
        assert!(recorder.executed_commands().is_empty(), "zero runner calls");
    }

    #[tokio::test]
    async fn malformed_env_key_is_rejected() {
        for bad in ["FOO=BAR", "", "NU\0L"] {
            let mock = MockShell::new();
            let recorder = mock.clone();
            let mut input = ExecInput::new("printenv");
            input.env.insert(bad.to_owned(), "v".to_owned());
            let err = verb(mock).run(input).await.unwrap_err();
            assert!(
                matches!(err, VerbExecError::InvalidParam { param: "env", .. }),
                "key {bad:?} rejected"
            );
            assert!(recorder.executed_commands().is_empty());
        }
    }

    #[tokio::test]
    async fn nul_byte_in_stdin_is_rejected() {
        let mock = MockShell::new();
        let mut input = ExecInput::new("cat");
        input.stdin = Some("data\0more".to_owned());
        let err = verb(mock).run(input).await.expect_err("NUL stdin refused");
        assert!(matches!(
            err,
            VerbExecError::InvalidParam { param: "stdin", .. }
        ));
    }
}

#[cfg(test)]
mod proptests {
    use std::sync::Arc;

    use nika_kernel_mock::MockShell;
    use proptest::prelude::*;

    use super::*;

    fn verb(mock: MockShell) -> ExecVerb<MockShell> {
        ExecVerb::new(Arc::new(mock))
    }

    proptest! {
        /// `shape_output` is total over `CaptureMode` and never panics:
        /// structured preserves the exit code verbatim, every other mode
        /// yields `Text`.
        #[test]
        fn shape_output_is_total(
            status in -255i32..=255,
            stdout in ".{0,80}",
            stderr in ".{0,80}",
        ) {
            let result = || nika_kernel::process::ShellResult::new(
                status, stdout.clone(), stderr.clone(), Duration::from_millis(1),
            );
            let structured_ok = matches!(
                shape_output(CaptureMode::Structured, result()),
                ExecValue::Structured { exit_code, .. } if exit_code == status
            );
            prop_assert!(structured_ok);
            for mode in [CaptureMode::Stdout, CaptureMode::Stderr, CaptureMode::Combined] {
                prop_assert!(matches!(shape_output(mode, result()), ExecValue::Text(_)));
            }
        }

        /// The whole `run()` split: a non-zero exit fails the default modes
        /// and is data in `structured` — over arbitrary status codes.
        #[test]
        fn run_capture_split_over_status(status in -255i32..=255) {
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            rt.block_on(async {
                for mode in [CaptureMode::Stdout, CaptureMode::Stderr, CaptureMode::Combined] {
                    let mock = MockShell::new().enqueue_result(
                        nika_kernel::process::ShellResult::new(
                            status, "o", "e", Duration::from_millis(1),
                        ),
                    );
                    let mut input = ExecInput::new("c");
                    input.capture = mode;
                    let ok = verb(mock).run(input).await.is_ok();
                    prop_assert_eq!(ok, status == 0);
                }
                let mock = MockShell::new().enqueue_result(
                    nika_kernel::process::ShellResult::new(
                        status, "o", "e", Duration::from_millis(1),
                    ),
                );
                let mut input = ExecInput::new("c");
                input.capture = CaptureMode::Structured;
                prop_assert!(verb(mock).run(input).await.is_ok());
                Ok(())
            })?;
        }

        /// Combined = stdout ⧺ stderr, exactly.
        #[test]
        fn combined_is_concatenation(stdout in ".{0,40}", stderr in ".{0,40}") {
            let result = nika_kernel::process::ShellResult::new(
                0, stdout.clone(), stderr.clone(), Duration::from_millis(1),
            );
            let expected = format!("{stdout}{stderr}");
            let combined_ok = matches!(
                shape_output(CaptureMode::Combined, result),
                ExecValue::Text(t) if t == expected
            );
            prop_assert!(combined_ok);
        }
    }
}
