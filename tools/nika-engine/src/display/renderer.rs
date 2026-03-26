//! CliRenderer — append-only event stream renderer.
//!
//! Receives Event structs from the runner and prints formatted lines.
//! NO ANSI cursor movement. Every print is a simple println!().

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;

use crate::display::{colors, icons, DetailLevel};
use crate::event::EventKind;

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
    pub structured_attempts: u32,
    pub structured_success_layer: Option<u8>,
    pub root_failure: Option<String>,
    /// Per-task timing: (task_id, verb, start_offset_ms, duration_ms)
    pub task_timeline: Vec<(String, String, u64, u64)>,
    /// Per-provider call: (task_id, in, out, cache, ttft_ms, cost)
    pub provider_calls: Vec<ProviderCallStat>,
}

#[derive(Debug)]
pub struct ProviderCallStat {
    pub task_id: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_tokens: u64,
    pub ttft_ms: Option<u64>,
    pub cost: f64,
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
                self.stats.tasks_passed += 1;
                let dur_secs = *duration_ms as f32 / 1000.0;

                // Look up verb
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

                // Record timeline (failed tasks should appear in Gantt chart too)
                if let Some((start, _)) = self.task_starts.get(task_id.as_ref()) {
                    self.stats.task_timeline.push((
                        task_id.to_string(),
                        verb.clone(),
                        start.saturating_sub(self.workflow_start_ms),
                        *duration_ms,
                    ));
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
            }

