// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Nika `exec:` verb — shell command execution.
//!
//! This crate contains the core execution logic for the `exec:` verb.
//! It receives **pre-validated, pre-resolved** inputs from the engine
//! bridge and delegates subprocess execution to `ShellExecutor` via
//! the kernel trait.
//!
//! ## What this crate does
//!
//! - Resolves default working directory from `working_dir_mode`
//! - Builds a `ShellCommand` from the input
//! - Calls `caps.shell.run(cmd)` (delegates to `TokioShell` in prod)
//! - Emits `ExecCompleted` events via `EventLog`
//! - Truncates stdout if it exceeds `max_stdout`
//!
//! ## What the engine bridge does (NOT this crate)
//!
//! - Template resolution (`{{with.alias}}` → concrete values)
//! - Security validation (SEC-2, command blocklist, shell injection)
//! - Error mapping (`VerbExecError` → `NikaError`)

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use nika_event::{EventKind, EventLog};
use nika_kernel::caps::ExecCaps;
use nika_kernel::shell::{ShellCommand, ShellError};

mod error;
pub use error::VerbExecError;

/// Pre-validated, pre-resolved inputs for the exec verb.
///
/// The engine bridge builds this from `ExecParams` after template
/// resolution and security validation. The verb crate never sees
/// raw templates.
pub struct ExecInput<'a> {
    /// Resolved command string (templates already expanded).
    pub command: &'a str,
    /// Run through shell (`sh -c`).
    pub shell: bool,
    /// Explicit working directory (already resolved + security-validated).
    pub cwd: Option<&'a Path>,
    /// Timeout for the command.
    pub timeout: Option<Duration>,
    /// Environment variables (already resolved + validated).
    pub env: Vec<(String, String)>,
    /// Environment variable names to REMOVE from the child's inherited env
    /// (e.g., sensitive API key env vars).
    pub env_remove: Vec<String>,
    /// Maximum stdout size in bytes before truncation.
    pub max_stdout: Option<u64>,
    /// Task ID for event emission.
    pub task_id: Arc<str>,
}

/// Execute a shell command.
///
/// # Arguments
/// * `input` — pre-validated input from the engine bridge
/// * `caps` — borrowed capability slice (shell, policy, clock, etc.)
/// * `event_log` — for emitting `ExecCompleted` events
///
/// # Returns
/// Trimmed stdout on success, or `VerbExecError` on failure.
pub async fn run(
    input: &ExecInput<'_>,
    caps: &ExecCaps<'_>,
    event_log: &EventLog,
) -> Result<String, VerbExecError> {
    // Resolve the effective working directory.
    let effective_cwd = input.cwd.map(PathBuf::from).or_else(|| {
        resolve_default_cwd(
            caps.working_dir_mode,
            caps.project_root,
            caps.workflow_base_dir,
        )
    });

    // Build the ShellCommand for the kernel executor.
    let cmd = ShellCommand {
        program: if input.shell {
            input.command.to_string()
        } else {
            // Shell-free: first token is the program.
            shlex_program(input.command)?
        },
        args: if input.shell {
            Vec::new()
        } else {
            shlex_args(input.command)?
        },
        env: input.env.iter().cloned().collect(),
        env_remove: input.env_remove.clone(),
        // The engine bridge calls `validate_exec_command_full` with raw-template
        // intent detection BEFORE building ExecInput, so the command has already
        // been vetted by the smart blocklist. Skip the executor's basic pattern
        // check which cannot distinguish interpreter patterns (python3 -c) from
        // injection attempts.
        pre_validated: true,
        cwd: effective_cwd,
        timeout: input.timeout,
        stdin: None,
        shell: input.shell,
        cancel: Some(caps.cancel.clone()),
    };

    let start = std::time::Instant::now();
    let result = caps.shell.run(cmd).await;
    let duration_ms = start.elapsed().as_millis() as u64;

    match result {
        Ok(shell_result) => {
            // Emit ExecCompleted event.
            event_log.emit(EventKind::ExecCompleted {
                task_id: Arc::clone(&input.task_id),
                exit_code: shell_result.status,
                stdout_len: shell_result.stdout.len(),
                stderr_len: shell_result.stderr.len(),
                duration_ms,
            });

            if !shell_result.success() {
                return Err(VerbExecError::NonZeroExit {
                    exit_code: shell_result.status,
                    stderr: shell_result.stderr.clone(),
                });
            }

            // Truncate stdout if needed.
            let stdout = truncate_stdout(
                &shell_result.stdout,
                input.max_stdout.unwrap_or(DEFAULT_MAX_STDOUT),
                &input.task_id,
            );

            Ok(stdout)
        }
        Err(ShellError::Cancelled) => Err(VerbExecError::Cancelled {
            task_id: input.task_id.to_string(),
        }),
        Err(ShellError::Timeout { duration_ms: ms }) => Err(VerbExecError::Timeout {
            duration_ms: ms,
        }),
        Err(ShellError::NotFound { program }) => Err(VerbExecError::NotFound { program }),
        Err(e) => Err(VerbExecError::Shell {
            reason: e.to_string(),
        }),
    }
}

