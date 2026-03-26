//! LiveRenderer — animated, in-place CLI display using indicatif.
//!
//! Uses `MultiProgress` to maintain a fixed status area at the bottom of the
//! terminal while scrolling event details above via `multi.println()`.
//!
//! Falls back to the classic `CliRenderer` when stdout is not a TTY.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use indexmap::IndexMap;
#[cfg(test)]
use indicatif::ProgressDrawTarget;
use indicatif::{MultiProgress, ProgressBar, ProgressFinish, ProgressStyle};

use crate::display::{colors, icons, spinner, DetailLevel};
use crate::event::{Event, EventKind};

use super::renderer::RunStats;

// ── Task state ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Skipped,
}

struct TaskBar {
    bar: ProgressBar,
    verb: String,
    status: TaskStatus,
}

// ── LiveRenderer ──────────────────────────────────────────────────────

pub struct LiveRenderer {
    multi: MultiProgress,
    /// Task bars in DAG topological order.
    task_bars: IndexMap<String, TaskBar>,
    /// Overall progress bar (bottom).
    overall_bar: ProgressBar,
    /// Separator line between log and task bars.
    separator_bar: ProgressBar,
    /// Accumulated statistics (shared format with CliRenderer).
    stats: RunStats,
    /// Verbosity control.
    detail: DetailLevel,
    /// Workflow start instant.
    start: Instant,
    /// Task start times for duration calculation: task_id → (timestamp_ms, verb).
    task_starts: HashMap<String, (u64, String)>,
    /// Per-task token accumulator for O(1) lookup in TaskCompleted.
    task_token_acc: HashMap<String, (u64, u64)>,
    /// Workflow start timestamp from events.
    workflow_start_ms: u64,
    /// Last rendered event ID for incremental rendering.
    last_rendered_id: Option<u64>,
    /// Whether multi is finalized (no more in-place updates).
    finalized: bool,
    /// Cached ProgressStyle for running tasks (with spinner).
    style_running: ProgressStyle,
    /// Cached ProgressStyle for static tasks (pending/completed/failed/skipped).
    style_static: ProgressStyle,
}

impl LiveRenderer {
    /// Shared construction logic — both `new()` and `hidden()` delegate here.
    fn build(multi: MultiProgress, detail: DetailLevel) -> Self {
        // Separator — heavy rule ━ to distinguish from header's ─
        let sep_style =
            ProgressStyle::with_template(spinner::SEPARATOR_TEMPLATE).expect("sep template");
        let separator_bar = multi.add(ProgressBar::new(0));
        separator_bar.set_style(sep_style);

        // Overall progress bar (bottom) — task bars inserted BEFORE it in init_tasks()
        let overall_style = ProgressStyle::with_template(spinner::OVERALL_TEMPLATE)
            .expect("overall template")
            .progress_chars(spinner::PROGRESS_CHARS);
        let overall_bar = multi.add(ProgressBar::new(0));
        overall_bar.set_style(overall_style);

        // Cache styles — ProgressStyle::clone() is cheap (Arc internals)
        let style_running = ProgressStyle::with_template(spinner::TASK_RUNNING_TEMPLATE)
            .expect("running template")
            .tick_strings(spinner::TICK_STRINGS);
        let style_static =
            ProgressStyle::with_template(spinner::TASK_STATIC_TEMPLATE).expect("static template");

        Self {
            multi,
            task_bars: IndexMap::new(),
            overall_bar,
            separator_bar,
            stats: RunStats::default(),
            detail,
            start: Instant::now(),
            task_starts: HashMap::new(),
            task_token_acc: HashMap::new(),
            workflow_start_ms: 0,
            last_rendered_id: None,
            finalized: false,
            style_running,
            style_static,
        }
    }

    /// Create a new LiveRenderer. Does NOT create task bars yet — call `init_tasks()` after DAG analysis.
    pub fn new(detail: DetailLevel) -> Self {
        let term_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0)
            .unwrap_or(80);

        let renderer = Self::build(MultiProgress::new(), detail);

        // Set separator line (only for real terminal output)
        let sep_line = "━"
            .repeat((term_width as usize).min(72))
            .dimmed()
            .to_string();
        renderer.separator_bar.set_message(sep_line);

