// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task dispatch — single task execution with binding resolution, retry, and fallback.
//!
//! Extracted from runner.rs. Contains `execute_task_iteration()` which handles the
//! complete lifecycle of a single task: bindings → when: → preset → lower → execute →
//! retry → structured output → artifacts → record compression.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use serde_json::Value;
use tracing::{debug, info};

use crate::ast::analyzed::{AnalyzedTask, OnErrorAction};
use crate::ast::artifact::ArtifactsConfig;
use crate::ast::lower::{lower_action, lower_output};
use crate::ast::TaskAction;
use crate::binding::ResolvedBindings;
use crate::event::{EventKind, EventLog};
use crate::store::{RunContext, TaskResult};

use super::artifact_processor::process_task_artifacts;
use super::executor::TaskExecutor;
use super::output::make_task_result;
use super::structured_output::StructuredOutputEngine;
use super::structured_retry::{execute_with_retry, get_retry_config, is_retryable};

/// Result of a single task iteration (regular or for_each item).
pub(crate) struct IterationResult {
    /// ID used for storage (task_id for regular, indexed for for_each)
    pub store_id: Arc<str>,
    /// The actual task result
    pub result: TaskResult,
    /// For for_each: (parent_id, index) to enable aggregation
    pub for_each_info: Option<(Arc<str>, usize)>,
}

/// Evaluate a template-resolved string for truthiness.
///
/// Falsy values: empty string, "false", "null", "0", "0.0".
pub(crate) fn is_truthy(value: &str) -> bool {
    let trimmed = value.trim();
    !matches!(trimmed, "" | "false" | "null" | "0" | "0.0")
}

/// Sprint 2 Item 5: compute the runtime trust level of a task's output.
///
/// Mirrors the compile-time `TaintAnalyzer::compute_output_trust` rules
/// but operates on the actual upstream task results via
/// `datastore.get_trust()`. Used to populate `TaskResult::trust_level`
/// and emit `TrustLevelAssigned` events so the SecuritySummary trust
/// counters can increment and downstream taint propagation works.
fn compute_runtime_trust(
    task: &AnalyzedTask,
    datastore: &RunContext,
) -> nika_core::trust::TrustLevel {
    use nika_core::ast::analyzed::AnalyzedTaskAction;
    use nika_core::binding::BindingSource;
    use nika_core::trust::{builtin_output_trust, TrustLevel};

    // Compute input trust by merging upstream binding sources.
    let mut input_trust = TrustLevel::Trusted;
    for entry in task.with_spec.values() {
        let binding_trust = match &entry.source.source {
            BindingSource::Task(tid) => datastore
                .get_trust(tid.as_ref())
                .unwrap_or(TrustLevel::Trusted),
            BindingSource::Input(_) | BindingSource::LoopVar(_) => {
                datastore.invocation_source().input_trust()
            }
            BindingSource::Context(_) | BindingSource::Vault { .. } | BindingSource::Env(_) => {
                TrustLevel::Trusted
            }
        };
        input_trust = input_trust.merge(binding_trust);
    }

    match &task.action {
        AnalyzedTaskAction::Fetch(_) => TrustLevel::Untrusted,
        AnalyzedTaskAction::Exec(_) => TrustLevel::Untrusted,
        AnalyzedTaskAction::Invoke(inv) => {
            if inv.tool.starts_with("nika:") {
                builtin_output_trust(&inv.tool, input_trust)
            } else {
                TrustLevel::Untrusted
            }
        }
        AnalyzedTaskAction::Infer(_) => {
            if input_trust.is_untrusted() {
                TrustLevel::ModelTainted
            } else {
                TrustLevel::ModelGenerated
            }
        }
        AnalyzedTaskAction::Agent(_) => TrustLevel::ModelTainted,
    }
}

