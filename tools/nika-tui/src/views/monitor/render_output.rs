// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! NovaNet Station panel rendering (Panel 3)
//!
//! MCP call list and response viewer.
//! Tab-aware rendering:
//! - Summary tab: MCP call list with status icons
//! - FullJson tab: full JSON response of selected call

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::MonitorView;
use crate::focus::PanelId;
use crate::state::TuiState;
use crate::theme::Theme;
use crate::views::NovanetTab;
use crate::widgets::ScrollIndicator;

impl MonitorView {
    /// Render NovaNet Station panel (Panel 3)
    pub(super) fn render_novanet_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        // Tab indicator in title
        let tab_indicator = state.ui.novanet_tab.title();

        let block = Block::default()
            .title(format!(" ⊛ NOVANET STATION [{}] ", tab_indicator))
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused));

        // Render block first
        let inner_area = block.inner(area);
        frame.render_widget(block.clone(), area);

        // Tab-aware content rendering
        match state.ui.novanet_tab {
            NovanetTab::FullJson => {
                self.render_novanet_full_json(frame, inner_area, state, theme);
                return;
            }
            NovanetTab::Summary => {
                // Fall through to default MCP call list below
            }
        }

        // Build MCP call list
        let items: Vec<ListItem> = state
            .mcp
            .calls
            .iter()
            .enumerate()
            .map(|(i, call)| {
                let icon = if call.is_error {
                    "✗"
                } else if call.completed {
                    "✓"
                } else {
                    "►"
                };

                let tool_name = call.tool.as_deref().unwrap_or("resource");
                let duration = call
                    .duration_ms
                    .map(|d| format!(" {}ms", d))
                    .unwrap_or_default();

                let style = if call.is_error {
                    Style::default().fg(theme.status_failed)
                } else if call.completed {
                    Style::default().fg(theme.status_success)
                } else {
                    Style::default().fg(theme.highlight)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("[{}] ", icon), style),
                    Span::styled(tool_name, Style::default().fg(theme.text_primary)),
                    Span::styled(duration, Style::default().fg(theme.text_muted)),
                ]))
                .style(
                    if i == self.scroll_offset(PanelId::RunnerNovanet) && focused {
                        Style::default().bg(theme.highlight)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect();

        if items.is_empty() {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                "  No MCP calls yet",
                Style::default().fg(theme.text_muted),
            )]));
            frame.render_widget(empty, inner_area);
        } else {
            let total_items = items.len();
            let visible_count = inner_area.height as usize;
            let list = List::new(items);
            frame.render_widget(list, inner_area);

            // Scroll indicator when MCP calls overflow panel
            if total_items > visible_count {
                let scroll_offset = self.scroll_offset(PanelId::RunnerNovanet);
                let scrollbar_area = Rect {
                    x: inner_area.x + inner_area.width.saturating_sub(1),
                    y: inner_area.y,
                    width: 1,
                    height: inner_area.height,
                };
                let indicator = ScrollIndicator::new()
                    .position(scroll_offset, total_items, visible_count)
                    .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                    .track_style(Style::default().fg(theme.scrollbar_track))
                    .show_arrows(true);
                frame.render_widget(indicator, scrollbar_area);
            }
        }
    }

    /// Render NovaNet FullJson tab
    /// Shows full JSON response of selected MCP call (uses cached JSON)
    pub(super) fn render_novanet_full_json(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        let selected_idx = self.scroll_offset(PanelId::RunnerNovanet);
        let call = state.mcp.calls.get(selected_idx);

        let content = if let Some(call) = call {
            let tool_name = call.tool.as_deref().unwrap_or("resource");
            // Use cached JSON (refreshed before render)
            format!("─── {} ───\n{}", tool_name, &self.cached_mcp_response_json)
        } else {
            "No MCP call selected".to_string()
        };

        let total_lines = content.lines().count();
        let visible_count = area.height as usize;
        let scroll_offset = self.scroll[Self::panel_index(PanelId::RunnerNovanet)];

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .scroll((scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);

        // Scroll indicator when JSON content overflows
        if total_lines > visible_count {
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y,
                width: 1,
                height: area.height,
            };
            let indicator = ScrollIndicator::new()
                .position(scroll_offset, total_lines, visible_count)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);
            frame.render_widget(indicator, scrollbar_area);
        }
    }
}
