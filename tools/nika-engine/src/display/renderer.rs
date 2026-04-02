//! CliRenderer — append-only event stream renderer.
//!
//! Receives Event structs from the runner and prints formatted lines.
//! NO ANSI cursor movement. Every print is a simple println!().
//!
//! Also defines the `Renderer` trait — the shared interface for all display backends.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;

use crate::display::{colors, icons, DetailLevel};
use crate::event::{Event, EventKind};

// ═══════════════════════════════════════════════════════════════════════
// Renderer trait
// ═══════════════════════════════════════════════════════════════════════

/// Shared interface for all display backends (CLI, Live, Test).
///
/// Enables the runner to be generic over display strategy — animated indicatif
/// spinners, plain append-only println, JSON NDJSON, or a test mock.
pub trait Renderer {
    /// Set task-to-DAG-layer mapping. Default: no-op.
    fn set_task_layers(&mut self, _layers: HashMap<Arc<str>, usize>) {}
    /// Initialize per-task display state. Default: no-op.
    fn init_tasks(&mut self, _task_ids: &[String], _task_deps: &HashMap<String, Vec<String>>) {}
    /// ID of the last rendered event.
    fn last_rendered_id(&self) -> Option<u64>;
    /// Render a single EventKind (synthesizes temporary Event).
    fn render_kind(&mut self, kind: &EventKind);
    /// Render all events newer than last_rendered_id.
    fn render_new_events(&mut self, events: &[Event]);
    /// Render full summary footer.
    fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>);
    /// Render compact one-line summary.
    fn render_quiet_summary(&mut self, total_duration_ms: u64);
    /// Access accumulated stats.
    fn stats(&self) -> &RunStats;
}

// ═══════════════════════════════════════════════════════════════════════
// TestRenderer — in-memory mock for assertions
// ═══════════════════════════════════════════════════════════════════════

/// Test-only renderer that records events in memory for assertions.
///
/// Does not print anything. Useful for verifying that the runner emits the
/// expected sequence of events without relying on terminal output.
#[cfg(test)]
pub struct TestRenderer {
    pub stats: RunStats,
    pub rendered_events: Vec<EventKind>,
    pub last_id: Option<u64>,
    pub summary_called: bool,
    pub quiet_summary_called: bool,
    pub summary_duration_ms: Option<u64>,
}

#[cfg(test)]
impl Default for TestRenderer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
impl TestRenderer {
    pub fn new() -> Self {
        Self {
            stats: RunStats::default(),
            rendered_events: Vec::new(),
            last_id: None,
            summary_called: false,
            quiet_summary_called: false,
            summary_duration_ms: None,
        }
    }
}

#[cfg(test)]
impl Renderer for TestRenderer {
    fn last_rendered_id(&self) -> Option<u64> {
        self.last_id
    }

    fn render_kind(&mut self, kind: &EventKind) {
        self.rendered_events.push(kind.clone());
    }

    fn render_new_events(&mut self, events: &[Event]) {
        for event in events {
            if self.last_id.is_none_or(|last| event.id > last) {
                self.stats.apply_event(event);
                self.rendered_events.push(event.kind.clone());
                self.last_id = Some(event.id);
            }
        }
    }

    fn render_summary(&mut self, total_duration_ms: u64, _trace_path: Option<&str>) {
        self.summary_called = true;
        self.summary_duration_ms = Some(total_duration_ms);
    }

    fn render_quiet_summary(&mut self, total_duration_ms: u64) {
        self.quiet_summary_called = true;
        self.summary_duration_ms = Some(total_duration_ms);
    }

    fn stats(&self) -> &RunStats {
        &self.stats
    }
}

/// Accumulated stats for the summary.
#[derive(Debug, Default)]
pub struct RunStats {
    pub tasks_passed: usize,
    pub tasks_failed: usize,
    pub tasks_skipped: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_tokens: u64,
    pub total_cost: f64,
    pub ttft_values: Vec<u64>,
    pub mcp_calls: u32,
    pub mcp_retries: u32,
    pub mcp_errors: u32,
    pub media_stored: u32,
    pub media_bytes: u64,
    pub media_dedup: u32,
    pub artifacts_count: u32,
    pub artifacts_bytes: u64,
    pub guardrails_passed: u32,
    pub guardrails_failed: u32,
    pub guardrails_escalations: u32,
    pub retries: u32,
    pub structured_attempts: u32,
    pub structured_success_layer: Option<u8>,
    pub root_failure: Option<String>,
    /// Per-task timing: (task_id, verb, start_offset_ms, duration_ms)
    pub task_timeline: Vec<(String, String, u64, u64)>,
    /// Per-provider call stats (populated from ProviderCalled + ProviderResponded events).
    pub provider_calls: Vec<ProviderCallStat>,
    /// Tracks the last ProviderCalled per task_id so ProviderResponded can join provider/model.
    #[doc(hidden)]
    pub pending_providers: HashMap<String, (String, String)>,
}

