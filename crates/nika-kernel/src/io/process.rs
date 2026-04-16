// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Shell executor traits — ISP decomposition of process management.
//!
//! 2 atomic traits: `ShellRun`, `ShellCancel`.
//! 1 super-trait: `ShellExecutor` (blanket for both).
//!
//! Design decision #9: cancel is a `fn cancel(&self, id)` method,
//! not a `CancellationToken` field on `ShellCommand`. This keeps
//! tokio-util out of nika-kernel.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

/// A shell command to execute.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellCommand {
    /// Program to execute.
    pub program: String,
    /// Command arguments.
    pub args: Vec<String>,
    /// Environment variables to set.
    pub env: BTreeMap<String, String>,
    /// Environment variable names to remove from inherited env.
    pub env_remove: Vec<String>,
    /// Working directory.
    pub cwd: Option<PathBuf>,
    /// Execution timeout.
    pub timeout: Option<Duration>,
    /// Data to send to stdin.
    pub stdin: Option<String>,
    /// Whether to run via `sh -c` (enables pipes, redirects).
    pub shell: bool,
    /// If `true`, skip the executor's default blocklist check.
    ///
    /// Set by callers that have already performed intelligent validation
    /// (e.g., the engine's `validate_exec_command_full`).
    pub pre_validated: bool,
}

impl ShellCommand {
    /// Create a new shell command with the given program.
    #[must_use]
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            env_remove: Vec::new(),
            cwd: None,
            timeout: None,
            stdin: None,
            shell: false,
            pre_validated: false,
        }
    }

    /// Add an argument.
    #[must_use]
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }
}

/// Result of a shell command execution.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShellResult {
    /// Process exit code.
    pub status: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
    /// Wall-clock duration of execution.
    pub duration: Duration,
}

impl ShellResult {
    /// Create a new shell result.
    #[must_use]
    pub fn new(
        status: i32,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            status,
            stdout: stdout.into(),
            stderr: stderr.into(),
            duration,
        }
    }

    /// Whether the process exited successfully (status 0).
    #[must_use]
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Shell execution errors.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum ShellError {
    /// Program not found.
    #[error("program not found: {program}")]
    NotFound {
        /// The program that was not found.
        program: String,
    },

    /// Execution timed out.
    #[error("execution timed out after {duration_ms}ms")]
    Timeout {
        /// Timeout duration in milliseconds.
        duration_ms: u64,
    },

    /// Execution was cancelled.
    #[error("execution cancelled: {id}")]
    Cancelled {
        /// The cancelled command identifier.
        id: String,
    },

    /// Command blocked by security policy.
    #[error("command blocked: {reason}")]
    Blocked {
        /// Why the command was blocked.
        reason: String,
    },

    /// Other execution error.
    #[error("shell error: {reason}")]
    Other {
        /// Error description.
        reason: String,
    },
}

/// Run shell commands.
#[trait_variant::make(ShellRunDyn: Send)]
pub trait ShellRun: Send + Sync {
    /// Execute a shell command and return the result.
    ///
    /// CANCEL SAFETY: cancel-safe IF the impl sets `kill_on_drop(true)`
    /// on its `tokio::process::Command` (INV-011). Dropping the future
    /// then sends SIGKILL to the child on drop — no orphan processes,
    /// no resource leak. Impls that do NOT set `kill_on_drop` are UNSAFE.
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError>;
}

/// Cancel running shell commands.
#[trait_variant::make(ShellCancelDyn: Send)]
pub trait ShellCancel: Send + Sync {
    /// Cancel a running command by its identifier.
    ///
    /// CANCEL SAFETY: cancel-safe and idempotent — signalling an already
    /// dead pid is a harmless no-op. Callers may race cancel against
    /// natural exit without corruption.
    async fn cancel(&self, id: &str) -> Result<(), ShellError>;
}

/// Full shell executor — blanket super-trait.
pub trait ShellExecutor: ShellRun + ShellCancel {}
impl<T: ShellRun + ShellCancel> ShellExecutor for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_command_new_defaults() {
        let cmd = ShellCommand::new("echo");
        assert_eq!(cmd.program, "echo");
        assert!(cmd.args.is_empty());
        assert!(cmd.env.is_empty());
        assert!(cmd.cwd.is_none());
        assert!(cmd.timeout.is_none());
        assert!(cmd.stdin.is_none());
        assert!(!cmd.shell);
        assert!(!cmd.pre_validated);
    }

    #[test]
    fn shell_command_builder() {
        let cmd = ShellCommand::new("ls").arg("-la").arg("/tmp");
        assert_eq!(cmd.args, vec!["-la", "/tmp"]);
    }

    #[test]
    fn shell_result_success() {
        let result = ShellResult::new(0, "output", "", Duration::from_millis(100));
        assert!(result.success());
        assert_eq!(result.stdout, "output");
    }

    #[test]
    fn shell_result_failure() {
        let result = ShellResult::new(1, "", "error", Duration::from_millis(50));
        assert!(!result.success());
    }

    #[test]
    fn shell_error_not_found_display() {
        let err = ShellError::NotFound {
            program: "git".into(),
        };
        assert_eq!(err.to_string(), "program not found: git");
    }

    #[test]
    fn shell_error_timeout_display() {
        let err = ShellError::Timeout { duration_ms: 30000 };
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn shell_error_blocked_display() {
        let err = ShellError::Blocked {
            reason: "rm -rf /".into(),
        };
        assert!(err.to_string().contains("blocked"));
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn shell_types_send_sync() {
        _assert_send_sync::<ShellCommand>();
        _assert_send_sync::<ShellResult>();
    }
}
