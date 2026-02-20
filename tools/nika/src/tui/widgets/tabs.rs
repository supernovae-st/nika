//! TabBar Widget
//!
//! Reusable tab bar component for panel switching.
//!
//! # Features
//!
//! - Horizontal or vertical tab layout
//! - Active tab highlighting
//! - Keyboard shortcut hints
//! - Consistent styling with TUI theme

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Widget,
};

/// Horizontal tab bar widget
pub struct TabBar<'a> {
    /// Tab titles
    titles: Vec<&'a str>,
    /// Currently selected tab index
    selected: usize,
    /// Style for inactive tabs
    inactive_style: Style,
    /// Style for active tab
    active_style: Style,
    /// Separator between tabs
    separator: &'a str,
    /// Whether to show bracket indicators
    show_brackets: bool,
}

impl<'a> TabBar<'a> {
    /// Create a new TabBar with the given titles
    pub fn new(titles: Vec<&'a str>, selected: usize) -> Self {
        Self {
            titles,
            selected,
            inactive_style: Style::default().fg(Color::DarkGray),
            active_style: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
            separator: " │ ",
            show_brackets: true,
        }
    }

    /// Set the inactive tab style
    pub fn inactive_style(mut self, style: Style) -> Self {
        self.inactive_style = style;
        self
    }

    /// Set the active tab style
    pub fn active_style(mut self, style: Style) -> Self {
        self.active_style = style;
        self
    }

    /// Set the separator between tabs
    pub fn separator(mut self, sep: &'a str) -> Self {
        self.separator = sep;
        self
    }

    /// Whether to show bracket indicators around active tab
    pub fn show_brackets(mut self, show: bool) -> Self {
        self.show_brackets = show;
        self
    }
}

impl Widget for TabBar<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 || self.titles.is_empty() {
            return;
        }

        let mut spans = Vec::new();

        for (i, title) in self.titles.iter().enumerate() {
            // Add separator before non-first tabs
            if i > 0 {
                spans.push(Span::styled(self.separator, self.inactive_style));
            }

            let is_active = i == self.selected;
            let style = if is_active {
                self.active_style
            } else {
                self.inactive_style
            };

            if is_active && self.show_brackets {
                spans.push(Span::styled("[", self.active_style));
                spans.push(Span::styled(*title, style));
                spans.push(Span::styled("]", self.active_style));
            } else {
                spans.push(Span::styled(*title, style));
            }
        }

        let line = Line::from(spans);

        // Render the line at the start of the area
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

/// Compact tab indicator (e.g., "Graph [YAML]")
pub struct TabIndicator<'a> {
    /// Tab titles
    titles: Vec<&'a str>,
    /// Currently selected tab index
    selected: usize,
    /// Style for inactive tabs
    inactive_style: Style,
    /// Style for active tab
    active_style: Style,
}

impl<'a> TabIndicator<'a> {
    /// Create a new TabIndicator
    pub fn new(titles: Vec<&'a str>, selected: usize) -> Self {
        Self {
            titles,
            selected,
            inactive_style: Style::default().fg(Color::DarkGray),
            active_style: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Set styles
    pub fn styles(mut self, inactive: Style, active: Style) -> Self {
        self.inactive_style = inactive;
        self.active_style = active;
        self
    }

    /// Render and return the Line (for embedding in titles)
    pub fn to_line(&self) -> Line<'a> {
        let mut spans = Vec::new();

        for (i, title) in self.titles.iter().enumerate() {
            if i > 0 {
                spans.push(Span::raw(" "));
            }

            let is_active = i == self.selected;
            if is_active {
                spans.push(Span::styled("[", self.active_style));
                spans.push(Span::styled(*title, self.active_style));
                spans.push(Span::styled("]", self.active_style));
            } else {
                spans.push(Span::styled(*title, self.inactive_style));
            }
        }

        Line::from(spans)
    }

    /// Get just the active tab title
    pub fn active_title(&self) -> &'a str {
        self.titles.get(self.selected).copied().unwrap_or("Unknown")
    }
}

impl Widget for TabIndicator<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let line = self.to_line();
        buf.set_line(area.x, area.y, &line, area.width);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_bar_creation() {
        let tabs = TabBar::new(vec!["Tab1", "Tab2", "Tab3"], 1);
        assert_eq!(tabs.selected, 1);
        assert_eq!(tabs.titles.len(), 3);
    }

    #[test]
    fn test_tab_bar_render() {
        let tabs = TabBar::new(vec!["Progress", "IO", "Output"], 0);
        let area = Rect::new(0, 0, 40, 1);
        let mut buf = Buffer::empty(area);

        tabs.render(area, &mut buf);

        // Should contain the first tab marked as active
        let content: String = buf
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<Vec<_>>()
            .join("");
        assert!(content.contains("[Progress]"));
    }

    #[test]
    fn test_tab_indicator_creation() {
        let indicator = TabIndicator::new(vec!["Graph", "YAML"], 1);
        assert_eq!(indicator.active_title(), "YAML");
    }

    #[test]
    fn test_tab_indicator_to_line() {
        let indicator = TabIndicator::new(vec!["Summary", "Full JSON"], 0);
        let line = indicator.to_line();

        // Should have spans for both tabs
        assert!(!line.spans.is_empty());
    }
}
