// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Message content rendering (headers, text, selection, verb boxes, thinking, execution).
//!
//! Renders individual `ChatMessage` items as `ListItem`s for the conversation list.
//! Each message gets a color-coded header, word-wrapped body, and optional
//! thinking/execution annotations.

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use super::colors::RenderColors;
use crate::theme::Theme;
use crate::utils::{truncate_str, wrap_text};
use crate::views::chat::{
    ChatMessage, ChatMessageSelection, ChatView, ExecutionStatus, MessageRole, SEPARATOR_200,
};
use crate::VerbColor;

/// Selection state passed into the rendering closure to avoid borrowing `self`.
pub(in crate::views::chat) struct SelectionContext {
    pub selection: Option<ChatMessageSelection>,
    pub selection_bg: Color,
}

impl ChatView {
    /// Build `ListItem`s for all chat messages.
    ///
    /// This replaces the large `flat_map` closure that was previously inline
    /// in `render_messages_v2`. It iterates `self.messages`, producing header
    /// lines, wrapped body text (with selection highlighting), optional verb
    /// boxes (for the welcome banner), thinking blocks, and execution results.
    pub(in crate::views::chat) fn build_message_items<'a>(
        &self,
        theme: &Theme,
        colors: &RenderColors,
        content_width: usize,
        sel_ctx: &SelectionContext,
    ) -> Vec<ListItem<'a>> {
        self.messages
            .iter()
            .enumerate()
            .flat_map(|(idx, msg)| {
                self.render_single_message(idx, msg, theme, colors, content_width, sel_ctx)
            })
            .collect()
    }

    /// Render a single chat message into a vector of `ListItem`s.
    fn render_single_message<'a>(
        &self,
        idx: usize,
        msg: &ChatMessage,
        theme: &Theme,
        colors: &RenderColors,
        content_width: usize,
        sel_ctx: &SelectionContext,
    ) -> Vec<ListItem<'a>> {
        // Skip "Thinking..." placeholder during streaming
        let is_last_message = idx == self.messages.len().saturating_sub(1);
        if self.is_streaming && is_last_message && msg.content == "Thinking..." {
            return vec![];
        }

        // WOW: Check if this message has the flash effect
        let is_flashing = self.copy_flash_index == Some(idx);

        // Color-coded message bubbles based on role
        let (_prefix, base_color) = match msg.role {
            MessageRole::User => ("👤 You", theme.trait_retrieved),
            MessageRole::Nika => ("🤖 AI", theme.status_success),
            MessageRole::System => ("💡 System", theme.status_running),
            MessageRole::Tool => ("🔧 Tool", theme.mcp_context),
        };

        // WOW: Flash effect - bright highlight when copied
        let color = if is_flashing {
            theme.highlight
        } else {
            base_color
        };
        let style = Style::default().fg(color);

        // PERF: Use const prefix strings to avoid format! allocation
        let prefix_with_space = match msg.role {
            MessageRole::User => "👤 You ",
            MessageRole::Nika => "🤖 AI ",
            MessageRole::System => "💡 System ",
            MessageRole::Tool => "🔧 Tool ",
        };

        // UX: Format timestamp as HH:MM:SS.mmm
        let ts_str = msg.timestamp.format("%H:%M:%S%.3f").to_string();

        // Dynamic separator at 75% width for visual balance
        let separator_len = (content_width * 75 / 100).saturating_sub(20);
        let separator_chars = separator_len.min(200);
        let dynamic_separator: String = SEPARATOR_200.chars().take(separator_chars).collect();

        // WOW: Add COPIED indicator when flashing
        let mut header_spans = vec![
            Span::styled(prefix_with_space, style.add_modifier(Modifier::BOLD)),
            Span::styled(dynamic_separator, Style::default().fg(theme.text_muted)),
            Span::styled(
                format!(" 🕐 {} ", ts_str),
                Style::default().fg(theme.text_muted),
            ),
        ];
        if is_flashing {
            header_spans.push(Span::styled(
                " ✓ COPIED ",
                Style::default()
                    .fg(Color::Black)
                    .bg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ));
        }
        let mut lines = vec![ListItem::new(Line::from(header_spans))];

        // Text Selection: Check if this message is in the selection range
        let is_selected = sel_ctx.selection.as_ref().is_some_and(|sel| {
            let (start, end) = sel.normalized();
            idx >= start.message_index && idx <= end.message_index
        });

        // Wrap message content to fit panel width
        let wrapped_lines: Vec<String> = if idx == 0 && matches!(msg.role, MessageRole::System) {
            smart_wrap_system_banner(&msg.content, content_width)
        } else {
            wrap_text(&msg.content, content_width)
        };

        // Render wrapped content lines with optional selection highlighting
        let mut char_offset = 0usize;
        for wrapped_line in &wrapped_lines {
            let line_len = wrapped_line.chars().count();

            let text_style = compute_selection_style(
                is_selected,
                &sel_ctx.selection,
                sel_ctx.selection_bg,
                idx,
                char_offset,
                line_len,
            );

            lines.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", Style::default().fg(color)),
                Span::styled(wrapped_line.to_string(), text_style),
            ])));

            char_offset += line_len + 1;
        }

        // Add colored verb boxes after welcome banner (first System message)
        if idx == 0 && matches!(msg.role, MessageRole::System) {
            render_verb_boxes(color, &mut lines);
        }

        // Add thinking display if present
        // PERF: inline check instead of pre-computed Vec<bool>
        if let Some(ref thinking) = msg.thinking {
            let is_visible = self.is_thinking_visible(idx);
            render_thinking(thinking, is_visible, color, colors, &mut lines);
        }

        // Add execution result if present
        if let Some(exec) = &msg.execution {
            render_execution(exec, theme, &mut lines);
        }

        lines.push(ListItem::new("")); // spacing
        lines
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Free functions (not methods on ChatView)
// ═══════════════════════════════════════════════════════════════════════════════

