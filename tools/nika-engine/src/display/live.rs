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
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
#[cfg(test)]
use indicatif::ProgressDrawTarget;

use crate::display::{colors, icons, spinner, DetailLevel};
use crate::event::{Event, EventKind};

use super::renderer::{format_bytes, ProviderCallStat, RunStats};

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
    /// Create a new LiveRenderer. Does NOT create task bars yet — call `init_tasks()` after DAG analysis.
    pub fn new(detail: DetailLevel) -> Self {
        let term_width = terminal_size::terminal_size()
            .map(|(w, _)| w.0)
            .unwrap_or(80);

        let multi = MultiProgress::new();

        // Separator — heavy rule ━ to distinguish from header's ─
        let sep_style =
            ProgressStyle::with_template(spinner::SEPARATOR_TEMPLATE).expect("sep template");
        let separator_bar = multi.add(ProgressBar::new(0));
        separator_bar.set_style(sep_style);
        let sep_line = "━"
            .repeat((term_width as usize).min(72))
            .dimmed()
            .to_string();
        separator_bar.set_message(sep_line);

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
        let style_static = ProgressStyle::with_template(spinner::TASK_STATIC_TEMPLATE)
            .expect("static template");

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

    /// Create a LiveRenderer with a hidden draw target (for testing).
    #[cfg(test)]
    pub fn hidden(detail: DetailLevel) -> Self {
        let multi = MultiProgress::with_draw_target(ProgressDrawTarget::hidden());

        let sep_style =
            ProgressStyle::with_template(spinner::SEPARATOR_TEMPLATE).expect("sep template");
        let separator_bar = multi.add(ProgressBar::new(0));
        separator_bar.set_style(sep_style);

        let overall_style = ProgressStyle::with_template(spinner::OVERALL_TEMPLATE)
            .expect("overall template")
            .progress_chars(spinner::PROGRESS_CHARS);
        let overall_bar = multi.add(ProgressBar::new(0));
        overall_bar.set_style(overall_style);

        let style_running = ProgressStyle::with_template(spinner::TASK_RUNNING_TEMPLATE)
            .expect("running template")
            .tick_strings(spinner::TICK_STRINGS);
        let style_static = ProgressStyle::with_template(spinner::TASK_STATIC_TEMPLATE)
            .expect("static template");

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

    /// Set task-to-layer mapping (for consistency with CliRenderer interface).
    pub fn set_task_layers(&mut self, _layers: HashMap<Arc<str>, usize>) {
        // LiveRenderer doesn't use layer separators — tasks are always visible in the fixed area.
    }

    /// Initialize task bars after DAG analysis.
    ///
    /// `task_ids` must be in topological (execution) order.
    /// `task_deps` maps task_id → dependency task_ids.
    pub fn init_tasks(
        &mut self,
        task_ids: &[String],
        task_deps: &HashMap<String, Vec<String>>,
    ) {
        let total = task_ids.len();
        let visible = total.min(spinner::MAX_VISIBLE_TASKS);

        let static_style = self.style_static.clone();

        for (i, task_id) in task_ids.iter().enumerate() {
            if i >= visible {
                break;
            }

            let bar = self.multi.insert_before(&self.overall_bar, ProgressBar::new(0));
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
            let bar = self.multi.insert_before(&self.overall_bar, ProgressBar::new(0));
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
        if id.len() <= width {
            format!("{:<width$}", id)
        } else {
            let cut = colors::floor_char_boundary(id, width - 1);
            format!("{:<width$}", format!("{}…", &id[..cut]))
        }
    }

    fn format_running(task_id: &str, verb: &str, elapsed: &str, tokens_info: &str) -> String {
        let padded_id = Self::truncate_task_id(task_id, 16);
        format!(
            "{} {} {}  {}{}",
            icons::verb(verb),
            padded_id.bold(),
            "running".white(),
            elapsed.dimmed(),
            if tokens_info.is_empty() {
                String::new()
            } else {
                format!("  {}", tokens_info)
            }
        )
    }

    fn format_completed(task_id: &str, verb: &str, dur_secs: f32, in_tok: u64, out_tok: u64) -> String {
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
        let padded_id = format!("{:<16}", task_id);
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
    /// Note: JSON mode is handled exclusively by `CliRenderer`. The `RunRenderer::auto()`
    /// constructor ensures `LiveRenderer` is never used with `DetailLevel::Json`.
    pub fn render(&mut self, event: &Event) {
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
                self.stats.task_count += 1;
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

                    let msg = Self::format_running(task_id, verb, "", "");
                    tb.bar.set_message(msg);
                }
            }

            EventKind::TaskCompleted {
                task_id,
                output,
                duration_ms,
            } => {
                self.stats.tasks_passed += 1;
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
                self.stats.tasks_failed += 1;
                if self.stats.root_failure.is_none() {
                    self.stats.root_failure = Some(task_id.to_string());
                }
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
                self.stats.tasks_skipped += 1;

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
                    self.log(&format!(
                        "{}     {} {} {}/{} {} {} chars",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::provider(),
                        provider.dimmed(),
                        model.white(),
                        "· prompt:".dimmed(),
                        prompt_len
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
                self.stats.total_input_tokens += input_tokens;
                self.stats.total_output_tokens += output_tokens;
                self.stats.total_cache_tokens += cache_read_tokens;
                self.stats.total_cost += cost_usd;
                if let Some(t) = ttft_ms {
                    self.stats.ttft_values.push(*t);
                }
                let provider_task_id = event.kind.task_id().unwrap_or("?").to_string();
                self.stats.provider_calls.push(ProviderCallStat {
                    task_id: provider_task_id.clone(),
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    cache_tokens: *cache_read_tokens,
                    ttft_ms: *ttft_ms,
                    cost: *cost_usd,
                });

                // O(1) per-task token accumulator (avoids O(n) scan in TaskCompleted)
                let acc = self.task_token_acc.entry(provider_task_id).or_default();
                acc.0 += input_tokens;
                acc.1 += output_tokens;

                if self.detail.show_sub_events() {
                    let ttft_str = ttft_ms
                        .map(|t| format!(" · ttft:{}", colors::ttft(t)))
                        .unwrap_or_default();
                    self.log(&format!(
                        "{}     {} {} {} in:{} out:{} cache:{}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::provider(),
                        "←".dimmed(),
                        colors::tokens(*input_tokens).as_str().dimmed(),
                        colors::tokens(*output_tokens).as_str().white(),
                        colors::tokens(*cache_read_tokens).as_str().dimmed(),
                        ttft_str
                    ));

                    if self.detail.show_sparklines() {
                        let max_tok = (*input_tokens).max(*output_tokens);
                        self.log(&format!(
                            "{}     {}    tok {} cost {}",
                            " ".repeat(6),
                            "│".dimmed(),
                            colors::sparkline(*output_tokens, max_tok),
                            colors::cost(*cost_usd)
                        ));
                    }
                }

                // Update running task bar with token info
                let task_id = event.kind.task_id().unwrap_or("?");
                if let Some(tb) = self.task_bars.get(task_id) {
                    if tb.status == TaskStatus::Running {
                        let elapsed = self.start.elapsed().as_secs_f32();
                        let elapsed_str = format!("+{:.1}s", elapsed);
                        let tok_info = format!(
                            "in:{} out:{}",
                            colors::tokens(*input_tokens),
                            colors::tokens(*output_tokens)
                        );
                        let msg = Self::format_running(task_id, &tb.verb, &elapsed_str, &tok_info);
                        tb.bar.set_message(msg);
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
                    let warn = if *budget_used_pct > 90.0 {
                        " ⚠".red().to_string()
                    } else {
                        String::new()
                    };
                    self.log(&format!(
                        "{}     {} {} {} src · {} tok · {}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "ctx".dimmed(),
                        sources.len(),
                        colors::tokens(*total_tokens),
                        colors::budget_bar(*budget_used_pct, 25),
                        warn
                    ));
                }
            }

            // ── MCP ───────────────────────────────────────────────
            EventKind::McpConnected { server_name } => {
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} connected {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        server_name.green()
                    ));
                }
            }

            EventKind::McpError { server_name, error } => {
                self.stats.mcp_errors += 1;
                self.log(&format!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::mcp(),
                    format!("{} ✗", server_name).red(),
                    error.red()
                ));
            }

            EventKind::McpInvoke {
                call_id,
                mcp_server,
                tool,
                resource,
                ..
            } => {
                self.stats.mcp_calls += 1;
                if self.detail.show_sub_events() {
                    let target = tool.as_deref().or(resource.as_deref()).unwrap_or("?");
                    self.log(&format!(
                        "{}     {} {} {} → {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        mcp_server.dimmed(),
                        target.white(),
                        format!("call:{}", call_id).dimmed()
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
                    let cache_tag = if *cached {
                        " cached".green().to_string()
                    } else {
                        String::new()
                    };
                    let err_tag = if *is_error {
                        " ✗".red().to_string()
                    } else {
                        String::new()
                    };
                    let call_label = format!("call:{}", call_id);
                    let dur_label = format!(" · {}ms", duration_ms);
                    let suffix = format!("{}{}", cache_tag, err_tag);
                    self.log(&format!(
                        "{}     {} {} {} {} · {}{}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        call_label.dimmed(),
                        "←".dimmed(),
                        format_bytes(*output_len as u64),
                        dur_label.dimmed(),
                        suffix
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
                self.stats.mcp_retries += 1;
                self.log(&format!(
                    "{}     {} {} {} {}/{} · {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::retry(),
                    format!("retry {}", operation).yellow(),
                    attempt.to_string().yellow(),
                    max_attempts,
                    error.dimmed()
                ));
            }

            // ── Agent ─────────────────────────────────────────────
            EventKind::AgentStart {
                max_turns,
                mcp_servers,
                ..
            } => {
                if self.detail.show_sub_events() {
                    let servers = mcp_servers.join(", ");
                    self.log(&format!(
                        "{}     {} {} {} max_turns:{} · mcp:[{}]",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        "agent".dimmed(),
                        max_turns,
                        servers.green()
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
                    self.log(&format!(
                        "{}     {} {} turn {}/…  {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        (turn_index + 1).to_string().white(),
                        kind.dimmed()
                    ));

                    if let Some(meta) = metadata {
                        if meta.stop_reason == "tool_use" {
                            self.log(&format!(
                                "{}     {} {} tool_use",
                                " ".repeat(6),
                                "│".dimmed(),
                                "↳".dimmed()
                            ));
                        }
                    }
                }

                // Update task bar with turn count
                let task_id = event.kind.task_id().unwrap_or("?");
                if let Some(tb) = self.task_bars.get(task_id) {
                    if tb.status == TaskStatus::Running {
                        let elapsed = self.start.elapsed().as_secs_f32();
                        let elapsed_str = format!("+{:.1}s", elapsed);
                        let turn_info = format!("turn {}", turn_index + 1);
                        let msg = Self::format_running(task_id, &tb.verb, &elapsed_str, &turn_info);
                        tb.bar.set_message(msg);
                    }
                }
            }

            EventKind::AgentComplete {
                turns,
                stop_reason,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} {} {} turns · {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        "done".green(),
                        turns,
                        stop_reason.dimmed()
                    ));
                }
            }

            EventKind::AgentSpawned {
                child_task_id,
                depth,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} spawned {} depth:{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "⤋".magenta(),
                        child_task_id.white(),
                        depth
                    ));
                }
            }

            // ── Guardrails ────────────────────────────────────────
            EventKind::GuardrailPassed {
                guardrail_type,
                description,
                ..
            } => {
                self.stats.guardrails_passed += 1;
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::guardrail(),
                        icons::success(),
                        format!("{} · {}", guardrail_type, description).dimmed()
                    ));
                }
            }

            EventKind::GuardrailFailed {
                guardrail_type,
                message,
                ..
            } => {
                self.stats.guardrails_failed += 1;
                self.log(&format!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::guardrail(),
                    icons::failed(),
                    format!("{} · {}", guardrail_type, message).red()
                ));
            }

            EventKind::GuardrailEscalation {
                severity, message, ..
            } => {
                self.stats.guardrails_escalations += 1;
                self.log(&format!(
                    "{}     {}   {} {} · {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::retry(),
                    format!("escalation · {}", severity).yellow(),
                    message.dimmed()
                ));
            }

            // ── Builtin / Log ─────────────────────────────────────
            EventKind::Log { level, message, .. } => {
                let level_colored = match level.as_str() {
                    "error" => level.red(),
                    "warn" => level.yellow(),
                    "info" => level.green(),
                    "debug" => level.dimmed(),
                    "trace" => level.dimmed(),
                    _ => level.normal(),
                };
                self.log(&format!(
                    "{}  {} {} · {}",
                    self.ts(),
                    icons::log(),
                    level_colored,
                    message
                ));
            }

            EventKind::Custom { name, payload, .. } => {
                if self.detail.show_sub_events() {
                    let preview = serde_json::to_string(payload).unwrap_or_default();
                    let short = if preview.len() > 60 {
                        format!(
                            "{}…",
                            &preview[..colors::floor_char_boundary(&preview, 60)]
                        )
                    } else {
                        preview
                    };
                    self.log(&format!(
                        "{}  {} {} · {}",
                        self.ts(),
                        icons::log(),
                        name.cyan(),
                        short.dimmed()
                    ));
                }
            }

            // ── Artifacts ─────────────────────────────────────────
            EventKind::ArtifactWritten {
                path,
                size,
                format,
                ..
            } => {
                self.stats.artifacts_count += 1;
                self.stats.artifacts_bytes += size;
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::artifact(),
                        format!("→ {}", path).cyan(),
                        format!("{} · {}", format_bytes(*size), format).dimmed()
                    ));
                }
            }

            EventKind::ArtifactFailed { path, reason, .. } => {
                self.log(&format!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::artifact(),
                    format!("✗ {}", path).red(),
                    reason.dimmed()
                ));
            }

            // ── Media ─────────────────────────────────────────────
            EventKind::MediaExtracted {
                block_count,
                content_types,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} {} blocks · types: [{}]",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::media(),
                        block_count,
                        content_types.join(", ").magenta()
                    ));
                }
            }

            EventKind::MediaStored {
                hash,
                path,
                size_bytes,
                deduplicated,
                ..
            } => {
                self.stats.media_stored += 1;
                self.stats.media_bytes += size_bytes;
                if *deduplicated {
                    self.stats.media_dedup += 1;
                }
                if self.detail.show_sub_events() {
                    let short_hash = if hash.len() > 16 { &hash[..16] } else { hash };
                    self.log(&format!(
                        "{}     {} {} {} · {} · {}…",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::media(),
                        format_bytes(*size_bytes),
                        path.dimmed(),
                        short_hash.dimmed()
                    ));
                }
            }

            EventKind::MediaStoreFailed { reason, .. } => {
                self.log(&format!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::media(),
                    "✗".red(),
                    reason.red()
                ));
            }

            // ── Structured output ─────────────────────────────────
            EventKind::StructuredOutputAttempt {
                layer,
                layer_name,
                success,
                error,
                ..
            } => {
                self.stats.structured_attempts += 1;
                if self.detail.show_sub_events() {
                    let status = if *success {
                        icons::success()
                    } else {
                        icons::failed()
                    };
                    let err_msg = error
                        .as_deref()
                        .map(|e| format!(" {}", e.dimmed()))
                        .unwrap_or_default();
                    self.log(&format!(
                        "{}     {} {} L{}: {} {}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::structured(),
                        layer,
                        layer_name,
                        status,
                        err_msg
                    ));
                }
            }

            EventKind::StructuredOutputSuccess { layer, .. } => {
                self.stats.structured_success_layer = Some(*layer);
            }

            // ── Vision ────────────────────────────────────────────
            EventKind::VisionContentResolved {
                image_count,
                total_bytes,
                resolve_ms,
                ..
            } => {
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} {} images · {} · resolved {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::vision(),
                        image_count,
                        format_bytes(*total_bytes),
                        resolve_ms
                    ));
                }
            }

            // ── HTTP ──────────────────────────────────────────────
            EventKind::HttpRequest { method, url, .. } => {
                if self.detail.show_sub_events() {
                    self.log(&format!(
                        "{}     {} {} → {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::http(),
                        method.cyan(),
                        url.underline()
                    ));
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
                    let status_colored = if *status_code < 300 {
                        status_code.to_string().green()
                    } else if *status_code < 400 {
                        status_code.to_string().yellow()
                    } else {
                        status_code.to_string().red()
                    };
                    let ct = content_type.as_deref().unwrap_or("?");
                    let cl = content_length.map(format_bytes).unwrap_or_default();
                    self.log(&format!(
                        "{}     {} {} ← {} · {} · {} · {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::http(),
                        status_colored,
                        ct.dimmed(),
                        cl,
                        elapsed_ms
                    ));
                }
            }

            // ── Template ──────────────────────────────────────────
            EventKind::TemplateResolved {
                template, result, ..
            } => {
                if self.detail.show_template_events() {
                    self.log(&format!(
                        "{}     {} {} {} → {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "tmpl".dimmed(),
                        template.dimmed(),
                        result.dimmed()
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
                    let status = if *failed > 0 {
                        format!("{}/{} ok · {} failed", succeeded, total, failed)
                            .yellow()
                            .to_string()
                    } else {
                        format!("{}/{} ok", succeeded, total)
                            .green()
                            .to_string()
                    };
                    self.log(&format!(
                        "{}     {} {} for_each {} · {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::log(),
                        task_id.dimmed(),
                        status
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
                    let code_str = if *exit_code == 0 {
                        "0".green().to_string()
                    } else {
                        exit_code.to_string().red().to_string()
                    };
                    self.log(&format!(
                        "{}     {} {} exit:{} · {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::verb("exec"),
                        code_str,
                        duration_ms
                    ));
                }
            }

            // ── Policy blocked ────────────────────────────────────
            EventKind::PolicyBlocked {
                policy_type,
                reason,
                ..
            } => {
                self.log(&format!(
                    "{}  {} {} {} · {}",
                    self.ts(),
                    "⊘".red().bold(),
                    "BLOCKED".red().bold(),
                    policy_type.yellow(),
                    reason.red()
                ));
            }

            // Catch-all for events handled elsewhere (media lifecycle, boot, etc.)
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
        assert_eq!(
            renderer.task_bars.len(),
            spinner::MAX_VISIBLE_TASKS + 1
        );
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
}
