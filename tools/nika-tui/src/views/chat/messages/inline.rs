// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Inline content rendering (MCP calls and Infer streams).
//!
//! Renders the `InlineContent::McpCall` and `InlineContent::InferStream` variants
//! as box-styled widgets in the conversation list.

use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

use super::colors::RenderColors;
use crate::utils::truncate_str;
use crate::views::chat::{InlineContent, SEPARATOR_52};

/// Render inline MCP call and Infer stream boxes into `items`.
///
/// Called from `render_messages_v2` after the message items have been built.
/// Only handles `McpCall` and `InferStream` variants -- `Task` is handled in `task_boxes`.
pub(in crate::views::chat) fn render_inline_content<'a>(
    inline_content: &[InlineContent],
    colors: &RenderColors,
    items: &mut Vec<ListItem<'a>>,
) {
    for content in inline_content {
        match content {
            InlineContent::McpCall(data) => {
                render_mcp_call(data, colors, items);
            }
            InlineContent::InferStream(data) => {
                render_infer_stream(data, colors, items);
            }
            // Task variants handled by task_boxes module
            InlineContent::Task(_) => {}
        }
    }
}

/// Render a single MCP call box.
fn render_mcp_call<'a>(
    data: &crate::widgets::McpCallData,
    colors: &RenderColors,
    items: &mut Vec<ListItem<'a>>,
) {
    let (status_char, status_color) = data.status.indicator(data.frame);
    let duration_str = format!("{:.1}s", data.duration.as_secs_f64());

    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            format!("╭─ 🔧 MCP: {} ", data.tool),
            Style::default().fg(colors.mcp_box_color),
        ),
        Span::styled(
            format!("{} {} ─╮", status_char, duration_str),
            Style::default().fg(status_color),
        ),
    ])));

    if !data.params.is_empty() {
        let params_display = truncate_str(&data.params, 40).into_owned();
        items.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(colors.mcp_box_color)),
            Span::styled("📥 ", Style::default().fg(colors.muted_color)),
            Span::raw(params_display),
        ])));
    }

    if let Some(ref result) = data.result {
        let result_display = truncate_str(result, 40).into_owned();
        items.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(colors.mcp_box_color)),
            Span::styled("📤 ", Style::default().fg(colors.success_color)),
            Span::raw(result_display),
        ])));
    } else if let Some(ref error) = data.error {
        let error_display = truncate_str(error, 40).into_owned();
        items.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(colors.mcp_box_color)),
            Span::styled("❌ ", Style::default().fg(colors.error_color)),
            Span::raw(error_display),
        ])));
    }

    // PERF: Use const for MCP box bottom border
    items.push(ListItem::new(Line::from(vec![Span::styled(
        SEPARATOR_52,
        Style::default().fg(colors.mcp_box_color),
    )])));
    items.push(ListItem::new("")); // spacing
}

/// Render a single Infer stream box.
fn render_infer_stream<'a>(
    data: &crate::widgets::InferStreamData,
    colors: &RenderColors,
    items: &mut Vec<ListItem<'a>>,
) {
    let (status_char, _) = data.status.indicator(data.frame);
    let duration_str = format!("{:.1}s", data.duration.as_secs_f64());

    items.push(ListItem::new(Line::from(vec![
        Span::styled(
            format!("╭─ 🧠 INFER: {} ", data.model),
            Style::default().fg(colors.infer_box_color),
        ),
        Span::styled(
            format!("{} {} ─╮", status_char, duration_str),
            Style::default().fg(colors.status_running_color),
        ),
    ])));

    // Token info
    items.push(ListItem::new(Line::from(vec![
        Span::styled("│ ", Style::default().fg(colors.infer_box_color)),
        Span::styled(
            format!("📊 {} in → {} out", data.tokens_in, data.tokens_out),
            Style::default().fg(colors.muted_color),
        ),
    ])));

    // Last lines of content
    let content_lines: Vec<&str> = data.content.lines().collect();
    let start = content_lines.len().saturating_sub(3);
    for line in content_lines.iter().skip(start) {
        let display = truncate_str(line, 50).into_owned();
        items.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(colors.infer_box_color)),
            Span::raw(display),
        ])));
    }

    items.push(ListItem::new(Line::from(vec![Span::styled(
        "╰───────────────────────────────────────────────────╯",
        Style::default().fg(colors.infer_box_color),
    )])));
    items.push(ListItem::new("")); // spacing
}