/// Execute a single task iteration (used for both regular tasks and for_each items).
///
/// Bridge conversions (`lower_action`, `lower_output`) happen here at the
/// `TaskExecutor` boundary — the rest of Runner works with `AnalyzedTask`.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn execute_task_iteration(
    task: Arc<AnalyzedTask>,
    task_id: Arc<str>,
    parent_task_id: Arc<str>,
    datastore: RunContext,
    executor: TaskExecutor,
    event_log: EventLog,
    for_each_binding: Option<(String, Value, usize)>,
    workflow_artifacts: Option<ArtifactsConfig>,
    base_path: PathBuf,
    is_fallback_execution: bool,
) -> IterationResult {
    // ── Nika Shield: scope tokio::task_local! state once for the whole
    //    iteration so any builtin tool dispatched downstream (Item 4
    //    nika:run, Item 3b path-recon, ...) can read the calling task's
    //    trust level + elevation flag without modifying BuiltinTool::call
    //    or its 24 implementations (P0-5 hardening).
    //
    //    Trust is read from the datastore — Sprint 1 already populates it
    //    via TaskExecutor when each task completes. Elevation comes from
    //    the AnalyzedTask itself (`trust: elevated` in the YAML).
    let shield_task_trust = datastore
        .get_trust(&task_id)
        .unwrap_or(nika_core::trust::TrustLevel::Trusted);
    let shield_task_elevated = task.trust_elevated;
    let shield_task_id_arc = Arc::clone(&task_id);
    // Use the executor's effective working dir for file tools. This respects
    // with_base_path() and with_working_dir_mode("project") so that relative
    // paths like ./data.txt resolve to the correct directory.
    let working_dir_for_scope = executor.file_tool_working_dir();
    crate::runtime::builtin::run::CURRENT_TASK_ID
        .scope(Some(shield_task_id_arc), async move {
            crate::runtime::builtin::run::CURRENT_TASK_TRUST
                .scope(shield_task_trust, async move {
                    crate::runtime::builtin::run::CURRENT_TASK_ELEVATED
                        .scope(shield_task_elevated, async move {
                            // Scope the current working dir for nika-builtin file tools.
                            // FileToolContext.validate_path() reads this to resolve paths.
                            nika_kernel::task_local::CURRENT_WORKING_DIR
                                .scope(Some(working_dir_for_scope), async move {
                                    execute_task_iteration_inner(
                                        task,
                                        task_id,
                                        parent_task_id,
                                        datastore,
                                        executor,
                                        event_log,
                                        for_each_binding,
                                        workflow_artifacts,
                                        base_path,
                                        is_fallback_execution,
                                    )
                                    .await
                                })
                                .await
                        })
                        .await
                })
                .await
        })
        .await
}

