// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Search bar rendering for chat conversation view.

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::theme::Theme;
use crate::views::chat::ChatView;

impl ChatView {
    /// Render search bar when in search mode
    pub(in crate::views::chat) fn render_search_bar(
        &self,
        frame: &mut Frame,
        area: Rect,
        theme: &Theme,
    ) {
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