            EventKind::TaskSkipped { task_id, reason } => {
                self.stats.tasks_skipped += 1;
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
                    println!("{}", super::format_event::fmt_template_resolved(template, result));
                }
            }

            EventKind::ProviderCalled {
                task_id: _,
                provider,
                model,
                prompt_len,
            } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_provider_called(provider, model, *prompt_len));
                }
            }

            EventKind::ProviderResponded {
                task_id: _,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                ttft_ms,
                finish_reason: _,
                cost_usd,
                ..
            } => {
                // Accumulate stats
                self.stats.total_input_tokens += input_tokens;
                self.stats.total_output_tokens += output_tokens;
                self.stats.total_cache_tokens += cache_read_tokens;
                self.stats.total_cost += cost_usd;
                if let Some(t) = ttft_ms {
                    self.stats.ttft_values.push(*t);
                }
                self.stats.provider_calls.push(ProviderCallStat {
                    task_id: event.kind.task_id().unwrap_or("?").to_string(),
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    cache_tokens: *cache_read_tokens,
                    ttft_ms: *ttft_ms,
                    cost: *cost_usd,
                });

                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_provider_responded(
                        *input_tokens, *output_tokens, *cache_read_tokens, *ttft_ms,
                    ));
                    if self.detail.show_sparklines() {
                        println!("{}", super::format_event::fmt_provider_sparkline(
                            *output_tokens, *input_tokens, *cost_usd,
                        ));
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
                    println!("{}", super::format_event::fmt_context_assembled(
                        sources.len(), *total_tokens, *budget_used_pct,
                    ));
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
                self.stats.mcp_errors += 1;
                println!("{}", super::format_event::fmt_mcp_error(server_name, error));
            }

            EventKind::McpInvoke {
                task_id: _,
                call_id,
                mcp_server,
                tool,
                resource,
                ..
            } => {
                self.stats.mcp_calls += 1;
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_mcp_invoke(
                        mcp_server, tool.as_deref(), resource.as_deref(), call_id,
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
                    println!("{}", super::format_event::fmt_mcp_response(
                        call_id, *output_len, *duration_ms, *cached, *is_error,
                    ));
                }
            }

            EventKind::McpRetry {
                task_id: _,
                server_name: _,
                operation,
                attempt,
                max_attempts,
                error,
            } => {
                self.stats.mcp_retries += 1;
                println!("{}", super::format_event::fmt_mcp_retry(
                    operation, *attempt, *max_attempts, error,
                ));
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
                    println!("{}", super::format_event::fmt_agent_start(*max_turns, mcp_servers));
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
                        if meta.stop_reason == "tool_use" {
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
                    println!("{}", super::format_event::fmt_agent_complete(*turns, stop_reason));
                }
            }

            EventKind::AgentSpawned {
                parent_task_id: _,
                child_task_id,
                depth,
            } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_agent_spawned(child_task_id, *depth));
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
                self.stats.guardrails_passed += 1;
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_guardrail_passed(guardrail_type, description));
                }
            }

            EventKind::GuardrailFailed {
                guardrail_type,
                message,
                ..
            } => {
                self.stats.guardrails_failed += 1;
                println!("{}", super::format_event::fmt_guardrail_failed(guardrail_type, message));
            }

            EventKind::GuardrailEscalation {
                guardrail_type: _,
                severity,
                message,
                ..
            } => {
                self.stats.guardrails_escalations += 1;
                println!("{}", super::format_event::fmt_guardrail_escalation(severity, message));
            }

            // ═══════════════════════════════════════
            // BUILTIN
            // ═══════════════════════════════════════
            EventKind::Log { level, message, .. } => {
                println!("{}", super::format_event::fmt_log(&self.ts(), level, message));
            }

            EventKind::Custom { name, payload, .. } => {
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_custom(&self.ts(), name, payload));
                }
            }

            // ═══════════════════════════════════════
            // ARTIFACTS
            // ═══════════════════════════════════════
            EventKind::ArtifactWritten {
                task_id: _,
                path,
                size,
                format,
                ..
            } => {
                self.stats.artifacts_count += 1;
                self.stats.artifacts_bytes += size;
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_artifact_written(path, *size, format));
                }
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
                    println!("{}", super::format_event::fmt_media_extracted(*block_count, content_types));
                }
            }

            EventKind::MediaStored {
                task_id: _,
                hash,
                path,
                size_bytes,
                verified,
                deduplicated,
                pipeline_ms,
            } => {
                self.stats.media_stored += 1;
                self.stats.media_bytes += size_bytes;
                if *deduplicated {
                    self.stats.media_dedup += 1;
                }

                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_media_stored(*size_bytes, path, hash));
                    if self.detail.show_previews() {
                        println!("{}", super::format_event::fmt_media_stored_detail(
                            *deduplicated, *verified, *pipeline_ms,
                        ));
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
                self.stats.structured_attempts += 1;
                if self.detail.show_sub_events() {
                    println!("{}", super::format_event::fmt_structured_output_attempt(
                        *layer, layer_name, *success, error.as_deref(),
                    ));
                }
            }

            EventKind::StructuredOutputSuccess {
                layer,
                layer_name: _,
                total_attempts: _,
                ..
            } => {
                self.stats.structured_success_layer = Some(*layer);
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
                    println!("{}", super::format_event::fmt_vision_content_resolved(
                        *image_count, *total_bytes, *resolve_ms,
                    ));
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
                    println!("{}", super::format_event::fmt_http_response(
                        *status_code, content_type.as_deref(), *content_length, *elapsed_ms,
                    ));
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
                    println!("{}", super::format_event::fmt_for_each_completed(
                        task_id, *total, *succeeded, *failed,
                    ));
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
                    println!("{}", super::format_event::fmt_exec_completed(*exit_code, *duration_ms));
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
                println!("{}", super::format_event::fmt_policy_blocked(&self.ts(), policy_type, reason));
            }

            // Catch-all for WorkflowCompleted/WorkflowFailed (handled by summary)
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

// ═══════════════════════════════════════
// HELPERS
// ═══════════════════════════════════════

/// Format bytes: 1234 → "1.2 KB", 1234567 → "1.2 MB"
pub(crate) fn format_bytes(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

use super::colors::{floor_char_boundary, stripped_len};

/// Format output preview as a mini box with syntax-highlighted content.
///
/// Returns a Vec of pre-formatted lines (with ANSI colors and box characters)
/// ready for printing. Returns empty Vec if output is null/empty.
pub(crate) fn format_output_preview(output: &Value, term_width: u16) -> Vec<String> {
    let text = match output {
        Value::String(s) => s.clone(),
        _ => serde_json::to_string_pretty(output).unwrap_or_default(),
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
        colors::markdown_preview(&text, 4)
            .into_iter()
            .map(|l| {
                if stripped_len(&l) > max_width {
                    let end = floor_char_boundary(&l, max_width - 1);
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
                if l.len() > max_width {
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
