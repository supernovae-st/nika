//! Inline content rendering for Chat View
//!
//! Extracted from messages.rs — renders MCP call and InferStream inline boxes
//! that appear in the conversation flow.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use super::{ChatView, SEPARATOR_52};
use crate::tui::utils::truncate_str;
use crate::tui::widgets::{InferStreamData, McpCallData};

impl ChatView {
    /// Render an inline MCP call box into the items list
    pub(super) fn render_inline_mcp_call<'a>(
        items: &mut Vec<ListItem<'a>>,
        data: &McpCallData,
        mcp_box_color: Color,
        success_color: Color,
        error_color: Color,
        muted_color: Color,
    ) {
        let (status_char, status_color) = data.status.indicator(data.frame);
        let duration_str = format!("{:.1}s", data.duration.as_secs_f64());

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("╭─ 🔧 MCP: {} ", data.tool),
                Style::default().fg(mcp_box_color),
            ),
            Span::styled(
                format!("{} {} ─╮", status_char, duration_str),
                Style::default().fg(status_color),
            ),
        ])));

        if !data.params.is_empty() {
            // UTF-8 safe truncation
            let params_display = truncate_str(&data.params, 40);
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(mcp_box_color)),
                Span::styled("📥 ", Style::default().fg(muted_color)),
                Span::raw(params_display),
            ])));
        }

        if let Some(ref result) = data.result {
            // UTF-8 safe truncation
            let result_display = truncate_str(result, 40);
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(mcp_box_color)),
                Span::styled("📤 ", Style::default().fg(success_color)),
                Span::raw(result_display),
            ])));
        } else if let Some(ref error) = data.error {
            // UTF-8 safe truncation
            let error_display = truncate_str(error, 40);
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(mcp_box_color)),
                Span::styled("❌ ", Style::default().fg(error_color)),
                Span::raw(error_display),
            ])));
        }

        // PERF: Use const for MCP box bottom border
        items.push(ListItem::new(Line::from(vec![Span::styled(
            SEPARATOR_52,
            Style::default().fg(mcp_box_color),
        )])));
        items.push(ListItem::new("")); // spacing
    }

    /// Render an inline InferStream box into the items list
    pub(super) fn render_inline_infer_stream<'a>(
        items: &mut Vec<ListItem<'a>>,
        data: &InferStreamData,
        infer_box_color: Color,
        muted_color: Color,
        status_running_color: Color,
    ) {
        let (status_char, _) = data.status.indicator(data.frame);
        let duration_str = format!("{:.1}s", data.duration.as_secs_f64());

        items.push(ListItem::new(Line::from(vec![
            Span::styled(
                format!("╭─ 🧠 INFER: {} ", data.model),
                Style::default().fg(infer_box_color),
            ),
            Span::styled(
                format!("{} {} ─╮", status_char, duration_str),
                Style::default().fg(status_running_color),
            ),
        ])));

        // Token info
        items.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(infer_box_color)),
            Span::styled(
                format!("📊 {} in → {} out", data.tokens_in, data.tokens_out),
                Style::default().fg(muted_color),
            ),
        ])));

        // Last lines of content
        let content_lines: Vec<&str> = data.content.lines().collect();
        let start = content_lines.len().saturating_sub(3);
        for line in content_lines.iter().skip(start) {
            // UTF-8 safe truncation
            let display = truncate_str(line, 50);
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(infer_box_color)),
                Span::raw(display),
            ])));
        }

        items.push(ListItem::new(Line::from(vec![Span::styled(
            "╰───────────────────────────────────────────────────╯",
            Style::default().fg(infer_box_color),
        )])));
        items.push(ListItem::new("")); // spacing
    }
}
