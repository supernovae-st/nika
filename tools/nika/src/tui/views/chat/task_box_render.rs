//! TaskBox Variant Rendering
//!
//! Renders the 5 TaskBox variants (Invoke, Infer, Exec, Fetch, Agent)
//! with Compact/Expanded/Full render modes and pulse animation.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use super::ChatView;
use crate::tui::utils::truncate_str;
use crate::tui::widgets::task_box::{RenderMode, TaskBox};

/// Colors shared across all TaskBox variants.
pub(super) struct TaskBoxColors {
    pub muted_color: Color,
    pub success_color: Color,
    pub error_color: Color,
}

impl ChatView {
    /// Render a single TaskBox into the items list.
    ///
    /// Handles Invoke, Infer, Exec, Fetch, and Agent variants
    /// with Compact/Expanded/Full render modes.
    pub(super) fn render_task_box_items(
        &self,
        items: &mut Vec<ListItem>,
        task_box: &TaskBox,
        colors: &TaskBoxColors,
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
        let state_suffix = task_box.state().suffix();

        match task_box {
            TaskBox::Invoke(invoke) => {
                self.render_invoke_box(items, invoke, task_box, border_color, state_icon, state_suffix, colors);
            }
            TaskBox::Infer(infer) => {
                self.render_infer_box(items, infer, task_box, border_color, state_icon, state_suffix, colors);
            }
            TaskBox::Exec(exec) => {
                Self::render_exec_box(items, exec, border_color, state_icon, state_suffix, colors, self.task_box_render_mode);
            }
            TaskBox::Fetch(fetch) => {
                Self::render_fetch_box(items, fetch, task_box, border_color, state_icon, state_suffix, colors, self.task_box_render_mode, self.frame);
            }
            TaskBox::Agent(agent) => {
                Self::render_agent_box(items, agent, task_box, border_color, state_icon, state_suffix, colors, self.task_box_render_mode, self.frame);
            }
        }
    }

