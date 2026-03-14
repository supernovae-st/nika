//! Message Rendering for Chat View
//!
//! Contains the large render_messages_v2 function and related helpers.
//! Extracted from mod.rs as part of Phase A1 refactoring.

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem},
    Frame,
};

use super::{
    ChatLinePosition, ChatPanel, ChatView, ExecutionStatus, InlineContent, MessageRole,
    SEPARATOR_200, SEPARATOR_52,
};
use crate::tui::theme::Theme;
use crate::tui::utils::{truncate_str, wrap_text};
use crate::tui::widgets::{task_box::RenderMode, AgentPhase, ScrollIndicator, TaskBox};
use crate::tui::VerbColor;

// ═══════════════════════════════════════════════════════════════════════════════
// Message Rendering Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Render messages v2 with inline MCP/Infer boxes
    pub(super) fn render_messages_v2(&mut self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // v0.9 Phase 3: Search bar at top when in search mode
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

        // v0.8.1 FIX: Calculate content width for word wrapping
        // area.width - 2 (borders) - 2 ("│ " prefix) = available text width
        let content_width = area.width.saturating_sub(4) as usize;

        // v0.8 FIX: Update visible count based on actual viewport height (minus borders)
        let viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
        self.conversation_scroll.visible = viewport_height;

        // v0.8 Text Selection: Build line positions cache for mouse hit testing
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

        // v0.8 Text Selection: Extract selection state for use in closure
        let selection = self.text_selection.clone();
        let selection_bg = theme.highlight; // Use highlight color for selection background

        // v0.9: Pre-compute thinking visibility for use in closure
        let thinking_visible: Vec<bool> = (0..self.messages.len())
            .map(|i| self.is_thinking_visible(i))
            .collect();

        let mut items: Vec<ListItem> = self
            .messages
            .iter()
            .enumerate()
            .flat_map(|(idx, msg)| {
                // v0.8.1 FIX: Skip "Thinking..." placeholder during streaming
                // The streaming section shows the Matrix Decrypt effect instead
                let is_last_message = idx == self.messages.len().saturating_sub(1);
                if self.is_streaming && is_last_message && msg.content == "Thinking..." {
                    return vec![]; // Don't render placeholder during streaming
                }

                // v0.8 WOW: Check if this message has the flash effect
                let is_flashing = self.copy_flash_index == Some(idx);

                // Color-coded message bubbles based on role
                let (_prefix, base_color) = match msg.role {
                    MessageRole::User => ("👤 You", theme.trait_retrieved),
                    MessageRole::Nika => ("🤖 AI", theme.status_success),
                    MessageRole::System => ("💡 System", theme.status_running),
                    MessageRole::Tool => ("🔧 Tool", theme.mcp_traverse),
                };

                // v0.8 WOW: Flash effect - bright highlight when copied
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

                // v0.9.5 UX: Format timestamp as HH:MM:SS.mmm (with seconds and milliseconds)
                let ts_str = msg.timestamp.format("%H:%M:%S%.3f").to_string();

                // v0.9.5: Dynamic separator at 75% width for visual balance
                // Leaves breathing room on the right side
                // PERF: Use static slice instead of .repeat() to avoid allocation
                let separator_len = (content_width * 75 / 100).saturating_sub(20); // 75% minus prefix+timestamp
                let separator_chars = separator_len.min(200); // Cap at SEPARATOR_200 length
                let separator_bytes = separator_chars * 3; // Each '─' is 3 UTF-8 bytes
                let dynamic_separator = &SEPARATOR_200[..separator_bytes];

                // v0.8 WOW: Add COPIED indicator when flashing
                // v0.9.5: Clock emoji before timestamp
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

                // v0.8 Text Selection: Check if this message is in the selection range
                let is_selected = selection.as_ref().is_some_and(|sel| {
                    let (start, end) = sel.normalized();
                    idx >= start.message_index && idx <= end.message_index
                });

                // v0.8.1 FIX: Wrap message content to fit panel width
                // v0.9.5: Smart wrap for System message - preserve ASCII art, wrap text
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

                // v0.8 Text Selection: Track char offset for selection highlighting
                let mut char_offset = 0usize;
                for wrapped_line in &wrapped_lines {
                    let line_len = wrapped_line.chars().count();

                    // v0.8 Text Selection: Apply highlighting if selected
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

                // v0.9.5: Add colored verb boxes after welcome banner (first System message)
                // Emoji widths are hardcoded per verb for perfect alignment
                if idx == 0 && matches!(msg.role, MessageRole::System) {
                    // Empty line before boxes
                    lines.push(ListItem::new(Line::from(vec![Span::styled(
                        "│ ",
                        Style::default().fg(color),
                    )])));

                    // Verb boxes with pre-computed content for consistent alignment
                    // Each box has: ┌────────────┐ (14 chars total: 12 dashes + 2 corners)
                    //               │ ⚡ /verb  │ (inner content = 12 display cols)
                    //               └────────────┘
                    // Emoji display widths: ⚡=2, 📟=2, 📡=2, 🔌=2, 🐔=2
                    // Formula: space(1) + emoji(2) + space(1) + /name(N+1) + padding = 12
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

        // Render inline content (MCP calls, Infer streams)
        for content in &self.inline_content {
            match content {
                InlineContent::McpCall(data) => {
                    // Render inline MCP call box representation
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
                InlineContent::InferStream(data) => {
                    // Render inline Infer stream box representation
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
                InlineContent::Task(task_box) => {
                    // v0.12.1: Render TaskBox with verb colors and pulse animation
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

                    // Render based on TaskBox variant
                    match task_box {
                        TaskBox::Invoke(invoke) => {
                            // v0.12.1: RenderMode-aware InvokeBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let emerald_400 = Color::Rgb(52, 211, 153);

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header, tool@server, footer
                                    // ╭─ 🔌 INVOKE ────────────────── ⏳ Running ─╮
                                    // │ novanet_describe @ novanet │ ✓ result   │
                                    // ╰────────────────────────────────────────────╯
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
                                        Span::styled(" @ ", Style::default().fg(muted_color)),
                                        Span::styled(
                                            truncate_str(&invoke.server, 12),
                                            Style::default().fg(emerald_400),
                                        ),
                                        Span::styled(
                                            format!(" │ {}", status_str),
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full InvokeBox rendering
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
                                        Span::styled(" @ ", Style::default().fg(muted_color)),
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
                                        Span::styled("📥 INPUT", Style::default().fg(muted_color)),
                                    ])));

                                    // 5. Params preview (truncated JSON)
                                    if !invoke.params.is_null() {
                                        let params_str = serde_json::to_string(&invoke.params)
                                            .unwrap_or_else(|_| "{}".to_string());
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                truncate_str(&params_str, 44),
                                                Style::default().fg(content_color),
                                            ),
                                        ])));
                                    } else {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                "(no params)",
                                                Style::default().fg(muted_color),
                                            ),
                                        ])));
                                    }

                                    // 6. OUTPUT section header
                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("📤 OUTPUT", Style::default().fg(muted_color)),
                                    ])));

                                    // 7. Result or error preview
                                    if let Some(ref result) = invoke.result {
                                        let result_str = serde_json::to_string(result)
                                            .unwrap_or_else(|_| "null".to_string());
                                        // Show first 2 lines of result
                                        let result_lines: Vec<&str> =
                                            result_str.lines().take(2).collect();
                                        if result_lines.is_empty() {
                                            items.push(ListItem::new(Line::from(vec![
                                                Span::styled(
                                                    "│ ",
                                                    Style::default().fg(border_color),
                                                ),
                                                Span::styled(
                                                    "┊ ",
                                                    Style::default().fg(muted_color),
                                                ),
                                                Span::styled(
                                                    truncate_str(&result_str, 44),
                                                    Style::default().fg(success_color),
                                                ),
                                            ])));
                                        } else {
                                            for line in result_lines {
                                                items.push(ListItem::new(Line::from(vec![
                                                    Span::styled(
                                                        "│ ",
                                                        Style::default().fg(border_color),
                                                    ),
                                                    Span::styled(
                                                        "┊ ",
                                                        Style::default().fg(muted_color),
                                                    ),
                                                    Span::styled(
                                                        truncate_str(line, 44),
                                                        Style::default().fg(success_color),
                                                    ),
                                                ])));
                                            }
                                        }
                                    } else if let Some(ref err) = invoke.error {
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled("❌ ", Style::default().fg(error_color)),
                                            Span::styled(
                                                truncate_str(err, 40),
                                                Style::default().fg(error_color),
                                            ),
                                        ])));
                                    } else if task_box.state().is_running() {
                                        // Streaming cursor for running invoke
                                        let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                "(calling MCP server)",
                                                Style::default().fg(muted_color),
                                            ),
                                            Span::styled(
                                                cursor,
                                                Style::default().fg(success_color),
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
                        TaskBox::Infer(infer) => {
                            // v0.12.1: RenderMode-aware InferBox rendering
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
                                    // COMPACT: 2 lines - header + content preview
                                    // ╭─ ⚡ INFER ──────────────────────── ⏳ Running ─╮
                                    // │ "Generate a headline..."  │ 127 in │ 45 out │
                                    // ╰────────────────────────────────────────────────╯
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
                                    // EXPANDED/FULL: Full InferBox rendering
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
                                            Style::default().fg(muted_color),
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
                                        Span::styled("PROMPT", Style::default().fg(muted_color)),
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
                                        Span::styled("RESPONSE", Style::default().fg(muted_color)),
                                        Span::styled(
                                            format!(
                                                "                              {}",
                                                response_indicator
                                            ),
                                            Style::default().fg(muted_color),
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
                                            Style::default().fg(muted_color),
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
                        TaskBox::Exec(exec) => {
                            // v0.12.1: RenderMode-aware ExecBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let exit_display = exec
                                .exit_code
                                .map(|c| format!("exit: {}", c))
                                .unwrap_or_else(|| "running...".to_string());
                            let exit_color = exec
                                .exit_code
                                .map(|c| if c == 0 { success_color } else { error_color })
                                .unwrap_or(muted_color);

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header + command + footer
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
                                    // EXPANDED/FULL: Full ExecBox rendering
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
                                        Span::styled("OUTPUT", Style::default().fg(muted_color)),
                                        Span::styled(
                                            format!(
                                                "                                  {}",
                                                output_indicator
                                            ),
                                            Style::default().fg(muted_color),
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
                                                Style::default().fg(muted_color),
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
                        TaskBox::Fetch(fetch) => {
                            // v0.12.1: RenderMode-aware FetchBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let amber_400 = Color::Rgb(251, 191, 36);

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header, method+url, footer
                                    // ╭─ 🛰️ FETCH ────────────────── ⏳ Running ─╮
                                    // │ GET https://api.example... │ 200 │ 45ms │
                                    // ╰────────────────────────────────────────────╯
                                    let status_str = fetch
                                        .status_code
                                        .map(|s| format!("{}", s))
                                        .unwrap_or_else(|| "---".to_string());
                                    let status_color = fetch
                                        .status_code
                                        .map(|s| {
                                            if (200..300).contains(&s) {
                                                success_color
                                            } else {
                                                error_color
                                            }
                                        })
                                        .unwrap_or(muted_color);

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
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(status_str, Style::default().fg(status_color)),
                                        Span::styled(
                                            format!(" │ {}ms", fetch.ttfb_ms),
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full FetchBox rendering
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
                                            Style::default().fg(muted_color),
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
                                                    Style::default().fg(muted_color),
                                                ),
                                                Span::styled(
                                                    truncate_str(line, 44),
                                                    Style::default().fg(content_color),
                                                ),
                                            ])));
                                        }
                                    } else if task_box.state().is_running() {
                                        // Streaming cursor for running fetch
                                        let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                "(awaiting response)",
                                                Style::default().fg(muted_color),
                                            ),
                                            Span::styled(
                                                cursor,
                                                Style::default().fg(success_color),
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
                                                success_color
                                            } else {
                                                error_color
                                            }
                                        })
                                        .unwrap_or(muted_color);

                                    items.push(ListItem::new(Line::from(vec![
                                        Span::styled("│ ", Style::default().fg(border_color)),
                                        Span::styled("Status: ", Style::default().fg(muted_color)),
                                        Span::styled(status_str, Style::default().fg(status_color)),
                                        Span::styled(
                                            format!(" │ TTFB: {}ms", fetch.ttfb_ms),
                                            Style::default().fg(muted_color),
                                        ),
                                        Span::styled(
                                            format!(" │ Retries: {}", fetch.retries),
                                            Style::default().fg(muted_color),
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
                        TaskBox::Agent(agent) => {
                            // v0.12.1: RenderMode-aware AgentBox rendering
                            let content_color = Color::Rgb(226, 232, 240); // slate-200
                            let amber_400 = Color::Rgb(251, 191, 36);
                            let agent_icon = if agent.is_subagent { "🐤" } else { "🐔" };

                            match self.task_box_render_mode {
                                RenderMode::Compact => {
                                    // COMPACT: 3 lines - header, goal, footer with stats
                                    // ╭─ 🐔 AGENT ────────────────── ⏳ Running ─╮
                                    // │ 🎯 "Analyze and report..." │ T:2/5 │ 1.2K │
                                    // ╰────────────────────────────────────────────╯
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
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));
                                    items.push(ListItem::new(Line::from(vec![Span::styled(
                                        "╰──────────────────────────────────────────────────╯",
                                        Style::default().fg(border_color),
                                    )])));
                                }
                                RenderMode::Expanded | RenderMode::Full => {
                                    // EXPANDED/FULL: Full AgentBox rendering
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
                                            Style::default().fg(muted_color),
                                        ),
                                    ])));

                                    // 5. Final response content (first 3 lines or streaming indicator)
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
                                                    Style::default().fg(muted_color),
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
                                                    Style::default().fg(muted_color),
                                                ),
                                                Span::styled(
                                                    format!(
                                                        "... ({} more lines)",
                                                        response.lines().count() - 3
                                                    ),
                                                    Style::default().fg(muted_color),
                                                ),
                                            ])));
                                        }
                                    } else if task_box.state().is_running() {
                                        // Streaming cursor for running agent
                                        let cursor = if self.frame % 2 == 0 { "▌" } else { " " };
                                        items.push(ListItem::new(Line::from(vec![
                                            Span::styled("│ ", Style::default().fg(border_color)),
                                            Span::styled("┊ ", Style::default().fg(muted_color)),
                                            Span::styled(
                                                format!(
                                                    "(turn {}/{} in progress)",
                                                    agent.turn, agent.max_turns
                                                ),
                                                Style::default().fg(muted_color),
                                            ),
                                            Span::styled(
                                                cursor,
                                                Style::default().fg(success_color),
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
                                            Style::default().fg(muted_color),
                                        ),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(
                                            format!("🎫 {} tokens", token_str),
                                            Style::default().fg(muted_color),
                                        ),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(
                                            format!("💰 ${:.2}", agent.cost),
                                            Style::default().fg(muted_color),
                                        ),
                                        Span::styled(" │ ", Style::default().fg(muted_color)),
                                        Span::styled(
                                            format!("🔌 {} calls", agent.tool_calls),
                                            Style::default().fg(muted_color),
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
                }
            }
        }

        // v0.8.1: Show agent phase indicator box when agent is active (not Idle)
        // This shows real phases (Syncing/Planning/Invoking/Processing/Inferring/Streaming) with Matrix effect
        // IMPORTANT: Show during ALL active phases including Streaming - users want to see activity!
        if self.agent_phase != AgentPhase::Idle {
            // Phase indicator box header - color by phase type
            let phase_color = match self.agent_phase {
                AgentPhase::Syncing => theme.status_running,
                AgentPhase::Planning => theme.status_running,
                AgentPhase::Routing => theme.status_running,
                AgentPhase::Invoking => theme.highlight,
                AgentPhase::Processing => theme.status_success,
                AgentPhase::Inferring => theme.status_running,
                AgentPhase::Composing => theme.status_running,
                AgentPhase::Streaming => theme.status_success, // Streaming = green = active output
                AgentPhase::Idle => theme.text_muted,          // Shouldn't reach here but safe
            };

            // Box top
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!(
                    "┌─ 🐔 Nika {}─┐",
                    "─".repeat(content_width.saturating_sub(15))
                ),
                Style::default().fg(phase_color),
            )])));

            // Phase indicator line with Matrix effect
            let phase_line = self.phase_indicator.build_line();
            let mut phase_spans = vec![Span::styled("│  ", Style::default().fg(phase_color))];
            phase_spans.extend(phase_line.spans);
            // Pad to box width
            phase_spans.push(Span::styled(
                format!("{:width$}│", "", width = content_width.saturating_sub(30)),
                Style::default().fg(phase_color),
            ));
            items.push(ListItem::new(Line::from(phase_spans)));

            // Show tool details if in Invoking/Processing phase
            if matches!(
                self.agent_phase,
                AgentPhase::Invoking | AgentPhase::Processing
            ) {
                if let Some(ref tool) = self.agent_phase_tool {
                    // Get last MCP call details if available
                    if let Some(InlineContent::McpCall(mcp_data)) = self.inline_content.last() {
                        // Server line
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("├── ", Style::default().fg(theme.text_muted)),
                            Span::styled("server: ", Style::default().fg(theme.text_muted)),
                            Span::styled(
                                mcp_data.server.clone(),
                                Style::default().fg(theme.highlight),
                            ),
                        ])));

                        // Params line (truncated)
                        if !mcp_data.params.is_empty() {
                            let params_preview = if mcp_data.params.len() > 40 {
                                format!("{}...", &mcp_data.params[..40])
                            } else {
                                mcp_data.params.clone()
                            };
                            items.push(ListItem::new(Line::from(vec![
                                Span::styled("│  ", Style::default().fg(phase_color)),
                                Span::styled("├── ", Style::default().fg(theme.text_muted)),
                                Span::styled("params: ", Style::default().fg(theme.text_muted)),
                                Span::styled(
                                    params_preview,
                                    Style::default().fg(theme.text_secondary),
                                ),
                            ])));
                        }

                        // Duration line
                        let elapsed_secs = mcp_data.duration.as_secs_f64();
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("└── ", Style::default().fg(theme.text_muted)),
                            Span::styled("⏱ ", Style::default().fg(theme.text_muted)),
                            Span::styled(
                                format!("{:.1}s", elapsed_secs),
                                Style::default().fg(if elapsed_secs > 5.0 {
                                    theme.status_failed
                                } else if elapsed_secs > 2.0 {
                                    theme.status_running
                                } else {
                                    theme.status_success
                                }),
                            ),
                        ])));
                    } else {
                        // Fallback: just show tool name
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("└── ", Style::default().fg(theme.text_muted)),
                            Span::styled("tool: ", Style::default().fg(theme.text_muted)),
                            Span::styled(tool.clone(), Style::default().fg(theme.highlight)),
                        ])));
                    }
                }
            }

            // Box bottom
            items.push(ListItem::new(Line::from(vec![Span::styled(
                format!("└{}┘", "─".repeat(content_width.saturating_sub(2))),
                Style::default().fg(phase_color),
            )])));

            items.push(ListItem::new("")); // spacing
        }

        // v0.12.1: REMOVED streaming AI bubble - InferBox REPLACES the AI message bubble
        // The response now displays INSIDE the InferBox widget (Option A)
        // See TaskBox::Infer rendering above which shows partial_response during streaming

        // v0.8 UX: Focus indicators for Conversation panel
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

        // v0.8.1 FIX: Update total item count for scroll state BEFORE any scroll operations
        let total_items = items.len();
        self.conversation_scroll.total = total_items;

        // v0.16.4 FIX: Snap to actual bottom when user was at bottom
        // This prevents jump caused by estimated vs actual total mismatch in auto_scroll_to_bottom()
        // The estimated total (messages.len() * 4) differs from actual total (items.len())
        let visible_count = viewport_height;
        if self.user_at_bottom && total_items > visible_count {
            self.conversation_scroll.offset = total_items.saturating_sub(visible_count);
            self.conversation_scroll.cursor = total_items.saturating_sub(1);
        }

        // v0.8.1 FIX (NovaNet pattern): Apply scroll using .skip().take() directly
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

        // v0.8.2 UX: Render custom ScrollIndicator with dynamic arrows
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

    /// v0.9 Phase 3: Render search bar when in search mode
    fn render_search_bar(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        use ratatui::widgets::{Block, Borders, Paragraph};

        // Build search content with match count
        let match_info = if self.search_results.is_empty() {
            if self.search_query.is_empty() {
                String::new()
            } else {
                " (no matches)".to_string()
            }
        } else {
            format!(
                " ({}/{})",
                self.search_current + 1,
                self.search_results.len()
            )
        };

        let search_text = format!("🔍 {}{}", self.search_query, match_info);

        let block = Block::default()
            .title(" Search (Esc to close) ")
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.highlight));

        let search_color = if self.search_results.is_empty() && !self.search_query.is_empty() {
            theme.status_failed // Red for no matches
        } else {
            theme.text_primary
        };

        let paragraph = Paragraph::new(search_text)
            .style(Style::default().fg(search_color))
            .block(block);

        frame.render_widget(paragraph, area);
    }
}