#[derive(Debug)]
pub struct ProviderCallStat {
    pub task_id: String,
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub ttft_ms: Option<u64>,
    pub cost: f64,
}

impl RunStats {
    /// Record a task in the timeline (deduplicates logic across 4 call sites).
    pub fn record_timeline(
        &mut self,
        task_id: &str,
        verb: &str,
        start_ms: u64,
        workflow_start_ms: u64,
        duration_ms: u64,
    ) {
        self.task_timeline.push((
            task_id.to_string(),
            verb.to_string(),
            start_ms.saturating_sub(workflow_start_ms),
            duration_ms,
        ));
    }

    /// Apply a single event to accumulate stats.
    ///
    /// Centralizes stat accumulation that was previously duplicated in both
    /// `CliRenderer::render()` and `LiveRenderer::render()`.
    /// Renderers call this at the top of their render method, then only handle
    /// display-specific logic (printing, progress bars, etc.) in their match arms.
    pub fn apply_event(&mut self, event: &crate::event::Event) {
        match &event.kind {
            EventKind::ProviderCalled {
                task_id,
                provider,
                model,
                ..
            } => {
                self.pending_providers
                    .insert(task_id.to_string(), (provider.clone(), model.clone()));
            }

            EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                ttft_ms,
                cost_usd,
                ..
            } => {
                self.total_input_tokens = self.total_input_tokens.saturating_add(*input_tokens);
                self.total_output_tokens = self.total_output_tokens.saturating_add(*output_tokens);
                self.total_cache_tokens =
                    self.total_cache_tokens.saturating_add(*cache_read_tokens);
                self.total_cost += cost_usd;
                if let Some(t) = ttft_ms {
                    self.ttft_values.push(*t);
                }
                let task_id_str = event.kind.task_id().unwrap_or("?").to_string();
                let (provider, model) = self
                    .pending_providers
                    .get(&task_id_str)
                    .cloned()
                    .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));
                self.provider_calls.push(ProviderCallStat {
                    task_id: task_id_str,
                    provider,
                    model,
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    cache_tokens: *cache_read_tokens,
                    ttft_ms: *ttft_ms,
                    cost: *cost_usd,
                });
            }

            EventKind::McpError { .. } => {
                self.mcp_errors += 1;
            }

            EventKind::McpInvoke { .. } => {
                self.mcp_calls += 1;
            }

            EventKind::McpRetry { .. } => {
                self.mcp_retries += 1;
            }

            EventKind::GuardrailPassed { .. } => {
                self.guardrails_passed += 1;
            }

            EventKind::GuardrailFailed { .. } => {
                self.guardrails_failed += 1;
            }

            EventKind::GuardrailEscalation { .. } => {
                self.guardrails_escalations += 1;
            }

            EventKind::ArtifactWritten { size, .. } => {
                self.artifacts_count += 1;
                self.artifacts_bytes += size;
            }

            EventKind::MediaStored {
                size_bytes,
                deduplicated,
                ..
            } => {
                self.media_stored += 1;
                self.media_bytes += size_bytes;
                if *deduplicated {
                    self.media_dedup += 1;
                }
            }

            EventKind::StructuredOutputAttempt { .. } => {
                self.structured_attempts += 1;
            }

            EventKind::StructuredOutputSuccess { layer, .. } => {
                self.structured_success_layer = Some(*layer);
            }

            EventKind::FetchRetry { .. }
            | EventKind::FetchExhausted { .. }
            | EventKind::TaskRetry { .. }
            | EventKind::ProviderAutoRetried { .. } => {
                self.retries += 1;
            }

            EventKind::TaskCompleted { .. } => {
                self.tasks_passed += 1;
            }

            EventKind::TaskFailed { task_id, error, .. } => {
                self.tasks_failed += 1;
                if self.root_failure.is_none() {
                    self.root_failure = Some(format!("{}: {}", task_id, error));
                }
            }

            EventKind::TaskSkipped { .. } => {
                self.tasks_skipped += 1;
            }

            _ => {}
        }
    }
}

