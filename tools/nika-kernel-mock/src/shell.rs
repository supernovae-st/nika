// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! MockShell — programmable shell executor for tests.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use nika_kernel::shell::{ShellCommand, ShellError, ShellExecutor, ShellResult};

/// A mock shell that returns canned responses in FIFO order.
#[derive(Clone, Default)]
pub struct MockShell {
    results: Arc<Mutex<VecDeque<Result<ShellResult, ShellError>>>>,
    commands: Arc<Mutex<Vec<ShellCommand>>>,
}

impl MockShell {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a successful command result.
    pub fn enqueue_ok(&self, stdout: impl Into<String>) {
        self.results.lock().push_back(Ok(ShellResult {
            status: 0,
            stdout: stdout.into(),
            stderr: String::new(),
            duration: Duration::from_millis(1),
        }));
    }

    /// Enqueue a failed command result.
    pub fn enqueue_fail(&self, status: i32, stderr: impl Into<String>) {
        self.results.lock().push_back(Ok(ShellResult {
            status,
            stdout: String::new(),
            stderr: stderr.into(),
            duration: Duration::from_millis(1),
        }));
    }

    /// Enqueue a shell error.
    pub fn enqueue_err(&self, error: ShellError) {
        self.results.lock().push_back(Err(error));
    }

    /// Get all commands that were executed (for assertions).
    pub fn executed_commands(&self) -> Vec<ShellCommand> {
        self.commands.lock().clone()
    }
}

#[async_trait::async_trait]
impl ShellExecutor for MockShell {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        self.commands.lock().push(command);
        self.results.lock().pop_front().unwrap_or_else(|| {
            Err(ShellError::Other {
                reason: "MockShell: no more enqueued results".to_string(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_and_run() {
        let shell = MockShell::new();
        shell.enqueue_ok("hello world");
        let result = shell
            .run(ShellCommand {
                program: "echo".into(),
                args: vec!["hello".into(), "world".into()],
                env: Default::default(),
                env_remove: Default::default(),
                pre_validated: false,
                cwd: None,
                timeout: None,
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();
        assert!(result.success());
        assert_eq!(result.stdout, "hello world");
    }

    #[tokio::test]
    async fn enqueue_failure() {
        let shell = MockShell::new();
        shell.enqueue_fail(1, "command not found");
        let result = shell
            .run(ShellCommand {
                program: "nonexistent".into(),
                args: vec![],
                env: Default::default(),
                env_remove: Default::default(),
                pre_validated: false,
                cwd: None,
                timeout: None,
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();
        assert!(!result.success());
        assert_eq!(result.status, 1);
    }

    #[tokio::test]
    async fn tracks_commands() {
        let shell = MockShell::new();
        shell.enqueue_ok("");
        shell
            .run(ShellCommand {
                program: "ls".into(),
                args: vec!["-la".into()],
                env: Default::default(),
                env_remove: Default::default(),
                pre_validated: false,
                cwd: None,
                timeout: None,
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();
        let cmds = shell.executed_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].program, "ls");
    }
}
