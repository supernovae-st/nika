// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

        // SECURITY CHECK: validate resolved command for control characters and general blocklist
        // BUG-032: pass raw template so interpreter patterns (python3 -c, node -e) written
        // by the developer in YAML are recognized as intentional and allowed through.
        let is_shell = params.shell == Some(true);
        crate::runtime::security::validate_exec_command_full(
            &resolved_cmd,
            is_shell,
            Some(&params.command),
        )?;

        // SEC-2: BLOCK unescaped template bindings in shell: true commands.
        // Any {{with.*}} or {{inputs.*}} without |shell transform is a shell injection
        // vector — the resolved value could contain metacharacters like ; && || etc.
        // Exempted: bindings inside single quotes (shell doesn't interpret those).
        if is_shell {
            use std::sync::LazyLock;
            static BINDING_RE: LazyLock<regex::Regex> = LazyLock::new(|| {
                regex::Regex::new(r"\{\{(with|inputs|context)\.[^}]+\}\}").expect("valid regex")
            });
            let mut unsafe_bindings = Vec::new();
            for cap in BINDING_RE.find_iter(&params.command) {
                let m = cap.as_str();
                if m.contains("| shell") || m.contains("|shell") {
                    continue; // shell-escaped, safe
                }
                if is_inside_single_quotes(&params.command, cap.start()) {
                    // SEC-2b: Single-quote context is safe ONLY if the resolved
                    // value doesn't contain '. POSIX single quotes have no escape
                    // mechanism — a ' in the value closes the quoting → injection.
                    let resolved = template_resolve(m, bindings, datastore)?;
                    if !value_safe_in_single_quotes(&resolved) {
                        let reason = format!(
                            "Binding {} is inside single quotes but resolved to a value \
                             containing a single quote, which breaks shell quoting. \
                             Use | shell transform instead of single quotes.",
                            m
                        );
                        self.event_log.emit(EventKind::PolicyBlocked {
                            task_id: Arc::clone(task_id),
                            verb: "exec".to_string(),
                            policy_type: "shell_injection".to_string(),
                            reason: reason.clone(),
                        });
                        return Err(NikaError::BlockedCommand {
                            command: crate::util::redact_secrets(&params.command),
                            reason,
                        });
                    }
                    continue; // value is safe inside single quotes
                }
                unsafe_bindings.push(m.to_string());
            }
            if !unsafe_bindings.is_empty() {
                let reason = format!(
                    "shell: true with unescaped binding(s): {}. \
                     Use | shell transform (e.g. {{{{with.val | shell}}}}) \
                     or single-quote the binding",
                    unsafe_bindings.join(", ")
                );
                self.event_log.emit(EventKind::PolicyBlocked {
                    task_id: Arc::clone(task_id),
                    verb: "exec".to_string(),
                    policy_type: "shell_injection".to_string(),
                    reason: reason.clone(),
                });
                return Err(NikaError::BlockedCommand {
                    command: crate::util::redact_secrets(&params.command),
                    reason,
                });
            }
        }

        // SHELL INJECTION CHECK: detect $(), backticks, <( that were INJECTED via
        // template data (not written by the dev in the YAML). Compares raw template
        // vs resolved command — patterns present in both are intentional.
        if is_shell {
            if let Err(e) =
                crate::runtime::security::check_shell_data_injection(&params.command, &resolved_cmd)
            {
                self.event_log.emit(EventKind::PolicyBlocked {
                    task_id: Arc::clone(task_id),
                    verb: "exec".to_string(),
                    policy_type: "shell_data_injection".to_string(),
                    reason: e.to_string(),
                });
                return Err(e);
            }
        }

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
                command = %redact_for_event(&resolved_cmd),
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
        let output = if params.shell == Some(true) {
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

            // Set working directory: explicit cwd (with security check) or default from config
            if let Some(ref cwd) = params.cwd {
                let resolved_cwd = template_resolve(cwd, bindings, datastore)?;
                let resolved = std::path::Path::new(resolved_cwd.as_ref())
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
            } else if let Some(default_cwd) = self.resolve_default_exec_cwd() {
                // Apply default cwd from [tools] working_dir config
                cmd.current_dir(default_cwd);
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
            let exec_fut = tokio::time::timeout(exec_deadline, child.wait_with_output());
            tokio::select! {
                biased;
                _ = self.cancel_token.cancelled() => {
                    // child is dropped here -> kill_on_drop sends SIGKILL
                    return Err(NikaError::TaskCancelled {
                        task_id: task_id.to_string(),
                        reason: "workflow cancelled during exec".to_string(),
                    });
                }
                result = exec_fut => match result {
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

            // Set working directory: explicit cwd (with security check) or default from config
            if let Some(ref cwd) = params.cwd {
                let resolved_cwd = template_resolve(cwd, bindings, datastore)?;
                let resolved = std::path::Path::new(resolved_cwd.as_ref())
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
            } else if let Some(default_cwd) = self.resolve_default_exec_cwd() {
                // Apply default cwd from [tools] working_dir config
                cmd.current_dir(default_cwd);
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
            let exec_fut = tokio::time::timeout(exec_deadline, child.wait_with_output());
            tokio::select! {
                biased;
                _ = self.cancel_token.cancelled() => {
                    // child is dropped here -> kill_on_drop sends SIGKILL
                    return Err(NikaError::TaskCancelled {
                        task_id: task_id.to_string(),
                        reason: "workflow cancelled during exec".to_string(),
                    });
                }
                result = exec_fut => match result {
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

        // Truncate stdout if it exceeds max_stdout (default: 50 MB)
        const DEFAULT_MAX_STDOUT: u64 = 50 * 1024 * 1024;
        let max = params.max_stdout.unwrap_or(DEFAULT_MAX_STDOUT) as usize;
        let stdout = if output.stdout.len() > max {
            tracing::warn!(
                task_id = %task_id,
                stdout_bytes = output.stdout.len(),
                limit_bytes = max,
                "exec: stdout truncated ({} bytes > {} limit)",
                output.stdout.len(),
                max,
            );
            // Truncate to max bytes, then find a valid UTF-8 boundary
            let truncated = &output.stdout[..max];
            let s = String::from_utf8_lossy(truncated);
            format!(
                "{}\n... [truncated: {} bytes total, {} byte limit]",
                s.trim_end(),
                output.stdout.len(),
                max,
            )
        } else {
            String::from_utf8_lossy(&output.stdout).trim().to_string()
        };

        Ok(stdout)
    }
}

/// Check if a value is safe to interpolate inside POSIX single quotes.
///
/// In POSIX shell, single quotes have NO escape mechanism — a `'` in the value
/// unconditionally closes the quoting, enabling command injection.
/// Example: template `echo '{{with.val}}'` with val = `x' && rm -rf / && echo '`
/// resolves to `echo 'x' && rm -rf / && echo ''` — shell injection.
fn value_safe_in_single_quotes(value: &str) -> bool {
    !value.contains('\'')
}

/// Check if position `pos` in `cmd` falls inside single quotes.
/// Single-quoted content in shell is not subject to metacharacter expansion,
/// so template bindings inside single quotes are safe without `| shell`.
fn is_inside_single_quotes(cmd: &str, pos: usize) -> bool {
    let mut in_single = false;
    for (i, c) in cmd.char_indices() {
        if i == pos {
            return in_single;
        }
        if c == '\'' && !in_single {
            in_single = true;
        } else if c == '\'' && in_single {
            in_single = false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_quote_detection() {
        // Inside single quotes — safe, no warning
        assert!(is_inside_single_quotes("jq --arg x '{{inputs.url}}' .", 12));
        // Outside quotes — unsafe, should warn
        assert!(!is_inside_single_quotes("echo {{inputs.url}}", 5));
        // Inside double quotes — still unsafe for shell
        assert!(!is_inside_single_quotes("echo \"{{inputs.url}}\"", 6));
        // After closing single quote — unsafe
        assert!(!is_inside_single_quotes("echo 'safe' {{inputs.url}}", 12));
    }

    // ═══════════════════════════════════════════════════════════════
    // SEC-2b: Single-quote breakout detection
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn value_safe_in_single_quotes_clean_value() {
        assert!(value_safe_in_single_quotes("hello world"));
        assert!(value_safe_in_single_quotes("no-special-chars"));
        assert!(value_safe_in_single_quotes(""));
        // Double quotes are fine inside single quotes
        assert!(value_safe_in_single_quotes("say \"hello\""));
    }

    #[test]
    fn value_safe_in_single_quotes_rejects_quote() {
        // Any single quote in value = breakout risk
        assert!(!value_safe_in_single_quotes(
            "hello' && echo pwned && echo '"
        ));
        assert!(!value_safe_in_single_quotes("it's a trap"));
        assert!(!value_safe_in_single_quotes("O'Brien"));
        assert!(!value_safe_in_single_quotes("'"));
    }
}
