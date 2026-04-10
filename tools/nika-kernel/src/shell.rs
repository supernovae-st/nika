// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ShellExecutor trait — abstract shell command runner replacing 24+ raw process spawn sites.
//!
//! Wire points: exec verb, daemon, doctor, install, machine.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

/// A shell command to execute.
#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    /// Environment variable names to REMOVE from the child's inherited env.
    ///
    /// Applied AFTER `env` so callers can explicitly strip sensitive variables
    /// (API keys, tokens) that would otherwise be inherited from the parent.
    pub env_remove: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Option<Duration>,
    pub stdin: Option<String>,
    /// If true, run via `sh -c` (allows pipes, redirects).
    pub shell: bool,
    /// If true, skip the default blocklist check in the executor.
    ///
    /// Callers that have already performed intelligent validation (e.g.,
    /// the engine's `validate_exec_command_full` which honors raw-template
    /// intent for interpreter patterns like `python3 -c`) should set this
    /// to `true` to avoid false positives from the executor's basic
    /// pattern blocklist.
    ///
    /// Defaults to `false` (blocklist enforced) so naive callers remain
    /// protected by defense-in-depth.
    pub pre_validated: bool,
    /// Cooperative cancellation token. When triggered, the executor kills the
    /// child process and returns `ShellError::Cancelled`.
    pub cancel: Option<CancellationToken>,
}

impl ShellCommand {
    /// Convenience constructor. `cancel` defaults to `None`.
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: HashMap::new(),
            env_remove: Vec::new(),
            cwd: None,
            timeout: None,
            stdin: None,
            shell: false,
            pre_validated: false,
            cancel: None,
        }
    }
}

/// Result of a shell command execution.
#[derive(Debug, Clone)]
pub struct ShellResult {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
    pub duration: Duration,
}

impl ShellResult {
    /// Whether the command exited successfully (status 0).
    pub fn success(&self) -> bool {
        self.status == 0
    }
}

/// Async shell command executor.
///
/// Production: `TokioShell` via `tokio::process::Command`.
/// Future: `SandboxedShell` (firejail / bubblewrap integration).
/// Tests: `MockShell` with programmable output + status.
#[async_trait::async_trait]
pub trait ShellExecutor: Send + Sync {
    /// Execute a shell command and return the result.
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError>;
}

/// Shell execution errors.
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    #[error("Command not found: {program}")]
    NotFound { program: String },

    #[error("Command timed out after {duration_ms}ms")]
    Timeout { duration_ms: u64 },

    #[error("Command cancelled")]
    Cancelled,

    #[error("Blocked command (NIKA-053): {reason}")]
    Blocked { reason: String },

    #[error("Shell error: {reason}")]
    Other { reason: String },
}
