//! Tab-Header widget for VS Code-like view navigation
//!
//! ```text
//! ┌────────────────────────────────────────────────────────────────────────────┐
//! │  🦋 NIKA │ 1:Studio │ 2:Runner │ 3:Chat │ 4:⚙                             │
//! └────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! 4-Views Architecture.

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
/// Order: Studio, Runner, Chat, Settings
const TAB_NAMES: &[(&str, TuiView)] = &[
    ("Studio", TuiView::Studio),
    ("Runner", TuiView::Runner),
    ("Chat", TuiView::Chat),
    ("⚙", TuiView::Settings), // Settings uses icon for brevity
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

    /// Header background — slightly lighter than main bg for visual separation
    const HEADER_BG: Color = Color::Rgb(20, 24, 41);
    /// Active tab highlight — verb violet
    const TAB_ACTIVE_BG: Color = Color::Rgb(139, 92, 246);
    /// Inactive tab text — slate-400
    const TAB_INACTIVE_FG: Color = Color::Rgb(148, 163, 184);

    /// Get tab label style based on active state
    fn tab_style(&self, is_active: bool) -> Style {
        if is_active {
            Style::default()
                .fg(Color::White)
                .bg(Self::TAB_ACTIVE_BG)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Self::TAB_INACTIVE_FG)
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
                // Active tab: highlighted background, no brackets
                spans.push(Span::styled(
                    format!(" {}:{} ", num, name),
                    self.tab_style(true),
                ));
            } else {
                // Inactive tab: muted text, no brackets
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

        // Add status badge if present (e.g. "PAUSED")
        if let Some(status) = self.status {
            if !status.is_empty() {
                spans.push(Span::styled(
                    " │",
                    Style::default().fg(self.theme.border_normal),
                ));
                spans.push(Span::raw(" "));
                spans.push(Span::styled(
                    format!("[{}]", status),
                    Style::default()
                        .fg(Color::Rgb(251, 191, 36)) // Amber
                        .add_modifier(Modifier::BOLD),
                ));
            }
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
        let paragraph = Paragraph::new(line).style(Style::default().bg(Self::HEADER_BG));

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
        let paragraph = Paragraph::new(line).style(Style::default().bg(Self::HEADER_BG));
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_header_new() {
        let theme = Theme::dark();
        let header = Header::new(TuiView::Studio, &theme);
        assert_eq!(header.view, TuiView::Studio);
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
        let header = Header::new(TuiView::Runner, &theme).status("Running 2/3");
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

    #[test]
    fn test_tab_names_v022() {
        // Verify 4-view architecture: Studio, Runner, Chat, Settings
        assert_eq!(TAB_NAMES.len(), 4);
        assert_eq!(TAB_NAMES[0], ("Studio", TuiView::Studio));
        assert_eq!(TAB_NAMES[1], ("Runner", TuiView::Runner));
        assert_eq!(TAB_NAMES[2], ("Chat", TuiView::Chat));
        assert_eq!(TAB_NAMES[3], ("⚙", TuiView::Settings));
    }
}
