//! Help View - Keyboard shortcuts reference, command guide
//!
//! Layout:
//! ```text
//! ╭───────────────────────────────────────────────────────────────────────────────╮
//! │  HELP                                                      [Esc] Back         │
//! ├───────────────────────────────────────────────────────────────────────────────┤
//! │                                                                               │
//! │  ┌─ GLOBAL ──────────────────────────────────────────────────────────────┐    │
//! │  │  Ctrl+C       Copy / Quit (2x)                                        │    │
//! │  │  ?            Help                                                    │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! │  ┌─ VIEW NAVIGATION ─────────────────────────────────────────────────────┐    │
//! │  │  1            Chat view                                               │    │
//! │  │  2            Home view                                               │    │
//! │  │  3            Studio view                                             │    │
//! │  │  4            Monitor view                                            │    │
//! │  └───────────────────────────────────────────────────────────────────────┘    │
//! │                                                                               │
//! │  [j/↓] Down  [k/↑] Up  [/] Search  [Esc] Back                                │
//! │                                                                               │
//! ╰───────────────────────────────────────────────────────────────────────────────╯
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use super::view_trait::View;
use super::{TuiView, ViewAction};
use crate::tui::keybindings::{format_key, keybindings_for_context, KeyCategory, Keybinding};
use crate::tui::mode::InputMode;
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Help view state
pub struct HelpView {
    /// Current scroll offset
    pub scroll_offset: u16,
    /// Total lines in content
    pub total_lines: u16,
}

impl HelpView {
    /// Create a new HelpView
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            total_lines: 0,
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        if self.scroll_offset < self.total_lines.saturating_sub(1) {
            self.scroll_offset += 1;
        }
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll to top
    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
    }

    /// Scroll to bottom
    pub fn scroll_to_bottom(&mut self) {
        self.scroll_offset = self.total_lines.saturating_sub(1);
    }

    /// Page down
    pub fn page_down(&mut self, visible_lines: u16) {
        self.scroll_offset =
            (self.scroll_offset + visible_lines).min(self.total_lines.saturating_sub(1));
    }

    /// Page up
    pub fn page_up(&mut self, visible_lines: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(visible_lines);
    }

    /// Get keybindings grouped by category
    fn get_grouped_keybindings(&self) -> Vec<(KeyCategory, Vec<Keybinding>)> {
        // Get all keybindings from Chat view (most comprehensive)
        let bindings = keybindings_for_context(TuiView::Chat, InputMode::Normal);

        // Group by category
        let mut groups: Vec<(KeyCategory, Vec<Keybinding>)> = vec![
            (KeyCategory::Global, vec![]),
            (KeyCategory::ViewNav, vec![]),
            (KeyCategory::PanelNav, vec![]),
            (KeyCategory::Mode, vec![]),
            (KeyCategory::Scroll, vec![]),
            (KeyCategory::Action, vec![]),
            (KeyCategory::Chat, vec![]),
            (KeyCategory::Monitor, vec![]),
        ];

        for binding in bindings {
            for (category, bindings_list) in groups.iter_mut() {
                if *category == binding.category {
                    bindings_list.push(binding.clone());
                    break;
                }
            }
        }

        // Filter out empty categories
        groups.into_iter().filter(|(_, b)| !b.is_empty()).collect()
    }

    /// Build help content lines
    fn build_content_lines(&self, theme: &Theme) -> Vec<Line<'static>> {
        let mut lines: Vec<Line<'static>> = vec![];
        let groups = self.get_grouped_keybindings();

        for (category, bindings) in groups {
            // Category header
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  ── {} ", category.label()),
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("─".repeat(50), Style::default().fg(theme.border_normal)),
            ]));
            lines.push(Line::from(""));

            // Keybindings
            for binding in bindings {
                let key_str = format_key(binding.code, binding.modifiers);
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default()),
                    Span::styled(
                        format!("{:<14}", key_str),
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        binding.description.to_string(),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));
            }
            lines.push(Line::from(""));
        }

        // Additional help sections
        lines.push(Line::from(vec![
            Span::styled(
                "  ── TIPS ",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("─".repeat(50), Style::default().fg(theme.border_normal)),
        ]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("Press ".to_string(), Style::default().fg(theme.text_muted)),
            Span::styled("i".to_string(), Style::default().fg(theme.highlight)),
            Span::styled(
                " to enter Insert mode for typing".to_string(),
                Style::default().fg(theme.text_muted),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled("Press ".to_string(), Style::default().fg(theme.text_muted)),
            Span::styled("Esc".to_string(), Style::default().fg(theme.highlight)),
            Span::styled(
                " to return to Normal mode".to_string(),
                Style::default().fg(theme.text_muted),
            ),
        ]));
        lines.push(Line::from(vec![
            Span::styled("    ", Style::default()),
            Span::styled(
                "Number keys ".to_string(),
                Style::default().fg(theme.text_muted),
            ),
            Span::styled("1-4".to_string(), Style::default().fg(theme.highlight)),
            Span::styled(
                " switch views in Normal mode".to_string(),
                Style::default().fg(theme.text_muted),
            ),
        ]));
        lines.push(Line::from(""));

        lines
    }
}

