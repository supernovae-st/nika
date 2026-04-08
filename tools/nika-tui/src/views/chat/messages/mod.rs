// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Message Rendering for Chat View
//!
//! Contains the `render_messages_v2` method and related helpers, split into
//! focused submodules:
//!
//! - `colors` -- Theme-derived color palette (`RenderColors`)
//! - `content` -- Message header, body, selection, verb boxes, thinking, execution
//! - `inline` -- Inline MCP call and Infer stream box rendering
//! - `task_boxes` -- TaskBox rendering (Invoke, Infer, Exec, Fetch, Agent)
//! - `agent_phase` -- Agent phase indicator box
//! - `search_bar` -- Search bar widget

mod agent_phase;
pub(in crate::views::chat) mod colors;
mod content;
mod inline;
mod search_bar;
mod task_boxes;

use ratatui::{
    layout::Rect,
    style::Style,
    widgets::{Block, Borders, List},
    Frame,
};

use self::colors::RenderColors;
use self::content::SelectionContext;
use super::{ChatLinePosition, ChatPanel, ChatView};
use crate::theme::Theme;
use crate::widgets::ScrollIndicator;

// ═══════════════════════════════════════════════════════════════════════════════
// Message Rendering Methods
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Render messages v2 with inline MCP/Infer boxes.
    ///
    /// This is the main entry point. It:
    /// 1. Optionally renders the search bar
    /// 2. Builds line-position cache for mouse hit testing
    /// 3. Delegates to sub-renderers for messages, inline content, task boxes, and agent phase
    /// 4. Applies scroll, renders the list widget, and draws a scroll indicator
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

        // Extract theme colors
        let colors = RenderColors::from_theme(theme);

        // Calculate content width for word wrapping
        // area.width - 2 (borders) - 2 ("│ " prefix) = available text width
        let content_width = area.width.saturating_sub(4) as usize;

        // Update visible count based on actual viewport height (minus borders)
        let viewport_height = area.height.saturating_sub(2) as usize; // -2 for borders
        self.conversation_scroll.visible = viewport_height;

        // Text Selection: Build line positions cache for mouse hit testing
        self.build_line_positions(area);

        // Text Selection: Extract selection state for use in rendering
        let sel_ctx = SelectionContext {
            selection: self.text_selection.clone(),
            selection_bg: colors.selection_bg,
        };

        // ── Phase 1: Build message items (CACHED) ──────────────────────────
        // PERF: Only rebuild when data actually changes — saves 80-90% render CPU
        let msg_count = self.messages.len();
        let streaming_len = self.messages.last().map(|m| m.content.len()).unwrap_or(0);
        let needs_rebuild = self.cached_msg_dirty
            || self.cached_msg_count != msg_count
            || self.cached_msg_width != content_width
            || (self.is_streaming && self.cached_streaming_len != streaming_len);

        if needs_rebuild {
            self.cached_msg_dirty = false;
            self.cached_msg_items =
                self.build_message_items(theme, &colors, content_width, &sel_ctx);
            self.cached_base_len = self.cached_msg_items.len();
            self.cached_msg_count = msg_count;
            self.cached_msg_width = content_width;
            self.cached_streaming_len = streaming_len;
        }

        // PERF: Build render items by extending from cache instead of clone().
        // Phase 1 (messages) is cached. Phases 2-4 are rebuilt each frame into
        // a separate vec that borrows the cached items via drain/extend.
        // This avoids deep-cloning hundreds of ListItem<'static> every frame.
        let mut items = Vec::with_capacity(self.cached_base_len + self.inline_content.len() + 8);
        // Extend from cache: ListItem is Clone but we only copy the phase-1 items
        // once per cache rebuild (not every frame). When cache is valid, this is
        // a shallow copy of the Vec (Arcs inside Spans are cheap to clone).
        items.extend(self.cached_msg_items.iter().cloned());

        // ── Phase 2: Render inline content (MCP calls, Infer streams) ────────
        inline::render_inline_content(&self.inline_content, &colors, &mut items);

        // ── Phase 3: Render task boxes (Invoke, Infer, Exec, Fetch, Agent) ───
        self.render_task_boxes(&colors, &mut items);

        // ── Phase 4: Render agent phase indicator ────────────────────────────
        self.render_agent_phase_indicator(theme, content_width, &mut items);

        // ── Phase 5: Finalize list widget with scroll ────────────────────────
        self.render_conversation_list(frame, area, theme, items, viewport_height);
    }

    /// Build line-position cache for text selection mouse hit testing.
    /// PERF: Only runs when selection is active (saves O(messages*lines) per frame)
    fn build_line_positions(&mut self, area: Rect) {
        // Guard: skip entirely when no selection is in progress
        if !self.is_selecting && self.text_selection.is_none() {
            self.line_positions.clear();
            return;
        }
        self.line_positions.clear();
        let content_start_x = area.x + 3; // "│ " prefix = 2 chars + border
        let mut current_line = 0usize;

        for (msg_idx, msg) in self.messages.iter().enumerate() {
            current_line += 1; // Header line (e.g., "👤 You ────")

            for (line_idx, line_text) in msg.content.lines().enumerate() {
                let line_in_list = current_line;
                let scroll_offset = self.conversation_scroll.offset;

                if line_in_list >= scroll_offset {
                    let screen_y = area.y + 1 + (line_in_list - scroll_offset) as u16;
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
                    current_line += 1; // Collapsed indicator
                }
            }

            // Account for execution result if present
            if msg.execution.is_some() {
                current_line += 1;
            }

            current_line += 1; // Spacing line
        }
    }

    /// Render the final conversation list widget with scroll handling.
    fn render_conversation_list(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
        items: Vec<ratatui::widgets::ListItem>,
        viewport_height: usize,
    ) {
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
        let visible_count = viewport_height;
        if self.user_at_bottom && total_items > visible_count {
            self.conversation_scroll.offset = total_items.saturating_sub(visible_count);
            self.conversation_scroll.cursor = total_items.saturating_sub(1);
        }

        // (NovaNet pattern): Apply scroll using .skip().take() directly
        let scroll_offset = self.conversation_scroll.offset;

        // Clamp offset to valid range
        let clamped_offset = if total_items > visible_count {
            scroll_offset.min(total_items.saturating_sub(visible_count))
        } else {
            0
        };
        self.conversation_scroll.offset = clamped_offset;

        // Apply scroll - only show visible lines
        let visible_items: Vec<ratatui::widgets::ListItem> = items
            .into_iter()
            .skip(clamped_offset)
            .take(visible_count)
            .collect();

        let list = List::new(visible_items).block(block);
        frame.render_widget(list, area);

        // UX: Render custom ScrollIndicator with dynamic arrows
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