#[allow(clippy::too_many_arguments)]
async fn execute_task_iteration_inner(
    task: Arc<AnalyzedTask>,
    task_id: Arc<str>,
    parent_task_id: Arc<str>,
    datastore: RunContext,
    executor: TaskExecutor,
    event_log: EventLog,
    for_each_binding: Option<(String, Value, usize)>,
    workflow_artifacts: Option<ArtifactsConfig>,
    base_path: PathBuf,
    is_fallback_execution: bool,
) -> IterationResult {
    let start = Instant::now();

    // Extract for_each info if present
    let for_each_info = for_each_binding
        .as_ref()
        .map(|(_, _, idx)| (Arc::clone(&parent_task_id), *idx));

    // Check cancellation before binding resolution (which is synchronous
    // and could involve deep JSON path traversal on large outputs)
    if executor.is_cancelled() {
        event_log.emit(EventKind::TaskCancelled {
            task_id: Arc::clone(&task_id),
            reason: "Cancelled before binding resolution".to_string(),
        });
        return IterationResult {
            store_id: task_id,
            result: TaskResult::skipped("Cancelled before binding resolution"),
            for_each_info,
        };
    }

    // Build bindings from with: spec (always present in AnalyzedTask)
    let (mut bindings, binding_events) = match ResolvedBindings::from_with_spec_traced(
        Some(&task.with_spec),
        &datastore,
        &task_id,
    ) {
        Ok(result) => result,
        Err(e) => {
            let duration = start.elapsed();
            event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(&task_id),
                error: e.to_string(),
                duration_ms: duration.as_millis() as u64,
                error_code: Some(e.code().to_string()),
            });
            return IterationResult {
                store_id: task_id,
                result: TaskResult::failed(e.to_string(), duration),
                for_each_info,
            };
        }
    };
    // Emit collected binding events
    for event in binding_events {
        event_log.emit(event);
    }

    // Add for_each binding if present (item value + iteration index)
    if let Some((var_name, value, idx)) = for_each_binding {
        bindings.set(&var_name, value);
        bindings.set(
            "for_each_index",
            Value::Number(serde_json::Number::from(idx)),
        );
    }

    // Evaluate when: conditional — skip task if condition is falsy
    if let Some(ref when_expr) = task.when {
        use crate::binding::template_resolve;
        match template_resolve(when_expr, &bindings, &datastore) {
            Ok(resolved) => {
                if !is_truthy(&resolved) {
                    event_log.emit(EventKind::TaskSkipped {
                        task_id: Arc::clone(&task_id),
                        reason: format!("when: condition evaluated to '{}'", resolved),
                    });
                    return IterationResult {
                        store_id: task_id,
                        result: TaskResult::skipped(format!(
                            "Skipped: when '{}' → '{}'",
                            when_expr, resolved
                        )),
                        for_each_info,
                    };
                }
            }
            Err(e) => {
                event_log.emit(EventKind::TaskSkipped {
                    task_id: Arc::clone(&task_id),
                    reason: format!("when: template resolution failed: {}", e),
                });
                return IterationResult {
                    store_id: task_id,
                    result: TaskResult::skipped(format!("Skipped: when resolution failed: {}", e)),
                    for_each_info,
                };
            }
        }
    }

    // Enforce context_budget if configured on this task
    let resolved_budget = match crate::runtime::resolve_typed::resolve_task_u32(
        &task.context_budget,
        &bindings,
        &datastore,
        "context_budget",
    ) {
        Ok(v) => v,
        Err(e) => {
            let duration = start.elapsed();
            event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(&task_id),
                error: e.to_string(),
                duration_ms: duration.as_millis() as u64,
                error_code: Some(e.code().to_string()),
            });
            return IterationResult {
                store_id: task_id,
                result: TaskResult::failed(e.to_string(), duration),
                for_each_info,
            };
        }
    };
    if let Some(budget) = resolved_budget {
        let budget_event =
            crate::binding::token_budget::enforce_budget(&mut bindings, budget, &task_id);
        event_log.emit(budget_event);
    }

    // Register env-sourced secret values for value-based redaction in traces.
    // This catches custom API keys (e.g., ELEVENLABS_API_KEY) that don't match
    // the pattern-based regex in redact_secrets().
    let env_secrets = bindings.env_sourced_values();
    if !env_secrets.is_empty() {
        crate::util::register_secrets(env_secrets);
    }

    // EMIT: TaskStarted (with redacted env-sourced secrets)
    event_log.emit(EventKind::TaskStarted {
        task_id: Arc::clone(&task_id),
        verb: Arc::from(task.action.verb_name()),
        inputs: Arc::new(bindings.to_value_redacted()),
    });

    // Bridge AnalyzedTask to lowered types at executor boundary
    // PERF(M4): pass references — lower_action clones only what each verb needs

    // Resolve Templatable<T> fields (e.g., temperature: "{{inputs.temperature}}")
    // before lowering, since lower_action extracts plain values from Templatable::Value.
    let resolved_action = match crate::runtime::resolve_typed::resolve_action_templates(
        &task.action,
        &bindings,
        &datastore,
    ) {
        Ok(action) => action,
        Err(e) => {
            let duration = start.elapsed();
            event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(&task_id),
                error: e.to_string(),
                duration_ms: duration.as_millis() as u64,
                error_code: Some(e.code().to_string()),
            });
            return IterationResult {
                store_id: task_id,
                result: TaskResult::failed(e.to_string(), duration),
                for_each_info,
            };
        }
    };

    // Resolve preset: merge agent preset values as fallback for provider/model
    // Precedence: task-level > preset > workflow-default
    let (effective_provider, effective_model) = if let Some(ref preset_name) = task.preset {
        if let Some(agent) = executor.get_preset(preset_name) {
            crate::runtime::preset::resolve_provider_model(&task.provider, &task.model, agent)
        } else {
            (task.provider.clone(), task.model.clone())
        }
    } else {
        (task.provider.clone(), task.model.clone())
    };

    // Extract provider fallback chain from routing config
    let provider_chain: Option<Vec<nika_core::ProviderName>> = task
        .routing
        .as_ref()
        .filter(|r| r.fallback.len() > 1)
        .map(|r| {
            r.fallback
                .iter()
                .map(|s| nika_core::ProviderName::parse(s))
                .collect()
        });
    // Resolve retry templates before lowering (fetch tasks embed retry in lowered action)
    let effective_retry_for_lower = match task
        .retry
        .as_ref()
        .map(|r| crate::runtime::resolve_typed::resolve_retry(r, &bindings, &datastore))
        .transpose()
    {
        Ok(resolved) => resolved,
        Err(e) => {
            let duration = start.elapsed();
            event_log.emit(EventKind::TaskFailed {
                task_id: Arc::clone(&task_id),
                error: e.to_string(),
                duration_ms: duration.as_millis() as u64,
                error_code: Some(e.code().to_string()),
            });
            return IterationResult {
                store_id: task_id,
                result: TaskResult::failed(e.to_string(), duration),
                for_each_info,
            };
        }
    };
    let mut lowered_action = lower_action(
        &resolved_action,
        &effective_provider,
        &effective_model,
        &effective_retry_for_lower,
        &provider_chain,
    );

    // Inject preset system/temperature into lowered action (task-level > preset)
    if let Some(ref preset_name) = task.preset {
        if let Some(agent) = executor.get_preset(preset_name) {
            // Emit PresetApplied event
            event_log.emit(crate::event::EventKind::PresetApplied {
                task_id: Arc::clone(&task_id),
                preset_name: preset_name.clone(),
                provider: agent.provider.clone(),
                model: agent.model.clone(),
            });

            crate::runtime::preset::apply_preset_fields(&mut lowered_action, agent);
        }
    }

    let lowered_output = if let Some(o) = task.output.as_ref() {
        // Resolve max_retries template before lowering
        let mut resolved_output = o.clone();
        if o.max_retries.as_ref().is_some_and(|mr| mr.is_template()) {
            match crate::runtime::resolve_typed::resolve_task_u32(
                &o.max_retries,
                &bindings,
                &datastore,
                "output.max_retries",
            ) {
                Ok(Some(val)) => {
                    resolved_output.max_retries =
                        Some(nika_core::ast::templatable::Templatable::Value(val));
                }
                Ok(None) => {}
                Err(e) => {
                    let duration = start.elapsed();
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: e.to_string(),
                        duration_ms: duration.as_millis() as u64,
                        error_code: Some(e.code().to_string()),
                    });
                    return IterationResult {
                        store_id: task_id,
                        result: TaskResult::failed(e.to_string(), duration),
                        for_each_info,
                    };
                }
            }
        }
        Some(lower_output(resolved_output))
    } else {
        None
    };

    // Bridge structured: config to OutputPolicy for executor Layer 0 dispatch.
    // If both output: and structured: are set, output: takes precedence (already lowered).
    // If only structured: is set, synthesize an OutputPolicy so the executor
    // can trigger Layer 0 tool injection and prompt schema instructions.
    let effective_output = if lowered_output.is_some() {
        lowered_output
    } else {
        task.structured.as_ref().map(|spec| spec.to_output_policy())
    };

    // Check if task qualifies for schema validation retry
    // Pass resolved_action so temperature/max_tokens are already concrete values
    let retry_config = get_retry_config(&task, &resolved_action);

    // Execute with retry loop if configured
    let mut task_result = if let Some((schema, max_retries, original_infer)) = retry_config {
        execute_with_retry(
            &task_id,
            original_infer,
            &schema,
            max_retries,
            &bindings,
            &datastore,
            &executor,
            &event_log,
            start,
            effective_output.as_ref(),
            task.routing.as_ref(),
        )
        .await
    } else {
        // Standard execution with optional task-level retry.
        // Fetch handles retry internally (HTTP 5xx backoff) — skip runner retry.
        let is_fetch = matches!(lowered_action, TaskAction::Fetch { .. });
        // Reuse resolved retry from pre-lowering (already error-checked above)
        let effective_retry = if !is_fetch {
            effective_retry_for_lower.as_ref()
        } else {
            None
        };
        let max_attempts =
            effective_retry.map_or(1u32, |r| r.max_attempts.value().unwrap_or(3).max(1));

        let result = if max_attempts <= 1 {
            // No retry — single execution (with routing fallback if configured)
            executor
                .execute_with_routing(
                    &task_id,
                    &lowered_action,
                    &bindings,
                    &datastore,
                    effective_output.as_ref(),
                    task.routing.as_ref(),
                )
                .await
        } else {
            // Task-level retry loop with exponential backoff.
            let delay_ms = effective_retry.map_or(1000u64, |r| r.delay_ms.value().unwrap_or(1000));
            let backoff = effective_retry
                .map_or(1.0f64, |r| {
                    r.backoff.as_ref().and_then(|b| b.value()).unwrap_or(1.0)
                })
                .clamp(1.0, 10.0);
            let mut last_err: Option<crate::error::NikaError> = None;
            let mut final_result = None;

            const MAX_RETRY_DELAY_MS: u64 = 300_000;

            for attempt in 1..=max_attempts {
                if attempt > 1 {
                    let exp = (attempt - 2).min(30) as i32;
                    let delay =
                        ((delay_ms as f64 * backoff.powi(exp)) as u64).min(MAX_RETRY_DELAY_MS);
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                    event_log.emit(EventKind::TaskRetry {
                        task_id: Arc::clone(&task_id),
                        attempt,
                        max_attempts,
                        backoff_ms: delay,
                        error: last_err.take().map(|e| e.to_string()),
                    });
                }

                match executor
                    .execute_with_routing(
                        &task_id,
                        &lowered_action,
                        &bindings,
                        &datastore,
                        effective_output.as_ref(),
                        task.routing.as_ref(),
                    )
                    .await
                {
                    Ok(output) => {
                        if attempt > 1 {
                            info!(
                                task_id = %task_id,
                                attempt = attempt,
                                "Task succeeded after retry"
                            );
                        }
                        final_result = Some(Ok(output));
                        break;
                    }
                    Err(e) if attempt < max_attempts && is_retryable(&e) => {
                        tracing::warn!(
                            task_id = %task_id,
                            attempt = attempt,
                            max_attempts = max_attempts,
                            error = %e,
                            "Task failed (attempt {}/{}), retrying...",
                            attempt,
                            max_attempts,
                        );
                        last_err = Some(e);
                    }
                    Err(e) => {
                        final_result = Some(Err(e));
                        break;
                    }
                }
            }

            final_result.unwrap_or_else(|| {
                Err(
                    last_err.unwrap_or_else(|| crate::error::NikaError::ExecError {
                        reason: format!(
                            "Task '{}' failed after {} attempts: no error captured",
                            task_id, max_attempts
                        ),
                    }),
                )
            })
        };
        let duration = start.elapsed();

        match result {
            Ok(output) => {
                // Structured output validation via 4-layer engine.
                let executor_already_validated = effective_output.is_some();
                let final_output = if !executor_already_validated {
                    if let Some(ref structured_spec) = task.structured {
                        let mut engine = StructuredOutputEngine::new(
                            structured_spec.clone(),
                            Arc::new(event_log.clone()),
                        )
                        .with_workflow_dir(base_path.clone());
                        match engine.validate(&task_id, &output).await {
                            Ok(result) => {
                                debug!(
                                    task_id = %task_id,
                                    layer = result.layer,
                                    layer_name = %result.layer_name,
                                    total_attempts = result.total_attempts,
                                    "Structured output validation succeeded (runner fallback)"
                                );
                                result.value.to_string()
                            }
                            Err(e) => {
                                let _ = datastore.take_media(&task_id);
                                event_log.emit(EventKind::TaskFailed {
                                    task_id: Arc::clone(&task_id),
                                    error: e.to_string(),
                                    duration_ms: duration.as_millis() as u64,
                                    error_code: Some(e.code().to_string()),
                                });
                                return IterationResult {
                                    store_id: task_id,
                                    result: TaskResult::failed(e.to_string(), duration),
                                    for_each_info,
                                };
                            }
                        }
                    } else {
                        output
                    }
                } else {
                    output
                };

                let mut tr =
                    make_task_result(final_output, effective_output.as_ref(), duration).await;
                tr = tr.with_media(datastore.take_media(&task_id));

                // ── Nika Shield Item 5: compute + emit TrustLevelAssigned ──
                // Runtime trust propagation. Mirrors the compile-time
                // TaintAnalyzer rules but operates on the actual upstream
                // task results via datastore.get_trust(). Without this, the
                // SecuritySummary trust counters never increment and the
                // success criterion #1 ("TrustLevelAssigned events for every
                // task") fails.
                let computed_trust = compute_runtime_trust(&task, &datastore);
                tr.trust_level = computed_trust;
                event_log.emit(EventKind::TrustLevelAssigned {
                    task_id: Arc::clone(&task_id),
                    trust_level: computed_trust.to_string(),
                });
                if task.trust_elevated {
                    event_log.emit(EventKind::TrustElevationUsed {
                        task_id: Arc::clone(&task_id),
                    });
                }

                if tr.is_success() {
                    event_log.emit(EventKind::TaskCompleted {
                        task_id: Arc::clone(&task_id),
                        output: Arc::clone(&tr.output),
                        duration_ms: duration.as_millis() as u64,
                    });
                } else {
                    event_log.emit(EventKind::TaskFailed {
                        task_id: Arc::clone(&task_id),
                        error: tr.error().unwrap_or("Unknown error").to_string(),
                        duration_ms: duration.as_millis() as u64,
                        error_code: Some("NIKA-060".to_string()),
                    });
                }
                tr
            }
            Err(e) => {
                let _ = datastore.take_media(&task_id);

                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                    error_code: Some(e.code().to_string()),
                });

                // ── on_error: fallback routing ──
                if !is_fallback_execution {
                    if let Some(ref on_error) = task.on_error {
                        match &on_error.action {
                            OnErrorAction::Ignore => {
                                event_log.emit(EventKind::TaskFallbackTriggered {
                                    task_id: Arc::clone(&task_id),
                                    action: "ignore".into(),
                                    target: String::new(),
                                    success: true,
                                });
                                TaskResult::success(Value::Null, duration)
                            }

                            OnErrorAction::RetryWithProvider { provider } => {
                                let fb_action = lower_action(
                                    &resolved_action,
                                    &Some(provider.clone()),
                                    &task.model.clone().or(effective_model.clone()),
                                    &None,
                                    &None,
                                );
                                let fb_result = executor
                                    .execute(
                                        &task_id,
                                        &fb_action,
                                        &bindings,
                                        &datastore,
                                        effective_output.as_ref(),
                                    )
                                    .await;
                                let fb_success = fb_result.is_ok();
                                event_log.emit(EventKind::TaskFallbackTriggered {
                                    task_id: Arc::clone(&task_id),
                                    action: "retry_with_provider".into(),
                                    target: provider.as_str().to_string(),
                                    success: fb_success,
                                });
                                match fb_result {
                                    Ok(output) => {
                                        make_task_result(
                                            output,
                                            effective_output.as_ref(),
                                            duration,
                                        )
                                        .await
                                    }
                                    Err(fb_err) => TaskResult::failed(
                                        format!(
                                            "Primary: {}; Fallback({}): {}",
                                            e,
                                            provider.as_str(),
                                            fb_err
                                        ),
                                        duration,
                                    ),
                                }
                            }

                            OnErrorAction::Fallback {
                                task_id: fb_task_id,
                            } => {
                                let fb_task = executor
                                    .workflow_tasks()
                                    .iter()
                                    .find(|t| t.id == *fb_task_id);

                                if let Some(fb_task) = fb_task {
                                    let fb_provider_chain = fb_task
                                        .routing
                                        .as_ref()
                                        .filter(|r| r.fallback.len() > 1)
                                        .map(|r| {
                                            r.fallback
                                                .iter()
                                                .map(|s| nika_core::ProviderName::parse(s))
                                                .collect()
                                        });
                                    // Resolve templates in fallback task before lowering
                                    let fb_resolved_action =
                                        match crate::runtime::resolve_typed::resolve_action_templates(
                                            &fb_task.action,
                                            &bindings,
                                            &datastore,
                                        ) {
                                            Ok(a) => a,
                                            Err(e) => {
                                                tracing::warn!(
                                                    task_id = %task_id,
                                                    error = %e,
                                                    "Fallback task template resolution failed"
                                                );
                                                fb_task.action.clone()
                                            }
                                        };
                                    let fb_action = lower_action(
                                        &fb_resolved_action,
                                        &fb_task.provider,
                                        &fb_task.model,
                                        &None,
                                        &fb_provider_chain,
                                    );
                                    let fb_result = executor
                                        .execute(
                                            &task_id,
                                            &fb_action,
                                            &bindings,
                                            &datastore,
                                            effective_output.as_ref(),
                                        )
                                        .await;
                                    let fb_success = fb_result.is_ok();
                                    event_log.emit(EventKind::TaskFallbackTriggered {
                                        task_id: Arc::clone(&task_id),
                                        action: "fallback".into(),
                                        target: fb_task.name.clone(),
                                        success: fb_success,
                                    });
                                    match fb_result {
                                        Ok(output) => {
                                            make_task_result(
                                                output,
                                                effective_output.as_ref(),
                                                duration,
                                            )
                                            .await
                                        }
                                        Err(fb_err) => TaskResult::failed(
                                            format!(
                                                "Primary: {}; Fallback({}): {}",
                                                e, fb_task.name, fb_err
                                            ),
                                            duration,
                                        ),
                                    }
                                } else {
                                    TaskResult::failed(
                                        format!(
                                            "on_error fallback task not found: {:?}",
                                            fb_task_id
                                        ),
                                        duration,
                                    )
                                }
                            }
                        }
                    } else {
                        TaskResult::failed(e.to_string(), duration)
                    }
                } else {
                    TaskResult::failed(e.to_string(), duration)
                }
            }
        }
    };

    // Process artifacts if task succeeded and has artifact config.
    if task_result.is_success() && for_each_info.is_none() {
        if let Some(ref artifact_spec) = task.artifact {
            let output_content = task_result.output_str().into_owned();

            let artifact_result = process_task_artifacts(
                &task_id,
                &output_content,
                artifact_spec,
                workflow_artifacts.as_ref(),
                &base_path,
                Some(&event_log),
                &bindings,
                &datastore,
                task_result.media.as_slice(),
            )
            .await;

            if artifact_result.written > 0 {
                debug!(
                    task_id = %task_id,
                    artifacts_written = artifact_result.written,
                    "Artifacts written"
                );
            }
            if !artifact_result.errors.is_empty() {
                let error_msgs: Vec<String> = artifact_result
                    .errors
                    .iter()
                    .map(|err| {
                        tracing::error!(
                            task_id = %task_id,
                            error = %err,
                            "Artifact write failed"
                        );
                        event_log.emit(EventKind::ArtifactFailed {
                            task_id: Arc::clone(&task_id),
                            path: String::new(),
                            reason: err.to_string(),
                        });
                        err.to_string()
                    })
                    .collect();
                let error = format!("Artifact write errors: {}", error_msgs.join("; "));
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: error.clone(),
                    error_code: Some("NIKA-281".to_string()),
                    duration_ms: start.elapsed().as_millis() as u64,
                });
                task_result = TaskResult::failed(error, start.elapsed());
            }
        }
    }

    // Record compression: if task succeeded and has record: { compress: true },
    // create a compressed Record via LLM (or truncation fallback) and store.
    if task_result.is_success() {
        if let Some(ref record_spec) = task.record {
            if record_spec.compress {
                let raw_output = task_result.output_str();
                let compressor =
                    crate::runtime::record_compress::RecordCompressor::new(event_log.clone());
                let compressor_model =
                    nika_core::catalogs::default_model_for_provider(executor.default_provider())
                        .unwrap_or("claude-haiku-4-5");
                let llm = crate::runtime::executor_compressor::ExecutorCompressorLlm::new(
                    &executor,
                    executor.default_provider(),
                    compressor_model,
                );
                let record = compressor
                    .compress(&task_id, &raw_output, record_spec, &llm)
                    .await;
                datastore.set_record(Arc::clone(&task_id), record);
            }
        }
    }

    IterationResult {
        store_id: task_id,
        result: task_result,
        for_each_info,
    }
}