impl Default for HelpView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for HelpView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(1),    // Content
                Constraint::Length(1), // Footer
            ])
            .split(area);

        // Build content
        let content_lines = self.build_content_lines(theme);
        self.total_lines = content_lines.len() as u16;

        // Content area with scroll
        let visible_height = chunks[0].height.saturating_sub(2); // Account for borders
        let content = Paragraph::new(content_lines)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.border_normal))
                    .title(" Keyboard Shortcuts ")
                    .title_style(
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
            )
            .scroll((self.scroll_offset, 0))
            .style(Style::default().fg(theme.text_primary));

        frame.render_widget(content, chunks[0]);

        // Scrollbar
        if self.total_lines > visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("↑"))
                .end_symbol(Some("↓"));
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(self.total_lines as usize)
                .position(self.scroll_offset as usize);
            frame.render_stateful_widget(
                scrollbar,
                chunks[0].inner(ratatui::layout::Margin {
                    vertical: 1,
                    horizontal: 0,
                }),
                &mut scrollbar_state,
            );
        }

        // Footer
        let footer = Paragraph::new(Line::from(vec![
            Span::styled("[j/↓]", Style::default().fg(theme.highlight)),
            Span::styled(" Down  ", Style::default().fg(theme.text_muted)),
            Span::styled("[k/↑]", Style::default().fg(theme.highlight)),
            Span::styled(" Up  ", Style::default().fg(theme.text_muted)),
            Span::styled("[g]", Style::default().fg(theme.highlight)),
            Span::styled(" Top  ", Style::default().fg(theme.text_muted)),
            Span::styled("[G]", Style::default().fg(theme.highlight)),
            Span::styled(" Bottom  ", Style::default().fg(theme.text_muted)),
            Span::styled("[Esc]", Style::default().fg(theme.highlight)),
            Span::styled(" Back", Style::default().fg(theme.text_muted)),
        ]))
        .style(Style::default().fg(theme.text_muted));
        frame.render_widget(footer, chunks[1]);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            // Escape returns to previous view (Home)
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Studio),

            // Scroll down
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down();
                ViewAction::None
            }

            // Scroll up
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up();
                ViewAction::None
            }

            // Page down
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page_down(10);
                ViewAction::None
            }
            KeyCode::PageDown => {
                self.page_down(10);
                ViewAction::None
            }

            // Page up
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.page_up(10);
                ViewAction::None
            }
            KeyCode::PageUp => {
                self.page_up(10);
                ViewAction::None
            }

            // Go to top
            KeyCode::Char('g') => {
                self.scroll_to_top();
                ViewAction::None
            }

            // Go to bottom
            KeyCode::Char('G') => {
                self.scroll_to_bottom();
                ViewAction::None
            }

            // ? opens Settings (from Help)
            KeyCode::Char('s') => ViewAction::SwitchView(TuiView::Settings),

            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        if self.total_lines > 0 {
            format!(
                "Help • Line {}/{}",
                self.scroll_offset + 1,
                self.total_lines
            )
        } else {
            "Help • Keyboard shortcuts reference".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_help_view_new() {
        let view = HelpView::new();
        assert_eq!(view.scroll_offset, 0);
        assert_eq!(view.total_lines, 0);
    }

    #[test]
    fn test_scroll_down() {
        let mut view = HelpView::new();
        view.total_lines = 100;
        view.scroll_down();
        assert_eq!(view.scroll_offset, 1);
    }

    #[test]
    fn test_scroll_down_at_bottom() {
        let mut view = HelpView::new();
        view.total_lines = 10;
        view.scroll_offset = 9;
        view.scroll_down();
        assert_eq!(view.scroll_offset, 9); // Stays at bottom
    }

    #[test]
    fn test_scroll_up() {
        let mut view = HelpView::new();
        view.scroll_offset = 5;
        view.scroll_up();
        assert_eq!(view.scroll_offset, 4);
    }

    #[test]
    fn test_scroll_up_at_top() {
        let mut view = HelpView::new();
        view.scroll_offset = 0;
        view.scroll_up();
        assert_eq!(view.scroll_offset, 0); // Stays at top
    }

    #[test]
    fn test_scroll_to_top() {
        let mut view = HelpView::new();
        view.scroll_offset = 50;
        view.scroll_to_top();
        assert_eq!(view.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut view = HelpView::new();
        view.total_lines = 100;
        view.scroll_to_bottom();
        assert_eq!(view.scroll_offset, 99);
    }

    #[test]
    fn test_page_down() {
        let mut view = HelpView::new();
        view.total_lines = 100;
        view.scroll_offset = 5;
        view.page_down(10);
        assert_eq!(view.scroll_offset, 15);
    }

    #[test]
    fn test_page_up() {
        let mut view = HelpView::new();
        view.scroll_offset = 15;
        view.page_up(10);
        assert_eq!(view.scroll_offset, 5);
    }

    #[test]
    fn test_status_line_with_content() {
        let mut view = HelpView::new();
        view.total_lines = 50;
        view.scroll_offset = 10;
        let state = TuiState::new("test");
        let status = view.status_line(&state);
        assert!(status.contains("11/50")); // Line numbers are 1-indexed
    }

    #[test]
    fn test_status_line_empty() {
        let view = HelpView::new();
        let state = TuiState::new("test");
        let status = view.status_line(&state);
        assert!(status.contains("Keyboard shortcuts"));
    }

    #[test]
    fn test_handle_key_escape_returns_studio() {
        let mut view = HelpView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Studio)));
    }

    #[test]
    fn test_handle_key_j_scrolls_down() {
        let mut view = HelpView::new();
        view.total_lines = 100;
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll_offset, 1);
    }

    #[test]
    fn test_handle_key_k_scrolls_up() {
        let mut view = HelpView::new();
        view.scroll_offset = 5;
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll_offset, 4);
    }

    #[test]
    fn test_handle_key_g_goes_to_top() {
        let mut view = HelpView::new();
        view.scroll_offset = 50;
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll_offset, 0);
    }

    #[test]
    fn test_handle_key_shift_g_goes_to_bottom() {
        let mut view = HelpView::new();
        view.total_lines = 100;
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll_offset, 99);
    }

    #[test]
    fn test_handle_key_s_goes_to_settings() {
        let mut view = HelpView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Settings)));
    }

    #[test]
    fn test_get_grouped_keybindings_not_empty() {
        let view = HelpView::new();
        let groups = view.get_grouped_keybindings();
        assert!(!groups.is_empty());
        // Should have at least Global category
        assert!(groups.iter().any(|(cat, _)| *cat == KeyCategory::Global));
    }
}
