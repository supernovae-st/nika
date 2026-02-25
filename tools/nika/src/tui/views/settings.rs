//! Settings View - Provider configuration, theme, preferences (v0.11.1)
//!
//! Layout:
//! ```text
//! ╭───────────────────────────────────────────────────────────────────────────────╮
//! │  SETTINGS                                                    [Esc] Back       │
//! ├───────────────────────────────────────────────────────────────────────────────┤
//! │                                                                               │
//! │  ┌─ PROVIDER ─────────────────────────────────────────────────────────────┐   │
//! │  │  Current: Claude (claude-sonnet-4-6)                                   │   │
//! │  │  [Enter] Configure  [Tab] Next section                                 │   │
//! │  └────────────────────────────────────────────────────────────────────────┘   │
//! │                                                                               │
//! │  ┌─ THEME ────────────────────────────────────────────────────────────────┐   │
//! │  │  Current: Dark                                                         │   │
//! │  │  [1] Light  [2] Dark  [3] Solarized                                    │   │
//! │  └────────────────────────────────────────────────────────────────────────┘   │
//! │                                                                               │
//! │  ┌─ KEYBOARD SHORTCUTS ───────────────────────────────────────────────────┐   │
//! │  │  [?] Help view for full reference                                      │   │
//! │  └────────────────────────────────────────────────────────────────────────┘   │
//! │                                                                               │
//! ╰───────────────────────────────────────────────────────────────────────────────╯
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::{TuiView, ViewAction};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Settings section for navigation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsSection {
    #[default]
    Provider,
    Theme,
    Shortcuts,
}

impl SettingsSection {
    /// Get next section
    pub fn next(&self) -> Self {
        match self {
            SettingsSection::Provider => SettingsSection::Theme,
            SettingsSection::Theme => SettingsSection::Shortcuts,
            SettingsSection::Shortcuts => SettingsSection::Provider,
        }
    }

    /// Get previous section
    pub fn prev(&self) -> Self {
        match self {
            SettingsSection::Provider => SettingsSection::Shortcuts,
            SettingsSection::Theme => SettingsSection::Provider,
            SettingsSection::Shortcuts => SettingsSection::Theme,
        }
    }

    /// Get section title
    pub fn title(&self) -> &'static str {
        match self {
            SettingsSection::Provider => "PROVIDER",
            SettingsSection::Theme => "THEME",
            SettingsSection::Shortcuts => "KEYBOARD SHORTCUTS",
        }
    }
}

/// Settings view state
pub struct SettingsView {
    /// Currently selected section
    pub section: SettingsSection,
    /// Provider name (cached for display)
    pub provider_name: String,
    /// Model name (cached for display)
    pub model_name: String,
    /// Current theme name
    pub theme_name: String,
}

impl SettingsView {
    /// Create a new SettingsView
    pub fn new() -> Self {
        Self {
            section: SettingsSection::Provider,
            provider_name: "Auto-detect".to_string(),
            model_name: "".to_string(),
            theme_name: "Dark".to_string(),
        }
    }

    /// Update provider info from state
    pub fn update_provider(&mut self, provider: &str, model: &str) {
        self.provider_name = provider.to_string();
        self.model_name = model.to_string();
    }

    /// Update theme name from theme mode
    pub fn update_theme_name(&mut self, name: &str) {
        self.theme_name = name.to_string();
    }

    /// Render a settings section box
    fn render_section(
        &self,
        frame: &mut Frame,
        area: Rect,
        section: SettingsSection,
        content: Vec<Line<'_>>,
        theme: &Theme,
    ) {
        let is_selected = self.section == section;
        let border_style = if is_selected {
            Style::default()
                .fg(theme.highlight)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style)
            .title(format!(" {} ", section.title()))
            .title_style(if is_selected {
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(theme.text_muted)
            });

        let paragraph = Paragraph::new(content)
            .block(block)
            .style(Style::default().fg(theme.text_primary));

        frame.render_widget(paragraph, area);
    }
}

