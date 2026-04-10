// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production [`ShellExecutor`] implementation via `tokio::process`.
//!
//! This crate sits at L1 in the dependency graph — it implements the
//! [`nika_kernel::shell::ShellExecutor`] trait using `tokio::process::Command`.
//!
//! Includes a command blocklist for security (NIKA-053 equivalent).

mod blocklist;

use std::time::Instant;

use nika_kernel::shell::{ShellCommand, ShellError, ShellExecutor, ShellResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

/// Production shell executor backed by `tokio::process::Command`.
///
/// Includes command blocklist security. Zero-size type.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioShell;

impl TokioShell {
    /// Create a new shell executor.
    pub fn new() -> Self {
        Self
    }
}

#[async_trait::async_trait]
impl ShellExecutor for TokioShell {
    async fn run(&self, command: ShellCommand) -> Result<ShellResult, ShellError> {
        // Security: check blocklist (always) + shell-mode blocklist (when shell: true)
        let full_command = if command.shell {
            format!("{} {}", command.program, command.args.join(" "))
        } else {
            // Also scan args in non-shell mode (e.g. program: "rm", args: ["-rf", "/"])
            format!("{} {}", command.program, command.args.join(" "))
        };
        blocklist::check_command(&full_command)?;

        if command.shell {
            blocklist::check_shell_mode(&full_command)?;
        }

        let start = Instant::now();

        let mut cmd = if command.shell {
            let mut c = Command::new("sh");
            let shell_cmd = if command.args.is_empty() {
                command.program.clone()
            } else {
                format!("{} {}", command.program, command.args.join(" "))
            };
            c.arg("-c").arg(&shell_cmd);
            c
        } else {
            let mut c = Command::new(&command.program);
            c.args(&command.args);
            c
        };

        // Environment
        for (key, value) in &command.env {
            cmd.env(key, value);
        }

        // Working directory
        if let Some(cwd) = &command.cwd {
            cmd.current_dir(cwd);
        }

        // Stdin
        if command.stdin.is_some() {
            cmd.stdin(std::process::Stdio::piped());
        } else {
            cmd.stdin(std::process::Stdio::null());
        }

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        // Kill the child process when the handle is dropped. Without this,
        // dropping `child` on cancel / timeout / panic would leave a zombie
        // subprocess running until the OS reaps it.
        cmd.kill_on_drop(true);

        // Spawn
        let mut child = cmd.spawn().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                ShellError::NotFound {
                    program: command.program.clone(),
                }
            } else {
                ShellError::Other {
                    reason: e.to_string(),
                }
            }
        })?;

        // Write stdin if provided
        if let Some(stdin_data) = &command.stdin {
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(stdin_data.as_bytes()).await;
                drop(stdin);
            }
        }

        // Wait with optional timeout + cancellation.
        //
        // IMPORTANT: stdout/stderr must be drained CONCURRENTLY with `wait()`.
        // If we wait first and only then read the pipes, a child that writes
        // more than the OS pipe buffer (~64 KB on Linux, ~16 KB on macOS)
        // blocks on write, and `wait()` blocks waiting for the child to exit
        // → deadlock. `tokio::try_join!` polls all three futures in parallel.
        let cancel = command.cancel.clone();

        let child_fut = async {
            let stdout_handle = child.stdout.take();
            let stderr_handle = child.stderr.take();

            async fn drain<R: tokio::io::AsyncRead + Unpin>(
                handle: Option<R>,
            ) -> std::io::Result<Vec<u8>> {
                match handle {
                    Some(mut h) => {
                        let mut buf = Vec::new();
                        h.read_to_end(&mut buf).await?;
                        Ok(buf)
                    }
                    None => Ok(Vec::new()),
                }
            }

            let (status, stdout, stderr) = tokio::try_join!(
                child.wait(),
                drain(stdout_handle),
                drain(stderr_handle),
            )?;
            Ok::<_, std::io::Error>(std::process::Output { status, stdout, stderr })
        };

        let output = match (command.timeout, cancel) {
            (None, None) => child_fut.await.map_err(|e| ShellError::Other {
                reason: e.to_string(),
            })?,
            (Some(t), None) => {
                match tokio::time::timeout(t, child_fut).await {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => {
                        return Err(ShellError::Other {
                            reason: e.to_string(),
                        });
                    }
                    Err(_) => {
                        return Err(ShellError::Timeout {
                            duration_ms: t.as_millis() as u64,
                        });
                    }
                }
            }
            (None, Some(tok)) => {
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => return Err(ShellError::Cancelled),
                    r = child_fut => r.map_err(|e| ShellError::Other {
                        reason: e.to_string(),
                    })?,
                }
            }
            (Some(t), Some(tok)) => {
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => return Err(ShellError::Cancelled),
                    _ = tokio::time::sleep(t) => return Err(ShellError::Timeout {
                        duration_ms: t.as_millis() as u64,
                    }),
                    r = child_fut => r.map_err(|e| ShellError::Other {
                        reason: e.to_string(),
                    })?,
                }
            }
        };

        let duration = start.elapsed();

        Ok(ShellResult {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn echo_simple() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "echo".to_string(),
                args: vec!["hello".to_string()],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();

        assert!(result.success());
        assert_eq!(result.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn shell_mode_pipes() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "echo hello | tr a-z A-Z".to_string(),
                args: vec![],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: None,
                shell: true,
                cancel: None,
            })
            .await
            .unwrap();

        assert!(result.success());
        assert_eq!(result.stdout.trim(), "HELLO");
    }

    #[tokio::test]
    async fn nonzero_exit_code() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "false".to_string(),
                args: vec![],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();

        assert!(!result.success());
        assert_ne!(result.status, 0);
    }

    #[tokio::test]
    async fn timeout_kills_process() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "sleep".to_string(),
                args: vec!["60".to_string()],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_millis(100)),
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await;

        assert!(matches!(result, Err(ShellError::Timeout { .. })));
    }

    #[tokio::test]
    async fn not_found_error() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "nonexistent_binary_xyz_123".to_string(),
                args: vec![],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await;

        assert!(matches!(result, Err(ShellError::NotFound { .. })));
    }

    #[tokio::test]
    async fn custom_env_var() {
        let shell = TokioShell;
        let mut env = std::collections::HashMap::new();
        env.insert("MY_VAR".to_string(), "my_value".to_string());

        let result = shell
            .run(ShellCommand {
                program: "echo $MY_VAR".to_string(),
                args: vec![],
                env,
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: None,
                shell: true,
                cancel: None,
            })
            .await
            .unwrap();

        assert!(result.success());
        assert_eq!(result.stdout.trim(), "my_value");
    }

    #[tokio::test]
    async fn stdin_piped() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "cat".to_string(),
                args: vec![],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: Some("piped input".to_string()),
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();

        assert!(result.success());
        assert_eq!(result.stdout, "piped input");
    }

    #[tokio::test]
    async fn measures_duration() {
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "true".to_string(),
                args: vec![],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(5)),
                stdin: None,
                shell: false,
                cancel: None,
            })
            .await
            .unwrap();

        assert!(result.duration < Duration::from_secs(5));
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TokioShell>();
    }

    #[test]
    fn blocked_command_rejected() {
        // "rm -rf /" should be blocked
        let result = blocklist::check_command("rm -rf /");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn cancelled_before_start_returns_cancelled() {
        use tokio_util::sync::CancellationToken;
        let tok = CancellationToken::new();
        tok.cancel(); // pre-cancel
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "sleep".into(),
                args: vec!["10".into()],
                env: Default::default(),
                cwd: None,
                timeout: None,
                stdin: None,
                shell: false,
                cancel: Some(tok),
            })
            .await;
        assert!(matches!(result, Err(ShellError::Cancelled)));
    }

    #[tokio::test]
    async fn large_output_does_not_deadlock() {
        // Regression test for the pipe buffer deadlock: a child that writes
        // more than the OS pipe buffer (~64 KB on Linux) must not hang waiting
        // for the parent to drain the pipe. 1 MB is well over any platform's
        // pipe buffer capacity.
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                // Emit ~1 MB to stdout via `yes` + `head`.
                program: "yes x | head -c 1048576".into(),
                args: vec![],
                env: Default::default(),
                cwd: None,
                timeout: Some(Duration::from_secs(30)),
                stdin: None,
                shell: true,
                cancel: None,
            })
            .await
            .expect("large output must not deadlock");
        assert!(result.success(), "command should exit 0: {result:?}");
        assert_eq!(
            result.stdout.len(),
            1_048_576,
            "should capture exactly 1 MB of stdout"
        );
    }

    #[tokio::test]
    async fn cancelled_mid_flight_returns_cancelled() {
        use tokio_util::sync::CancellationToken;
        let tok = CancellationToken::new();
        let tok2 = tok.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            tok2.cancel();
        });
        let shell = TokioShell;
        let result = shell
            .run(ShellCommand {
                program: "sleep".into(),
                args: vec!["5".into()],
                env: Default::default(),
                cwd: None,
                timeout: None,
                stdin: None,
                shell: false,
                cancel: Some(tok),
            })
            .await;
        assert!(matches!(result, Err(ShellError::Cancelled)));
    }
}
