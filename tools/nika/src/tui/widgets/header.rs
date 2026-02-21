//! Tab-Header widget for VS Code-like view navigation
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  ◆ NIKA │ 1:Chat │ 2:Home │ 3:Studio │ 4:Monitor │     ⌘K palette   q:quit │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};
use unicode_width::UnicodeWidthStr;

use crate::tui::theme::Theme;
use crate::tui::views::TuiView;

/// Tab names for each view
const TAB_NAMES: &[(&str, TuiView)] = &[
    ("Chat", TuiView::Chat),
    ("Home", TuiView::Home),
    ("Studio", TuiView::Studio),
    ("Monitor", TuiView::Monitor),
];

/// Header configuration
pub struct Header<'a> {
    /// Current active view
    pub view: TuiView,
    /// Optional context string (file name, workflow name)
    pub context: Option<&'a str>,
    /// Optional status string
    pub status: Option<&'a str>,
    /// Theme for colors
    pub theme: &'a Theme,
}

impl<'a> Header<'a> {
    pub fn new(view: TuiView, theme: &'a Theme) -> Self {
        Self {
            view,
            context: None,
            status: None,
            theme,
        }
    }

    pub fn context(mut self, ctx: &'a str) -> Self {
        self.context = Some(ctx);
        self
    }

    pub fn status(mut self, status: &'a str) -> Self {
        self.status = Some(status);
        self
    }

    /// Get tab label style based on active state
    fn tab_style(&self, is_active: bool) -> Style {
        if is_active {
            Style::default()
                .fg(self.theme.highlight)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(self.theme.text_muted)
        }
    }
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 40 {
            // Compact mode for narrow terminals
            self.render_compact(area, buf);
            return;
        }

        // Build left side: ◆ NIKA │ tabs...
        let mut spans = vec![
            Span::styled(" ◆ ", Style::default().fg(self.theme.highlight)),
            Span::styled(
                "NIKA",
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │", Style::default().fg(self.theme.border_normal)),
        ];

        // Add tabs: 1:Chat │ 2:Home │ 3:Studio │ 4:Monitor
        for (name, view) in TAB_NAMES {
            let is_active = *view == self.view;
            let num = view.number();

            spans.push(Span::raw(" "));

            if is_active {
                // Active tab: [1:Chat]
                spans.push(Span::styled(
                    format!("[{}:{}]", num, name),
                    self.tab_style(true),
                ));
            } else {
                // Inactive tab: 2:Home
                spans.push(Span::styled(
                    format!("{}:{}", num, name),
                    self.tab_style(false),
                ));
            }
        }

        // Add context if present
        if let Some(ctx) = self.context {
            spans.push(Span::styled(
                " │",
                Style::default().fg(self.theme.border_normal),
            ));
            spans.push(Span::raw(" "));
            // Truncate context if too long (UTF-8 safe, char-based)
            let max_ctx = 30;
            let char_count = ctx.chars().count();
            let display_ctx = if char_count > max_ctx {
                // Keep last (max_ctx - 3) chars, prepend "..."
                let skip = char_count.saturating_sub(max_ctx - 3);
                format!("...{}", ctx.chars().skip(skip).collect::<String>())
            } else {
                ctx.to_string()
            };
            spans.push(Span::styled(
                display_ctx,
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        // Calculate current width (unicode-aware for proper terminal alignment)
        let left_width: usize = spans.iter().map(|s| s.content.width()).sum();

        // Build right side: ⌘K palette  q:quit
        let right_spans = vec![
            Span::styled("⌘K", Style::default().fg(Color::Cyan)),
            Span::styled(" palette", Style::default().fg(self.theme.text_muted)),
            Span::raw("  "),
            Span::styled("q", Style::default().fg(Color::Red)),
            Span::styled(":quit ", Style::default().fg(self.theme.text_muted)),
        ];

        let right_width: usize = right_spans.iter().map(|s| s.content.width()).sum();
        let padding = area
            .width
            .saturating_sub(left_width as u16 + right_width as u16);

        // Combine with padding
        let mut all_spans = spans;
        if padding > 0 {
            all_spans.push(Span::raw(" ".repeat(padding as usize)));
        }
        all_spans.extend(right_spans);

        let line = Line::from(all_spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(self.theme.background));

        paragraph.render(area, buf);
    }
}

impl Header<'_> {
    /// Render compact header for narrow terminals
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        let spans = vec![
            Span::styled(" ◆ ", Style::default().fg(self.theme.highlight)),
            Span::styled(
                format!("{}", self.view.number()),
                Style::default()
                    .fg(self.theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(":"),
            Span::styled(
                TAB_NAMES
                    .iter()
                    .find(|(_, v)| *v == self.view)
                    .map(|(n, _)| *n)
                    .unwrap_or("?"),
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line).style(Style::default().bg(self.theme.background));
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_new() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Home, &theme);
        assert_eq!(header.view, TuiView::Home);
        assert!(header.context.is_none());
        assert!(header.status.is_none());
    }

    #[test]
    fn test_header_with_context() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Studio, &theme).context("workflow.nika.yaml");
        assert_eq!(header.context, Some("workflow.nika.yaml"));
    }

    #[test]
    fn test_header_with_status() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Monitor, &theme).status("Running 2/3");
        assert_eq!(header.status, Some("Running 2/3"));
    }

    #[test]
    fn test_tab_style_active() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Chat, &theme);
        let style = header.tab_style(true);
        assert!(style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_tab_style_inactive() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Chat, &theme);
        let style = header.tab_style(false);
        assert!(!style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn test_header_context_truncation_utf8() {
        // Test that UTF-8 characters don't cause panics
        let theme = Theme::dark();
        let long_ctx = "génération_de_contenu_français_très_long.nika.yaml";
        let header = Header::new(TuiView::Studio, &theme).context(long_ctx);
        assert!(header.context.is_some());
        // Context should be truncated to 30 chars max
        let char_count = long_ctx.chars().count();
        assert!(char_count > 30);
    }

    #[test]
    fn test_header_context_short() {
        let theme = Theme::dark();
        let short_ctx = "short.yaml";
        let header = Header::new(TuiView::Studio, &theme).context(short_ctx);
        assert_eq!(header.context, Some(short_ctx));
    }
}
