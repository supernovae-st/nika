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
        // Security: check blocklist
        let full_command = if command.shell {
            format!("{} {}", command.program, command.args.join(" "))
        } else {
            command.program.clone()
        };
        blocklist::check_command(&full_command)?;

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
                use tokio::io::AsyncWriteExt;
                let _ = stdin.write_all(stdin_data.as_bytes()).await;
                drop(stdin);
            }
        }

        // Wait with optional timeout
        let output = match command.timeout {
            Some(timeout) => {
                // Use wait() + read stdout/stderr separately to allow kill on timeout
                let stdout_handle = child.stdout.take();
                let stderr_handle = child.stderr.take();

                let wait_result = tokio::time::timeout(timeout, async {
                    let status = child.wait().await?;
                    let stdout = match stdout_handle {
                        Some(mut out) => {
                            use tokio::io::AsyncReadExt;
                            let mut buf = Vec::new();
                            out.read_to_end(&mut buf).await?;
                            buf
                        }
                        None => Vec::new(),
                    };
                    let stderr = match stderr_handle {
                        Some(mut err) => {
                            use tokio::io::AsyncReadExt;
                            let mut buf = Vec::new();
                            err.read_to_end(&mut buf).await?;
                            buf
                        }
                        None => Vec::new(),
                    };
                    Ok::<_, std::io::Error>((status, stdout, stderr))
                })
                .await;

                match wait_result {
                    Ok(Ok((status, stdout, stderr))) => std::process::Output {
                        status,
                        stdout,
                        stderr,
                    },
                    Ok(Err(e)) => {
                        return Err(ShellError::Other {
                            reason: e.to_string(),
                        });
                    }
                    Err(_) => {
                        // Kill the process on timeout
                        let _ = child.kill().await;
                        return Err(ShellError::Timeout {
                            duration_ms: timeout.as_millis() as u64,
                        });
                    }
                }
            }
            None => child.wait_with_output().await.map_err(|e| ShellError::Other {
                reason: e.to_string(),
            })?,
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
}
