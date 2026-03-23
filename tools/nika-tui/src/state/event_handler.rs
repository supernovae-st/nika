//! Event handler for TuiState
//!
//! Contains the `handle_event()` method that processes runtime events
//! and updates TUI state accordingly. This is the largest single method
//! in TuiState, handling workflow, task, MCP, agent, provider, and
//! telemetry events.

use std::sync::Arc;
use std::time::Instant;

use nika_engine::event::EventKind;

use super::notification::Notification;
use super::types::{
    AgentTurnState, ContextAssembly, McpCall, SpawnedAgent, TemplateResolution, FRAME_CYCLE,
    FRAME_DIV_GLACIAL, FRAME_DIV_NORMAL,
};
use super::TuiState;

use crate::theme::{MissionPhase, TaskStatus};

/// Maximum number of entries in token_history and latency_history sparklines
const MAX_HISTORY_ENTRIES: usize = 50;

impl TuiState {
    /// Handle an event from the runtime
    pub fn handle_event(&mut self, kind: &EventKind, timestamp_ms: u64) {
        match kind {
            // ═══════════════════════════════════════════
            // WORKFLOW EVENTS
            // ═══════════════════════════════════════════
            EventKind::WorkflowStarted {
                task_count,
                generation_id,
                ..
            } => {
                self.workflow.task_count = *task_count;
                self.workflow.phase = MissionPhase::Countdown;
                self.workflow.started_at = Some(Instant::now());
                self.workflow.generation_id = Some(generation_id.clone());
                // TIER 4.1: Mark all panels dirty on workflow start
                self.dirty.mark_all();
                // TIER 4.4: Clear JSON cache on workflow start
                self.json_cache.clear();
            }

            EventKind::WorkflowCompleted {
                final_output,
                total_duration_ms,
            } => {
                self.workflow.phase = MissionPhase::MissionSuccess;
                self.workflow.final_output = Some(Arc::clone(final_output));
                self.workflow.total_duration_ms = Some(*total_duration_ms);
                self.current_task = None;

                // TIER 3.4: Add success notification
                let duration_secs = *total_duration_ms as f64 / 1000.0;
                self.add_notification(Notification::success(
                    format!(
                        "Magnificent! Warped through in {:.1}s ({}/{} tasks)",
                        duration_secs, self.workflow.tasks_completed, self.workflow.task_count
                    ),
                    timestamp_ms,
                ));
                // TIER 4.1: Mark progress and status dirty
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            EventKind::WorkflowFailed { error, .. } => {
                self.workflow.phase = MissionPhase::Abort;
                self.workflow.error_message = Some(error.clone());

                // TIER 3.4: Add error notification
                self.add_notification(Notification::error(
                    format!("RAWR! Mission failed: {}", error),
                    timestamp_ms,
                ));
                // TIER 4.1: Mark progress, status, and notifications dirty
                self.dirty.progress = true;
                self.dirty.status = true;
                self.dirty.notifications = true;
            }

            EventKind::WorkflowAborted {
                reason,
                duration_ms,
                running_tasks,
            } => {
                self.workflow.phase = MissionPhase::Abort;
                self.workflow.error_message = Some(format!("Aborted: {}", reason));
                self.workflow.total_duration_ms = Some(*duration_ms);
                self.current_task = None;

                // TIER 3.4: Add abort notification
                let task_info = if running_tasks.is_empty() {
                    String::new()
                } else {
                    format!(" ({} tasks interrupted)", running_tasks.len())
                };
                self.add_notification(Notification::warning(
                    format!("Mission aborted: {}{}", reason, task_info),
                    timestamp_ms,
                ));
                // Mark all relevant panels dirty
                self.dirty.progress = true;
                self.dirty.status = true;
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // TASK EVENTS
            // ═══════════════════════════════════════════
            EventKind::TaskScheduled {
                task_id,
                dependencies,
            } => {
                let deps: Vec<String> = dependencies
                    .iter()
                    .map(|s: &std::sync::Arc<str>| s.to_string())
                    .collect();
                let task = super::types::TaskState::new(task_id.to_string(), deps);
                self.tasks.insert(task_id.to_string(), task);
                self.task_order.push(task_id.to_string());
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.invalidate_timeline_cache();
            }

            EventKind::TaskStarted {
                task_id,
                verb,
                inputs,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Running;
                    task.started_at = Some(Instant::now());
                    task.input = Some(Arc::new(inputs.clone()));
                    task.task_type = Some(verb.to_string());
                }
                self.current_task = Some(task_id.to_string());

                // Update phase
                if self.workflow.phase == MissionPhase::Countdown {
                    self.workflow.phase = MissionPhase::Launch;
                } else {
                    self.workflow.phase = MissionPhase::Orbital;
                }
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.invalidate_timeline_cache();
                // TIER 4.4: Invalidate task cache on start (will need re-format later)
                self.json_cache.invalidate(&format!("task:{}", task_id));
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Success;
                    task.duration_ms = Some(*duration_ms);
                    task.output = Some(output.clone());
                }
                self.workflow.tasks_completed += 1;

                // TIER 3.4: Notify on slow tasks
                let duration_secs = *duration_ms as f64 / 1000.0;
                if *duration_ms > 30_000 {
                    self.add_notification(Notification::alert(
                        format!(
                            "Sloth mode! '{}' crawled in at {:.1}s",
                            task_id, duration_secs
                        ),
                        timestamp_ms,
                    ));
                } else if *duration_ms > 10_000 {
                    self.add_notification(Notification::warning(
                        format!("Taking its time... '{}' at {:.1}s", task_id, duration_secs),
                        timestamp_ms,
                    ));
                }

                // P1 Fix: Only clear agent state if this task was actually an agent task
                // Check task_type to avoid clearing during parallel workflows
                if let Some(task) = self.tasks.get(task_id.as_ref()) {
                    if task.task_type.as_deref() == Some("agent") {
                        self.agent.turns.clear();
                        self.agent.streaming_buffer.clear();
                        self.agent.max_turns = None;
                    }
                }
                // TIER 4.1: Mark progress and dag dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.invalidate_timeline_cache();
                // TIER 4.4: Invalidate task cache on completion (new output)
                self.json_cache.invalidate(&format!("task:{}", task_id));
            }

            EventKind::TaskFailed {
                task_id,
                error,
                duration_ms,
                ..
            } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Failed;
                    task.duration_ms = Some(*duration_ms);
                    task.error = Some(error.clone());
                }
                // TIER 4.1: Mark progress, dag, and status dirty
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.dirty.status = true;
                self.invalidate_timeline_cache();
            }

