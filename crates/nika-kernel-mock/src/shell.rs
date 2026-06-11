// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockShell` — scripted shell command results with call recording.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;

use nika_kernel::process::{ShellCancelDyn, ShellCommand, ShellError, ShellResult, ShellRunDyn};

/// Programmable shell executor for tests.
///
/// Enqueue results with `enqueue_ok` / `enqueue_fail` / `enqueue_err`.
/// Inspect recorded commands with `executed_commands`.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct MockShell {
    results: Arc<Mutex<VecDeque<Result<ShellResult, ShellError>>>>,
    commands: Arc<Mutex<Vec<ShellCommand>>>,
}

impl MockShell {
    /// Create a new empty mock shell.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Enqueue a successful result (exit 0) with given stdout.
    #[must_use]
    pub fn enqueue_ok(self, stdout: impl Into<String>) -> Self {
        let result = ShellResult::new(0, stdout, "", Duration::from_millis(1));
        self.results.lock().push_back(Ok(result));
        self
    }

    /// Enqueue a failed result with given exit code and stderr.
    #[must_use]
    pub fn enqueue_fail(self, status: i32, stderr: impl Into<String>) -> Self {
        let result = ShellResult::new(status, "", stderr, Duration::from_millis(1));
        self.results.lock().push_back(Ok(result));
        self
    }

    /// Enqueue an error.
    #[must_use]
    pub fn enqueue_err(self, error: ShellError) -> Self {
        self.results.lock().push_back(Err(error));
        self
    }

    /// Get all recorded commands.
    #[must_use]
    pub fn executed_commands(&self) -> Vec<ShellCommand> {
        self.commands.lock().clone()
    }
}

// Send-variant canon (uniform across effect impls · the trait_variant
// blanket hands `ShellRun`/`ShellCancel` back to existing consumers).
impl ShellRunDyn for MockShell {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        self.commands.lock().push(command);
        self.results.lock().pop_front().unwrap_or_else(|| {
            Err(ShellError::Other {
                reason: "MockShell: result queue exhausted".into(),
            })
        })
    }
}

impl ShellCancelDyn for MockShell {
    async fn cancel(&self, id: &str) -> Result<(), ShellError> {
        Err(ShellError::Cancelled { id: id.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enqueue_and_run() {
        let shell = MockShell::new().enqueue_ok("hello");
        let cmd = ShellCommand::new("echo").arg("hello");
        let result = shell.run(cmd).await.unwrap();
        assert!(result.success());
        assert_eq!(result.stdout, "hello");
    }

    #[tokio::test]
    async fn enqueue_fail_and_run() {
        let shell = MockShell::new().enqueue_fail(1, "error");
        let result = shell.run(ShellCommand::new("false")).await.unwrap();
        assert!(!result.success());
        assert_eq!(result.stderr, "error");
    }

    #[tokio::test]
    async fn fifo_ordering() {
        let shell = MockShell::new().enqueue_ok("first").enqueue_ok("second");
        let r1 = shell.run(ShellCommand::new("a")).await.unwrap();
        let r2 = shell.run(ShellCommand::new("b")).await.unwrap();
        assert_eq!(r1.stdout, "first");
        assert_eq!(r2.stdout, "second");
    }

    #[tokio::test]
    async fn empty_queue_returns_error() {
        let shell = MockShell::new();
        let result = shell.run(ShellCommand::new("ls")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn records_commands() {
        let shell = MockShell::new().enqueue_ok("");
        let _ = shell.run(ShellCommand::new("git").arg("status")).await;
        let cmds = shell.executed_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].program, "git");
        assert_eq!(cmds[0].args, vec!["status"]);
    }

    #[tokio::test]
    async fn enqueue_err_populates_queue() {
        let shell = MockShell::new().enqueue_err(ShellError::NotFound {
            program: "missing-bin".into(),
        });
        let result = shell.run(ShellCommand::new("missing-bin")).await;
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), ShellError::NotFound { program } if program == "missing-bin")
        );
    }

    #[tokio::test]
    async fn cancel_returns_cancelled_error() {
        let shell = MockShell::new();
        let result = shell.cancel("job-42").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), ShellError::Cancelled { id } if id == "job-42"));
    }

    #[tokio::test]
    async fn clone_shares_state() {
        let s1 = MockShell::new().enqueue_ok("shared");
        let s2 = s1.clone();
        // Run via s1, verify s2 sees the recorded command.
        let _ = s1.run(ShellCommand::new("echo")).await;
        let cmds = s2.executed_commands();
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].program, "echo");
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_shell_is_send_sync() {
        _assert_send_sync::<MockShell>();
    }

    #[test]
    fn mock_shell_satisfies_shell_executor() {
        fn _accepts<T: nika_kernel::ShellExecutor>(_: &T) {}
        let s = MockShell::new();
        _accepts(&s);
    }
}