/// Smart-wrap a system banner message, preserving ASCII art lines.
fn smart_wrap_system_banner(content: &str, content_width: usize) -> Vec<String> {
    let mut result = Vec::new();
    for line in content.lines() {
        let is_ascii_art = line.chars().any(|c| {
            matches!(
                c,
                '█' | '╔'
                    | '╗'
                    | '╚'
                    | '╝'
                    | '║'
                    | '═'
                    | '╭'
                    | '╮'
                    | '╰'
                    | '╯'
                    | '─'
                    | '┌'
                    | '┐'
                    | '└'
                    | '┘'
                    | '│'
                    | '▀'
                    | '▄'
                    | '░'
                    | '▒'
                    | '▓'
            )
        });
        if is_ascii_art || line.len() <= content_width {
            result.push(line.to_string());
        } else {
            result.extend(wrap_text(line, content_width));
        }
    }
    result
}

/// Compute the style for a text line, applying selection background if applicable.
fn compute_selection_style(
    is_selected: bool,
    selection: &Option<ChatMessageSelection>,
    selection_bg: Color,
    msg_idx: usize,
    char_offset: usize,
    line_len: usize,
) -> Style {
    if !is_selected {
        return Style::default();
    }
    let Some(sel) = selection else {
        return Style::default();
    };
    let (start, end) = sel.normalized();
    let line_start = char_offset;
    let line_end = char_offset + line_len;

    let sel_start_in_msg = if msg_idx == start.message_index {
        start.char_offset
    } else {
        0
    };
    let sel_end_in_msg = if msg_idx == end.message_index {
        end.char_offset
    } else {
        usize::MAX
    };

    if line_end > sel_start_in_msg && line_start < sel_end_in_msg {
        Style::default().bg(selection_bg).fg(Color::Black)
    } else {
        Style::default()
    }
}

