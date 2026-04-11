// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Exec verb bridge — delegates to `nika-verb-exec` (S13-B2).
//!
//! This file is now a thin bridge: template resolution + security
//! validation + policy checks stay here (engine-internal helpers),
//! then subprocess execution is delegated to `nika_verb_exec::run()`
//! via the `ShellExecutor` kernel trait (TokioShell).
//!
//! The actual execution logic lives in the `nika-verb-exec` crate.
//! Session 14 dissolves this bridge when `TaskExecutor` is deleted.

use std::sync::Arc;

use tracing::instrument;

use nika_clock::SystemClock;
use nika_exec_runner::TokioShell;
use nika_fs::TokioFs;
use nika_kernel::caps::ExecCaps;
use nika_verb_exec::{ExecInput, VerbExecError};

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

        // ═══════════════════════════════════════════════════════════════
        // PHASE 2: resolve cwd + env with security checks
        // ═══════════════════════════════════════════════════════════════

        // Resolve explicit cwd with security check against workflow_base_dir.
        let explicit_cwd = if let Some(ref cwd) = params.cwd {
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
            Some(resolved)
        } else {
            None
        };

        // Resolve env vars with template expansion + validation.
        let resolved_env: Vec<(String, String)> = if let Some(ref env_vars) = params.env {
            let pairs: Vec<(String, String)> = env_vars
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect();
            crate::runtime::security::validate_env_vars(&pairs)?;
            let mut out = Vec::with_capacity(pairs.len());
            for (key, value) in env_vars {
                let resolved_value = template_resolve(value, bindings, datastore)?;
                out.push((key.clone(), resolved_value.into_owned()));
            }
            out
        } else {
            Vec::new()
        };

        // Sensitive env vars to strip from the child's inherited environment.
        let env_remove: Vec<String> = crate::runtime::security::sensitive_env_vars()
            .into_iter()
            .map(|s| s.to_string())
            .collect();

        // ═══════════════════════════════════════════════════════════════
        // PHASE 3: delegate to nika-verb-exec (GATE-S13-3 workaround)
        //
        // We clone PolicyEnforcer out of the RwLock BEFORE building ExecCaps.
        // parking_lot::RwLockReadGuard is !Send — holding it across the
        // nika_verb_exec::run() await would break on multi-threaded tokio.
        // The clone is cheap (PolicyConfig + TokenBudget).
        // ═══════════════════════════════════════════════════════════════

        let policy_clone = self.policy_enforcer.read().clone();
        let shell = TokioShell;
        let clock = SystemClock;
        let fs = TokioFs;

        let caps = ExecCaps::new(
            &shell,
            &policy_clone,
            &clock,
            &fs,
            &self.cancel_token,
            &self.workflow_base_dir,
            self.working_dir_mode.as_deref(),
            self.project_root.as_deref(),
        );

        let exec_deadline = params
            .timeout
            .map(std::time::Duration::from_secs)
            .unwrap_or(EXEC_TIMEOUT);

        let input = ExecInput {
            command: &resolved_cmd,
            shell: params.shell == Some(true),
            cwd: explicit_cwd.as_deref(),
            timeout: Some(exec_deadline),
            env: resolved_env,
            env_remove,
            max_stdout: params.max_stdout,
            task_id: Arc::clone(task_id),
        };

        nika_verb_exec::run(&input, &caps, &self.event_log)
            .await
            .map_err(map_verb_exec_error)
    }
}

/// Convert a `VerbExecError` to the engine's `NikaError` at the bridge boundary.
fn map_verb_exec_error(err: VerbExecError) -> NikaError {
    match err {
        VerbExecError::NonZeroExit { exit_code, stderr } => ExecutionError::ExecFailed {
            reason: format!("Command failed (exit {exit_code}): {stderr}"),
        }
        .into(),
        VerbExecError::Cancelled { task_id } => NikaError::TaskCancelled {
            task_id,
            reason: "workflow cancelled during exec".to_string(),
        },
        VerbExecError::Timeout { duration_ms } => ExecutionError::ExecFailed {
            reason: format!("Command timed out after {}s", duration_ms / 1000),
        }
        .into(),
        VerbExecError::NotFound { program } => NikaError::ExecError {
            reason: format!("Failed to spawn command: program '{}' not found", program),
        },
        VerbExecError::Parse { reason } => NikaError::ExecError { reason },
        VerbExecError::Shell { reason } => ExecutionError::ExecFailed { reason }.into(),
        // VerbExecError is `#[non_exhaustive]` (invariant #25). When S15/S16
        // grows new variants, they fall through to a generic ExecFailed here
        // and must be explicitly mapped in a follow-up commit. Using
        // `format!("{err:?}")` preserves the full variant name for triage.
        other => ExecutionError::ExecFailed {
            reason: format!("exec: unmapped verb error variant: {other:?}"),
        }
        .into(),
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
