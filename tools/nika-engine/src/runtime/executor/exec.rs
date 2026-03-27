//! Exec verb implementation for TaskExecutor
//!
//! Contains `run_exec` for shell command execution.

use std::sync::Arc;
use std::time::Instant;

use tracing::instrument;

use crate::ast::ExecParams;
use crate::binding::{template_resolve, ResolvedBindings};
use crate::error::NikaError;
use crate::error_domains::ExecutionError;
use crate::event::EventKind;
use crate::runtime::policy::PolicyDecision;
use crate::store::RunContext;
use crate::util::EXEC_TIMEOUT;

use super::verbs::redact_for_event;
use super::TaskExecutor;

impl TaskExecutor {
    #[instrument(skip(self, bindings, datastore), fields(%task_id))]
    pub(super) async fn run_exec(
        &self,
        task_id: &Arc<str>,
        params: &ExecParams,
        bindings: &ResolvedBindings,
        datastore: &RunContext,
    ) -> Result<String, NikaError> {
        // Resolve {{with.alias}} templates
        // Note: Shell escaping is NOT applied by default.
        // For values that need shell escaping, use {{with.alias|shell}} syntax.
        let resolved_cmd = template_resolve(&params.command, bindings, datastore)?;

        // SECURITY CHECK: validate command for control characters and blocklist
        // In shell mode, also block command substitution ($(), backticks)
        let is_shell = params.shell == Some(true);
        crate::runtime::security::validate_exec_command_with_shell(&resolved_cmd, is_shell)?;

        // POLICY CHECK: exec verb
        let policy_decision = self.policy_enforcer.read().check_exec(&resolved_cmd);
        if let PolicyDecision::Block(reason) = policy_decision {
            // EMIT: PolicyBlocked
            self.event_log.emit(EventKind::PolicyBlocked {
                task_id: Arc::clone(task_id),
                verb: "exec".to_string(),
                policy_type: "command_blocklist".to_string(),
                reason: reason.clone(),
            });
            tracing::warn!(
                task_id = %task_id,
                command = %resolved_cmd,
                reason = %reason,
                "exec: blocked by policy"
            );
            return Err(NikaError::PolicyViolation { reason });
        }

        // EMIT: TemplateResolved (redacted to avoid leaking secrets)
        self.event_log.emit(EventKind::TemplateResolved {
            task_id: Arc::clone(task_id),
            template: params.command.clone(),
            result: redact_for_event(&resolved_cmd),
        });

        // Use per-task timeout if specified, otherwise fall back to global default
        let exec_deadline = params
            .timeout
            .map(std::time::Duration::from_secs)
            .unwrap_or(EXEC_TIMEOUT);

        // Shell-free execution by default, opt-in to shell mode
        // Support for env vars
        let exec_start = Instant::now();
        let output =
            if params.shell == Some(true) {
                // Shell mode: sh -c on Unix, cmd.exe /C on Windows
                let (shell, flag) = if cfg!(windows) {
                    ("cmd", "/C")
                } else {
                    ("sh", "-c")
                };
                tracing::debug!(task_id = %task_id, "exec: using shell mode ({shell})");
                let mut cmd = tokio::process::Command::new(shell);
                cmd.arg(flag).arg(resolved_cmd.as_ref());

                // Pipe stdout/stderr for capture (required by spawn + wait_with_output)
                cmd.stdout(std::process::Stdio::piped());
                cmd.stderr(std::process::Stdio::piped());

                // Strip sensitive env vars from child process
                crate::runtime::security::strip_sensitive_env_vars(&mut cmd);

                // Set working directory if specified (with path traversal protection)
                // Resolve templates in cwd (e.g. {{inputs.output_dir}}, {{with.dir}})
                if let Some(ref cwd) = params.cwd {
                    let resolved_cwd = template_resolve(cwd, bindings, datastore)?;
                    let resolved =
                        std::path::Path::new(resolved_cwd.as_ref())
                            .canonicalize()
                            .map_err(|e| NikaError::ExecError {
                                reason: format!("Invalid cwd '{}': {}", resolved_cwd, e),
                            })?;
                    let working_dir = self
                        .workflow_base_dir
                        .canonicalize()
                        .unwrap_or_else(|_| self.workflow_base_dir.clone());
                    if !resolved.starts_with(&working_dir) {
                        return Err(ExecutionError::ExecFailed {
                            reason: format!(
                                "Security: exec cwd '{}' escapes working directory '{}'",
                                resolved_cwd,
                                working_dir.display()
                            ),
                        }
                        .into());
                    }
                    cmd.current_dir(resolved);
                }

                // Add environment variables if specified (validate first)
                if let Some(ref env_vars) = params.env {
                    let pairs: Vec<(String, String)> = env_vars
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    crate::runtime::security::validate_env_vars(&pairs)?;
                    for (key, value) in env_vars {
                        let resolved_value = template_resolve(value, bindings, datastore)?;
                        cmd.env(key, resolved_value.as_ref());
                    }
                }

                // kill_on_drop ensures child is killed when dropped on timeout (prevents orphans)
                cmd.kill_on_drop(true);
                let child = cmd.spawn().map_err(|e| NikaError::ExecError {
                    reason: format!("Failed to spawn command: {}", e),
                })?;
                match tokio::time::timeout(exec_deadline, child.wait_with_output()).await {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => {
                        return Err(ExecutionError::ExecFailed {
                            reason: format!("Failed to execute command: {}", e),
                        }
                        .into());
                    }
                    Err(_) => {
                        // child is dropped here -> kill_on_drop sends SIGKILL
                        return Err(ExecutionError::ExecFailed {
                            reason: format!("Command timed out after {}s", exec_deadline.as_secs()),
                        }
                        .into());
                    }
                }
            } else {
                // Shell-free mode (default): parse with shlex, execute directly
                tracing::debug!(task_id = %task_id, "exec: using shell-free mode (shlex)");
                let parts = shlex::split(&resolved_cmd).ok_or_else(|| NikaError::ExecError {
                    reason: format!(
                        "Failed to parse command (unbalanced quotes?): {}",
                        resolved_cmd
                    ),
                })?;

                if parts.is_empty() {
                    return Err(ExecutionError::ExecFailed {
                        reason: "Empty command".to_string(),
                    }
                    .into());
                }

                let mut cmd = tokio::process::Command::new(&parts[0]);
                cmd.args(&parts[1..]);

                // Pipe stdout/stderr for capture (required by spawn + wait_with_output)
                cmd.stdout(std::process::Stdio::piped());
                cmd.stderr(std::process::Stdio::piped());

                // Strip sensitive env vars from child process
                crate::runtime::security::strip_sensitive_env_vars(&mut cmd);

                // Set working directory if specified (with path traversal protection)
                // Resolve templates in cwd (e.g. {{inputs.output_dir}}, {{with.dir}})
                if let Some(ref cwd) = params.cwd {
                    let resolved_cwd = template_resolve(cwd, bindings, datastore)?;
                    let resolved =
                        std::path::Path::new(resolved_cwd.as_ref())
                            .canonicalize()
                            .map_err(|e| NikaError::ExecError {
                                reason: format!("Invalid cwd '{}': {}", resolved_cwd, e),
                            })?;
                    let working_dir = self
                        .workflow_base_dir
                        .canonicalize()
                        .unwrap_or_else(|_| self.workflow_base_dir.clone());
                    if !resolved.starts_with(&working_dir) {
                        return Err(ExecutionError::ExecFailed {
                            reason: format!(
                                "Security: exec cwd '{}' escapes working directory '{}'",
                                resolved_cwd,
                                working_dir.display()
                            ),
                        }
                        .into());
                    }
                    cmd.current_dir(resolved);
                }

                // Add environment variables if specified (validate first)
                if let Some(ref env_vars) = params.env {
                    let pairs: Vec<(String, String)> = env_vars
                        .iter()
                        .map(|(k, v)| (k.clone(), v.clone()))
                        .collect();
                    crate::runtime::security::validate_env_vars(&pairs)?;
                    for (key, value) in env_vars {
                        let resolved_value = template_resolve(value, bindings, datastore)?;
                        cmd.env(key, resolved_value.as_ref());
                    }
                }

                // kill_on_drop ensures child is killed when dropped on timeout (prevents orphans)
                cmd.kill_on_drop(true);
                let child = cmd.spawn().map_err(|e| NikaError::ExecError {
                    reason: format!("Failed to spawn command: {}", e),
                })?;
                match tokio::time::timeout(exec_deadline, child.wait_with_output()).await {
                    Ok(Ok(out)) => out,
                    Ok(Err(e)) => {
                        return Err(ExecutionError::ExecFailed {
                            reason: format!("Failed to execute command: {}", e),
                        }
                        .into());
                    }
                    Err(_) => {
                        // child is dropped here -> kill_on_drop sends SIGKILL
                        return Err(ExecutionError::ExecFailed {
                            reason: format!("Command timed out after {}s", exec_deadline.as_secs()),
                        }
                        .into());
                    }
                }
            };

        // EMIT: ExecCompleted (emitted for both success and failure)
        let exec_duration_ms = exec_start.elapsed().as_millis() as u64;
        let exit_code = output.status.code().unwrap_or(-1);
        self.event_log.emit(EventKind::ExecCompleted {
            task_id: Arc::clone(task_id),
            exit_code,
            stdout_len: output.stdout.len(),
            stderr_len: output.stderr.len(),
            duration_ms: exec_duration_ms,
        });

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ExecutionError::ExecFailed {
                reason: format!("Command failed: {}", stderr),
            }
            .into());
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }
}