pub struct CliRenderer {
    detail: DetailLevel,
    start: Instant,
    pub(crate) stats: RunStats,
    /// Track which DAG layer each task belongs to (for layer separators)
    task_layers: HashMap<Arc<str>, usize>,
    /// Current layer being displayed
    current_layer: usize,
    /// Terminal width for layout
    term_width: u16,
    /// Track task start times for timeline
    task_starts: HashMap<String, (u64, String)>,
    /// Workflow start timestamp for offset calculation
    workflow_start_ms: u64,
    /// Last rendered event ID for incremental rendering.
    /// `None` means no events have been rendered yet (first call renders ALL events).
    last_rendered_id: Option<u64>,
}

impl CliRenderer {
    pub fn new(detail: DetailLevel) -> Self {
        let term_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0)
            .unwrap_or(80);

        Self {
            detail,
            start: Instant::now(),
            stats: RunStats::default(),
            task_layers: HashMap::new(),
            current_layer: 0,
            term_width,
            task_starts: HashMap::new(),
            workflow_start_ms: 0,
            last_rendered_id: None,
        }
    }

    pub fn last_rendered_id(&self) -> Option<u64> {
        self.last_rendered_id
    }

    /// Set task-to-layer mapping (called after DAG analysis).
    pub fn set_task_layers(&mut self, layers: HashMap<Arc<str>, usize>) {
        self.task_layers = layers;
    }

    /// Format timestamp offset from workflow start.
    fn ts(&self) -> String {
        let elapsed = self.start.elapsed().as_secs_f32();
        format!("{:>6}", format!("+{:.1}s", elapsed))
            .dimmed()
            .to_string()
    }

    pub fn render_new_events(&mut self, events: &[crate::event::Event]) {
        for event in events {
            if self.last_rendered_id.is_none_or(|last| event.id > last) {
                self.render(event);
                self.last_rendered_id = Some(event.id);
            }
        }
    }

    pub fn render_kind(&mut self, kind: &crate::event::EventKind) {
        let event = crate::event::Event {
            id: 0,
            timestamp_ms: self.start.elapsed().as_millis() as u64,
            kind: kind.clone(),
        };
        self.render(&event);
    }

    /// Main entry point: render a single event.
    pub fn render(&mut self, event: &crate::event::Event) {
        if self.detail.is_json() {
            // JSON mode: print raw NDJSON
            match serde_json::to_string(event) {
                Ok(json) => println!("{}", json),
                Err(e) => {
                    // Emit a minimal error event so NDJSON consumers see the gap
                    eprintln!(
                        "{{\"error\":\"event_serialization_failed\",\"detail\":\"{}\"}}",
                        e.to_string().replace('"', "'")
                    );
                }
            }
            return;
        }

        // Centralized stat accumulation — covers all ~15 stat-bearing event types
        self.stats.apply_event(event);

        match &event.kind {
            // ═══════════════════════════════════════
            // WORKFLOW LEVEL
            // ═══════════════════════════════════════
            EventKind::WorkflowStarted { .. } => {
                self.workflow_start_ms = event.timestamp_ms;
                // Header already printed by main.rs
            }
            EventKind::WorkflowPaused => {
                println!("{} {} paused", self.ts(), "⏸".yellow());
            }
            EventKind::WorkflowResumed => {
                println!("{} {} resumed", self.ts(), "▶".green());
            }
            EventKind::WorkflowAborted {
                reason,
                running_tasks,
                ..
            } => {
                println!(
                    "{} {} {}",
                    self.ts(),
                    "⚠".red().bold(),
                    "ABORTED".red().bold()
                );
                println!(
                    "{}   {} {}",
                    " ".repeat(6),
                    "reason:".dimmed(),
                    reason.red()
                );
                if !running_tasks.is_empty() {
                    let names: Vec<&str> = running_tasks.iter().map(|s| s.as_ref()).collect();
                    println!(
                        "{}   {} {}",
                        " ".repeat(6),
                        "running:".dimmed(),
                        names.join(", ").yellow()
                    );
                }
            }

            // ═══════════════════════════════════════
            // TASK LEVEL
            // ═══════════════════════════════════════
            EventKind::TaskScheduled {
                task_id,
                dependencies,
            } => {
                // Check if we need a layer separator
                if self.detail.show_layer_separators() {
                    if let Some(&layer) = self.task_layers.get(task_id) {
                        if layer > self.current_layer && self.current_layer > 0 {
                            println!();
                            let label = format!(" layer {} ", layer + 1);
                            let dash = "─ ".dimmed();
                            let half =
                                (self.term_width as usize / 4).saturating_sub(label.len() / 2);
                            println!(
                                "{}{}{}{}",
                                " ".repeat(14),
                                dash.to_string().repeat(half / 2),
                                label.dimmed(),
                                dash.to_string().repeat(half / 2)
                            );
                            println!();
                        }
                        self.current_layer = layer;
                    }
                }

                let deps_str = if dependencies.is_empty() {
                    "—".dimmed().to_string()
                } else {
                    dependencies
                        .iter()
                        .map(|d| d.as_ref())
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                // Look up verb for this task — will be filled by TaskStarted
                let padded_id = format!("{:<14}", task_id);
                println!(
                    "{}  {} {} {} {} {}",
                    self.ts(),
                    icons::pending(),
                    " ".normal(), // placeholder — verb not known yet at schedule time
                    padded_id.bold(),
                    "scheduled".dimmed(),
                    format!("deps: {}", deps_str).dimmed()
                );
            }

            EventKind::TaskStarted { task_id, verb, .. } => {
                self.task_starts
                    .insert(task_id.to_string(), (event.timestamp_ms, verb.to_string()));
                let padded_id = format!("{:<14}", task_id);
                println!(
                    "{}  {} {} {} {}",
                    self.ts(),
                    icons::running(),
                    icons::verb(verb),
                    padded_id.bold(),
                    "running".white()
                );
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                let dur_secs = *duration_ms as f32 / 1000.0;

                // Look up verb
                let verb = self
                    .task_starts
                    .get(task_id.as_ref())
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();

                // Record timeline
                if let Some((start, _)) = self.task_starts.get(task_id.as_ref()) {
                    self.stats.record_timeline(
                        task_id,
                        &verb,
                        *start,
                        self.workflow_start_ms,
                        *duration_ms,
                    );
                }

                let padded_id = format!("{:<14}", task_id);
                println!(
                    "{}  {} {} {} {}",
                    self.ts(),
                    icons::success(),
                    icons::verb(&verb),
                    padded_id.bold(),
                    colors::duration(dur_secs)
                );

                // Output preview
                if self.detail.show_previews() {
                    self.render_output_preview(output);
                }
            }

            EventKind::TaskFailed {
                task_id,
                error,
                duration_ms,
                error_code,
            } => {
                let dur_secs = *duration_ms as f32 / 1000.0;
                let verb = self
                    .task_starts
                    .get(task_id.as_ref())
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();

                // Record timeline (failed tasks should appear in Gantt chart too)
                if let Some((start, _)) = self.task_starts.get(task_id.as_ref()) {
                    self.stats.record_timeline(
                        task_id,
                        &verb,
                        *start,
                        self.workflow_start_ms,
                        *duration_ms,
                    );
                }

                let padded_id = format!("{:<14}", task_id);
                println!(
                    "{}  {} {} {} {}",
                    self.ts(),
                    icons::failed(),
                    icons::verb(&verb),
                    padded_id.bold().red(),
                    colors::duration(dur_secs)
                );
                println!(
                    "{}  {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    "error".red(),
                    error.red()
                );
                // Show fix suggestion if available (parse error code from message)
                if let Some(err_code) = error_code {
                    if let Some(fix) = crate::error::fix_suggestion_for_code(err_code) {
                        println!(
                            "{}  {} {} {}",
                            " ".repeat(6),
                            "│".dimmed(),
                            "→ Fix:".yellow(),
                            fix.dimmed()
                        );
                    }
                }
            }

            EventKind::TaskSkipped { task_id, reason } => {
                let padded_id = format!("{:<14}", task_id);
                println!(
                    "{}  {} {} {} {}",
                    self.ts(),
                    "\u{2298}".dimmed(),
                    " ".normal(),
                    padded_id.dimmed(),
                    format!("skipped \u{2014} {}", reason).dimmed()
                );
            }

            // ═══════════════════════════════════════
            // FINE-GRAINED
            // ═══════════════════════════════════════
            EventKind::TemplateResolved {
                task_id: _,
                template,
                result,
            } => {
                if self.detail.show_template_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_template_resolved(template, result)
                    );
                }
            }

            EventKind::ProviderCalled {
                task_id: _,
                provider,
                model,
                prompt_len,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_provider_called(provider, model, *prompt_len)
                    );
                }
            }

            EventKind::ProviderResponded {
                input_tokens,
                output_tokens,
                cache_read_tokens,
                ttft_ms,
                cost_usd,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_provider_responded(
                            *input_tokens,
                            *output_tokens,
                            *cache_read_tokens,
                            *ttft_ms,
                        )
                    );
                    if self.detail.show_sparklines() {
                        println!(
                            "{}",
                            super::format_event::fmt_provider_sparkline(
                                *output_tokens,
                                *input_tokens,
                                *cost_usd,
                            )
                        );
                    }
                }
            }

            // ═══════════════════════════════════════
            // CONTEXT
            // ═══════════════════════════════════════
            EventKind::ContextAssembled {
                task_id: _,
                sources,
                total_tokens,
                budget_used_pct,
                truncated: _,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_context_assembled(
                            sources.len(),
                            *total_tokens,
                            *budget_used_pct,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // MCP
            // ═══════════════════════════════════════
            EventKind::McpConnected { server_name } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_mcp_connected(server_name));
                }
            }

            EventKind::McpError { server_name, error } => {
                println!("{}", super::format_event::fmt_mcp_error(server_name, error));
            }

            EventKind::McpInvoke {
                call_id,
                mcp_server,
                tool,
                resource,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_mcp_invoke(
                            mcp_server,
                            tool.as_deref(),
                            resource.as_deref(),
                            call_id,
                        )
                    );
                }
            }

            EventKind::McpResponse {
                call_id,
                output_len,
                duration_ms,
                cached,
                is_error,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_mcp_response(
                            call_id,
                            *output_len,
                            *duration_ms,
                            *cached,
                            *is_error,
                        )
                    );
                }
            }

            EventKind::McpRetry {
                operation,
                attempt,
                max_attempts,
                error,
                ..
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_mcp_retry(operation, *attempt, *max_attempts, error,)
                );
            }

            // ═══════════════════════════════════════
            // AGENT
            // ═══════════════════════════════════════
            EventKind::AgentStart {
                task_id: _,
                max_turns,
                mcp_servers,
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_agent_start(*max_turns, mcp_servers)
                    );
                }
            }

            EventKind::AgentTurn {
                task_id: _,
                turn_index,
                kind,
                metadata,
            } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_agent_turn(*turn_index, kind));
                    if let Some(meta) = metadata {
                        if meta.stop_reason == nika_event::AgentStopReason::ToolUse {
                            println!("{}", super::format_event::fmt_agent_turn_tool_use());
                        }
                    }
                }
            }

            EventKind::AgentComplete {
                task_id: _,
                turns,
                stop_reason,
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_agent_complete(*turns, stop_reason)
                    );
                }
            }

            EventKind::AgentSpawned {
                parent_task_id: _,
                child_task_id,
                depth,
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_agent_spawned(child_task_id, *depth)
                    );
                }
            }

            // ═══════════════════════════════════════
            // GUARDRAILS
            // ═══════════════════════════════════════
            EventKind::GuardrailPassed {
                guardrail_type,
                description,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_guardrail_passed(guardrail_type, description)
                    );
                }
            }

            EventKind::GuardrailFailed {
                guardrail_type,
                message,
                ..
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_guardrail_failed(guardrail_type, message)
                );
            }

            EventKind::GuardrailEscalation {
                guardrail_type,
                severity,
                message,
                ..
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_guardrail_escalation(
                        guardrail_type,
                        severity,
                        message,
                    )
                );
            }

            // ═══════════════════════════════════════
            // BUILTIN
            // ═══════════════════════════════════════
            EventKind::Log { level, message, .. } => {
                println!(
                    "{}",
                    super::format_event::fmt_log(&self.ts(), level, message)
                );
            }

            EventKind::Custom { name, payload, .. } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_custom(&self.ts(), name, payload)
                    );
                }
            }

            // ═══════════════════════════════════════
            // ARTIFACTS
            // ═══════════════════════════════════════
            EventKind::ArtifactWritten {
                path, size, format, ..
            } => {
                // Always show artifact success (matches ArtifactFailed which is unconditional).
                // Users need to know WHERE their output files landed.
                println!(
                    "{}",
                    super::format_event::fmt_artifact_written(path, *size, format)
                );
            }

            EventKind::ArtifactFailed { path, reason, .. } => {
                println!("{}", super::format_event::fmt_artifact_failed(path, reason));
            }

            // ═══════════════════════════════════════
            // MEDIA
            // ═══════════════════════════════════════
            EventKind::MediaExtracted {
                task_id: _,
                block_count,
                content_types,
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_media_extracted(*block_count, content_types)
                    );
                }
            }

            EventKind::MediaStored {
                hash,
                path,
                size_bytes,
                verified,
                deduplicated,
                pipeline_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_media_stored(*size_bytes, path, hash)
                    );
                    if self.detail.show_previews() {
                        println!(
                            "{}",
                            super::format_event::fmt_media_stored_detail(
                                *deduplicated,
                                *verified,
                                *pipeline_ms,
                            )
                        );
                    }
                }
            }

            EventKind::MediaProcessed { .. } => {
                // Grouped into MediaStored line — no separate output
            }

            EventKind::MediaStoreFailed {
                hash: _, reason, ..
            } => {
                println!("{}", super::format_event::fmt_media_store_failed(reason));
            }

            EventKind::MediaIntegrityCheck { .. } => {
                // Stored for summary
            }

            EventKind::MediaCleanup { .. } => {
                // Stored for summary
            }

            // ═══════════════════════════════════════
            // STRUCTURED OUTPUT
            // ═══════════════════════════════════════
            EventKind::StructuredOutputAttempt {
                layer,
                layer_name,
                success,
                error,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_structured_output_attempt(
                            *layer,
                            layer_name,
                            *success,
                            error.as_deref(),
                        )
                    );
                }
            }

            EventKind::StructuredOutputSuccess { .. } => {
                // Stats handled by apply_event()
            }

            // ═══════════════════════════════════════
            // VISION
            // ═══════════════════════════════════════
            EventKind::VisionContentResolved {
                image_count,
                total_bytes,
                resolve_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_vision_content_resolved(
                            *image_count,
                            *total_bytes,
                            *resolve_ms,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // HTTP
            // ═══════════════════════════════════════
            EventKind::HttpRequest { method, url, .. } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_http_request(method, url));
                }
            }

            EventKind::HttpResponse {
                status_code,
                content_type,
                content_length,
                elapsed_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_http_response(
                            *status_code,
                            content_type.as_deref(),
                            *content_length,
                            *elapsed_ms,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // FOR-EACH
            // ═══════════════════════════════════════
            EventKind::ForEachCompleted {
                task_id,
                total,
                succeeded,
                failed,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_for_each_completed(
                            task_id, *total, *succeeded, *failed,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // EXEC
            // ═══════════════════════════════════════
            EventKind::ExecCompleted {
                exit_code,
                duration_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_exec_completed(*exit_code, *duration_ms)
                    );
                }
            }

            // ═══════════════════════════════════════
            // POLICY
            // ═══════════════════════════════════════
            EventKind::PolicyBlocked {
                policy_type,
                reason,
                ..
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_policy_blocked(&self.ts(), policy_type, reason)
                );
            }

            // ═══════════════════════════════════════
            // FETCH RETRY (always show)
            // ═══════════════════════════════════════
            EventKind::FetchRetry {
                url,
                attempt,
                max_attempts,
                status_code,
                backoff_ms,
                ..
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_fetch_retry(
                        url,
                        *attempt,
                        *max_attempts,
                        *status_code,
                        *backoff_ms,
                    )
                );
            }

            // ═══════════════════════════════════════
            // FETCH EXHAUSTED (always show)
            // ═══════════════════════════════════════
            EventKind::FetchExhausted {
                url,
                attempts,
                last_status,
                reason,
                ..
            } => {
                let status_str = last_status
                    .map(|s| format!(" (HTTP {})", s))
                    .unwrap_or_default();
                println!(
                    "  \x1b[31m✗\x1b[0m fetch exhausted after {} attempts{}: {} — {}",
                    attempts, status_str, url, reason,
                );
            }

            // ═══════════════════════════════════════
            // TASK RETRY (always show)
            // ═══════════════════════════════════════
            EventKind::TaskRetry {
                task_id,
                attempt,
                max_attempts,
                backoff_ms,
                error,
            } => {
                let err_msg = error.as_deref().unwrap_or("transient failure");
                println!(
                    "  {} {} retry {}/{} (backoff {}ms): {}",
                    self.ts(),
                    task_id,
                    attempt,
                    max_attempts,
                    backoff_ms,
                    err_msg,
                );
            }

            EventKind::ProviderAutoRetried {
                task_id,
                attempt,
                max_attempts,
                delay_ms,
                error,
            } => {
                println!(
                    "  {} {} provider retry {}/{} (backoff {}ms): {}",
                    self.ts(),
                    task_id,
                    attempt,
                    max_attempts,
                    delay_ms,
                    error,
                );
            }

            // ═══════════════════════════════════════
            // BOOT (always show)
            // ═══════════════════════════════════════
            EventKind::BootPhaseCompleted {
                phase,
                success,
                duration_ms,
                warnings,
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_boot_phase(phase, *success, *duration_ms, warnings,)
                );
            }

            // ═══════════════════════════════════════
            // NATIVE MODEL
            // ═══════════════════════════════════════
            EventKind::NativeModelLoaded {
                model,
                kind,
                duration_ms,
                is_vision,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_native_model_loaded(
                            model,
                            kind,
                            *duration_ms,
                            *is_vision,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // CONTEXT BUDGET EVENTS
            // ═══════════════════════════════════════
            EventKind::BudgetOk { budget, actual, .. } => {
                if self.detail.show_sub_events() {
                    println!("  budget ok: {actual}/{budget} tokens");
                }
            }
            EventKind::BudgetExceeded {
                budget,
                actual,
                truncated_fields,
                ..
            } => {
                println!(
                    "  budget exceeded: {actual} -> {budget} tokens (truncated: {})",
                    truncated_fields.join(", ")
                );
            }

            // ═══════════════════════════════════════
            // BINDING EVENTS
            // ═══════════════════════════════════════
            EventKind::BindingDefaultApplied { alias, path, .. } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_binding_default(alias, path));
                }
            }

            EventKind::BindingTransformApplied {
                alias,
                transform_chain,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_binding_transform(alias, transform_chain)
                    );
                }
            }

            EventKind::BindingEnvResolved {
                var_name, found, ..
            } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_binding_env(var_name, *found));
                }
            }

            // ═══════════════════════════════════════
            // DECOMPOSE
            // ═══════════════════════════════════════
            EventKind::DecomposeStarted {
                task_id, strategy, ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_decompose_started(task_id, strategy)
                    );
                }
            }

            EventKind::DecomposeCompleted {
                task_id,
                item_count,
                duration_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_decompose_completed(
                            task_id,
                            *item_count,
                            *duration_ms,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // FOR-EACH STARTED
            // ═══════════════════════════════════════
            EventKind::ForEachStarted {
                task_id,
                item_count,
                concurrency,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_for_each_started(
                            task_id,
                            *item_count,
                            *concurrency,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // PROVIDER INITIALIZED
            // ═══════════════════════════════════════
            EventKind::ProviderInitialized {
                provider,
                model,
                cached,
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_provider_initialized(provider, model, *cached,)
                    );
                }
            }

            // ═══════════════════════════════════════
            // BUILTIN TOOL INVOKED
            // ═══════════════════════════════════════
            EventKind::BuiltinToolInvoked {
                tool_name,
                duration_ms,
                success,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_builtin_tool_invoked(
                            tool_name,
                            *duration_ms,
                            *success,
                        )
                    );
                }
            }

            // ═══════════════════════════════════════
            // EXTRACT APPLIED
            // ═══════════════════════════════════════
            EventKind::ExtractApplied {
                mode,
                input_len,
                output_len,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}",
                        super::format_event::fmt_extract_applied(mode, *input_len, *output_len,)
                    );
                }
            }

            // Routing events
            EventKind::FallbackTriggered {
                from_provider,
                to_provider,
                reason,
                attempt,
                ..
            } => {
                println!(
                    "{}",
                    super::format_event::fmt_fallback_triggered(
                        from_provider,
                        to_provider,
                        reason,
                        *attempt,
                    )
                );
            }

            EventKind::RecordCreated {
                model,
                compression_ratio,
                confidence,
                ..
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "  ◆ Record compressed (model={model}, ratio={:.0}%, confidence={confidence:.2})",
                        compression_ratio * 100.0,
                    );
                }
            }
            EventKind::RecordSkipped { reason, .. } => {
                if self.detail.show_sub_events() {
                    println!("  ◇ Record skipped: {reason}");
                }
            }

            // Remaining events (WorkflowCompleted/Failed, WorkflowPaused/Resumed, etc.)
            // are either handled by summary or intentionally not rendered per-event.
            _ => {}
        }
    }

    /// Render the output preview box with syntax highlighting.
    fn render_output_preview(&self, output: &Value) {
        for line in format_output_preview(output, self.term_width) {
            println!("{}", line);
        }
    }

    pub fn render_quiet_summary(&self, total_duration_ms: u64) {
        super::summary::print_run_quiet_summary(&self.stats, total_duration_ms);
    }

    /// Render the full summary footer.
    pub fn render_summary(&self, total_duration_ms: u64, trace_path: Option<&str>) {
        super::summary::print_run_summary(&self.stats, self.detail, total_duration_ms, trace_path);
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Renderer trait impl for CliRenderer
// ═══════════════════════════════════════════════════════════════════════

impl Renderer for CliRenderer {
    fn set_task_layers(&mut self, layers: HashMap<Arc<str>, usize>) {
        self.task_layers = layers;
    }

    // init_tasks: uses default no-op (CliRenderer has no progress bars to set up)

    fn last_rendered_id(&self) -> Option<u64> {
        self.last_rendered_id
    }

    fn render_kind(&mut self, kind: &EventKind) {
        CliRenderer::render_kind(self, kind);
    }

    fn render_new_events(&mut self, events: &[Event]) {
        CliRenderer::render_new_events(self, events);
    }

    fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>) {
        super::summary::print_run_summary(&self.stats, self.detail, total_duration_ms, trace_path);
    }

    fn render_quiet_summary(&mut self, total_duration_ms: u64) {
        super::summary::print_run_quiet_summary(&self.stats, total_duration_ms);
    }

    fn stats(&self) -> &RunStats {
        &self.stats
    }
}

