// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TaskBox rendering for chat conversation inline content.
//!
//! Renders `InlineContent::Task` variants (Invoke, Infer, Exec, Fetch, Agent)
//! as box-styled widgets with Compact / Expanded / Full render modes.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use super::colors::RenderColors;
use crate::utils::truncate_str;
use crate::views::chat::{ChatView, InlineContent};
use crate::widgets::{task_box::RenderMode, TaskBox};

/// Common rendering context for task box functions, reducing argument count.
struct TaskBoxCtx<'a> {
    border_color: Color,
    state_icon: &'a str,
    state_suffix: &'a str,
    colors: &'a RenderColors,
}

impl ChatView {
    /// Render all `InlineContent::Task` variants into `items`.
    ///
    /// Called from `render_messages_v2` after inline MCP/Infer stream boxes.
    pub(in crate::views::chat) fn render_task_boxes<'a>(
        &self,
        colors: &RenderColors,
        items: &mut Vec<ListItem<'a>>,
    ) {
        for content in &self.inline_content {
            if let InlineContent::Task(task_box) = content {
                self.render_single_task_box(task_box, colors, items);
            }
        }
    }

    /// Render a single TaskBox with verb colors and pulse animation.
    fn render_single_task_box<'a>(
        &self,
        task_box: &TaskBox,
        colors: &RenderColors,
        items: &mut Vec<ListItem<'a>>,
    ) {
        let verb_color = task_box.verb_color().rgb();
        // Calculate pulse intensity for running state (0.0-1.0 sine wave)
        let pulse_intensity = if task_box.state().is_running() {
            ((self.frame as f32 / 30.0).sin() + 1.0) / 2.0
        } else {
            0.0
        };
        let border_color = task_box
            .state()
            .border_color_with_pulse(verb_color, pulse_intensity);
        let state_icon = task_box.state().icon();
        let state_suffix_cow = task_box.state().suffix();
        let state_suffix: &str = &state_suffix_cow;

        match task_box {
            TaskBox::Invoke(invoke) => {
                self.render_invoke_box(
                    invoke,
                    border_color,
                    state_icon,
                    state_suffix,
                    colors,
                    items,
                );
            }
            TaskBox::Infer(infer) => {
                let ctx = TaskBoxCtx {
                    border_color,
                    state_icon,
                    state_suffix,
                    colors,
                };
                self.render_infer_box(infer, &ctx, task_box, items);
            }
            TaskBox::Exec(exec) => {
                self.render_exec_box(exec, border_color, state_icon, state_suffix, colors, items);
            }
            TaskBox::Fetch(fetch) => {
                let ctx = TaskBoxCtx {
                    border_color,
                    state_icon,
                    state_suffix,
                    colors,
                };
                self.render_fetch_box(fetch, &ctx, task_box, items);
            }
            TaskBox::Agent(agent) => {
                let ctx = TaskBoxCtx {
                    border_color,
                    state_icon,
                    state_suffix,
                    colors,
                };
                self.render_agent_box(agent, &ctx, task_box, items);
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Invoke
    // ─────────────────────────────────────────────────────────────────────────

    fn render_invoke_box<'a>(
        &self,
        invoke: &crate::widgets::task_box::InvokeBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &RenderColors,
        items: &mut Vec<ListItem<'a>>,
    ) {
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let emerald_400 = Color::Rgb(52, 211, 153);

        match self.task_box_render_mode {
            RenderMode::Compact => {
                let status_str = if invoke.result.is_some() {
                    "✓ result"
                } else if invoke.error.is_some() {
                    "❌ error"
                } else if invoke.state.is_running() {
                    "⏳"
                } else {
                    ""
                };
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ 🔌 INVOKE ──────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        truncate_str(&invoke.tool, 20).into_owned(),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(" @ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        truncate_str(&invoke.server, 12).into_owned(),
                        Style::default().fg(emerald_400),
                    ),
                    Span::styled(
                        format!(" │ {}", status_str),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
            RenderMode::Expanded | RenderMode::Full => {
                // 1. Top border with status
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ 🔌 INVOKE ────────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));

                // 2. Tool + Server line
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        truncate_str(&invoke.tool, 25).into_owned(),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(" @ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        truncate_str(&invoke.server, 18).into_owned(),
                        Style::default().fg(emerald_400),
                    ),
                ])));

                // 3. Separator
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "├──────────────────────────────────────────────────┤",
                    Style::default().fg(border_color),
                )])));

                // 4. INPUT section header
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("📥 INPUT", Style::default().fg(colors.muted_color)),
                ])));

                // 5. Params preview (truncated JSON) — use cache to avoid serde in render loop (H9)
                if !invoke.params.is_null() {
                    let params_str = invoke
                        .params_oneline_cached
                        .as_deref()
                        .unwrap_or("{}")
                        .to_string();
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            truncate_str(&params_str, 44).into_owned(),
                            Style::default().fg(content_color),
                        ),
                    ])));
                } else {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled("(no params)", Style::default().fg(colors.muted_color)),
                    ])));
                }

                // 6. OUTPUT section header
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("📤 OUTPUT", Style::default().fg(colors.muted_color)),
                ])));

                // 7. Result or error preview — use cache to avoid serde in render loop (H9)
                if invoke.result.is_some() {
                    let result_str = invoke
                        .result_oneline_cached
                        .as_deref()
                        .unwrap_or("null")
                        .to_string();
                    let result_lines: Vec<&str> = result_str.lines().take(2).collect();
                    if result_lines.is_empty() {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                            Span::styled(
                                truncate_str(&result_str, 44).into_owned(),
                                Style::default().fg(colors.success_color),
                            ),
                        ])));
                    } else {
                        for line in result_lines {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(border_color)),
                                Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                                Span::styled(
                                    truncate_str(line, 44).into_owned(),
                                    Style::default().fg(colors.success_color),
                                ),
                            ])));
                        }
                    }
                } else if let Some(ref err) = invoke.error {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled("❌ ", Style::default().fg(colors.error_color)),
                        Span::styled(
                            truncate_str(err, 40).into_owned(),
                            Style::default().fg(colors.error_color),
                        ),
                    ])));
                } else if invoke.state.is_running() {
                    // Streaming cursor for running invoke
                    let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            "(calling MCP server)",
                            Style::default().fg(colors.muted_color),
                        ),
                        Span::styled(cursor, Style::default().fg(colors.success_color)),
                    ])));
                }

                // 8. Bottom border
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new("")); // spacing
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Infer
    // ─────────────────────────────────────────────────────────────────────────

    fn render_infer_box<'a>(
        &self,
        infer: &crate::widgets::task_box::InferBox,
        ctx: &TaskBoxCtx<'_>,
        _task_box: &TaskBox,
        items: &mut Vec<ListItem<'a>>,
    ) {
        let (border_color, state_icon, state_suffix, colors) = (
            ctx.border_color,
            ctx.state_icon,
            ctx.state_suffix,
            ctx.colors,
        );
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let provider_icon = if infer.model.contains("claude") {
            "🧠"
        } else if infer.model.contains("gpt") {
            "🤖"
        } else if infer.model.contains("mistral") {
            "🌀"
        } else if infer.model.contains("llama") {
            "🦙"
        } else {
            "💬"
        };

        match self.task_box_render_mode {
            RenderMode::Compact => {
                let prompt_flat = infer.prompt.replace('\n', " ");
                let prompt_preview = truncate_str(&prompt_flat, 28);
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ ⚡ INFER ──────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!(
                            "\"{}\" │ {} in │ {} out",
                            prompt_preview, infer.tokens_in, infer.tokens_out
                        ),
                        Style::default().fg(content_color),
                    ),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
            RenderMode::Expanded | RenderMode::Full => {
                // 1. Top border
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ ⚡ INFER ────────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));

                // 2. Model line
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!(
                            "model: {} {}",
                            provider_icon,
                            truncate_str(&infer.model, 35)
                        ),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 3. Separator
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "├──────────────────────────────────────────────────┤",
                    Style::default().fg(border_color),
                )])));

                // 4. PROMPT section
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("PROMPT", Style::default().fg(colors.muted_color)),
                ])));
                let prompt_display = infer.prompt.replace('\n', " ");
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!("┊ {}", truncate_str(&prompt_display, 45)),
                        Style::default().fg(content_color),
                    ),
                ])));

                // 5. Separator
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "├──────────────────────────────────────────────────┤",
                    Style::default().fg(border_color),
                )])));

                // 6. RESPONSE section
                let response_text = if infer.state.is_running() && self.is_streaming {
                    &self.partial_response
                } else {
                    &infer.response
                };
                let response_indicator = if infer.state.is_running() && self.is_streaming {
                    "▼ streaming...".to_string()
                } else if !response_text.is_empty() {
                    format!("▼ {} chars", response_text.len())
                } else {
                    String::new()
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("RESPONSE", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        format!("                              {}", response_indicator),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 7. Response content
                if response_text.is_empty() && infer.state.is_running() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ...", Style::default().fg(content_color)),
                    ])));
                } else {
                    let response_lines: Vec<&str> = response_text.lines().collect();
                    let start = response_lines.len().saturating_sub(3);
                    let lines_to_show: Vec<_> = response_lines.iter().skip(start).collect();
                    for (idx, line) in lines_to_show.iter().enumerate() {
                        let is_last = idx == lines_to_show.len() - 1;
                        let cursor = if is_last && infer.state.is_running() && self.is_streaming {
                            "█"
                        } else {
                            ""
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled(
                                format!("┊ {}{}", truncate_str(line, 44), cursor),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                }

                // 8. Footer with metrics
                let cost = crate::widgets::task_box::InferBox::calculate_cost(
                    infer.tokens_in,
                    infer.tokens_out,
                    &infer.model,
                );
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!(
                            "📊 {} in │ {} out │ {} {} │ 💰 ${:.4}",
                            infer.tokens_in,
                            infer.tokens_out,
                            provider_icon,
                            truncate_str(&infer.model, 12),
                            cost
                        ),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 9. Bottom border
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
        }
        items.push(ListItem::new("")); // spacing
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Exec
    // ─────────────────────────────────────────────────────────────────────────

    fn render_exec_box<'a>(
        &self,
        exec: &crate::widgets::task_box::ExecBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &RenderColors,
        items: &mut Vec<ListItem<'a>>,
    ) {
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let exit_display = exec
            .exit_code
            .map(|c| format!("exit: {}", c))
            .unwrap_or_else(|| "running...".to_string());
        let exit_color = exec
            .exit_code
            .map(|c| {
                if c == 0 {
                    colors.success_color
                } else {
                    colors.error_color
                }
            })
            .unwrap_or(colors.muted_color);

        match self.task_box_render_mode {
            RenderMode::Compact => {
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ 📟 EXEC ──────────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!("$ {} │ ", truncate_str(&exec.command, 30)),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(exit_display.clone(), Style::default().fg(exit_color)),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
            RenderMode::Expanded | RenderMode::Full => {
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ 📟 EXEC ─────────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!("$ {}", truncate_str(&exec.command, 45)),
                        Style::default().fg(content_color),
                    ),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "├──────────────────────────────────────────────────┤",
                    Style::default().fg(border_color),
                )])));
                let output_indicator = if !exec.stdout.is_empty() {
                    format!("▼ {} lines", exec.stdout.lines().count())
                } else {
                    String::new()
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("OUTPUT", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        format!("                                  {}", output_indicator),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));
                if exec.stdout.is_empty() && exec.state.is_running() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ...", Style::default().fg(content_color)),
                    ])));
                } else if exec.stdout.is_empty() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ (no output)", Style::default().fg(colors.muted_color)),
                    ])));
                } else {
                    let output_lines: Vec<&str> = exec.stdout.lines().collect();
                    let start = output_lines.len().saturating_sub(3);
                    for line in output_lines.iter().skip(start) {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled(
                                format!("┊ {}", truncate_str(line, 44)),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                }
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(exit_display, Style::default().fg(exit_color)),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
        }
        items.push(ListItem::new("")); // spacing
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Fetch
    // ─────────────────────────────────────────────────────────────────────────

    fn render_fetch_box<'a>(
        &self,
        fetch: &crate::widgets::task_box::FetchBox,
        ctx: &TaskBoxCtx<'_>,
        task_box: &TaskBox,
        items: &mut Vec<ListItem<'a>>,
    ) {
        let (border_color, state_icon, state_suffix, colors) = (
            ctx.border_color,
            ctx.state_icon,
            ctx.state_suffix,
            ctx.colors,
        );
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let amber_400 = Color::Rgb(251, 191, 36);

        match self.task_box_render_mode {
            RenderMode::Compact => {
                let status_str = fetch
                    .status_code
                    .map(|s| format!("{}", s))
                    .unwrap_or_else(|| "---".to_string());
                let status_color = fetch
                    .status_code
                    .map(|s| {
                        if (200..300).contains(&s) {
                            colors.success_color
                        } else {
                            colors.error_color
                        }
                    })
                    .unwrap_or(colors.muted_color);

                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ 🛰️ FETCH ────────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(format!("{} ", fetch.method), Style::default().fg(amber_400)),
                    Span::styled(
                        truncate_str(&fetch.url, 28).into_owned(),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(" │ ", Style::default().fg(colors.muted_color)),
                    Span::styled(status_str, Style::default().fg(status_color)),
                    Span::styled(
                        format!(" │ {}ms", fetch.ttfb_ms),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
            RenderMode::Expanded | RenderMode::Full => {
                // 1. Top border with status
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ 🛰️ FETCH ─────────────────────────────── {} {} ─╮",
                        state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));

                // 2. Method + URL line
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(format!("{} ", fetch.method), Style::default().fg(amber_400)),
                    Span::styled(
                        truncate_str(&fetch.url, 42).into_owned(),
                        Style::default().fg(content_color),
                    ),
                ])));

                // 3. Separator
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "├──────────────────────────────────────────────────┤",
                    Style::default().fg(border_color),
                )])));

                // 4. RESPONSE section header
                let size_str = if fetch.response_size > 0 {
                    format!(" ({:.1}KB)", fetch.response_size as f64 / 1024.0)
                } else {
                    String::new()
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!("💬 RESPONSE{}", size_str),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 5. Response body preview (first 2 lines or status)
                if let Some(body) = &fetch.response_body {
                    let preview_lines: Vec<&str> = body.lines().take(2).collect();
                    for line in preview_lines {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                            Span::styled(
                                truncate_str(line, 44).into_owned(),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                } else if task_box.state().is_running() {
                    // Streaming cursor for running fetch
                    let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            "(awaiting response)",
                            Style::default().fg(colors.muted_color),
                        ),
                        Span::styled(cursor, Style::default().fg(colors.success_color)),
                    ])));
                }

                // 6. Footer with metrics
                let status_str = fetch
                    .status_code
                    .map(|s| format!("{}", s))
                    .unwrap_or_else(|| "---".to_string());
                let status_color = fetch
                    .status_code
                    .map(|s| {
                        if (200..300).contains(&s) {
                            colors.success_color
                        } else {
                            colors.error_color
                        }
                    })
                    .unwrap_or(colors.muted_color);

                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("Status: ", Style::default().fg(colors.muted_color)),
                    Span::styled(status_str, Style::default().fg(status_color)),
                    Span::styled(
                        format!(" │ TTFB: {}ms", fetch.ttfb_ms),
                        Style::default().fg(colors.muted_color),
                    ),
                    Span::styled(
                        format!(" │ Retries: {}", fetch.retries),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 7. Bottom border
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new("")); // spacing
            }
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Agent
    // ─────────────────────────────────────────────────────────────────────────

    fn render_agent_box<'a>(
        &self,
        agent: &crate::widgets::task_box::AgentBox,
        ctx: &TaskBoxCtx<'_>,
        task_box: &TaskBox,
        items: &mut Vec<ListItem<'a>>,
    ) {
        let (border_color, state_icon, state_suffix, colors) = (
            ctx.border_color,
            ctx.state_icon,
            ctx.state_suffix,
            ctx.colors,
        );
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let amber_400 = Color::Rgb(251, 191, 36);
        let agent_icon = if agent.is_subagent { "🐤" } else { "🐔" };

        match self.task_box_render_mode {
            RenderMode::Compact => {
                let total_tokens = agent.tokens_in + agent.tokens_out;
                let token_str = if total_tokens > 1000 {
                    format!("{:.1}K", total_tokens as f64 / 1000.0)
                } else {
                    format!("{}", total_tokens)
                };

                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ {} AGENT ────────────────────────────── {} {} ─╮",
                        agent_icon, state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("🎯 ", Style::default().fg(amber_400)),
                    Span::styled(
                        truncate_str(&agent.prompt, 25).into_owned(),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(
                        format!(" │ T:{}/{} │ {}", agent.turn, agent.max_turns, token_str),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
            }
            RenderMode::Expanded | RenderMode::Full => {
                // 1. Top border with status
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    format!(
                        "╭─ {} AGENT ─────────────────────────────── {} {} ─╮",
                        agent_icon, state_icon, state_suffix
                    ),
                    Style::default().fg(border_color),
                )])));

                // 2. Goal/prompt line
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("🎯 ", Style::default().fg(amber_400)),
                    Span::styled(
                        truncate_str(&agent.prompt, 45).into_owned(),
                        Style::default().fg(content_color),
                    ),
                ])));

                // 3. Separator
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "├──────────────────────────────────────────────────┤",
                    Style::default().fg(border_color),
                )])));

                // 4. RESPONSE section header
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("💬 RESPONSE", Style::default().fg(colors.muted_color)),
                ])));

                // 5. Final response content (first 3 lines or streaming indicator)
                if let Some(response) = &agent.final_response {
                    let response_lines: Vec<&str> = response.lines().take(3).collect();
                    for line in response_lines {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                            Span::styled(
                                truncate_str(line, 44).into_owned(),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                    if response.lines().count() > 3 {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                            Span::styled(
                                format!("... ({} more lines)", response.lines().count() - 3),
                                Style::default().fg(colors.muted_color),
                            ),
                        ])));
                    }
                } else if task_box.state().is_running() {
                    // Streaming cursor for running agent
                    let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            format!("(turn {}/{} in progress)", agent.turn, agent.max_turns),
                            Style::default().fg(colors.muted_color),
                        ),
                        Span::styled(cursor, Style::default().fg(colors.success_color)),
                    ])));
                }

                // 6. Footer with turn count, tokens, cost, tool calls
                let total_tokens = agent.tokens_in + agent.tokens_out;
                let token_str = if total_tokens > 1000 {
                    format!("{:.1}K", total_tokens as f64 / 1000.0)
                } else {
                    format!("{}", total_tokens)
                };
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled(
                        format!("Turn {}/{}", agent.turn, agent.max_turns),
                        Style::default().fg(colors.muted_color),
                    ),
                    Span::styled(" │ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        format!("🎫 {} tokens", token_str),
                        Style::default().fg(colors.muted_color),
                    ),
                    Span::styled(" │ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        format!("💰 ${:.2}", agent.cost),
                        Style::default().fg(colors.muted_color),
                    ),
                    Span::styled(" │ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        format!("🔌 {} calls", agent.tool_calls),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 7. Bottom border
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰──────────────────────────────────────────────────╯",
                    Style::default().fg(border_color),
                )])));
                items.push(ListItem::new("")); // spacing
            }
        }
    }
}
