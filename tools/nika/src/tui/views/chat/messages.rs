//! Message Rendering for Chat View
//!
//! Orchestrates message rendering by delegating to specialized submodules:
//! - `inline_render.rs` — MCP call and InferStream inline boxes
//! - `task_box_render.rs` — 5 TaskBox variants (Invoke/Infer/Exec/Fetch/Agent)
//! - `agent_phase_render.rs` — Agent phase indicator with Matrix effect
//! - `search_bar.rs` — Search overlay when search mode is active

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use super::{
    ChatLinePosition, ChatPanel, ChatView, ExecutionStatus, InlineContent, MessageRole,
    SEPARATOR_200,
};
use crate::tui::theme::Theme;
use crate::tui::utils::{truncate_str, wrap_text};
use crate::tui::widgets::ScrollIndicator;
use crate::tui::VerbColor;

// ═══════════════════════════════════════════════════════════════════════════════
// Message Rendering Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Render messages v2 with inline MCP/Infer boxes
    pub(super) fn render_messages_v2(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let (search_area, messages_area) = if self.search_mode {
            use ratatui::layout::{Constraint, Direction, Layout};
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(1)])
                .split(area);
            (Some(chunks[0]), chunks[1])
        } else {
            (None, area)
        };

        // Render search bar if active
        if let Some(search_rect) = search_area {
            self.render_search_bar(frame, search_rect, theme);
        }

        // Use messages_area for rest of rendering
        let area = messages_area;

        // Extract theme colors with fallbacks
        let thinking_header_color = theme.status_running; // Use running (amber-ish) for thinking
        let thinking_content_color = theme.text_muted;
        let muted_color = theme.text_muted;
        let mcp_box_color = theme.status_success; // Emerald-like
        let success_color = theme.status_success;
        let error_color = theme.status_failed;
        let infer_box_color = theme.highlight; // Violet-like
        let status_running_color = theme.status_running;

        // Calculate content width for word wrapping
        // area.width - 2 (borders) - 2 ("│ " prefix) = available text width
        let content_width = area.width.saturating_sub(4) as usize;

        // Update visible count based on actual viewport height (minus borders)
        let viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
        self.conversation_scroll.visible = viewport_height;

        // Text Selection: Build line positions cache for mouse hit testing
        self.line_positions.clear();
        let content_start_x = area.x + 3; // "│ " prefix = 2 chars + border
        let mut current_line = 0usize;
        for (msg_idx, msg) in self.messages.iter().enumerate() {
            current_line += 1; // Header line (e.g., "👤 You ────")

            for (line_idx, line_text) in msg.content.lines().enumerate() {
                // Calculate screen Y based on scroll offset
                let line_in_list = current_line;
                let scroll_offset = self.conversation_scroll.offset;

                // Only track lines that could be visible
                if line_in_list >= scroll_offset {
                    let screen_y = area.y + 1 + (line_in_list - scroll_offset) as u16; // +1 for border
                    if screen_y < area.y + area.height - 1 {
                        self.line_positions.push(ChatLinePosition {
                            message_index: msg_idx,
                            line_in_message: line_idx,
                            screen_y,
                            start_x: content_start_x,
                            text: line_text.to_string(),
                        });
                    }
                }
                current_line += 1;
            }

            // Account for thinking lines if present and visible
            if let Some(ref thinking) = msg.thinking {
                if self.is_thinking_visible(msg_idx) {
                    current_line += 1; // "🧠 Thinking:" header
                    let think_lines = thinking.lines().take(3).count();
                    current_line += think_lines;
                    if thinking.lines().count() > 3 {
                        current_line += 1; // "... (N more lines)"
                    }
                } else {
                    current_line += 1; // Collapsed indicator: "🧠 Thinking: (collapsed)"
                }
            }

            // Account for execution result if present
            if msg.execution.is_some() {
                current_line += 1;
            }

            current_line += 1; // Spacing line
        }

        // Text Selection: Extract selection state for use in closure
        let selection = self.text_selection.clone();
        let selection_bg = theme.highlight; // Use highlight color for selection background

        // Pre-compute thinking visibility for use in closure
        let thinking_visible: Vec<bool> = (0..self.messages.len())
            .map(|i| self.is_thinking_visible(i))
            .collect();

        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .enumerate()
            .flat_map(|(idx, msg)| {
                // Skip "Thinking..." placeholder during streaming
                // The streaming section shows the Matrix Decrypt effect instead
                let is_last_message = idx == self.messages.len().saturating_sub(1);
                if self.is_streaming && is_last_message && msg.content == "Thinking..." {
                    return vec![]; // Don't render placeholder during streaming
                }

                // WOW: Check if this message has the flash effect
                let is_flashing = self.copy_flash_index == Some(idx);

                // Color-coded message bubbles based on role
                let (_prefix, base_color) = match msg.role {
                    MessageRole::User => ("👤 You", theme.trait_retrieved),
                    MessageRole::Nika => ("🤖 AI", theme.status_success),
                    MessageRole::System => ("💡 System", theme.status_running),
                    MessageRole::Tool => ("🔧 Tool", theme.mcp_traverse),
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

                // UX: Format timestamp as HH:MM:SS.mmm (with seconds and milliseconds)
                let ts_str = msg.timestamp.format("%H:%M:%S%.3f").to_string();

                // Dynamic separator at 75% width for visual balance
                // Leaves breathing room on the right side
                // PERF: Use static slice instead of .repeat() to avoid allocation
                let separator_len = (content_width * 75 / 100).saturating_sub(20); // 75% minus prefix+timestamp
                let separator_chars = separator_len.min(200); // Cap at SEPARATOR_200 length
                let separator_bytes = separator_chars * 3; // Each '─' is 3 UTF-8 bytes
                let dynamic_separator = &SEPARATOR_200[..separator_bytes];

                // WOW: Add COPIED indicator when flashing
                // Clock emoji before timestamp
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
                let is_selected = selection.as_ref().is_some_and(|sel| {
                    let (start, end) = sel.normalized();
                    idx >= start.message_index && idx <= end.message_index
                });

                // Wrap message content to fit panel width
                // Smart wrap for System message - preserve ASCII art, wrap text
                let wrapped_lines: Vec<String> =
                    if idx == 0 && matches!(msg.role, MessageRole::System) {
                        // For System banner: keep ASCII art lines intact, wrap text lines
                        let mut result = Vec::new();
                        for line in msg.content.lines() {
                            // ASCII art detection: contains block chars (█╔╗╚╝║═╭╮╰╯─┌┐└┘│)
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
                                // Word-wrap long text lines
                                result.extend(wrap_text(line, content_width));
                            }
                        }
                        result
                    } else {
                        wrap_text(&msg.content, content_width)
                    };

                // Text Selection: Track char offset for selection highlighting
                let mut char_offset = 0usize;
                for wrapped_line in &wrapped_lines {
                    let line_len = wrapped_line.chars().count();

                    // Text Selection: Apply highlighting if selected
                    let text_style = if is_selected {
                        if let Some(ref sel) = selection {
                            let (start, end) = sel.normalized();
                            // Check if this line is within the selection
                            let line_start = char_offset;
                            let line_end = char_offset + line_len;

                            // Calculate selection overlap with this line
                            let sel_start_in_msg = if idx == start.message_index {
                                start.char_offset
                            } else {
                                0
                            };
                            let sel_end_in_msg = if idx == end.message_index {
                                end.char_offset
                            } else {
                                usize::MAX
                            };

                            // Check if this line overlaps with selection
                            if line_end > sel_start_in_msg && line_start < sel_end_in_msg {
                                Style::default().bg(selection_bg).fg(Color::Black)
                            } else {
                                Style::default()
                            }
                        } else {
                            Style::default()
                        }
                    } else {
                        Style::default()
                    };

                    lines.push(ListItem::new(Line::from(vec![
                        Span::styled("│ ", Style::default().fg(color)),
                        Span::styled(wrapped_line.to_string(), text_style),
                    ])));

                    char_offset += line_len + 1; // +1 for newline/wrap
                }

                // Add colored verb boxes after welcome banner (first System message)
                // Emoji widths are hardcoded per verb for perfect alignment
                if idx == 0 && matches!(msg.role, MessageRole::System) {
                    // Empty line before boxes
                    lines.push(ListItem::new(Line::from(vec![Span::styled(
                        "│ ",
                        Style::default().fg(color),
                    )])));

                    // Verb boxes with pre-computed content for consistent alignment
                    let verbs: [(VerbColor, &str); 5] = [
                        (VerbColor::Infer, " ⚡ /infer  "),  // 1+2+1+6+2 = 12
                        (VerbColor::Exec, " 📟 /exec   "),   // 1+2+1+5+3 = 12
                        (VerbColor::Fetch, " 📡 /fetch  "),  // 1+2+1+6+2 = 12
                        (VerbColor::Invoke, " 🔌 /invoke "), // 1+2+1+7+1 = 12
                        (VerbColor::Agent, " 🐔 /agent  "),  // 1+2+1+6+2 = 12
                    ];

                    let box_border = "────────────"; // 12 dashes

                    // Top borders line (2-space indent to align with banner)
                    let mut top_spans = vec![Span::styled("│  ", Style::default().fg(color))];
                    for (verb_color, _) in &verbs {
                        let c = verb_color.rgb();
                        top_spans.push(Span::styled(
                            format!("┌{}┐", box_border),
                            Style::default().fg(c),
                        ));
                        top_spans.push(Span::raw(" ")); // 1 space gap
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
                        content_spans.push(Span::raw(" ")); // 1 space gap
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
                        bottom_spans.push(Span::raw(" ")); // 1 space gap
                    }
                    lines.push(ListItem::new(Line::from(bottom_spans)));
                }

                // Add thinking display if present
                if let Some(ref thinking) = msg.thinking {
                    let is_visible = thinking_visible.get(idx).copied().unwrap_or(false);

                    if is_visible {
                        // Expanded: show header and content
                        lines.push(ListItem::new(Line::from(vec![
                            Span::styled("│ ", Style::default().fg(color)),
                            Span::styled(
                                "🧠 Thinking: [t to collapse]",
                                Style::default()
                                    .fg(thinking_header_color)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ])));

                        // Truncate thinking to first 3 lines for inline display
                        let thinking_lines: Vec<&str> = thinking.lines().take(3).collect();
                        for think_line in &thinking_lines {
                            // Truncate each line to 60 chars (UTF-8 safe)
                            let display_line = truncate_str(think_line, 60);
                            lines.push(ListItem::new(Line::from(vec![
                                Span::styled("│   ", Style::default().fg(color)),
                                Span::styled(
                                    display_line,
                                    Style::default()
                                        .fg(thinking_content_color)
                                        .add_modifier(Modifier::ITALIC),
                                ),
                            ])));
                        }

                        // Show ellipsis if there are more lines
                        let total_lines = thinking.lines().count();
                        if total_lines > 3 {
                            lines.push(ListItem::new(Line::from(vec![
                                Span::styled("│   ", Style::default().fg(color)),
                                Span::styled(
                                    format!("... ({} more lines)", total_lines - 3),
                                    Style::default().fg(muted_color),
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
                                    .fg(muted_color)
                                    .add_modifier(Modifier::ITALIC),
                            ),
                        ])));
                    }
                }

                // Add execution result if present
                if let Some(exec) = &msg.execution {
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
                                status_icon,
                                exec.workflow_name,
                                exec.tasks_completed,
                                exec.tasks_total
                            ),
                            Style::default().fg(status_color),
                        ),
                    ])));
                }

                lines.push(ListItem::new("")); // spacing
                lines
            })
            .collect();

        // ─── Inline content (MCP calls, Infer streams, TaskBoxes) ───────────
        let task_box_colors = super::task_box_render::TaskBoxColors {
            muted_color,
            success_color,
            error_color,
        };

        for content in &self.inline_content {
            match content {
                InlineContent::McpCall(data) => {
                    Self::render_inline_mcp_call(
                        &mut items,
                        data,
                        mcp_box_color,
                        success_color,
                        error_color,
                        muted_color,
                    );
                }
                InlineContent::InferStream(data) => {
                    Self::render_inline_infer_stream(
                        &mut items,
                        data,
                        infer_box_color,
                        muted_color,
                        status_running_color,
                    );
                }
                InlineContent::Task(task_box) => {
                    self.render_task_box_items(&mut items, task_box, &task_box_colors);
                }
            }
        }

        // ─── Agent phase indicator ──────────────────────────────────────────
        self.render_agent_phase_items(&mut items, content_width, theme);

        // ─── Scroll assembly and rendering ──────────────────────────────────
        // REMOVED streaming AI bubble - InferBox REPLACES the AI message bubble
        // The response now displays INSIDE the InferBox widget (Option A)
        // See TaskBox::Infer rendering above which shows partial_response during streaming

        // UX: Focus indicators for Conversation panel
        let is_focused = self.focused_panel == ChatPanel::Conversation;
        let title = if is_focused {
            " ▸ 💬 CONVERSATION "
        } else {
            " 💬 CONVERSATION "
        };

        // Add scroll indicator to title if scrollable
        let title_with_indicator = if let Some(indicator) = self.conversation_scroll.indicator() {
            format!("{}{}", title, indicator)
        } else {
            title.to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title_with_indicator)
            .title_style(theme.border_style(is_focused))
            .border_style(theme.border_style(is_focused));

        // Update total item count for scroll state BEFORE any scroll operations
        let total_items = items.len();
        self.conversation_scroll.total = total_items;

        // Snap to actual bottom when user was at bottom
        // This prevents jump caused by estimated vs actual total mismatch in auto_scroll_to_bottom()
        // The estimated total (messages.len() * 4) differs from actual total (items.len())
        let visible_count = viewport_height;
        if self.user_at_bottom && total_items > visible_count {
            self.conversation_scroll.offset = total_items.saturating_sub(visible_count);
            self.conversation_scroll.cursor = total_items.saturating_sub(1);
        }

        // (NovaNet pattern): Apply scroll using .skip().take() directly
        // This is more reliable than relying on ListState's internal offset mechanism
        let scroll_offset = self.conversation_scroll.offset;

        // Clamp offset to valid range
        let clamped_offset = if total_items > visible_count {
            scroll_offset.min(total_items.saturating_sub(visible_count))
        } else {
            0
        };
        self.conversation_scroll.offset = clamped_offset;

        // Apply scroll - only show visible lines (NovaNet pattern)
        let visible_items: Vec<ListItem> = items
            .into_iter()
            .skip(clamped_offset)
            .take(visible_count)
            .collect();

        let list = List::new(visible_items).block(block);
        frame.render_widget(list, area);

        // UX: Render custom ScrollIndicator with dynamic arrows
        // Shows △/▲ based on can_scroll state for better visual feedback
        if total_items > visible_count {
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y + 1,
                width: 1,
                height: area.height.saturating_sub(2),
            };

            let scroll_indicator = ScrollIndicator::new()
                .position(clamped_offset, total_items, visible_count)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);

            frame.render_widget(scroll_indicator, scrollbar_area);
        }
    }
}