// ═══════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════

/// Format bytes: 1234 → "1.2 KB", 1234567 → "1.2 MB"
pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

use super::colors::{floor_char_boundary, stripped_len};

/// Format output preview as a mini box with syntax-highlighted content.
///
/// Returns a Vec of pre-formatted lines (with ANSI colors and box characters)
/// ready for printing. Returns empty Vec if output is null/empty.
pub(crate) fn format_output_preview(output: &Value, term_width: u16) -> Vec<String> {
    // Use Cow to avoid cloning large LLM output strings (often 10KB+)
    // when we only need the first few lines for the preview box.
    let owned;
    let text: &str = match output {
        Value::String(s) => s.as_str(),
        _ => {
            owned = serde_json::to_string_pretty(output).unwrap_or_default();
            &owned
        }
    };

    if text.is_empty() || text == "null" {
        return Vec::new();
    }

    let is_json = text.starts_with('{') || text.starts_with('[');
    let is_markdown = text.starts_with('#') || text.contains("\n## ");

    let max_width = (term_width as usize).min(72).saturating_sub(16);
    let dashes = "\u{254c}".repeat(max_width);
    let size_label = format!("{} ch", text.chars().count());
    let padding = max_width.saturating_sub(size_label.len() + 1);

    let mut lines = Vec::new();

    // Top border
    lines.push(format!(
        "{}     {} {}{}{}",
        " ".repeat(6),
        "\u{2502}".dimmed(),
        "\u{256d}\u{254c}".dimmed(),
        dashes.dimmed(),
        "\u{256e}".dimmed()
    ));

    // Content lines
    let preview_lines: Vec<String> = if is_json {
        vec![colors::json_preview(&text.replace('\n', " "), max_width)]
    } else if is_markdown {
        // Markdown preview lines may contain ANSI codes from colorization.
        // Truncate the raw text first, then re-colorize would be ideal,
        // but for simplicity we accept slightly imprecise truncation on
        // ANSI-colored headings (cosmetic only, no crash due to floor_char_boundary).
        colors::markdown_preview(text, 4)
            .into_iter()
            .map(|l| {
                let visual_len = stripped_len(&l);
                if visual_len > max_width {
                    // For ANSI strings, truncation by byte offset may cut escape codes.
                    // Use stripped text length as a heuristic upper bound for byte position.
                    let byte_budget = l.len().min(max_width * 4); // generous for ANSI overhead
                    let end = floor_char_boundary(&l, byte_budget);
                    format!("{}\u{2026}", &l[..end])
                } else {
                    l
                }
            })
            .collect()
    } else {
        text.lines()
            .take(2)
            .map(|l| {
                if stripped_len(l) > max_width {
                    let end = floor_char_boundary(l, max_width - 1);
                    format!("{}\u{2026}", &l[..end])
                } else {
                    l.to_string()
                }
            })
            .collect()
    };

    for line in &preview_lines {
        let pad = max_width.saturating_sub(stripped_len(line));
        lines.push(format!(
            "{}     {} {} {}{} {}",
            " ".repeat(6),
            "\u{2502}".dimmed(),
            "\u{2502}".dimmed(),
            line,
            " ".repeat(pad),
            "\u{2502}".dimmed()
        ));
    }

    // Bottom border
    lines.push(format!(
        "{}     {} {}{} {} {}",
        " ".repeat(6),
        "\u{2502}".dimmed(),
        "\u{2570}\u{254c}".dimmed(),
        "\u{254c}".repeat(padding).dimmed(),
        size_label.dimmed(),
        "\u{254c}\u{256f}".dimmed()
    ));

    lines
}