        renderer
    }

    /// Create a LiveRenderer with a hidden draw target (for testing).
    #[cfg(test)]
    pub fn hidden(detail: DetailLevel) -> Self {
        Self::build(
            MultiProgress::with_draw_target(ProgressDrawTarget::hidden()),
            detail,
        )
    }

    /// Set task-to-layer mapping (for consistency with CliRenderer interface).
    pub fn set_task_layers(&mut self, _layers: HashMap<Arc<str>, usize>) {
        // LiveRenderer doesn't use layer separators — tasks are always visible in the fixed area.
    }

    /// Initialize task bars after DAG analysis.
    ///
    /// `task_ids` must be in topological (execution) order.
    /// `task_deps` maps task_id → dependency task_ids.
    pub fn init_tasks(&mut self, task_ids: &[String], task_deps: &HashMap<String, Vec<String>>) {
        let total = task_ids.len();
        let visible = total.min(spinner::MAX_VISIBLE_TASKS);

        let static_style = self.style_static.clone();

        for (i, task_id) in task_ids.iter().enumerate() {
            if i >= visible {
                break;
            }

            let bar = self.multi.insert_before(
                &self.overall_bar,
                ProgressBar::new(0).with_finish(ProgressFinish::Abandon),
            );
            bar.set_style(static_style.clone());

            // Initial pending message
            let deps = task_deps.get(task_id).cloned().unwrap_or_default();
            let dep_refs: Vec<&str> = deps.iter().map(|s| s.as_str()).collect();
            let msg = Self::format_pending(task_id, &dep_refs);
            bar.set_message(msg);

            self.task_bars.insert(
                task_id.clone(),
                TaskBar {
                    bar,
                    verb: String::new(),
                    status: TaskStatus::Pending,
                },
            );
        }

        // If there are more tasks than visible, add an overflow indicator
        if total > visible {
            let overflow = total - visible;
            let bar = self
                .multi
                .insert_before(&self.overall_bar, ProgressBar::new(0));
            bar.set_style(static_style);
            bar.set_message(format!("  {} +{} more tasks", "…".dimmed(), overflow).to_string());
            // Store with a synthetic key
            self.task_bars.insert(
                "__overflow__".to_string(),
                TaskBar {
                    bar,
                    verb: String::new(),
                    status: TaskStatus::Pending,
                },
            );
        }

        // Update the overall bar's total (already in multi from new())
        self.overall_bar.set_length(total as u64);
        self.overall_bar.set_message("$0.0000".to_string());
    }

    /// Render a single EventKind (without an Event wrapper).
    pub fn render_kind(&mut self, kind: &EventKind) {
        let event = Event {
            id: 0,
            timestamp_ms: self.start.elapsed().as_millis() as u64,
            kind: kind.clone(),
        };
        self.render(&event);
    }

    /// Get last rendered event ID for incremental rendering.
    pub fn last_rendered_id(&self) -> Option<u64> {
        self.last_rendered_id
    }

    /// Render new events since last render call.
    pub fn render_new_events(&mut self, events: &[Event]) {
        for event in events {
            if self.last_rendered_id.is_none_or(|last| event.id > last) {
                self.render(event);
                self.last_rendered_id = Some(event.id);
            }
        }
    }

    // ── Event Log (scrolling above) ───────────────────────────────────

    /// Emit a line that scrolls above the fixed task area.
    fn log(&self, line: &str) {
        if !self.finalized {
            let _ = self.multi.println(line);
        } else {
            println!("{}", line);
        }
    }

    /// Format timestamp offset.
    fn ts(&self) -> String {
        let elapsed = self.start.elapsed().as_secs_f32();
        format!("{:>6}", format!("+{:.1}s", elapsed))
            .dimmed()
            .to_string()
    }

    // ── Task bar formatting ───────────────────────────────────────────

    fn format_pending_arc(task_id: &str, deps: &[Arc<str>]) -> String {
        let dep_strs: Vec<&str> = deps.iter().map(|d| d.as_ref()).collect();
        Self::format_pending(task_id, &dep_strs)
    }

    fn format_pending(task_id: &str, deps: &[&str]) -> String {
        let padded_id = Self::truncate_task_id(task_id, 16);
        let deps_str = if deps.is_empty() {
            String::new()
        } else {
            format!("deps: {}", deps.join(", ")).dimmed().to_string()
        };
        format!(
            "{} {} {}  {}",
            icons::pending(),
            " ".normal(), // 1-char verb placeholder (matches icon width)
            padded_id.dimmed(),
            if deps_str.is_empty() {
                "pending".dimmed().to_string()
            } else {
                deps_str
            }
        )
    }

    /// Truncate task ID to fit within `width` columns, adding ellipsis if needed.
    fn truncate_task_id(id: &str, width: usize) -> String {
        if colors::stripped_len(id) <= width {
            format!("{:<width$}", id)
        } else {
            let cut = colors::floor_char_boundary(id, width - 1);
            format!("{:<width$}", format!("{}…", &id[..cut]))
        }
    }

    /// Build the prefix (stable part) for a running task bar.
    /// Goes into `set_prefix()` — doesn't change while task runs.
    fn format_running_prefix(task_id: &str, verb: &str) -> String {
        let padded_id = Self::truncate_task_id(task_id, 16);
        format!("{} {}", icons::verb(verb), padded_id.bold())
    }

    /// Build the message (volatile part) for a running task bar.
    /// Goes into `set_message()` — updated on each ProviderResponded.
    fn format_running_msg(tokens_info: &str) -> String {
        if tokens_info.is_empty() {
            "running".white().to_string()
        } else {
            format!("{}  {}", "running".white(), tokens_info)
        }
    }


    fn format_completed(
        task_id: &str,
        verb: &str,
        dur_secs: f32,
        in_tok: u64,
        out_tok: u64,
    ) -> String {
        let padded_id = Self::truncate_task_id(task_id, 16);
        let tok_str = if in_tok > 0 || out_tok > 0 {
            format!(
                "  in:{} out:{}",
                colors::tokens(in_tok),
                colors::tokens(out_tok)
            )
        } else {
            String::new()
        };
        format!(
            "{} {} {} {}{}",
            icons::success(),
            icons::verb(verb),
            padded_id.bold(),
            colors::duration(dur_secs),
            tok_str.dimmed()
        )
    }

    fn format_failed(task_id: &str, verb: &str, dur_secs: f32, error: &str) -> String {
        let padded_id = Self::truncate_task_id(task_id, 16);
        let short_err = if error.len() > 40 {
            let cut = colors::floor_char_boundary(error, 39);
            format!("{}…", &error[..cut])
        } else {
            error.to_string()
        };
        format!(
            "{} {} {} {} {}",
            icons::failed(),
            icons::verb(verb),
            padded_id.bold().red(),
            colors::duration(dur_secs),
            short_err.red()
        )
    }

    fn format_skipped(task_id: &str, reason: &str) -> String {
        let padded_id = format!("{:<16}", task_id);
        format!(
            "{} {} {} {}",
            icons::skipped(),
            "  ".normal(),
            padded_id.dimmed(),
            format!("skipped — {}", reason).dimmed()
        )
    }

    /// Update the overall progress bar cost display.
    fn update_overall_cost(&self) {
        let cost_str = if self.stats.total_cost > 0.0 {
            colors::cost(self.stats.total_cost).to_string()
        } else {
            String::new()
        };
        self.overall_bar.set_message(cost_str);
    }

    // ── Main render dispatch ──────────────────────────────────────────

    /// Render a single event.
    ///
    /// Note: JSON mode is handled exclusively by `CliRenderer`. The `auto_renderer()`
    /// factory ensures `LiveRenderer` is never used with `DetailLevel::Json`.
    pub fn render(&mut self, event: &Event) {
        // Centralized stat accumulation — covers all ~15 stat-bearing event types
        self.stats.apply_event(event);

        match &event.kind {
            // ── Workflow level ─────────────────────────────────────
            EventKind::WorkflowStarted { .. } => {
                self.workflow_start_ms = event.timestamp_ms;
            }

            EventKind::WorkflowPaused => {
                self.log(&format!("{} {} paused", self.ts(), "⏸".yellow()));
            }

            EventKind::WorkflowResumed => {
                self.log(&format!("{} {} resumed", self.ts(), "▶".green()));
            }

            EventKind::WorkflowAborted {
                reason,
                running_tasks,
                ..
            } => {
                self.log(&format!(
                    "{} {} {}",
                    self.ts(),
                    "⚠".red().bold(),
                    "ABORTED".red().bold()
                ));
                self.log(&format!(
                    "{}   {} {}",
                    " ".repeat(6),
                    "reason:".dimmed(),
                    reason.red()
                ));
                if !running_tasks.is_empty() {
                    let names: Vec<&str> = running_tasks.iter().map(|s| s.as_ref()).collect();
                    self.log(&format!(
                        "{}   {} {}",
                        " ".repeat(6),
                        "running:".dimmed(),
                        names.join(", ").yellow()
                    ));
                }
            }

            // ── Task level ────────────────────────────────────────
            EventKind::TaskScheduled {
                task_id,
                dependencies,
            } => {
                // Update the task bar to show deps
                if let Some(tb) = self.task_bars.get_mut(task_id.as_ref()) {
                    let msg = Self::format_pending_arc(task_id, dependencies);
                    tb.bar.set_message(msg);
                }
            }

            EventKind::TaskStarted { task_id, verb, .. } => {
                self.task_starts
                    .insert(task_id.to_string(), (event.timestamp_ms, verb.to_string()));

                if let Some(tb) = self.task_bars.get_mut(task_id.as_ref()) {
                    tb.verb = verb.to_string();
                    tb.status = TaskStatus::Running;

                    // Switch to running style with spinner
                    tb.bar.set_style(self.style_running.clone());
                    tb.bar.enable_steady_tick(spinner::TICK_INTERVAL);

                    // Use prefix/msg split: prefix = verb+id (stable), msg = volatile info
                    tb.bar.set_prefix(Self::format_running_prefix(task_id, verb));
                    tb.bar.set_message(Self::format_running_msg(""));
                }
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                let dur_secs = *duration_ms as f32 / 1000.0;

                let verb = self
                    .task_starts
                    .get(task_id.as_ref())
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();

                // Record timeline
                if let Some((start, _)) = self.task_starts.get(task_id.as_ref()) {
                    self.stats.task_timeline.push((
                        task_id.to_string(),
                        verb.clone(),
                        start.saturating_sub(self.workflow_start_ms),
                        *duration_ms,
                    ));
                }

                // O(1) token lookup from per-task accumulator
                let (in_tok, out_tok) = self
                    .task_token_acc
                    .get(task_id.as_ref())
                    .copied()
                    .unwrap_or_default();

                if let Some(tb) = self.task_bars.get_mut(task_id.as_ref()) {
                    tb.status = TaskStatus::Completed;
                    tb.bar.set_style(self.style_static.clone());
                    let msg = Self::format_completed(task_id, &verb, dur_secs, in_tok, out_tok);
                    tb.bar.finish_with_message(msg);
                }

                self.overall_bar
                    .set_position(self.stats.tasks_passed as u64 + self.stats.tasks_skipped as u64);
                self.update_overall_cost();

                // Output preview (Max detail only)
                if self.detail.show_previews() {
                    let tw = terminal_size::terminal_size()
                        .map(|(w, _)| w.0)
                        .unwrap_or(80);
                    for line in super::renderer::format_output_preview(output, tw) {
                        self.log(&line);
                    }
                }
            }

            EventKind::TaskFailed {
                task_id,
                error,
                duration_ms,
                ..
            } => {
                let dur_secs = *duration_ms as f32 / 1000.0;
                let verb = self
                    .task_starts
                    .get(task_id.as_ref())
                    .map(|(_, v)| v.clone())
                    .unwrap_or_default();

                // Record timeline
                if let Some((start, _)) = self.task_starts.get(task_id.as_ref()) {
                    self.stats.task_timeline.push((
                        task_id.to_string(),
                        verb.clone(),
                        start.saturating_sub(self.workflow_start_ms),
                        *duration_ms,
                    ));
                }

                if let Some(tb) = self.task_bars.get_mut(task_id.as_ref()) {
                    tb.status = TaskStatus::Failed;
                    tb.bar.set_style(self.style_static.clone());
                    let msg = Self::format_failed(task_id, &verb, dur_secs, error);
                    tb.bar.finish_with_message(msg);
                }

                // Update progress bar (failed tasks count toward completion)
                self.overall_bar.set_position(
                    (self.stats.tasks_passed + self.stats.tasks_skipped + self.stats.tasks_failed)
                        as u64,
                );

                // Also log the full error as a scrolling line
                self.log(&format!(
                    "{}  {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    "error".red(),
                    error.red()
                ));
            }

            EventKind::TaskSkipped { task_id, reason } => {
                if let Some(tb) = self.task_bars.get_mut(task_id.as_ref()) {
                    tb.status = TaskStatus::Skipped;
                    tb.bar.set_style(self.style_static.clone());
                    let msg = Self::format_skipped(task_id, reason);
                    tb.bar.finish_with_message(msg);
                }

                self.overall_bar
                    .set_position(self.stats.tasks_passed as u64 + self.stats.tasks_skipped as u64);
            }

            // ── Provider events (scrolling log) ───────────────────
            EventKind::ProviderCalled {
                provider,
                model,
                prompt_len,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_provider_called(
                        provider,
                        model,
                        *prompt_len,
                    ));
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
                // O(1) per-task token accumulator (LiveRenderer-specific: for task bar display)
                let provider_task_id = event.kind.task_id().unwrap_or("?").to_string();
                let acc = self.task_token_acc.entry(provider_task_id).or_default();
                acc.0 += input_tokens;
                acc.1 += output_tokens;

                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_provider_responded(
                        *input_tokens,
                        *output_tokens,
                        *cache_read_tokens,
                        *ttft_ms,
                    ));
                    if self.detail.show_sparklines() {
                        self.log(&super::format_event::fmt_provider_sparkline(
                            *output_tokens,
                            *input_tokens,
                            *cost_usd,
                        ));
                    }
                }

                // Update running task bar with token info (elapsed auto-updates via template)
                let task_id = event.kind.task_id().unwrap_or("?");
                if let Some(tb) = self.task_bars.get(task_id) {
                    if tb.status == TaskStatus::Running {
                        let tok_info = format!(
                            "in:{} out:{}",
                            colors::tokens(*input_tokens),
                            colors::tokens(*output_tokens)
                        );
                        tb.bar.set_message(Self::format_running_msg(&tok_info));
                    }
                }

                self.update_overall_cost();
            }

            // ── Context ───────────────────────────────────────────
            EventKind::ContextAssembled {
                sources,
                total_tokens,
                budget_used_pct,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_context_assembled(
                        sources.len(),
                        *total_tokens,
                        *budget_used_pct,
                    ));
                }
            }

            // ── MCP ───────────────────────────────────────────────
            EventKind::McpConnected { server_name } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_mcp_connected(server_name));
                }
            }

            EventKind::McpError { server_name, error } => {
                self.log(&super::format_event::fmt_mcp_error(server_name, error));
            }

            EventKind::McpInvoke {
                call_id,
                mcp_server,
                tool,
                resource,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_mcp_invoke(
                        mcp_server,
                        tool.as_deref(),
                        resource.as_deref(),
                        call_id,
                    ));
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
                    self.log(&super::format_event::fmt_mcp_response(
                        call_id,
                        *output_len,
                        *duration_ms,
                        *cached,
                        *is_error,
                    ));
                }
            }

            EventKind::McpRetry {
                operation,
                attempt,
                max_attempts,
                error,
                ..
            } => {
                self.log(&super::format_event::fmt_mcp_retry(
                    operation,
                    *attempt,
                    *max_attempts,
                    error,
                ));
            }

            // ── Agent ─────────────────────────────────────────────
            EventKind::AgentStart {
                max_turns,
                mcp_servers,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_agent_start(
                        *max_turns,
                        mcp_servers,
                    ));
                }
            }

            EventKind::AgentTurn {
                turn_index,
                kind,
                metadata,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_agent_turn(*turn_index, kind));
                    if let Some(meta) = metadata {
                        if meta.stop_reason == "tool_use" {
                            self.log(&super::format_event::fmt_agent_turn_tool_use());
                        }
                    }
                }

                // Update task bar with turn count
                let task_id = event.kind.task_id().unwrap_or("?");
                if let Some(tb) = self.task_bars.get(task_id) {
                    if tb.status == TaskStatus::Running {
                        let turn_info = format!("turn {}", turn_index + 1);
                        tb.bar.set_message(Self::format_running_msg(&turn_info));
                    }
                }
            }

            EventKind::AgentComplete {
                turns, stop_reason, ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_agent_complete(
                        *turns,
                        stop_reason,
                    ));
                }
            }

            EventKind::AgentSpawned {
                child_task_id,
                depth,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_agent_spawned(
                        child_task_id,
                        *depth,
                    ));
                }
            }

            // ── Guardrails ────────────────────────────────────────
            EventKind::GuardrailPassed {
                guardrail_type,
                description,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_guardrail_passed(
                        guardrail_type,
                        description,
                    ));
                }
            }

            EventKind::GuardrailFailed {
                guardrail_type,
                message,
                ..
            } => {
                self.log(&super::format_event::fmt_guardrail_failed(
                    guardrail_type,
                    message,
                ));
            }

            EventKind::GuardrailEscalation {
                guardrail_type,
                severity,
                message,
                ..
            } => {
                self.log(&super::format_event::fmt_guardrail_escalation(
                    guardrail_type, severity, message,
                ));
            }

            // ── Builtin / Log ─────────────────────────────────────
            EventKind::Log { level, message, .. } => {
                self.log(&super::format_event::fmt_log(&self.ts(), level, message));
            }

            EventKind::Custom { name, payload, .. } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_custom(&self.ts(), name, payload));
                }
            }

            // ── Artifacts ─────────────────────────────────────────
            EventKind::ArtifactWritten {
                path, size, format, ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_artifact_written(
                        path, *size, format,
                    ));
                }
            }

            EventKind::ArtifactFailed { path, reason, .. } => {
                self.log(&super::format_event::fmt_artifact_failed(path, reason));
            }

            // ── Media ─────────────────────────────────────────────
            EventKind::MediaExtracted {
                block_count,
                content_types,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_media_extracted(
                        *block_count,
                        content_types,
                    ));
                }
            }

            EventKind::MediaStored {
                hash,
                path,
                size_bytes,
                deduplicated,
                verified,
                pipeline_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_media_stored(
                        *size_bytes,
                        path,
                        hash,
                    ));
                    if self.detail.show_previews() {
                        self.log(&super::format_event::fmt_media_stored_detail(
                            *deduplicated,
                            *verified,
                            *pipeline_ms,
                        ));
                    }
                }
            }

            EventKind::MediaStoreFailed { reason, .. } => {
                self.log(&super::format_event::fmt_media_store_failed(reason));
            }

            // ── Structured output ─────────────────────────────────
            EventKind::StructuredOutputAttempt {
                layer,
                layer_name,
                success,
                error,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_structured_output_attempt(
                        *layer,
                        layer_name,
                        *success,
                        error.as_deref(),
                    ));
                }
            }

            EventKind::StructuredOutputSuccess { .. } => {
                // Stats handled by apply_event()
            }

            // ── Vision ────────────────────────────────────────────
            EventKind::VisionContentResolved {
                image_count,
                total_bytes,
                resolve_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_vision_content_resolved(
                        *image_count,
                        *total_bytes,
                        *resolve_ms,
                    ));
                }
            }

            // ── HTTP ──────────────────────────────────────────────
            EventKind::HttpRequest { method, url, .. } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_http_request(method, url));
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
                    self.log(&super::format_event::fmt_http_response(
                        *status_code,
                        content_type.as_deref(),
                        *content_length,
                        *elapsed_ms,
                    ));
                }
            }

            // ── Template ──────────────────────────────────────────
            EventKind::TemplateResolved {
                template, result, ..
            } => {
                if self.detail.show_template_events() {
                    self.log(&super::format_event::fmt_template_resolved(
                        template, result,
                    ));
                }
            }

            // ── Workflow completion ────────────────────────────────
            EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. } => {
                // Defensive: ensure bars are finalized even if caller forgets.
                // The summary is printed by the caller via render_summary().
                self.finalize_bars();
            }

            // ── For-each ──────────────────────────────────────────
            EventKind::ForEachCompleted {
                task_id,
                total,
                succeeded,
                failed,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_for_each_completed(
                        task_id, *total, *succeeded, *failed,
                    ));
                }
            }

            // ── Exec completed ────────────────────────────────────
            EventKind::ExecCompleted {
                exit_code,
                duration_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_exec_completed(
                        *exit_code,
                        *duration_ms,
                    ));
                }
            }

            // ── Policy blocked ────────────────────────────────────
            EventKind::PolicyBlocked {
                policy_type,
                reason,
                ..
            } => {
                self.log(&super::format_event::fmt_policy_blocked(
                    &self.ts(),
                    policy_type,
                    reason,
                ));
            }

            // ── Fetch retry (always show) ─────────────────────────
            EventKind::FetchRetry {
                url,
                attempt,
                max_attempts,
                status_code,
                backoff_ms,
                ..
            } => {
                self.log(&super::format_event::fmt_fetch_retry(
                    url,
                    *attempt,
                    *max_attempts,
                    *status_code,
                    *backoff_ms,
                ));
            }

            // ── Boot (always show) ───────────────────────────────
            EventKind::BootPhaseCompleted {
                phase,
                success,
                duration_ms,
                warnings,
            } => {
                self.log(&super::format_event::fmt_boot_phase(
                    phase,
                    *success,
                    *duration_ms,
                    warnings,
                ));
            }

            // ── Native model ─────────────────────────────────────
            EventKind::NativeModelLoaded {
                model,
                kind,
                duration_ms,
                is_vision,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_native_model_loaded(
                        model,
                        kind,
                        *duration_ms,
                        *is_vision,
                    ));
                }
            }

            // ── Binding events ───────────────────────────────────
            EventKind::BindingDefaultApplied { alias, path, .. } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_binding_default(alias, path));
                }
            }

            EventKind::BindingTransformApplied {
                alias,
                transform_chain,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_binding_transform(
                        alias,
                        transform_chain,
                    ));
                }
            }

            EventKind::BindingEnvResolved {
                var_name, found, ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_binding_env(var_name, *found));
                }
            }

            // ── Decompose ────────────────────────────────────────
            EventKind::DecomposeStarted {
                task_id, strategy, ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_decompose_started(
                        task_id, strategy,
                    ));
                }
            }

            EventKind::DecomposeCompleted {
                task_id,
                item_count,
                duration_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_decompose_completed(
                        task_id,
                        *item_count,
                        *duration_ms,
                    ));
                }
            }

            // ── For-each started ─────────────────────────────────
            EventKind::ForEachStarted {
                task_id,
                item_count,
                concurrency,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_for_each_started(
                        task_id,
                        *item_count,
                        *concurrency,
                    ));
                }
            }

            // ── Provider initialized ─────────────────────────────
            EventKind::ProviderInitialized {
                provider,
                model,
                cached,
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_provider_initialized(
                        provider, model, *cached,
                    ));
                }
            }

            // ── Builtin tool invoked ─────────────────────────────
            EventKind::BuiltinToolInvoked {
                tool_name,
                duration_ms,
                success,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_builtin_tool_invoked(
                        tool_name,
                        *duration_ms,
                        *success,
                    ));
                }
            }

            // ── Extract applied ──────────────────────────────────
            EventKind::ExtractApplied {
                mode,
                input_len,
                output_len,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&super::format_event::fmt_extract_applied(
                        mode,
                        *input_len,
                        *output_len,
                    ));
                }
            }

            // Catch-all for events handled elsewhere (media lifecycle, etc.)
            _ => {}
        }
    }

    // ── Summary ───────────────────────────────────────────────────────

    /// Finalize the multi-progress and print the summary box.
    pub fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>) {
        self.finalize_bars();
        super::summary::print_run_summary(&self.stats, self.detail, total_duration_ms, trace_path);
    }

    /// Finalize the multi-progress and print compact summary.
    pub fn render_quiet_summary(&mut self, total_duration_ms: u64) {
        self.finalize_bars();
        super::summary::print_run_quiet_summary(&self.stats, total_duration_ms);
    }

    /// Clear all progress bars so they stop updating.
    fn finalize_bars(&mut self) {
        if self.finalized {
            return;
        }
        self.finalized = true;

        // Finish all bars
        for tb in self.task_bars.values() {
            if !tb.bar.is_finished() {
                tb.bar.abandon();
            }
        }
        self.separator_bar.finish_and_clear();
        self.overall_bar.finish_and_clear();

        // Clear the multi-progress draw area
        self.multi.clear().ok();
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Renderer trait impl for LiveRenderer
// ═══════════════════════════════════════════════════════════════════════

impl super::renderer::Renderer for LiveRenderer {
    fn set_task_layers(&mut self, _layers: HashMap<Arc<str>, usize>) {
        // LiveRenderer doesn't use layer separators — tasks are always visible in the fixed area.
    }

    fn init_tasks(&mut self, task_ids: &[String], task_deps: &HashMap<String, Vec<String>>) {
        LiveRenderer::init_tasks(self, task_ids, task_deps);
    }

    fn last_rendered_id(&self) -> Option<u64> {
        self.last_rendered_id
    }

    fn render_kind(&mut self, kind: &EventKind) {
        LiveRenderer::render_kind(self, kind);
    }

    fn render_new_events(&mut self, events: &[Event]) {
        LiveRenderer::render_new_events(self, events);
    }

    fn render_summary(&mut self, total_duration_ms: u64, trace_path: Option<&str>) {
        LiveRenderer::render_summary(self, total_duration_ms, trace_path);
    }

    fn render_quiet_summary(&mut self, total_duration_ms: u64) {
        LiveRenderer::render_quiet_summary(self, total_duration_ms);
    }

    fn stats(&self) -> &super::renderer::RunStats {
        &self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_live_renderer() {
        let renderer = LiveRenderer::hidden(DetailLevel::Max);
        assert_eq!(renderer.last_rendered_id(), None);
        assert!(!renderer.finalized);
    }

    #[test]
    fn test_init_tasks_creates_bars() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        let task_ids = vec!["fetch".to_string(), "summarize".to_string()];
        let mut deps = HashMap::new();
        deps.insert("summarize".to_string(), vec!["fetch".to_string()]);
        deps.insert("fetch".to_string(), vec![]);

        renderer.init_tasks(&task_ids, &deps);
        assert_eq!(renderer.task_bars.len(), 2);
    }

    #[test]
    fn test_init_tasks_overflow() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        let task_ids: Vec<String> = (0..30).map(|i| format!("task_{}", i)).collect();
        let deps = HashMap::new();

        renderer.init_tasks(&task_ids, &deps);
        // MAX_VISIBLE_TASKS + 1 overflow bar
        assert_eq!(renderer.task_bars.len(), spinner::MAX_VISIBLE_TASKS + 1);
    }

    #[test]
    fn test_format_pending() {
        let msg = LiveRenderer::format_pending("fetch_data", &[]);
        assert!(msg.contains("fetch_data"));
    }

    #[test]
    fn test_format_completed() {
        let msg = LiveRenderer::format_completed("fetch_data", "fetch", 1.5, 1200, 342);
        assert!(msg.contains("fetch_data"));
    }

    #[test]
    fn test_format_failed() {
        let msg = LiveRenderer::format_failed("broken", "infer", 0.5, "NIKA-044: Template error");
        assert!(msg.contains("broken"));
    }

    #[test]
    fn test_format_skipped() {
        let msg = LiveRenderer::format_skipped("review", "dependency failed");
        assert!(msg.contains("review"));
        assert!(msg.contains("skipped"));
    }

    #[test]
    fn test_finalize_is_idempotent() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        renderer.finalize_bars();
        assert!(renderer.finalized);
        renderer.finalize_bars(); // second call is a no-op
        assert!(renderer.finalized);
    }

    // ── Event rendering tests (Task 9) ───────────────────────────────

    /// Helper: create a minimal Event with the given kind and id.
    fn make_event(id: u64, timestamp_ms: u64, kind: EventKind) -> Event {
        Event {
            id,
            timestamp_ms,
            kind,
        }
    }

    #[test]
    fn event_task_started_through_completed_updates_stats() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        let task_ids = vec!["research".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps);

        // TaskStarted
        renderer.render(&make_event(
            1,
            0,
            EventKind::TaskStarted {
                task_id: Arc::from("research"),
                verb: Arc::from("infer"),
                inputs: serde_json::Value::Null,
            },
        ));
        assert_eq!(renderer.task_bars["research"].status, TaskStatus::Running);

        // ProviderResponded
        renderer.render(&make_event(
            2,
            100,
            EventKind::ProviderResponded {
                task_id: Arc::from("research"),
                request_id: None,
                input_tokens: 500,
                output_tokens: 200,
                cache_read_tokens: 50,
                ttft_ms: Some(120),
                finish_reason: "stop".to_string(),
                cost_usd: 0.002,
            },
        ));
        assert_eq!(renderer.stats.total_input_tokens, 500);
        assert_eq!(renderer.stats.total_output_tokens, 200);
        assert_eq!(renderer.stats.total_cache_tokens, 50);
        assert_eq!(renderer.stats.total_cost, 0.002);
        assert_eq!(renderer.stats.ttft_values, vec![120]);

        // TaskCompleted
        renderer.render(&make_event(
            3,
            1500,
            EventKind::TaskCompleted {
                task_id: Arc::from("research"),
                output: Arc::new(serde_json::Value::String("result".to_string())),
                duration_ms: 1500,
            },
        ));
        assert_eq!(renderer.stats.tasks_passed, 1);
        assert_eq!(renderer.task_bars["research"].status, TaskStatus::Completed);
        assert_eq!(renderer.stats.task_timeline.len(), 1);
    }

    #[test]
    fn event_task_failed_sets_root_failure() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        let task_ids = vec!["broken".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps);

        // Start the task
        renderer.render(&make_event(
            1,
            0,
            EventKind::TaskStarted {
                task_id: Arc::from("broken"),
                verb: Arc::from("exec"),
                inputs: serde_json::Value::Null,
            },
        ));

        // Fail the task
        renderer.render(&make_event(
            2,
            500,
            EventKind::TaskFailed {
                task_id: Arc::from("broken"),
                error: "command not found".to_string(),
                duration_ms: 500,
                error_code: Some("NIKA-055".to_string()),
            },
        ));
        assert_eq!(renderer.stats.tasks_failed, 1);
        assert_eq!(
            renderer.stats.root_failure.as_deref(),
            Some("broken"),
            "should record first failure as root"
        );
        assert_eq!(renderer.task_bars["broken"].status, TaskStatus::Failed);
    }

    #[test]
    fn event_render_new_events_incremental() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        let task_ids = vec!["t1".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps);

        let events = vec![
            make_event(
                1,
                0,
                EventKind::TaskStarted {
                    task_id: Arc::from("t1"),
                    verb: Arc::from("fetch"),
                    inputs: serde_json::Value::Null,
                },
            ),
            make_event(
                2,
                1000,
                EventKind::TaskCompleted {
                    task_id: Arc::from("t1"),
                    output: Arc::new(serde_json::Value::Null),
                    duration_ms: 1000,
                },
            ),
        ];

        // First call renders both
        renderer.render_new_events(&events);
        assert_eq!(renderer.last_rendered_id(), Some(2));
        assert_eq!(renderer.stats.tasks_passed, 1);

        // Second call with same events renders nothing new
        let prev_passed = renderer.stats.tasks_passed;
        renderer.render_new_events(&events);
        assert_eq!(renderer.stats.tasks_passed, prev_passed, "should not re-render");
    }

    #[test]
    fn detail_min_suppresses_sub_events() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Min);
        let task_ids = vec!["t1".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps);

        // ProviderCalled is a sub-event — Min should suppress it
        renderer.render(&make_event(
            1,
            0,
            EventKind::ProviderCalled {
                task_id: Arc::from("t1"),
                provider: "anthropic".to_string(),
                model: "claude-sonnet-4-20250514".to_string(),
                prompt_len: 500,
            },
        ));
        // No panic, no assertion on output (hidden target) — just verifying no crash
        // Stats are still accumulated even when display is suppressed
        assert_eq!(renderer.stats.tasks_passed, 0, "provider call does not affect pass count");
    }

    #[test]
    fn event_task_skipped_updates_stats() {
        let mut renderer = LiveRenderer::hidden(DetailLevel::Max);
        let task_ids = vec!["skippable".to_string()];
        let deps = HashMap::new();
        renderer.init_tasks(&task_ids, &deps);

        renderer.render(&make_event(
            1,
            0,
            EventKind::TaskSkipped {
                task_id: Arc::from("skippable"),
                reason: "dependency failed".to_string(),
            },
        ));
        assert_eq!(renderer.stats.tasks_skipped, 1);
        assert_eq!(renderer.task_bars["skippable"].status, TaskStatus::Skipped);
    }
}