const DEFAULT_MAX_STDOUT: u64 = 50 * 1024 * 1024; // 50 MB

/// Resolve the default cwd based on working_dir_mode config.
fn resolve_default_cwd(
    mode: Option<&str>,
    project_root: Option<&Path>,
    workflow_base_dir: &Path,
) -> Option<PathBuf> {
    match mode {
        Some("project") => project_root.map(PathBuf::from),
        Some("none") => None,
        // "workflow" or None → use workflow_base_dir
        _ => Some(workflow_base_dir.to_path_buf()),
    }
}

/// Extract the program name from a command string using shlex.
fn shlex_program(command: &str) -> Result<String, VerbExecError> {
    let parts = shlex::split(command).ok_or_else(|| VerbExecError::Parse {
        reason: format!("Failed to parse command (unbalanced quotes?): {command}"),
    })?;
    parts.into_iter().next().ok_or_else(|| VerbExecError::Parse {
        reason: "Empty command".to_string(),
    })
}

/// Extract the arguments from a command string using shlex (skip program).
fn shlex_args(command: &str) -> Result<Vec<String>, VerbExecError> {
    let parts = shlex::split(command).ok_or_else(|| VerbExecError::Parse {
        reason: format!("Failed to parse command (unbalanced quotes?): {command}"),
    })?;
    Ok(parts.into_iter().skip(1).collect())
}

