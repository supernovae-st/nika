// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! ShellExecutor trait — abstract shell command runner replacing 24+ raw process spawn sites.
//!
//! Wire points: exec verb, daemon, doctor, install, machine.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

/// A shell command to execute.
#[derive(Debug, Clone)]
pub struct ShellCommand {
    pub program: String,
    pub args: Vec<String>,
    pub env: HashMap<String, String>,
    pub cwd: Option<PathBuf>,
    pub timeout: Option<Duration>,
    pub stdin: Option<String>,
    /// If true, run via `sh -c` (allows pipes, redirects).
    pub shell: bool,
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

    #[error("Blocked command (NIKA-053): {reason}")]
    Blocked { reason: String },

    #[error("Shell error: {reason}")]
    Other { reason: String },
}