#[cfg(test)]
mod tests_shield_runtime_trust {
    //! Sprint 2 Item 5 — runtime trust computation regression tests.
    //!
    //! Verifies `compute_runtime_trust()` mirrors the compile-time
    //! TaintAnalyzer rules so the SecuritySummary trust counters and
    //! downstream taint propagation actually work.

    use std::sync::Arc;
    use std::time::Duration;

    use serde_json::json;

    use super::compute_runtime_trust;
    use crate::store::{RunContext, TaskResult};
    use nika_core::ast::analyzed::{
        AnalyzedExecAction, AnalyzedFetchAction, AnalyzedInferAction, AnalyzedTask,
        AnalyzedTaskAction, AnalyzedWorkflow,
    };
    use nika_core::binding::types::{BindingPath, BindingSource};
    use nika_core::binding::{WithEntry, WithSpec};
    use nika_core::source::Span;
    use nika_core::trust::{InvocationSource, TrustLevel};

    fn make_task(name: &str, action: AnalyzedTaskAction, with_spec: WithSpec) -> AnalyzedTask {
        let mut wf = AnalyzedWorkflow::default();
        let id = wf.task_table.insert(name);
        AnalyzedTask {
            id,
            name: name.to_string(),
            description: None,
            action,
            provider: None,
            model: None,
            with_spec,
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        }
    }