impl Default for SettingsView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for SettingsView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Layout: 3 sections with equal height
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Length(5), // Provider section
                Constraint::Length(5), // Theme section
                Constraint::Length(5), // Shortcuts section
                Constraint::Min(0),    // Remaining space
            ])
            .split(area);

        // Provider section
        let provider_content = vec![
            Line::from(vec![
                Span::styled("  Current: ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    &self.provider_name,
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
                if !self.model_name.is_empty() {
                    Span::styled(
                        format!(" ({})", self.model_name),
                        Style::default().fg(theme.text_muted),
                    )
                } else {
                    Span::raw("")
                },
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("[Ctrl+P]", Style::default().fg(theme.highlight)),
                Span::styled(" Configure provider", Style::default().fg(theme.text_muted)),
            ]),
        ];
        self.render_section(frame, chunks[0], SettingsSection::Provider, provider_content, theme);

        // Theme section
        let theme_content = vec![
            Line::from(vec![
                Span::styled("  Current: ", Style::default().fg(theme.text_muted)),
                Span::styled(
                    &self.theme_name,
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ),
            ]),
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("[1]", Style::default().fg(theme.highlight)),
                Span::styled(" Light  ", Style::default().fg(theme.text_muted)),
                Span::styled("[2]", Style::default().fg(theme.highlight)),
                Span::styled(" Dark  ", Style::default().fg(theme.text_muted)),
                Span::styled("[3]", Style::default().fg(theme.highlight)),
                Span::styled(" Solarized", Style::default().fg(theme.text_muted)),
            ]),
        ];
        self.render_section(frame, chunks[1], SettingsSection::Theme, theme_content, theme);

        // Shortcuts section
        let shortcuts_content = vec![
            Line::from(vec![
                Span::styled("  Press ", Style::default().fg(theme.text_muted)),
                Span::styled("[?]", Style::default().fg(theme.highlight)),
                Span::styled(
                    " to view full keyboard shortcut reference",
                    Style::default().fg(theme.text_muted),
                ),
            ]),
            Line::from(vec![
                Span::styled("  Press ", Style::default().fg(theme.text_muted)),
                Span::styled("[Esc]", Style::default().fg(theme.highlight)),
                Span::styled(" to return to previous view", Style::default().fg(theme.text_muted)),
            ]),
        ];
        self.render_section(frame, chunks[2], SettingsSection::Shortcuts, shortcuts_content, theme);

        // Footer with navigation hints
        if chunks[3].height > 0 {
            let footer = Paragraph::new(Line::from(vec![
                Span::styled("[Tab]", Style::default().fg(theme.highlight)),
                Span::styled(" Next section  ", Style::default().fg(theme.text_muted)),
                Span::styled("[Shift+Tab]", Style::default().fg(theme.highlight)),
                Span::styled(" Previous  ", Style::default().fg(theme.text_muted)),
                Span::styled("[Esc]", Style::default().fg(theme.highlight)),
                Span::styled(" Back", Style::default().fg(theme.text_muted)),
            ]))
            .style(Style::default().fg(theme.text_muted));
            frame.render_widget(footer, chunks[3]);
        }
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            // Escape returns to Home
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Home),

            // Tab/Shift+Tab cycles sections
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.section = self.section.prev();
                } else {
                    self.section = self.section.next();
                }
                ViewAction::None
            }
            KeyCode::BackTab => {
                self.section = self.section.prev();
                ViewAction::None
            }

            // j/k for vim-style navigation
            KeyCode::Char('j') | KeyCode::Down => {
                self.section = self.section.next();
                ViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.section = self.section.prev();
                ViewAction::None
            }

            // Theme shortcuts
            KeyCode::Char('1') => ViewAction::ToggleTheme, // Will cycle to Light
            KeyCode::Char('2') => ViewAction::ToggleTheme, // Will cycle to Dark
            KeyCode::Char('3') => ViewAction::ToggleTheme, // Will cycle to Solarized

            // ? opens Help view
            KeyCode::Char('?') => ViewAction::SwitchView(TuiView::Help),

            // Enter on Provider opens modal (via chat's Ctrl+P)
            KeyCode::Enter if self.section == SettingsSection::Provider => {
                // Return None - app.rs will need to handle this specially
                ViewAction::OpenSettings // Reuse to trigger provider modal
            }

            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!("Settings • {} selected", self.section.title())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_settings_view_new() {
        let view = SettingsView::new();
        assert_eq!(view.section, SettingsSection::Provider);
        assert_eq!(view.provider_name, "Auto-detect");
    }

    #[test]
    fn test_section_next() {
        assert_eq!(SettingsSection::Provider.next(), SettingsSection::Theme);
        assert_eq!(SettingsSection::Theme.next(), SettingsSection::Shortcuts);
        assert_eq!(SettingsSection::Shortcuts.next(), SettingsSection::Provider);
    }

    #[test]
    fn test_section_prev() {
        assert_eq!(SettingsSection::Provider.prev(), SettingsSection::Shortcuts);
        assert_eq!(SettingsSection::Theme.prev(), SettingsSection::Provider);
        assert_eq!(SettingsSection::Shortcuts.prev(), SettingsSection::Theme);
    }

    #[test]
    fn test_section_titles() {
        assert_eq!(SettingsSection::Provider.title(), "PROVIDER");
        assert_eq!(SettingsSection::Theme.title(), "THEME");
        assert_eq!(SettingsSection::Shortcuts.title(), "KEYBOARD SHORTCUTS");
    }

    #[test]
    fn test_update_provider() {
        let mut view = SettingsView::new();
        view.update_provider("Claude", "claude-sonnet-4-6");
        assert_eq!(view.provider_name, "Claude");
        assert_eq!(view.model_name, "claude-sonnet-4-6");
    }

    #[test]
    fn test_update_theme_name() {
        let mut view = SettingsView::new();
        view.update_theme_name("Light");
        assert_eq!(view.theme_name, "Light");
        view.update_theme_name("Solarized");
        assert_eq!(view.theme_name, "Solarized");
    }

    #[test]
    fn test_status_line() {
        let view = SettingsView::new();
        let state = TuiState::new("test");
        assert!(view.status_line(&state).contains("PROVIDER"));
    }

    #[test]
    fn test_handle_key_escape_returns_home() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Home)));
    }

    #[test]
    fn test_handle_key_tab_cycles_sections() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        assert_eq!(view.section, SettingsSection::Provider);

        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Theme);

        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Shortcuts);

        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Provider);
    }

    #[test]
    fn test_handle_key_shift_tab_cycles_backwards() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT);
        view.handle_key(key, &mut state);
        assert_eq!(view.section, SettingsSection::Shortcuts);
    }

    #[test]
    fn test_handle_key_question_opens_help() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Help)));
    }

    #[test]
    fn test_handle_key_vim_navigation() {
        let mut view = SettingsView::new();
        let mut state = TuiState::new("test");

        let key_j = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        view.handle_key(key_j, &mut state);
        assert_eq!(view.section, SettingsSection::Theme);

        let key_k = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        view.handle_key(key_k, &mut state);
        assert_eq!(view.section, SettingsSection::Provider);
    }
}