            EventKind::TaskSkipped { task_id, reason } => {
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.status = TaskStatus::Skipped;
                    task.error = Some(format!("skipped: {}", reason));
                }
                self.dirty.progress = true;
                self.dirty.dag = true;
                self.dirty.status = true;
                self.invalidate_timeline_cache();
            }

            // ═══════════════════════════════════════════
            // MCP EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpInvoke {
                task_id,
                mcp_server,
                tool,
                resource,
                call_id,
                params,
            } => {
                let call = McpCall {
                    call_id: call_id.clone(),
                    seq: self.mcp.seq,
                    server: mcp_server.clone(),
                    tool: tool.clone(),
                    resource: resource.clone(),
                    task_id: task_id.to_string(),
                    completed: false,
                    output_len: None,
                    timestamp_ms,
                    params: params.clone(),
                    response: None,
                    is_error: false,
                    duration_ms: None,
                };
                self.mcp.calls.push(call);
                self.mcp.seq += 1;

                // Update phase
                self.workflow.phase = MissionPhase::Rendezvous;

                // Track in metrics
                if let Some(ref tool_name) = tool {
                    let entry = self.metrics.mcp_calls.entry(tool_name.clone()).or_insert(0);
                    *entry += 1;
                }
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
            }

            EventKind::McpResponse {
                task_id: _,
                output_len,
                call_id,
                duration_ms,
                cached,
                is_error,
                response,
            } => {
                // Track MCP cache hit/miss
                if *cached {
                    self.metrics.mcp_cache_hits += 1;
                } else {
                    self.metrics.mcp_cache_misses += 1;
                }

                // Find and update the matching call by call_id
                let tool_name = self
                    .mcp
                    .calls
                    .iter()
                    .find(|c| c.call_id == *call_id)
                    .and_then(|c| c.tool.clone());

                if let Some(call) = self.mcp.calls.iter_mut().find(|c| c.call_id == *call_id) {
                    call.completed = true;
                    call.output_len = Some(*output_len);
                    call.response = response.clone();
                    call.is_error = *is_error;
                    call.duration_ms = Some(*duration_ms);
                }

                // Track MCP latency for sparkline
                if self.metrics.latency_history.len() >= MAX_HISTORY_ENTRIES {
                    self.metrics.latency_history.remove(0);
                }
                self.metrics.latency_history.push(*duration_ms);

                // TIER 3.4: Notify on slow MCP responses (> 5s)
                if *duration_ms > 5_000 {
                    let duration_secs = *duration_ms as f64 / 1000.0;
                    let tool_display = tool_name.as_deref().unwrap_or("resource");
                    self.add_notification(Notification::warning(
                        format!(
                            "Tentacles reaching... '{}' at {:.1}s",
                            tool_display, duration_secs
                        ),
                        timestamp_ms,
                    ));
                }

                // Return to orbital phase
                self.workflow.phase = MissionPhase::Orbital;
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
                // TIER 4.4: Invalidate MCP call cache on response
                self.json_cache.invalidate(&format!("mcp:{}", call_id));
            }

            // ═══════════════════════════════════════════
            // CONTEXT EVENTS
            // ═══════════════════════════════════════════
            EventKind::ContextAssembled {
                sources,
                excluded,
                total_tokens,
                budget_used_pct,
                truncated,
                ..
            } => {
                self.mcp.context_assembly = ContextAssembly {
                    sources: sources.clone(),
                    excluded: excluded.clone(),
                    total_tokens: *total_tokens,
                    budget_used_pct: *budget_used_pct,
                    truncated: *truncated,
                };
                // TIER 4.1: Mark novanet panel dirty
                self.dirty.novanet = true;
            }

            // ═══════════════════════════════════════════
            // BINDING EVENTS
            // ═══════════════════════════════════════════
            EventKind::TemplateResolved {
                task_id,
                template,
                result,
            } => {
                // Keep last 10 resolutions
                if self.agent.recent_templates.len() >= 10 {
                    self.agent.recent_templates.pop_front();
                }
                self.agent.recent_templates.push_back(TemplateResolution {
                    task_id: task_id.to_string(),
                    template: template.clone(),
                    result: result.clone(),
                    timestamp_ms,
                });
                // Mark context panel dirty (template bindings are context-related)
                self.dirty.novanet = true;
            }

            // ═══════════════════════════════════════════
            // AGENT EVENTS
            // ═══════════════════════════════════════════
            EventKind::AgentStart { max_turns, .. } => {
                self.agent.turns.clear();
                self.agent.streaming_buffer.clear();
                self.agent.max_turns = Some(*max_turns);
                // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentTurn {
                turn_index,
                kind,
                metadata,
                ..
            } => {
                // Extract tokens from metadata if present
                let tokens = metadata.as_ref().map(|m| m.total_tokens());
                // Extract thinking and response_text from metadata
                let thinking = metadata.as_ref().and_then(|m| m.thinking.clone());
                let response_text = metadata.as_ref().map(|m| m.response_text.clone());

                let turn = AgentTurnState {
                    index: *turn_index,
                    status: kind.clone(),
                    tokens,
                    tool_calls: Vec::new(),
                    thinking,
                    response_text,
                };
                // Update or add turn
                if let Some(existing) = self.agent.turns.iter_mut().find(|t| t.index == *turn_index)
                {
                    existing.status = kind.clone();
                    existing.tokens = tokens;
                    existing.thinking = turn.thinking;
                    existing.response_text = turn.response_text;
                } else {
                    self.agent.turns.push(turn);
                }
                // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentComplete { turns, .. } => {
                // Update metrics
                if let Some(last_turn) = self.agent.turns.last() {
                    if let Some(tokens) = last_turn.tokens {
                        if self.metrics.token_history.len() >= MAX_HISTORY_ENTRIES {
                            self.metrics.token_history.remove(0);
                        }
                        self.metrics.token_history.push(tokens);
                    }
                }
                let _ = turns; // Used for logging
                               // TIER 4.1: Mark reasoning panel dirty
                self.dirty.reasoning = true;
            }

            EventKind::AgentSpawned {
                parent_task_id,
                child_task_id,
                depth,
            } => {
                // Track spawned sub-agent
                self.agent.spawned_agents.push(SpawnedAgent {
                    parent_task_id: parent_task_id.to_string(),
                    child_task_id: child_task_id.to_string(),
                    depth: *depth,
                });

                // Add notification for nested agent spawn
                self.add_notification(Notification::info(
                    format!(
                        "Hatching '{}' at depth {} - fly little one!",
                        child_task_id, depth
                    ),
                    timestamp_ms,
                ));

                // TIER 4.1: Mark reasoning and notifications dirty
                self.dirty.reasoning = true;
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // PROVIDER EVENTS
            // ═══════════════════════════════════════════
            EventKind::ProviderCalled {
                task_id,
                provider,
                model,
                prompt_len,
            } => {
                // Update task's provider info
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.provider = Some(provider.clone());
                    task.model = Some(model.clone());
                    task.prompt_len = Some(*prompt_len);
                }

                // Update metrics
                self.metrics.provider_calls += 1;
                self.metrics.last_model = Some(model.clone());

                // TIER 4.1: Mark progress dirty (for provider display)
                self.dirty.progress = true;
            }

            EventKind::ProviderResponded {
                task_id,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                cost_usd,
                ttft_ms,
                finish_reason,
                ..
            } => {
                // Capture finish_reason on the task
                if let Some(task) = self.tasks.get_mut(task_id.as_ref()) {
                    task.finish_reason = Some(finish_reason.clone());
                }

                self.metrics.input_tokens += input_tokens;
                self.metrics.output_tokens += output_tokens;
                self.metrics.cache_read_tokens += cache_read_tokens;
                self.metrics.total_tokens += input_tokens + output_tokens;
                self.metrics.cost_usd += cost_usd;
                if self.metrics.token_history.len() >= MAX_HISTORY_ENTRIES {
                    self.metrics.token_history.remove(0);
                }
                self.metrics
                    .token_history
                    .push(input_tokens + output_tokens);
                if let Some(ttft) = ttft_ms {
                    if self.metrics.latency_history.len() >= MAX_HISTORY_ENTRIES {
                        self.metrics.latency_history.remove(0);
                    }
                    self.metrics.latency_history.push(*ttft);
                    // Calculate tokens/sec from TTFT and push to velocity tracker
                    // TTFT in ms, output_tokens is total - estimate avg rate
                    let ttft_secs = (*ttft as f32).max(1.0) / 1000.0;
                    let velocity = *output_tokens as f32 / ttft_secs;
                    // TokenVelocity is a ring buffer — push() handles eviction
                    self.metrics.token_velocity.push(velocity);
                } else if *output_tokens > 0 {
                    // Fallback: assume ~1 second if no TTFT, just track relative activity
                    self.metrics.token_velocity.push(*output_tokens as f32);
                }

                // TIER 3.4: Token usage progression with cosmic pirate emojis
                const CONTEXT_WINDOW: u64 = 100_000;
                let pct = (self.metrics.total_tokens as f64 / CONTEXT_WINDOW as f64) * 100.0;

                if pct > 95.0 {
                    self.add_notification(Notification::alert(
                        format!(
                            "ABANDON SHIP! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 85.0 {
                    self.add_notification(Notification::alert(
                        format!(
                            "Danger zone! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 70.0 {
                    self.add_notification(Notification::warning(
                        format!(
                            "Getting spicy! {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                } else if pct > 50.0 {
                    self.add_notification(Notification::info(
                        format!(
                            "Heating up... {:.0}% fuel ({}/{}k)",
                            pct,
                            self.metrics.total_tokens,
                            CONTEXT_WINDOW / 1000
                        ),
                        timestamp_ms,
                    ));
                }
                // TIER 4.1: Mark progress dirty (for metrics display)
                self.dirty.progress = true;
            }

            // ═══════════════════════════════════════════
            // PAUSE/RESUME EVENTS
            // ═══════════════════════════════════════════
            EventKind::WorkflowPaused => {
                // P2 Fix: Save phase before pausing for proper restoration
                self.workflow.phase_before_pause = Some(self.workflow.phase);
                self.workflow.paused = true;
                self.workflow.phase = MissionPhase::Pause;
                self.add_notification(Notification::warning(
                    "Mission paused - press SPACE to resume",
                    timestamp_ms,
                ));
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            EventKind::WorkflowResumed => {
                self.workflow.paused = false;
                // P2 Fix: Restore saved phase, or infer from current state
                if let Some(phase) = self.workflow.phase_before_pause.take() {
                    self.workflow.phase = phase;
                } else if self.current_task.is_some() {
                    self.workflow.phase = MissionPhase::Orbital;
                } else {
                    self.workflow.phase = MissionPhase::Countdown;
                }
                self.add_notification(Notification::info(
                    "Mission resumed - engines back online!",
                    timestamp_ms,
                ));
                self.dirty.progress = true;
                self.dirty.status = true;
            }

            // ═══════════════════════════════════════════
            // MCP CONNECTION EVENTS
            // ═══════════════════════════════════════════
            EventKind::McpConnected { server_name, .. } => {
                self.add_notification(Notification::success(
                    format!("MCP server '{}' connected", server_name),
                    timestamp_ms,
                ));
                self.dirty.status = true;
            }

            EventKind::McpError {
                server_name, error, ..
            } => {
                self.add_notification(Notification::error(
                    format!("MCP '{}' error: {}", server_name, error),
                    timestamp_ms,
                ));
                self.dirty.status = true;
            }

            // P2 Fix: Handle MCP retry event for visibility
            EventKind::McpRetry {
                server_name,
                operation,
                attempt,
                max_attempts,
                error,
                ..
            } => {
                self.add_notification(Notification::warning(
                    format!(
                        "MCP '{}' retry {}/{} for '{}': {}",
                        server_name, attempt, max_attempts, operation, error
                    ),
                    timestamp_ms,
                ));
                self.dirty.status = true;
            }

            // Handle builtin log events
            EventKind::Log {
                level,
                message,
                task_id,
            } => {
                let prefix = match level.as_str() {
                    "error" => "[ERR]",
                    "warn" => "[WRN]",
                    "info" => "[INF]",
                    "debug" => "[DBG]",
                    "trace" => "[TRC]",
                    _ => "[LOG]",
                };
                let task_suffix = task_id
                    .as_ref()
                    .map(|t| format!(" [{}]", t))
                    .unwrap_or_default();
                self.add_notification(Notification::info(
                    format!("{} {}{}", prefix, message, task_suffix),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // Handle custom events from nika:emit
            EventKind::Custom {
                name,
                payload,
                task_id,
            } => {
                let task_suffix = task_id
                    .as_ref()
                    .map(|t| format!(" [{}]", t))
                    .unwrap_or_default();
                // Compact payload display (first 50 chars, UTF-8 safe)
                let payload_preview = if payload.is_null() {
                    String::new()
                } else {
                    let s = payload.to_string();
                    if s.chars().count() > 50 {
                        let truncated: String = s.chars().take(47).collect();
                        format!(": {}...", truncated)
                    } else {
                        format!(": {}", s)
                    }
                };
                self.add_notification(Notification::info(
                    format!("[EVT] {}{}{}", name, payload_preview, task_suffix),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // ARTIFACT EVENTS
            // ═══════════════════════════════════════════
            EventKind::ArtifactWritten {
                task_id,
                path,
                size,
                ..
            } => {
                let size_str = if *size < 1024 {
                    format!("{} B", size)
                } else if *size < 1024 * 1024 {
                    format!("{:.1} KB", *size as f64 / 1024.0)
                } else {
                    format!("{:.1} MB", *size as f64 / (1024.0 * 1024.0))
                };
                self.add_notification(Notification::success(
                    format!("[{}] Artifact written: {} ({})", task_id, path, size_str),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::ArtifactFailed {
                task_id,
                path,
                reason,
            } => {
                self.add_notification(Notification::error(
                    format!("[{}] Artifact failed: {} - {}", task_id, path, reason),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // STRUCTURED OUTPUT EVENTS
            // ═══════════════════════════════════════════
            EventKind::StructuredOutputAttempt {
                task_id,
                layer,
                layer_name,
                attempt,
                success,
                error,
            } => {
                // Add notification for structured output attempts
                if *success {
                    self.add_notification(Notification::info(
                        format!(
                            "[{}] Layer {} ({}) attempt {} succeeded",
                            task_id, layer, layer_name, attempt
                        ),
                        timestamp_ms,
                    ));
                } else if let Some(err) = error {
                    self.add_notification(Notification::warning(
                        format!(
                            "[{}] Layer {} ({}) attempt {} failed: {}",
                            task_id, layer, layer_name, attempt, err
                        ),
                        timestamp_ms,
                    ));
                }
                self.dirty.notifications = true;
            }

            EventKind::StructuredOutputSuccess {
                task_id,
                layer,
                layer_name,
                total_attempts,
            } => {
                self.add_notification(Notification::success(
                    format!(
                        "[{}] Structured output extracted via layer {} ({}) after {} attempt(s)",
                        task_id, layer, layer_name, total_attempts
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // GUARDRAIL EVENTS
            // ═══════════════════════════════════════════
            EventKind::GuardrailPassed {
                task_id,
                guardrail_type,
                description,
            } => {
                self.add_notification(Notification::success(
                    format!(
                        "[{}] Guardrail {} passed: {}",
                        task_id, guardrail_type, description
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::GuardrailFailed {
                task_id,
                guardrail_type,
                description,
                message,
            } => {
                self.add_notification(Notification::error(
                    format!(
                        "[{}] Guardrail {} failed ({}): {}",
                        task_id, guardrail_type, description, message
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::GuardrailEscalation {
                task_id,
                guardrail_type,
                guardrail_id,
                message,
                severity,
                suggested_action,
            } => {
                let action_text = suggested_action
                    .as_ref()
                    .map(|a| format!(" Action: {}", a))
                    .unwrap_or_default();
                self.add_notification(Notification::error(
                    format!(
                        "[{}] ESCALATION ({}) {} [{}]: {}{}",
                        task_id, severity, guardrail_type, guardrail_id, message, action_text
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // MEDIA EVENTS (no TUI action needed yet)
            // ═══════════════════════════════════════════
            // ═══════════════════════════════════════════
            // HTTP TELEMETRY EVENTS
            // ═══════════════════════════════════════════
            EventKind::HttpRequest {
                task_id,
                method,
                url,
                ..
            } => {
                self.add_notification(Notification::info(
                    format!("[{}] HTTP {} {}", task_id, method, url),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::HttpResponse {
                task_id,
                status_code,
                elapsed_ms,
                ..
            } => {
                let status_label = if *status_code < 400 { "OK" } else { "ERR" };
                self.add_notification(Notification::info(
                    format!(
                        "[{}] HTTP {} {} ({}ms)",
                        task_id, status_code, status_label, elapsed_ms
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            EventKind::MediaExtracted {
                task_id,
                block_count,
                ..
            } => {
                self.add_notification(Notification::info(
                    format!("[{}] Extracted {} media block(s)", task_id, block_count),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }
            EventKind::MediaProcessed { .. } | EventKind::MediaStored { .. } => {
                // Tracked via events, no TUI notification for individual blocks
            }
            EventKind::MediaStoreFailed {
                task_id, reason, ..
            } => {
                self.add_notification(Notification::error(
                    format!("[{}] Media store failed: {}", task_id, reason),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }
            EventKind::MediaCleanup {
                removed,
                bytes_freed,
                dry_run,
            } => {
                if !dry_run {
                    self.add_notification(Notification::info(
                        format!(
                            "Media cleanup: removed {} files, freed {} bytes",
                            removed, bytes_freed
                        ),
                        timestamp_ms,
                    ));
                    self.dirty.notifications = true;
                }
            }
            EventKind::MediaIntegrityCheck { checked, warnings } => {
                if *warnings > 0 {
                    self.add_notification(Notification::warning(
                        format!(
                            "Media integrity: {}/{} refs had warnings",
                            warnings, checked
                        ),
                        timestamp_ms,
                    ));
                    self.dirty.notifications = true;
                }
            }

            EventKind::VisionContentResolved {
                task_id,
                image_count,
                total_bytes,
                resolve_ms,
            } => {
                self.add_notification(Notification::info(
                    format!(
                        "Vision: {} image(s) resolved ({} bytes, {}ms) for task {}",
                        image_count, total_bytes, resolve_ms, task_id
                    ),
                    timestamp_ms,
                ));
                self.dirty.notifications = true;
            }

            // ═══════════════════════════════════════════
            // EXEC / FETCH / POLICY EVENTS (observability)
            // ═══════════════════════════════════════════
            EventKind::ExecCompleted { .. }
            | EventKind::FetchRetry { .. }
            | EventKind::PolicyBlocked { .. } => {
                // These events are captured for tracing/logging but don't
                // require TUI state mutations — the TUI already tracks
                // task-level status via TaskStarted/TaskCompleted/TaskFailed.
            }
        }
    }

    /// Update elapsed time and animation frame (call on each render frame)
    ///
    /// Animation frame wraps at FRAME_CYCLE (60) for 1-second cycles at 60 FPS.
    pub fn tick(&mut self) {
        // Update elapsed time
        if let Some(started) = self.workflow.started_at {
            self.workflow.elapsed_ms = started.elapsed().as_millis() as u64;
        }

        // Advance animation frame (wraps at FRAME_CYCLE for 1-second cycles)
        self.frame = self.frame.wrapping_add(1) % FRAME_CYCLE;

        // Expire old status messages
        self.status_messages.tick();
    }

    /// Get spinner character for current frame
    /// Uses braille spinner
    /// Divides by FRAME_DIV_NORMAL (6) for ~10 FPS animation
    pub fn spinner_char(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let idx = (self.frame / FRAME_DIV_NORMAL) as usize % SPINNER.len();
        SPINNER[idx]
    }

    /// Get rocket animation character for current frame
    /// Used during Launch phase
    /// Divides by FRAME_DIV_GLACIAL (15) for ~4 FPS animation
    pub fn rocket_char(&self) -> char {
        const ROCKET: &[char] = &['🚀', '🔥', '💨', '✨'];
        let idx = (self.frame / FRAME_DIV_GLACIAL) as usize % ROCKET.len();
        ROCKET[idx]
    }
}
