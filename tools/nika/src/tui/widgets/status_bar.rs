//! Unified status bar widget showing contextual keybindings
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │ [Enter] Send  [Up/Down] History  [Tab] Views  [Ctrl+L] Clear  [q] Quit       │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Key hint for status bar
#[derive(Debug, Clone)]
pub struct KeyHint {
    pub key: &'static str,
    pub action: &'static str,
}

impl KeyHint {
    pub const fn new(key: &'static str, action: &'static str) -> Self {
        Self { key, action }
    }
}

/// Status bar configuration
pub struct StatusBar<'a> {
    /// Current view (determines which hints to show)
    pub view: TuiView,
    /// Optional custom hints (overrides defaults)
    pub hints: Option<Vec<KeyHint>>,
    /// Theme for colors
    pub theme: &'a Theme,
}

impl<'a> StatusBar<'a> {
    pub fn new(view: TuiView, theme: &'a Theme) -> Self {
        Self {
            view,
            hints: None,
            theme,
        }
    }

    pub fn hints(mut self, hints: Vec<KeyHint>) -> Self {
        self.hints = Some(hints);
        self
    }

    fn default_hints(&self) -> Vec<KeyHint> {
        match self.view {
            TuiView::Chat => vec![
                KeyHint::new("Enter", "Send"),
                KeyHint::new("Up/Down", "History"),
                KeyHint::new("Tab", "Views"),
                KeyHint::new("Ctrl+L", "Clear"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Home => vec![
                KeyHint::new("Up/Down", "Navigate"),
                KeyHint::new("Enter", "Run"),
                KeyHint::new("e", "Edit"),
                KeyHint::new("n", "New"),
                KeyHint::new("/", "Search"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Quit"),
            ],
            TuiView::Studio => vec![
                KeyHint::new("i", "Insert"),
                KeyHint::new("Esc", "Normal"),
                KeyHint::new("F5", "Run"),
                KeyHint::new("Ctrl+S", "Save"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Back"),
            ],
            TuiView::Monitor => vec![
                KeyHint::new("1-4", "Focus"),
                KeyHint::new("Tab", "Cycle"),
                KeyHint::new("Space", "Pause"),
                KeyHint::new("r", "Restart"),
                KeyHint::new("c", "Chat"),
                KeyHint::new("q", "Stop"),
            ],
        }
    }
}

impl Widget for StatusBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Get default hints before potentially moving self.hints
        let default = self.default_hints();
        let hints = self.hints.unwrap_or(default);

        let mut spans = vec![Span::raw(" ")];

        for (i, hint) in hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw("  "));
            }
            spans.push(Span::styled(
                format!("[{}]", hint.key),
                Style::default()
                    .fg(self.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                hint.action,
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(self.theme.background));

        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_default_hints_home() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Home, &theme);
        let hints = bar.default_hints();
        assert!(hints.iter().any(|h| h.key == "Enter" && h.action == "Run"));
        assert!(hints.iter().any(|h| h.key == "e" && h.action == "Edit"));
    }

    #[test]
    fn test_status_bar_default_hints_studio() {
        let theme = Theme::dark();
        let bar = StatusBar::new(TuiView::Studio, &theme);
        let hints = bar.default_hints();
        assert!(hints.iter().any(|h| h.key == "F5" && h.action == "Run"));
        assert!(hints
            .iter()
            .any(|h| h.key == "Ctrl+S" && h.action == "Save"));
    }

    #[test]
    fn test_status_bar_custom_hints() {
        let theme = Theme::dark();
        let custom = vec![KeyHint::new("x", "Custom")];
        let bar = StatusBar::new(TuiView::Chat, &theme).hints(custom);
        assert!(bar.hints.is_some());
        assert_eq!(bar.hints.unwrap().len(), 1);
    }
}