    fn spec_with_task(alias: &str, task_id: &str) -> WithSpec {
        let mut spec = WithSpec::default();
        spec.insert(
            alias.to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Task(Arc::from(task_id)),
                segments: vec![],
            }),
        );
        spec
    }

    fn insert_with_trust(ctx: &RunContext, task_id: &str, trust: TrustLevel) {
        let arc_id: Arc<str> = Arc::from(task_id);
        let mut tr = TaskResult::success(json!("output"), Duration::from_millis(1));
        tr.trust_level = trust;
        ctx.insert(arc_id, tr);
    }

    #[test]
    fn fetch_task_output_is_always_untrusted() {
        let ctx = RunContext::new(InvocationSource::Cli);
        let task = make_task(
            "fetch",
            AnalyzedTaskAction::Fetch(AnalyzedFetchAction::default()),
            WithSpec::default(),
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::Untrusted);
    }

    #[test]
    fn shell_task_output_is_always_untrusted() {
        let ctx = RunContext::new(InvocationSource::Cli);
        let task = make_task(
            "shell",
            AnalyzedTaskAction::Exec(AnalyzedExecAction::default()),
            WithSpec::default(),
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::Untrusted);
    }

    #[test]
    fn infer_with_trusted_inputs_is_model_generated() {
        let ctx = RunContext::new(InvocationSource::Cli);
        insert_with_trust(&ctx, "literal", TrustLevel::Trusted);
        let task = make_task(
            "summarize",
            AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
            spec_with_task("data", "literal"),
        );
        assert_eq!(
            compute_runtime_trust(&task, &ctx),
            TrustLevel::ModelGenerated
        );
    }

    #[test]
    fn infer_with_untrusted_inputs_is_model_tainted() {
        let ctx = RunContext::new(InvocationSource::Cli);
        insert_with_trust(&ctx, "fetched", TrustLevel::Untrusted);
        let task = make_task(
            "summarize",
            AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
            spec_with_task("data", "fetched"),
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::ModelTainted);
    }

    #[test]
    fn infer_with_serve_input_is_model_tainted() {
        use rustc_hash::FxHashMap;

        let ctx = RunContext::new(InvocationSource::Serve);
        let mut inputs = FxHashMap::default();
        inputs.insert("topic".to_string(), json!("from HTTP"));
        ctx.set_inputs(inputs);
        let mut spec = WithSpec::default();
        spec.insert(
            "topic".to_string(),
            WithEntry::simple(BindingPath {
                source: BindingSource::Input("topic".into()),
                segments: vec![],
            }),
        );
        let task = make_task(
            "summarize",
            AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
            spec,
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::ModelTainted);
    }

    #[test]
    fn agent_task_is_always_model_tainted() {
        use nika_core::ast::analyzed::AnalyzedAgentAction;

        let ctx = RunContext::new(InvocationSource::Cli);
        let task = make_task(
            "agent",
            AnalyzedTaskAction::Agent(Box::<AnalyzedAgentAction>::default()),
            WithSpec::default(),
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::ModelTainted);
    }

    #[test]
    fn invoke_propagating_builtin_passes_input_trust() {
        use nika_core::ast::analyzed::AnalyzedInvokeAction;

        let ctx = RunContext::new(InvocationSource::Cli);
        insert_with_trust(&ctx, "fetched", TrustLevel::Untrusted);
        let inv = AnalyzedInvokeAction {
            tool: "nika:jq".to_string(),
            ..Default::default()
        };
        let task = make_task(
            "filter",
            AnalyzedTaskAction::Invoke(inv),
            spec_with_task("data", "fetched"),
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::Untrusted);
    }

    #[test]
    fn invoke_pure_builtin_returns_trusted() {
        use nika_core::ast::analyzed::AnalyzedInvokeAction;

        let ctx = RunContext::new(InvocationSource::Cli);
        insert_with_trust(&ctx, "fetched", TrustLevel::Untrusted);
        let inv = AnalyzedInvokeAction {
            tool: "nika:sleep".to_string(),
            ..Default::default()
        };
        let task = make_task(
            "wait",
            AnalyzedTaskAction::Invoke(inv),
            spec_with_task("data", "fetched"),
        );
        assert_eq!(compute_runtime_trust(&task, &ctx), TrustLevel::Trusted);
    }
}