/// Render the 5 verb boxes after the system welcome banner.
fn render_verb_boxes<'a>(color: Color, lines: &mut Vec<ListItem<'a>>) {
    // Empty line before boxes
    lines.push(ListItem::new(Line::from(vec![Span::styled(
        "│ ",
        Style::default().fg(color),
    )])));

    let verbs: [(VerbColor, &str); 5] = [
        (VerbColor::Infer, " ⚡ /infer  "),
        (VerbColor::Exec, " 📟 /exec   "),
        (VerbColor::Fetch, " 📡 /fetch  "),
        (VerbColor::Invoke, " 🔌 /invoke "),
        (VerbColor::Agent, " 🐔 /agent  "),
    ];

    let box_border = "────────────"; // 12 dashes

    // Top borders line
    let mut top_spans = vec![Span::styled("│  ", Style::default().fg(color))];
    for (verb_color, _) in &verbs {
        let c = verb_color.rgb();
        top_spans.push(Span::styled(
            format!("┌{}┐", box_border),
            Style::default().fg(c),
        ));
        top_spans.push(Span::raw(" "));
    }
    lines.push(ListItem::new(Line::from(top_spans)));

    // Content line with emojis and names
    let mut content_spans = vec![Span::styled("│  ", Style::default().fg(color))];
    for (verb_color, content) in &verbs {
        let c = verb_color.rgb();
        content_spans.push(Span::styled("│", Style::default().fg(c)));
        content_spans.push(Span::styled(
            *content,
            Style::default().fg(c).add_modifier(Modifier::BOLD),
        ));
        content_spans.push(Span::styled("│", Style::default().fg(c)));
        content_spans.push(Span::raw(" "));
    }
    lines.push(ListItem::new(Line::from(content_spans)));

    // Bottom borders line
    let mut bottom_spans = vec![Span::styled("│  ", Style::default().fg(color))];
    for (verb_color, _) in &verbs {
        let c = verb_color.rgb();
        bottom_spans.push(Span::styled(
            format!("└{}┘", box_border),
            Style::default().fg(c),
        ));
        bottom_spans.push(Span::raw(" "));
    }
    lines.push(ListItem::new(Line::from(bottom_spans)));
}

/// Render thinking block (expanded or collapsed).
fn render_thinking<'a>(
    thinking: &str,
    is_visible: bool,
    color: Color,
    colors: &RenderColors,
    lines: &mut Vec<ListItem<'a>>,
) {
    if is_visible {
        // Expanded: show header and content
        lines.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(color)),
            Span::styled(
                "🧠 Thinking: [t to collapse]",
                Style::default()
                    .fg(colors.thinking_header_color)
                    .add_modifier(Modifier::ITALIC),
            ),
        ])));

        let thinking_lines: Vec<&str> = thinking.lines().take(3).collect();
        for think_line in &thinking_lines {
            let display_line = truncate_str(think_line, 60).into_owned();
            lines.push(ListItem::new(Line::from(vec![
                Span::styled("│   ", Style::default().fg(color)),
                Span::styled(
                    display_line,
                    Style::default()
                        .fg(colors.thinking_content_color)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])));
        }

        let total_lines = thinking.lines().count();
        if total_lines > 3 {
            lines.push(ListItem::new(Line::from(vec![
                Span::styled("│   ", Style::default().fg(color)),
                Span::styled(
                    format!("... ({} more lines)", total_lines - 3),
                    Style::default().fg(colors.muted_color),
                ),
            ])));
        }
    } else {
        // Collapsed: show only header with hint
        lines.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", Style::default().fg(color)),
            Span::styled(
                "🧠 Thinking: (collapsed) [t to expand]",
                Style::default()
                    .fg(colors.muted_color)
                    .add_modifier(Modifier::ITALIC),
            ),
        ])));
    }
}

/// Render execution result line.
fn render_execution<'a>(
    exec: &crate::views::chat::ExecutionResult,
    theme: &Theme,
    lines: &mut Vec<ListItem<'a>>,
) {
    let (status_icon, status_color) = match exec.status {
        ExecutionStatus::Running => ("⏳", theme.status_running),
        ExecutionStatus::Completed => ("✅", theme.status_success),
        ExecutionStatus::Failed => ("❌", theme.status_failed),
    };
    lines.push(ListItem::new(Line::from(vec![
        Span::raw("  "),
        Span::styled(
            format!(
                "└─ {} {} ({}/{}) ",
                status_icon, exec.workflow_name, exec.tasks_completed, exec.tasks_total
            ),
            Style::default().fg(status_color),
        ),
    ])));
}
