//! Provider Modal v2
//!
//! Rich tabbed modal for provider management:
//! - Cloud providers with cards and sparklines
//! - Ollama local models with install/pull
//! - API key management via system keychain
//! - Configuration preferences

mod components;
mod keyring;
mod ollama_client;
mod state;
mod tabs;

pub use components::*;
pub use keyring::*;
pub use ollama_client::*;
pub use state::*;
pub use tabs::*;

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::Span,
    widgets::{Block, BorderType, Borders, Clear, Tabs, Widget},
};

/// Main provider modal widget
pub struct ProviderModal<'a> {
    state: &'a ProviderModalState,
}

impl<'a> ProviderModal<'a> {
    pub fn new(state: &'a ProviderModalState) -> Self {
        Self { state }
    }

    fn render_tabs(&self, area: Rect, buf: &mut Buffer) {
        let tab_titles: Vec<Span> = vec![
            Span::raw(ProviderModalTab::Cloud.label()),
            Span::raw(ProviderModalTab::Ollama.label()),
            Span::raw(ProviderModalTab::Keys.label()),
            Span::raw(ProviderModalTab::Config.label()),
        ];

        let tabs = Tabs::new(tab_titles)
            .select(self.state.active_tab as usize)
            .style(Style::default().fg(Color::Rgb(156, 163, 175)))
            .highlight_style(
                Style::default()
                    .fg(Color::Rgb(99, 102, 241))
                    .add_modifier(Modifier::BOLD),
            )
            .divider(Span::raw("│"));

        tabs.render(area, buf);
    }

    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let hints = match self.state.active_tab {
            ProviderModalTab::Cloud => "[↑↓] Navigate │ [Enter] Select │ [Tab] Next │ [Esc] Close",
            ProviderModalTab::Ollama => "[↑↓] Navigate │ [p] Pull │ [d] Delete │ [Esc] Close",
            ProviderModalTab::Keys => "[↑↓] Navigate │ [Enter] Edit │ [t] Test │ [Esc] Close",
            ProviderModalTab::Config => "[↑↓] Navigate │ [Enter] Toggle │ [Esc] Close",
        };

        buf.set_string(
            area.x + 1,
            area.y,
            hints,
            Style::default().fg(Color::Rgb(107, 114, 128)),
        );
    }
}

impl Widget for ProviderModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if !self.state.visible {
            return;
        }

        // Calculate modal size (80% width, 70% height, centered)
        let modal_width = (area.width * 80 / 100).min(100).max(50);
        let modal_height = (area.height * 70 / 100).min(30).max(15);
        let modal_x = (area.width - modal_width) / 2 + area.x;
        let modal_y = (area.height - modal_height) / 2 + area.y;

        let modal_area = Rect::new(modal_x, modal_y, modal_width, modal_height);

        // Clear background
        Clear.render(modal_area, buf);

        // Main border
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::Rgb(99, 102, 241)))
            .style(Style::default().bg(Color::Rgb(17, 24, 39)))
            .title(Span::styled(
                " PROVIDERS ",
                Style::default()
                    .fg(Color::Rgb(229, 231, 235))
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(modal_area);
        block.render(modal_area, buf);

        // Layout: tabs | content | footer
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Tabs
                Constraint::Min(5),    // Content
                Constraint::Length(1), // Footer
            ])
            .split(inner);

        // Render tabs
        self.render_tabs(chunks[0], buf);

        // Render tab content
        match self.state.active_tab {
            ProviderModalTab::Cloud => CloudTab.render(chunks[1], buf),
            ProviderModalTab::Ollama => OllamaTab.render(chunks[1], buf),
            ProviderModalTab::Keys => KeysTab.render(chunks[1], buf),
            ProviderModalTab::Config => {
                buf.set_string(
                    chunks[1].x + 2,
                    chunks[1].y + 1,
                    "Configuration coming soon...",
                    Style::default().fg(Color::Rgb(156, 163, 175)),
                );
            }
        }

        // Render footer
        self.render_footer(chunks[2], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::buffer::Buffer;
    use ratatui::layout::Rect;

    #[test]
    fn test_modal_hidden_does_not_render() {
        let state = ProviderModalState::default(); // visible: false
        let modal = ProviderModal::new(&state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        modal.render(Rect::new(0, 0, 80, 24), &mut buf);

        // All cells should be empty space
        assert!(buf.content().iter().all(|c| c.symbol() == " "));
    }

    #[test]
    fn test_modal_visible_renders_border() {
        let mut state = ProviderModalState::default();
        state.visible = true;
        let modal = ProviderModal::new(&state);

        let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
        modal.render(Rect::new(0, 0, 80, 24), &mut buf);

        // Should render PROVIDERS title
        let content: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(content.contains("PROVIDERS"));
    }
}