    fn render_invoke_box(
        &self,
        items: &mut Vec<ListItem>,
        invoke: &crate::tui::widgets::task_box::InvokeBox,
        task_box: &TaskBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &TaskBoxColors,
    ) {
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let emerald_400 = Color::Rgb(52, 211, 153);

        match self.task_box_render_mode {
            RenderMode::Compact => {
                let status_str = if invoke.result.is_some() {
                    "✓ result"
                } else if invoke.error.is_some() {
                    "❌ error"
                } else if task_box.state().is_running() {
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
                        truncate_str(&invoke.tool, 20),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(" @ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        truncate_str(&invoke.server, 12),
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
                        truncate_str(&invoke.tool, 25),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(" @ ", Style::default().fg(colors.muted_color)),
                    Span::styled(
                        truncate_str(&invoke.server, 18),
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

                // 5. Params preview (truncated JSON)
                if !invoke.params.is_null() {
                    let params_str = serde_json::to_string(&invoke.params)
                        .unwrap_or_else(|_| "{}".to_string());
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            truncate_str(&params_str, 44),
                            Style::default().fg(content_color),
                        ),
                    ])));
                } else {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            "(no params)",
                            Style::default().fg(colors.muted_color),
                        ),
                    ])));
                }

                // 6. OUTPUT section header
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", Style::default().fg(border_color)),
                    Span::styled("📤 OUTPUT", Style::default().fg(colors.muted_color)),
                ])));

                // 7. Result or error preview
                if let Some(ref result) = invoke.result {
                    let result_str = serde_json::to_string(result)
                        .unwrap_or_else(|_| "null".to_string());
                    let result_lines: Vec<&str> =
                        result_str.lines().take(2).collect();
                    if result_lines.is_empty() {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(border_color)),
                            Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                            Span::styled(
                                truncate_str(&result_str, 44),
                                Style::default().fg(colors.success_color),
                            ),
                        ])));
                    } else {
                        for line in result_lines {
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled("│ ", Style::default().fg(border_color)),
                                Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                                Span::styled(
                                    truncate_str(line, 44),
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
                            truncate_str(err, 40),
                            Style::default().fg(colors.error_color),
                        ),
                    ])));
                } else if task_box.state().is_running() {
                    let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            "(calling MCP server)",
                            Style::default().fg(colors.muted_color),
                        ),
                        Span::styled(
                            cursor,
                            Style::default().fg(colors.success_color),
                        ),
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

    fn render_infer_box(
        &self,
        items: &mut Vec<ListItem>,
        infer: &crate::tui::widgets::task_box::InferBox,
        _task_box: &TaskBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &TaskBoxColors,
    ) {
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
                let prompt_preview =
                    truncate_str(&infer.prompt.replace('\n', " "), 28);
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
                let response_text =
                    if infer.state.is_running() && self.is_streaming {
                        &self.partial_response
                    } else {
                        &infer.response
                    };
                let response_indicator =
                    if infer.state.is_running() && self.is_streaming {
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
                        format!(
                            "                              {}",
                            response_indicator
                        ),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 7. Response content
                if response_text.is_empty() && infer.state.is_running() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled(
                            "┊ ...",
                            Style::default().fg(content_color),
                        ),
                    ])));
                } else {
                    let response_lines: Vec<&str> =
                        response_text.lines().collect();
                    let start = response_lines.len().saturating_sub(3);
                    let lines_to_show: Vec<_> =
                        response_lines.iter().skip(start).collect();
                    for (idx, line) in lines_to_show.iter().enumerate() {
                        let is_last = idx == lines_to_show.len() - 1;
                        let cursor = if is_last
                            && infer.state.is_running()
                            && self.is_streaming
                        {
                            "█"
                        } else {
                            ""
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                "│ ",
                                Style::default().fg(border_color),
                            ),
                            Span::styled(
                                format!(
                                    "┊ {}{}",
                                    truncate_str(line, 44),
                                    cursor
                                ),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                }

                // 8. Footer with metrics
                let cost =
                    crate::tui::widgets::task_box::InferBox::calculate_cost(
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

    fn render_exec_box(
        items: &mut Vec<ListItem>,
        exec: &crate::tui::widgets::task_box::ExecBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &TaskBoxColors,
        render_mode: RenderMode,
    ) {
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let exit_display = exec
            .exit_code
            .map(|c| format!("exit: {}", c))
            .unwrap_or_else(|| "running...".to_string());
        let exit_color = exec
            .exit_code
            .map(|c| if c == 0 { colors.success_color } else { colors.error_color })
            .unwrap_or(colors.muted_color);

        match render_mode {
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
                    Span::styled(
                        exit_display.clone(),
                        Style::default().fg(exit_color),
                    ),
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
                        format!(
                            "                                  {}",
                            output_indicator
                        ),
                        Style::default().fg(colors.muted_color),
                    ),
                ])));
                if exec.stdout.is_empty() && exec.state.is_running() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled(
                            "┊ ...",
                            Style::default().fg(content_color),
                        ),
                    ])));
                } else if exec.stdout.is_empty() {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled(
                            "┊ (no output)",
                            Style::default().fg(colors.muted_color),
                        ),
                    ])));
                } else {
                    let output_lines: Vec<&str> = exec.stdout.lines().collect();
                    let start = output_lines.len().saturating_sub(3);
                    for line in output_lines.iter().skip(start) {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                "│ ",
                                Style::default().fg(border_color),
                            ),
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

    #[allow(clippy::too_many_arguments)]
    fn render_fetch_box(
        items: &mut Vec<ListItem>,
        fetch: &crate::tui::widgets::task_box::FetchBox,
        task_box: &TaskBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &TaskBoxColors,
        render_mode: RenderMode,
        frame: u8,
    ) {
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let amber_400 = Color::Rgb(251, 191, 36);

        match render_mode {
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
                    Span::styled(
                        format!("{} ", fetch.method),
                        Style::default().fg(amber_400),
                    ),
                    Span::styled(
                        truncate_str(&fetch.url, 28),
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
                    Span::styled(
                        format!("{} ", fetch.method),
                        Style::default().fg(amber_400),
                    ),
                    Span::styled(
                        truncate_str(&fetch.url, 42),
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
                    let preview_lines: Vec<&str> =
                        body.lines().take(2).collect();
                    for line in preview_lines {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                "│ ",
                                Style::default().fg(border_color),
                            ),
                            Span::styled(
                                "┊ ",
                                Style::default().fg(colors.muted_color),
                            ),
                            Span::styled(
                                truncate_str(line, 44),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                } else if task_box.state().is_running() {
                    let cursor = if frame % 2 == 0 { "▌" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            "(awaiting response)",
                            Style::default().fg(colors.muted_color),
                        ),
                        Span::styled(
                            cursor,
                            Style::default().fg(colors.success_color),
                        ),
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

    #[allow(clippy::too_many_arguments)]
    fn render_agent_box(
        items: &mut Vec<ListItem>,
        agent: &crate::tui::widgets::task_box::AgentBox,
        task_box: &TaskBox,
        border_color: Color,
        state_icon: &str,
        state_suffix: &str,
        colors: &TaskBoxColors,
        render_mode: RenderMode,
        frame: u8,
    ) {
        let content_color = Color::Rgb(226, 232, 240); // slate-200
        let amber_400 = Color::Rgb(251, 191, 36);
        let agent_icon = if agent.is_subagent { "🐤" } else { "🐔" };

        match render_mode {
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
                        truncate_str(&agent.prompt, 25),
                        Style::default().fg(content_color),
                    ),
                    Span::styled(
                        format!(
                            " │ T:{}/{} │ {}",
                            agent.turn, agent.max_turns, token_str
                        ),
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
                        truncate_str(&agent.prompt, 45),
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
                    Span::styled(
                        "💬 RESPONSE",
                        Style::default().fg(colors.muted_color),
                    ),
                ])));

                // 5. Final response content
                if let Some(response) = &agent.final_response {
                    let response_lines: Vec<&str> =
                        response.lines().take(3).collect();
                    for line in response_lines {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                "│ ",
                                Style::default().fg(border_color),
                            ),
                            Span::styled(
                                "┊ ",
                                Style::default().fg(colors.muted_color),
                            ),
                            Span::styled(
                                truncate_str(line, 44),
                                Style::default().fg(content_color),
                            ),
                        ])));
                    }
                    if response.lines().count() > 3 {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled(
                                "│ ",
                                Style::default().fg(border_color),
                            ),
                            Span::styled(
                                "┊ ",
                                Style::default().fg(colors.muted_color),
                            ),
                            Span::styled(
                                format!(
                                    "... ({} more lines)",
                                    response.lines().count() - 3
                                ),
                                Style::default().fg(colors.muted_color),
                            ),
                        ])));
                    }
                } else if task_box.state().is_running() {
                    let cursor = if frame % 2 == 0 { "▌" } else { " " };
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(border_color)),
                        Span::styled("┊ ", Style::default().fg(colors.muted_color)),
                        Span::styled(
                            format!(
                                "(turn {}/{} in progress)",
                                agent.turn, agent.max_turns
                            ),
                            Style::default().fg(colors.muted_color),
                        ),
                        Span::styled(
                            cursor,
                            Style::default().fg(colors.success_color),
                        ),
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
