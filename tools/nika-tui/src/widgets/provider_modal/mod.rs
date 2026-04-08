// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Provider Modal v2
//!
//! Rich tabbed modal for provider management:
//! - Cloud providers with cards and sparklines
//! - Native local models via mistral.rs
//! - API key management via encrypted vault
//! - Configuration preferences

mod components;
mod handler;
mod loader;
mod provider_checker;
mod state;
mod tabs;

pub use components::*;
pub use handler::*;
pub use loader::*;
pub use nika_engine::secrets::{
    mask_api_key, migrate_env_to_vault, validate_key_format, KeyringError, MigrationReport,
};

// Re-export provider_env_var from unified providers module
pub use crate::providers::env_var as provider_env_var;
pub use provider_checker::*;
pub use state::*;
pub use tabs::*;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Clear, Tabs, Widget},
};

use super::super::theme::Theme;

/// Main provider modal widget
pub struct ProviderModal<'a> {
    state: &'a mut ProviderModalState,
    theme: &'a Theme,
}

impl<'a> ProviderModal<'a> {
    pub fn new(state: &'a mut ProviderModalState, theme: &'a Theme) -> Self {
        Self { state, theme }
    }

    fn render_tabs(&mut self, area: Rect, buf: &mut Buffer) {
        // All tabs now have dynamic labels with live info
        let cloud_label = self.state.cloud_tab_label();
        let native_label = self.state.native_tab_label();
        let keys_label = self.state.keys_tab_label();

        let tab_titles: Vec<Span> = vec![
            Span::raw(cloud_label),
            Span::raw(native_label),
            Span::raw(keys_label),
            Span::raw(ProviderModalTab::Config.label()),
        ];

        let tabs = Tabs::new(tab_titles)
            .select(self.state.active_tab as usize)
            .style(Style::default().fg(self.theme.text_secondary))
            .highlight_style(
                Style::default()
                    .fg(self.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .divider(Span::raw("│"));

        tabs.render(area, buf);
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let hints = match self.state.active_tab {
            ProviderModalTab::Cloud => {
                "[hjkl] Navigate │ [Enter] Select │ [Tab] Next │ [Esc] Close"
            }
            ProviderModalTab::Native => "[jk] Navigate │ [Esc] Close",
            ProviderModalTab::Keys => "[jk] Navigate │ [Enter] Edit │ [t] Test │ [Esc] Close",
            ProviderModalTab::Config => "[jk] Navigate │ [Enter] Toggle │ [Esc] Close",
        };

        // Render hints on the left
        buf.set_string(
            area.x + 1,
            area.y,
            hints,
            Style::default().fg(self.theme.text_muted),
        );

        // Render session stats on the right
        let stats = self.state.get_session_stats();
        FooterStatsBar::new(&stats).render(area, buf);
    }
}

impl Widget for ProviderModal<'_> {
    fn render(mut self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate modal size (80% width, 70% height, centered)
        let modal_width = (area.width * 80 / 100).clamp(50, 100);
        let modal_height = (area.height * 70 / 100).clamp(15, 30);
        let modal_x = (area.width - modal_width) / 2 + area.x;
        let modal_y = (area.height - modal_height) / 2 + area.y;

        let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

        // Clear background
        Clear.render(modal_area, buf);

        // Main border (no title - we use AnimatedHeader)
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(self.theme.highlight))
            .style(Style::default().bg(self.theme.background));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Layout: header | tabs | content | footer
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Header (AnimatedHeader)
                Constraint::Length(1), // Tabs
                Constraint::Min(5),    // Content
                Constraint::Length(1), // Footer
            ])
            .split(inner);

        // Render animated cosmic header
        let header = AnimatedHeader::new()
            .with_title("PROVIDERS")
            .with_frame(self.state.animation_frame);
        header.render(chunks[0], buf);

        // Render tabs
        self.render_tabs(chunks[1], buf);

        // Render tab content
        match self.state.active_tab {
            ProviderModalTab::Cloud => {
                // Use live provider statuses from state
                let statuses = self.state.get_provider_statuses();
                CloudTab::with_statuses(self.state, statuses).render(chunks[2], buf);
            }
            ProviderModalTab::Native => {
                // Use live native models from state
                NativeTab::new(
                    &self.state.native_models,
                    self.state.selected_idx,
                    &self.state.download_state,
                )
                .available(self.state.native_available)
                .render(chunks[2], buf);
            }
            ProviderModalTab::Keys => {
                KeysTab::new(
                    self.state.selected_idx,
                    self.state.key_input_mode,
                    &self.state.key_input_buffer,
                )
                .render(chunks[2], buf);
            }
            ProviderModalTab::Config => {
                ConfigTab::new(self.state.selected_idx).render(chunks[2], buf);
            }
        }

        // Render footer
        self.render_footer(chunks[3], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_modal_hidden_does_not_render() {
        let mut state = ProviderModalState::default(); // visible: false
        let theme = Theme::default();
        let modal = ProviderModal::new(&mut state, &theme);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        modal.render(Rect::new(0, 0, 80, 24), &mut buf);

        // All cells should be empty space
        assert!(buf.content().iter().all(|c| c.symbol() == " "));
    }

    #[test]
    fn test_modal_visible_renders_border() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        let theme = Theme::default();
        let modal = ProviderModal::new(&mut state, &theme);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        modal.render(Rect::new(0, 0, 80, 24), &mut buf);

        // Should render PROVIDERS title
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("PROVIDERS"));
    }
}
