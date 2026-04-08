// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent Reasoning panel rendering (Panel 4)
//!
//! Agent turn list and thinking content viewer.
//! Tab-aware rendering:
//! - Turns tab: agent turn list with thinking previews
//! - Thinking tab: full thinking content for selected turn
//! - Steps tab: step-by-step breakdown of agent reasoning

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::MonitorView;
use crate::focus::PanelId;
use crate::state::TuiState;
use crate::theme::Theme;
use crate::unicode::truncate_to_width;
use crate::views::ReasoningTab;
use crate::widgets::ScrollIndicator;

impl MonitorView {
    /// Render Agent Reasoning panel (Panel 4)
    pub(super) fn render_agent_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        // Tab indicator in title
        let tab_indicator = state.ui.reasoning_tab.title();

        let block = Block::default()
            .title(format!(" ⊕ AGENT REASONING [{}] ", tab_indicator))
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
        match state.ui.reasoning_tab {
            ReasoningTab::Thinking => {
                self.render_agent_thinking(frame, inner_area, state, theme);
                return;
            }
            ReasoningTab::Steps => {
                self.render_agent_steps(frame, inner_area, state, theme);
                return;
            }
            ReasoningTab::Turns => {
                // Fall through to default agent turns list below
            }
        }

        // Build agent turn list with thinking display
        let items: Vec<ListItem> = state
            .agent
            .turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let tools = if turn.tool_calls.is_empty() {
                    "".to_string()
                } else {
                    format!(" → {}", turn.tool_calls.join(", "))
                };

                let tokens = turn
                    .tokens
                    .map(|t| format!(" [{}T]", t))
                    .unwrap_or_default();

                // Main turn line
                let main_line = Line::from(vec![
                    Span::styled(
                        format!("Turn {}: ", turn.index + 1),
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&turn.status, Style::default().fg(theme.text_primary)),
                    Span::styled(tools, Style::default().fg(theme.text_muted)),
                    Span::styled(tokens, Style::default().fg(theme.status_paused)),
                ]);

                // Build lines vec - main line plus optional thinking
                let mut lines = vec![main_line];

                // Add thinking content if present
                // Use unicode-aware truncation to avoid panic on multi-byte chars
                if let Some(ref thinking) = turn.thinking {
                    let truncated = truncate_to_width(thinking, 100);
                    lines.push(Line::from(vec![
                        Span::styled("  💭 ", Style::default().fg(theme.status_paused)),
                        Span::styled(
                            truncated,
                            Style::default()
                                .fg(theme.text_muted)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }

                ListItem::new(Text::from(lines)).style(
                    if i == self.scroll_offset(PanelId::RunnerReasoning) && focused {
                        Style::default().bg(theme.highlight)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect();

        if items.is_empty() {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                "  No agent activity",
                Style::default().fg(theme.text_muted),
            )]));
            frame.render_widget(empty, inner_area);
        } else {
            let total_items = items.len();
            let visible_count = inner_area.height as usize;
            let list = List::new(items);
            frame.render_widget(list, inner_area);

            // Scroll indicator when agent turns overflow panel
            if total_items > visible_count {
                let scroll_offset = self.scroll_offset(PanelId::RunnerReasoning);
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

    /// Render Agent Thinking tab
    /// Shows full thinking/reasoning content for selected turn
    pub(super) fn render_agent_thinking(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        let selected_idx = self.scroll_offset(PanelId::RunnerReasoning);
        let turn = state.agent.turns.get(selected_idx);

        let content = if let Some(turn) = turn {
            if let Some(ref thinking) = turn.thinking {
                format!("─── Turn {} Thinking ───\n{}", turn.index + 1, thinking)
            } else {
                format!("Turn {} has no thinking content", turn.index + 1)
            }
        } else {
            "No agent turn selected".to_string()
        };

        let total_lines = content.lines().count();
        let visible_count = area.height as usize;
        let scroll_offset = self.scroll[Self::panel_index(PanelId::RunnerReasoning)];

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .scroll((scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);

        // Scroll indicator when thinking content overflows
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

    /// Render Agent Steps tab
    /// Shows step-by-step breakdown of agent reasoning
    pub(super) fn render_agent_steps(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        let selected_idx = self.scroll_offset(PanelId::RunnerReasoning);
        let turn = state.agent.turns.get(selected_idx);

        let content = if let Some(turn) = turn {
            let mut steps = format!("─── Turn {} Steps ───\n", turn.index + 1);
            steps.push_str(&format!("Status: {}\n", turn.status));

            if !turn.tool_calls.is_empty() {
                steps.push_str("\nTool Calls:\n");
                for (i, tool) in turn.tool_calls.iter().enumerate() {
                    steps.push_str(&format!("  {}. {}\n", i + 1, tool));
                }
            }

            if let Some(tokens) = turn.tokens {
                steps.push_str(&format!("\nTokens: {}\n", tokens));
            }

            steps
        } else {
            "No agent turn selected".to_string()
        };

        let total_lines = content.lines().count();
        let visible_count = area.height as usize;
        let scroll_offset = self.scroll[Self::panel_index(PanelId::RunnerReasoning)];

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .scroll((scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);

        // Scroll indicator when steps content overflows
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
