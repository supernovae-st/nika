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
    pub task_count: usize,
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
    stats: RunStats,
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
            if let Ok(json) = serde_json::to_string(event) {
                println!("{}", json);
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
                        start - self.workflow_start_ms,
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
                    println!(
                        "{}     {} {} {} → {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "tmpl".dimmed(),
                        template.dimmed(),
                        result.dimmed()
                    );
                }
            }

            EventKind::ProviderCalled {
                task_id: _,
                provider,
                model,
                prompt_len,
            } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {}/{} {} {} chars",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::provider(),
                        provider.dimmed(),
                        model.white(),
                        "· prompt:".dimmed(),
                        prompt_len
                    );
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
                    let ttft_str = ttft_ms
                        .map(|t| format!(" · ttft:{}", colors::ttft(t)))
                        .unwrap_or_default();
                    println!(
                        "{}     {} {} {} in:{} out:{} cache:{}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::provider(),
                        "←".dimmed(),
                        colors::tokens(*input_tokens).as_str().dimmed(),
                        colors::tokens(*output_tokens).as_str().white(),
                        colors::tokens(*cache_read_tokens).as_str().dimmed(),
                        ttft_str
                    );

                    if self.detail.show_sparklines() {
                        let max_tok = (*input_tokens).max(*output_tokens);
                        println!(
                            "{}     {}    tok {} cost {}",
                            " ".repeat(6),
                            "│".dimmed(),
                            colors::sparkline(*output_tokens, max_tok),
                            colors::cost(*cost_usd)
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
                    let warn = if *budget_used_pct > 90.0 {
                        " ⚠".red().to_string()
                    } else {
                        String::new()
                    };
                    println!(
                        "{}     {} {} {} src · {} tok · {}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "ctx".dimmed(),
                        sources.len(),
                        colors::tokens(*total_tokens),
                        colors::budget_bar(*budget_used_pct, 25),
                        warn
                    );
                }
            }

            // ═══════════════════════════════════════
            // MCP
            // ═══════════════════════════════════════
            EventKind::McpConnected { server_name } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} connected {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        server_name.green()
                    );
                }
            }

            EventKind::McpError { server_name, error } => {
                self.stats.mcp_errors += 1;
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::mcp(),
                    format!("{} ✗", server_name).red(),
                    error.red()
                );
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
                    let target = tool.as_deref().or(resource.as_deref()).unwrap_or("?");
                    println!(
                        "{}     {} {} {} → {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        mcp_server.dimmed(),
                        target.white(),
                        format!("call:{}", call_id).dimmed()
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
                    let suffix = format!("{}{}", cache_tag, err_tag);
                    println!(
                        "{}     {} {} {} {} · {}{}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::mcp(),
                        format!("call:{}", call_id).dimmed(),
                        "←".dimmed(),
                        format_bytes(*output_len as u64),
                        format!(" · {}ms", duration_ms).dimmed(),
                        suffix
                    );
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
                println!(
                    "{}     {} {} {} {}/{} · {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::retry(),
                    format!("retry {}", operation).yellow(),
                    attempt.to_string().yellow(),
                    max_attempts,
                    error.dimmed()
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
                    let servers = mcp_servers.join(", ");
                    println!(
                        "{}     {} {} {} max_turns:{} · mcp:[{}]",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        "agent".dimmed(),
                        max_turns,
                        servers.green()
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
                    println!(
                        "{}     {} {} turn {}/…  {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        (turn_index + 1).to_string().white(),
                        kind.dimmed()
                    );

                    // If metadata available, show tool_use or end_turn
                    if let Some(meta) = metadata {
                        if meta.stop_reason == "tool_use" {
                            println!(
                                "{}     {} {} tool_use",
                                " ".repeat(6),
                                "│".dimmed(),
                                "↳".dimmed()
                            );
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
                        "{}     {} {} {} {} turns · {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::agent_meta(),
                        "done".green(),
                        turns,
                        stop_reason.dimmed()
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
                        "{}     {} {} spawned {} depth:{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        "⤋".magenta(),
                        child_task_id.white(),
                        depth
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
                self.stats.guardrails_passed += 1;
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::guardrail(),
                        icons::success(),
                        format!("{} · {}", guardrail_type, description).dimmed()
                    );
                }
            }

            EventKind::GuardrailFailed {
                guardrail_type,
                message,
                ..
            } => {
                self.stats.guardrails_failed += 1;
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::guardrail(),
                    icons::failed(),
                    format!("{} · {}", guardrail_type, message).red()
                );
            }

            EventKind::GuardrailEscalation {
                guardrail_type: _,
                severity,
                message,
                ..
            } => {
                self.stats.guardrails_escalations += 1;
                println!(
                    "{}     {}   {} {} · {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::retry(),
                    format!("escalation · {}", severity).yellow(),
                    message.dimmed()
                );
            }

            // ═══════════════════════════════════════
            // BUILTIN
            // ═══════════════════════════════════════
            EventKind::Log { level, message, .. } => {
                let level_colored = match level.as_str() {
                    "error" => level.red(),
                    "warn" => level.yellow(),
                    "info" => level.green(),
                    "debug" => level.dimmed(),
                    "trace" => level.dimmed(),
                    _ => level.normal(),
                };
                println!(
                    "{}  {} {} · {}",
                    self.ts(),
                    icons::log(),
                    level_colored,
                    message
                );
            }

            EventKind::Custom { name, payload, .. } => {
                if self.detail.show_sub_events() {
                    let preview = serde_json::to_string(payload).unwrap_or_default();
                    let short = if preview.len() > 60 {
                        format!("{}…", &preview[..floor_char_boundary(&preview, 60)])
                    } else {
                        preview
                    };
                    println!(
                        "{}  {} {} · {}",
                        self.ts(),
                        icons::log(),
                        name.cyan(),
                        short.dimmed()
                    );
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
                    println!(
                        "{}     {} {} {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::artifact(),
                        format!("→ {}", path).cyan(),
                        format!("{} · {}", format_bytes(*size), format).dimmed()
                    );
                }
            }

            EventKind::ArtifactFailed { path, reason, .. } => {
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::artifact(),
                    format!("✗ {}", path).red(),
                    reason.dimmed()
                );
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
                        "{}     {} {} {} blocks · types: [{}]",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::media(),
                        block_count,
                        content_types.join(", ").magenta()
                    );
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
                    let short_hash = if hash.len() > 16 { &hash[..16] } else { hash };
                    println!(
                        "{}     {} {} {} · {} · {}…",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::media(),
                        format_bytes(*size_bytes),
                        path.dimmed(),
                        short_hash.dimmed()
                    );
                    if self.detail.show_previews() {
                        let dedup = if *deduplicated {
                            "yes".yellow()
                        } else {
                            "no".dimmed()
                        };
                        let verif = if *verified { "yes".green() } else { "no".red() };
                        println!(
                            "{}     {}   dedup:{} · verified:{} · pipeline:{}ms",
                            " ".repeat(6),
                            "│".dimmed(),
                            dedup,
                            verif,
                            pipeline_ms
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
                println!(
                    "{}     {} {} {} {}",
                    " ".repeat(6),
                    "│".dimmed(),
                    icons::media(),
                    "✗".red(),
                    reason.red()
                );
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
                    let status = if *success {
                        icons::success()
                    } else {
                        icons::failed()
                    };
                    let err_msg = error
                        .as_deref()
                        .map(|e| format!(" {}", e.dimmed()))
                        .unwrap_or_default();
                    println!(
                        "{}     {} {} L{}: {} {}{}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::structured(),
                        layer,
                        layer_name,
                        status,
                        err_msg
                    );
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
                    println!(
                        "{}     {} {} {} images · {} · resolved {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::vision(),
                        image_count,
                        format_bytes(*total_bytes),
                        resolve_ms
                    );
                }
            }

            // ═══════════════════════════════════════
            // HTTP
            // ═══════════════════════════════════════
            EventKind::HttpRequest { method, url, .. } => {
                if self.detail.show_sub_events() {
                    println!(
                        "{}     {} {} → {} {}",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::http(),
                        method.cyan(),
                        url.underline()
                    );
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
                    println!(
                        "{}     {} {} ← {} · {} · {} · {}ms",
                        " ".repeat(6),
                        "│".dimmed(),
                        icons::http(),
                        status_colored,
                        ct.dimmed(),
                        cl,
                        elapsed_ms
                    );
                }
            }

            // Catch-all for WorkflowCompleted/WorkflowFailed (handled by summary)
            _ => {}
        }
    }

    /// Render the output preview box with syntax highlighting.
    fn render_output_preview(&self, output: &Value) {
        let text = match output {
            Value::String(s) => s.clone(),
            _ => serde_json::to_string_pretty(output).unwrap_or_default(),
        };

        if text.is_empty() || text == "null" {
            return;
        }

        let is_json = text.starts_with('{') || text.starts_with('[');
        let is_markdown = text.starts_with('#') || text.contains("\n## ");

        let max_width = (self.term_width as usize).min(72).saturating_sub(16);
        let dashes = "╌".repeat(max_width);
        let size_label = format!("{} ch", text.chars().count());
        let padding = max_width.saturating_sub(size_label.len() + 1);

        println!(
            "{}     {} {}{}{}",
            " ".repeat(6),
            "│".dimmed(),
            "╭╌".dimmed(),
            dashes.dimmed(),
            "╮".dimmed()
        );

        let preview_lines: Vec<String> = if is_json {
            // Take first line of compact JSON, syntax-highlighted
            vec![colors::json_preview(&text.replace('\n', " "), max_width)]
        } else if is_markdown {
            colors::markdown_preview(&text, 4)
                .into_iter()
                .map(|l| {
                    if stripped_len(&l) > max_width {
                        let end = floor_char_boundary(&l, max_width - 1);
                        format!("{}…", &l[..end])
                    } else {
                        l
                    }
                })
                .collect()
        } else {
            // Plain text
            text.lines()
                .take(2)
                .map(|l| {
                    if l.len() > max_width {
                        let end = floor_char_boundary(l, max_width - 1);
                        format!("{}…", &l[..end])
                    } else {
                        l.to_string()
                    }
                })
                .collect()
        };

        for line in &preview_lines {
            let pad = max_width.saturating_sub(stripped_len(line));
            println!(
                "{}     {} {} {}{} {}",
                " ".repeat(6),
                "│".dimmed(),
                "│".dimmed(),
                line,
                " ".repeat(pad),
                "│".dimmed()
            );
        }

        println!(
            "{}     {} {}{} {} {}",
            " ".repeat(6),
            "│".dimmed(),
            "╰╌".dimmed(),
            "╌".repeat(padding).dimmed(),
            size_label.dimmed(),
            "╌╯".dimmed()
        );
    }

    pub fn render_quiet_summary(&self, total_duration_ms: u64) {
        let dur_secs = total_duration_ms as f32 / 1000.0;
        let total = self.stats.tasks_passed + self.stats.tasks_failed + self.stats.tasks_skipped;
        let status = if self.stats.tasks_failed > 0 {
            icons::failed()
        } else {
            icons::success()
        };
        let cost_str = if self.stats.total_cost > 0.0 {
            format!(" · {}", colors::cost(self.stats.total_cost))
        } else {
            String::new()
        };
        println!(
            "{} {} · {}/{}{}",
            status,
            colors::duration(dur_secs),
            self.stats.tasks_passed,
            total,
            cost_str
        );
    }

    /// Render the full summary footer.
    pub fn render_summary(&self, total_duration_ms: u64, trace_path: Option<&str>) {
        if self.detail.is_json() {
            return;
        }

        if self.detail == DetailLevel::Min {
            self.render_quiet_summary(total_duration_ms);
            return;
        }

        let dur_secs = total_duration_ms as f32 / 1000.0;

        println!();

        // ── Summary box ──
        let w = (self.term_width as usize).min(72);
        let border = "─".repeat(w);
        println!("╭{}╮", border.dimmed());
        println!("│{}│", " ".repeat(w));

        // Done/Failed line
        if self.stats.tasks_failed > 0 {
            let root_cause = self.stats.root_failure.as_deref().unwrap_or("unknown");
            let failed_line = format!(
                "  {}  F A I L E D                                            {}",
                icons::failed(),
                colors::duration(dur_secs)
            );
            println!("│{}│", pad_right(&failed_line, w));
            println!(
                "│{}│",
                pad_right(&format!("  root cause: {}", root_cause.red()), w)
            );
        } else {
            let done = format!(
                "  {}  D O N E                                              {}",
                icons::success(),
                colors::duration(dur_secs)
            );
            println!("│{}│", pad_right(&done, w));
        }
        println!("│{}│", " ".repeat(w));

        // Tasks
        let passed = self.stats.tasks_passed.to_string().green();
        let total = (self.stats.tasks_passed + self.stats.tasks_failed + self.stats.tasks_skipped)
            .to_string();
        println!(
            "│{}│",
            pad_right(&format!("  Tasks    {}/{} passed", passed, total), w)
        );
        println!("│{}│", " ".repeat(w));

        // ── Tokens ──
        if self.stats.total_input_tokens > 0 && self.detail.show_full_summary() {
            println!(
                "│{}│",
                pad_right(
                    &format!("  {} Tokens {}", "──".dimmed(), "─".repeat(w - 16).dimmed()),
                    w
                )
            );
            let max_tok = self
                .stats
                .total_input_tokens
                .max(self.stats.total_output_tokens)
                .max(self.stats.total_cache_tokens);
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "    in {} {}",
                        token_bar(self.stats.total_input_tokens, max_tok, 30, "blue"),
                        colors::tokens(self.stats.total_input_tokens)
                    ),
                    w
                )
            );
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "   out {} {}",
                        token_bar(self.stats.total_output_tokens, max_tok, 30, "magenta"),
                        colors::tokens(self.stats.total_output_tokens)
                    ),
                    w
                )
            );
            if self.stats.total_cache_tokens > 0 {
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "    $↻ {} {} saved",
                            token_bar(self.stats.total_cache_tokens, max_tok, 30, "green"),
                            colors::tokens(self.stats.total_cache_tokens)
                        ),
                        w
                    )
                );
            }
            println!("│{}│", " ".repeat(w));
        }

        // ── Cost ──
        if self.stats.total_cost > 0.0 && self.detail.show_full_summary() {
            println!(
                "│{}│",
                pad_right(
                    &format!("  {} Cost {}", "──".dimmed(), "─".repeat(w - 14).dimmed()),
                    w
                )
            );
            // Per-task cost breakdown using ▪ blocks
            // Group provider_calls by task_id
            let mut task_costs: Vec<(String, f64)> = Vec::new();
            for call in &self.stats.provider_calls {
                if let Some(existing) = task_costs.iter_mut().find(|(t, _)| *t == call.task_id) {
                    existing.1 += call.cost;
                } else {
                    task_costs.push((call.task_id.clone(), call.cost));
                }
            }
            let mut cost_parts = Vec::new();
            for (task, c) in &task_costs {
                let blocks = ((c / self.stats.total_cost) * 20.0).round() as usize;
                cost_parts.push(format!("{} {}", task.dimmed(), "▪".repeat(blocks.max(1))));
            }
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {} {}",
                        colors::cost(self.stats.total_cost),
                        cost_parts.join("  ")
                    ),
                    w
                )
            );
            println!("│{}│", " ".repeat(w));
        }

        // ── Performance ──
        if !self.stats.ttft_values.is_empty() && self.detail.show_full_summary() {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {} Performance {}",
                        "──".dimmed(),
                        "─".repeat(w - 20).dimmed()
                    ),
                    w
                )
            );
            let avg_ttft =
                self.stats.ttft_values.iter().sum::<u64>() / self.stats.ttft_values.len() as u64;
            let min_ttft = self.stats.ttft_values.iter().min().copied().unwrap_or(0);
            let max_ttft = self.stats.ttft_values.iter().max().copied().unwrap_or(0);
            let throughput = if dur_secs > 0.0 {
                (self.stats.total_output_tokens as f32 / dur_secs).round() as u64
            } else {
                0
            };

            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  TTFT     avg {} · min {} · max {}",
                        colors::ttft(avg_ttft),
                        colors::ttft(min_ttft),
                        colors::ttft(max_ttft)
                    ),
                    w
                )
            );
            println!(
                "│{}│",
                pad_right(&format!("  Throughput  {} tok/s", throughput), w)
            );
            println!("│{}│", " ".repeat(w));
        }

        // ── Infrastructure ──
        if self.detail.show_full_summary() {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {} Infrastructure {}",
                        "──".dimmed(),
                        "─".repeat(w - 24).dimmed()
                    ),
                    w
                )
            );
            if self.stats.mcp_calls > 0 {
                let errors_str = if self.stats.mcp_errors > 0 {
                    self.stats.mcp_errors.to_string().red().to_string()
                } else {
                    self.stats.mcp_errors.to_string().green().to_string()
                };
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "  MCP      {} calls · {} retries · {} errors",
                            self.stats.mcp_calls,
                            self.stats.mcp_retries.to_string().yellow(),
                            errors_str
                        ),
                        w
                    )
                );
            }
            if self.stats.media_stored > 0 {
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "  Media    {} stored · {} · {} dedup · ✓ integrity",
                            self.stats.media_stored,
                            format_bytes(self.stats.media_bytes),
                            self.stats.media_dedup
                        ),
                        w
                    )
                );
            }
            if self.stats.artifacts_count > 0 {
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "  Output   {} artifacts · {} total",
                            self.stats.artifacts_count,
                            format_bytes(self.stats.artifacts_bytes)
                        ),
                        w
                    )
                );
            }
            if self.stats.guardrails_passed + self.stats.guardrails_failed > 0 {
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "  Guards   {} passed · {} failed · {} escalations",
                            self.stats.guardrails_passed.to_string().green(),
                            self.stats.guardrails_failed.to_string().yellow(),
                            self.stats.guardrails_escalations
                        ),
                        w
                    )
                );
            }
            println!("│{}│", " ".repeat(w));
        }

        // ── Timeline (Gantt) ──
        if !self.stats.task_timeline.is_empty() && self.detail.show_full_summary() {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {} Timeline {}",
                        "──".dimmed(),
                        "─".repeat(w - 18).dimmed()
                    ),
                    w
                )
            );

            let total_ms = total_duration_ms;
            let bar_width = 38;

            for (task_id, verb, start_ms, dur_ms) in &self.stats.task_timeline {
                let start_pct = *start_ms as f64 / total_ms as f64;
                let dur_pct = *dur_ms as f64 / total_ms as f64;
                let start_col = (start_pct * bar_width as f64).round() as usize;
                let dur_col = (dur_pct * bar_width as f64).round().max(1.0) as usize;
                let end_col = (start_col + dur_col).min(bar_width);

                let mut bar = String::new();
                for i in 0..bar_width {
                    if i >= start_col && i < end_col {
                        bar.push('█');
                    } else {
                        bar.push('░');
                    }
                }
                // Color the bar based on verb
                let colored_bar = match verb.as_str() {
                    "infer" => bar.magenta().to_string(),
                    "exec" => bar.yellow().to_string(),
                    "fetch" => bar.cyan().to_string(),
                    "invoke" => bar.green().to_string(),
                    "agent" => bar.red().to_string(),
                    _ => bar,
                };
                let dur_secs = *dur_ms as f32 / 1000.0;
                let padded_id = format!("{:<12}", task_id);
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "  {} {} {} {:>5}",
                            icons::verb(verb),
                            padded_id.dimmed(),
                            colored_bar,
                            colors::duration(dur_secs)
                        ),
                        w
                    )
                );
            }

            // Time axis
            let axis = format!(
                "  {:12} 0s{:>12}{:>12} {:.1}s",
                "",
                "",
                "",
                total_ms as f64 / 1000.0
            );
            println!("│{}│", pad_right(&axis.dimmed().to_string(), w));
            println!("│{}│", " ".repeat(w));
        }

        // ── Provider Breakdown Table ──
        if !self.stats.provider_calls.is_empty() && self.detail.show_full_summary() {
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {} Provider Breakdown {}",
                        "──".dimmed(),
                        "─".repeat(w - 27).dimmed()
                    ),
                    w
                )
            );
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  {}   {}      {}    {}   {}    {}",
                        "#".dimmed(),
                        "Task".dimmed(),
                        "In".dimmed(),
                        "Out".dimmed(),
                        "Cache".dimmed(),
                        "Cost".dimmed()
                    ),
                    w
                )
            );
            for (i, call) in self.stats.provider_calls.iter().enumerate() {
                let _ttft_str = call
                    .ttft_ms
                    .map(|t| format!("{}ms", t))
                    .unwrap_or_else(|| "—".to_string());
                let padded_task = format!("{:<12}", call.task_id);
                println!(
                    "│{}│",
                    pad_right(
                        &format!(
                            "  {}   {} {:>5}  {:>5}  {:>5}   {}",
                            i + 1,
                            padded_task,
                            colors::tokens(call.input_tokens),
                            colors::tokens(call.output_tokens),
                            colors::tokens(call.cache_tokens),
                            colors::cost(call.cost)
                        ),
                        w
                    )
                );
            }
            // Totals row
            println!(
                "│{}│",
                pad_right(&format!("  {}", "─".repeat(w - 4).dimmed()), w)
            );
            println!(
                "│{}│",
                pad_right(
                    &format!(
                        "  Σ   {:12} {:>5}  {:>5}  {:>5}   {}",
                        "",
                        colors::tokens(self.stats.total_input_tokens),
                        colors::tokens(self.stats.total_output_tokens),
                        colors::tokens(self.stats.total_cache_tokens),
                        colors::cost(self.stats.total_cost)
                    ),
                    w
                )
            );
            println!("│{}│", " ".repeat(w));
        }

        // Trace path
        if let Some(path) = trace_path {
            println!("│{}│", pad_right(&format!("  trace {}", path.dimmed()), w));
        }

        println!("│{}│", " ".repeat(w));
        println!("╰{}╯", border.dimmed());
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

/// Pad a string to width, accounting for ANSI escape codes.
fn pad_right(s: &str, width: usize) -> String {
    let visible = stripped_len(s);
    if visible >= width {
        s.to_string()
    } else {
        format!("{}{}", s, " ".repeat(width - visible))
    }
}

/// Generate a token bar: █ for filled, ░ for empty.
fn token_bar(value: u64, max: u64, width: usize, color: &str) -> String {
    let ratio = if max == 0 {
        0.0
    } else {
        value as f64 / max as f64
    };
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!("{}{}", "█".repeat(filled), "░".repeat(empty));
    match color {
        "blue" => bar.blue().to_string(),
        "magenta" => bar.magenta().to_string(),
        "green" => bar.green().to_string(),
        _ => bar,
    }
}