/// Truncate stdout if it exceeds the limit.
fn truncate_stdout(stdout: &str, max_bytes: u64, task_id: &str) -> String {
    let max = max_bytes as usize;
    if stdout.len() > max {
        tracing::warn!(
            task_id = %task_id,
            stdout_bytes = stdout.len(),
            limit_bytes = max,
            "exec: stdout truncated"
        );
        let truncated = &stdout.as_bytes()[..max];
        let s = String::from_utf8_lossy(truncated);
        format!(
            "{}\n... [truncated: {} bytes total, {} byte limit]",
            s.trim_end(),
            stdout.len(),
            max,
        )
    } else {
        stdout.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cwd_project_mode() {
        let cwd = resolve_default_cwd(
            Some("project"),
            Some(Path::new("/project")),
            Path::new("/workflow"),
        );
        assert_eq!(cwd, Some(PathBuf::from("/project")));
    }

    #[test]
    fn resolve_cwd_workflow_mode() {
        let cwd = resolve_default_cwd(
            Some("workflow"),
            Some(Path::new("/project")),
            Path::new("/workflow"),
        );
        assert_eq!(cwd, Some(PathBuf::from("/workflow")));
    }

    #[test]
    fn resolve_cwd_none_mode() {
        let cwd = resolve_default_cwd(Some("none"), Some(Path::new("/project")), Path::new("/wf"));
        assert_eq!(cwd, None);
    }

    #[test]
    fn resolve_cwd_default() {
        let cwd = resolve_default_cwd(None, Some(Path::new("/project")), Path::new("/workflow"));
        assert_eq!(cwd, Some(PathBuf::from("/workflow")));
    }

    #[test]
    fn shlex_program_extracts_first_token() {
        assert_eq!(shlex_program("echo hello world").unwrap(), "echo");
    }

    #[test]
    fn shlex_args_skips_program() {
        let args = shlex_args("echo hello world").unwrap();
        assert_eq!(args, vec!["hello", "world"]);
    }

    #[test]
    fn shlex_empty_command_errors() {
        assert!(shlex_program("").is_err());
    }

    #[test]
    fn truncate_stdout_within_limit() {
        let result = truncate_stdout("hello", 1000, "test");
        assert_eq!(result, "hello");
    }

    #[test]
    fn truncate_stdout_exceeds_limit() {
        let big = "x".repeat(100);
        let result = truncate_stdout(&big, 50, "test");
        assert!(result.contains("truncated"));
        assert!(result.contains("100 bytes total"));
    }

    #[tokio::test]
    async fn run_echo_via_mock_shell() {
        use nika_kernel_mock::policy::MockPolicyChecker;
        use nika_kernel_mock::shell::MockShell;
        use nika_kernel_mock::clock::MockClock;
        use nika_kernel_mock::filesystem::InMemoryFs;

        let shell = MockShell::new();
        shell.enqueue_ok("hello golden\n");

        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let fs = InMemoryFs::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let workflow_dir = PathBuf::from("/tmp/test");

        let caps = ExecCaps::new(
            &shell, &policy, &clock, &fs, &cancel,
            &workflow_dir, None, None,
        );

        let event_log = EventLog::new();
        let input = ExecInput {
            command: "echo hello golden",
            shell: false,
            cwd: None,
            timeout: Some(Duration::from_secs(5)),
            env: vec![],
            env_remove: vec![],
            max_stdout: None,
            task_id: Arc::from("test_exec"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(result.is_ok(), "run failed: {result:?}");
        assert_eq!(result.unwrap(), "hello golden");

        // Verify ExecCompleted event was emitted.
        let events = event_log.events();
        let exec_completed = events.iter().find(|e| {
            matches!(e.kind, EventKind::ExecCompleted { .. })
        });
        assert!(exec_completed.is_some(), "ExecCompleted event not emitted");
    }

    /// **GATE-S13-1 regression test** — subprocess must not deadlock on >1 MB output.
    ///
    /// Goes end-to-end through `nika_verb_exec::run()` → `TokioShell::run()`
    /// to verify the G1 pattern (tokio::try_join! concurrent pipe drain +
    /// kill_on_drop) is preserved in the verb-crate execution path.
    ///
    /// Without the fix, this test hangs forever (pipe buffer full → child
    /// blocks on write → wait() blocks waiting for child → deadlock).
    /// Sacred invariant #13: every subprocess code MUST be regression-tested
    /// with >1 MB output.
    #[tokio::test]
    async fn subprocess_does_not_deadlock_on_large_output() {
        use nika_exec_runner::TokioShell;
        use nika_kernel_mock::clock::MockClock;
        use nika_kernel_mock::filesystem::InMemoryFs;
        use nika_kernel_mock::policy::MockPolicyChecker;

        // Real TokioShell — not MockShell — because we need actual subprocess I/O
        // to reproduce the pipe-buffer deadlock condition.
        let shell = TokioShell;
        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let fs = InMemoryFs::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let workflow_dir = std::env::temp_dir();

        let caps = ExecCaps::new(
            &shell, &policy, &clock, &fs, &cancel,
            &workflow_dir, None, None,
        );

        let event_log = EventLog::new();
        let input = ExecInput {
            // Emit 1 MB to stdout via `yes` + `head`.
            command: "yes x | head -c 1048576",
            shell: true,
            cwd: None,
            // 30-second timeout: if the fix is broken, test hangs until here.
            timeout: Some(Duration::from_secs(30)),
            env: vec![],
            env_remove: vec![],
            max_stdout: None,
            task_id: Arc::from("gate_s13_1_deadlock"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(
            result.is_ok(),
            "1 MB output must not deadlock — got: {result:?}"
        );
        let output = result.unwrap();
        // Output passes through truncate_stdout which trims; the 1 MB is
        // under the default 50 MB limit so the full content returns.
        // After trim(), trailing newlines are removed — but the content is
        // still ~1 MB of 'x\n' pairs.
        assert!(
            output.len() > 1_000_000,
            "expected ~1 MB of output, got {} bytes",
            output.len()
        );
    }

    #[tokio::test]
    async fn run_nonzero_exit_returns_error() {
        use nika_kernel_mock::policy::MockPolicyChecker;
        use nika_kernel_mock::shell::MockShell;
        use nika_kernel_mock::clock::MockClock;
        use nika_kernel_mock::filesystem::InMemoryFs;

        let shell = MockShell::new();
        shell.enqueue_fail(1, "command failed");

        let policy = MockPolicyChecker::allow_all();
        let clock = MockClock::new();
        let fs = InMemoryFs::new();
        let cancel = tokio_util::sync::CancellationToken::new();
        let workflow_dir = PathBuf::from("/tmp/test");

        let caps = ExecCaps::new(
            &shell, &policy, &clock, &fs, &cancel,
            &workflow_dir, None, None,
        );

        let event_log = EventLog::new();
        let input = ExecInput {
            command: "false",
            shell: false,
            cwd: None,
            timeout: None,
            env: vec![],
            env_remove: vec![],
            max_stdout: None,
            task_id: Arc::from("test_fail"),
        };

        let result = run(&input, &caps, &event_log).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, VerbExecError::NonZeroExit { .. }));
    }
}
