//! Unified header widget for all views
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │  ◆ NIKA [VIEW] › [context]              [status]        1 2 3 4    [?] [×] │
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
}

impl Widget for Header<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Build left side: ◆ NIKA [VIEW] › context
        let mut left_spans = vec![
            Span::styled(" ◆ ", Style::default().fg(self.theme.highlight)),
            Span::styled(
                self.view.title(),
                Style::default()
                    .fg(self.theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ];

        if let Some(ctx) = self.context {
            left_spans.push(Span::raw(" › "));
            left_spans.push(Span::styled(
                ctx,
                Style::default().fg(self.theme.text_secondary),
            ));
        }

        // Build right side: status  1 2 3 4  [?] [×]
        let mut right_spans = vec![];

        if let Some(status) = self.status {
            right_spans.push(Span::styled(
                status,
                Style::default().fg(self.theme.text_secondary),
            ));
            right_spans.push(Span::raw("  "));
        }

        // View tabs: 1 2 3 4 (active one highlighted)
        for v in TuiView::all() {
            let num = v.number().to_string();
            if *v == self.view {
                right_spans.push(Span::styled(
                    format!("[{}]", num),
                    Style::default()
                        .fg(self.theme.highlight)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                right_spans.push(Span::styled(
                    format!(" {} ", num),
                    Style::default().fg(self.theme.text_muted),
                ));
            }
        }

        right_spans.push(Span::raw("  "));
        right_spans.push(Span::styled(
            "[?]",
            Style::default().fg(self.theme.text_muted),
        ));
        right_spans.push(Span::raw(" "));
        right_spans.push(Span::styled(
            "[×]",
            Style::default().fg(self.theme.text_muted),
        ));
        right_spans.push(Span::raw(" "));

        // Calculate widths
        let left_width: usize = left_spans.iter().map(|s| s.content.len()).sum();
        let right_width: usize = right_spans.iter().map(|s| s.content.len()).sum();
        let padding = area
            .width
            .saturating_sub(left_width as u16 + right_width as u16);

        // Combine with padding
        let mut all_spans = left_spans;
        all_spans.push(Span::raw(" ".repeat(padding as usize)));
        all_spans.extend(right_spans);

        let line = Line::from(all_spans);
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
}
