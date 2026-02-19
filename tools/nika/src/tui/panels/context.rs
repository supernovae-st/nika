//! NovaNet Station Panel
//!
//! Displays MCP integration and context assembly:
//! - MCP call history with tool/resource names
//! - Context assembly progress
//! - Token budget visualization
//! - Sources included/excluded

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::state::TuiState;
use crate::tui::theme::Theme;
use crate::tui::widgets::{Gauge, McpEntry, McpLog};

/// NovaNet Station panel (Panel 3)
pub struct ContextPanel<'a> {
    state: &'a TuiState,
    theme: &'a Theme,
    focused: bool,
}

impl<'a> ContextPanel<'a> {
    pub fn new(state: &'a TuiState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            focused: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Get animated spinner for pending operations
    fn spinner(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let idx = (self.state.frame / 6) as usize % SPINNER.len();
        SPINNER[idx]
    }

    /// Build MCP entries from state
    fn build_mcp_entries(&self) -> Vec<McpEntry> {
        self.state
            .mcp_calls
            .iter()
            .map(|call| {
                let mut entry = McpEntry::new(call.seq, &call.server);

                if let Some(ref tool) = call.tool {
                    entry = entry.with_tool(tool);
                }
                if let Some(ref resource) = call.resource {
                    entry = entry.with_resource(resource);
                }
                if call.completed {
                    entry = entry.completed(call.output_len.unwrap_or(0));
                }

                entry
            })
            .collect()
    }

    /// Render MCP header with call count
    fn render_mcp_header(&self, area: Rect, buf: &mut Buffer) {
        let total_calls = self.state.mcp_calls.len();
        let completed = self.state.mcp_calls.iter().filter(|c| c.completed).count();
        let pending = total_calls - completed;

        // Animated icon when MCP calls are pending
        let (icon, icon_color) = if pending > 0 {
            // Animated connection icon
            let frames = &["⊛", "⊕", "⊗", "⊙"];
            let idx = (self.state.frame / 8) as usize % frames.len();
            (frames[idx], Color::Rgb(245, 158, 11)) // amber for pending
        } else {
            ("⊛", Color::Rgb(139, 92, 246)) // violet when idle
        };

        let header = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(format!("{} ", icon), Style::default().fg(icon_color)),
            Span::styled("MCP: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} calls", total_calls), Style::default().fg(Color::White)),
            if pending > 0 {
                Span::styled(
                    format!(" {} ({} pending)", self.spinner(), pending),
                    Style::default().fg(Color::Rgb(245, 158, 11)),
                )
            } else {
                Span::styled("", Style::default())
            },
        ]);

        let paragraph = Paragraph::new(header);
        paragraph.render(area, buf);
    }

    /// Render MCP call log
    fn render_mcp_log(&self, area: Rect, buf: &mut Buffer) {
        let entries = self.build_mcp_entries();
        let max_entries = area.height as usize;

        let mcp_log = McpLog::new(&entries)
            .reverse(true)
            .max_entries(max_entries);

        // Offset by 2 for padding
        let log_area = Rect {
            x: area.x + 2,
            y: area.y,
            width: area.width.saturating_sub(4),
            height: area.height,
        };

        mcp_log.render(log_area, buf);
    }

    /// Render context assembly section
    fn render_context(&self, area: Rect, buf: &mut Buffer) {
        let ctx = &self.state.context_assembly;

        if ctx.total_tokens == 0 {
            buf.set_string(
                area.x + 2,
                area.y,
                "(no context assembled)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Context header
        let header = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("📦 ", Style::default()),
            Span::styled("Context: ", Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!("{} tokens", format_number(ctx.total_tokens)),
                Style::default().fg(Color::Rgb(139, 92, 246)),
            ),
            if ctx.truncated {
                Span::styled(" (truncated)", Style::default().fg(Color::Rgb(239, 68, 68)))
            } else {
                Span::styled("", Style::default())
            },
        ]);

        let paragraph = Paragraph::new(header);
        paragraph.render(area, buf);

        // Budget gauge
        if area.height > 1 {
            let gauge_area = Rect {
                x: area.x + 2,
                y: area.y + 1,
                width: area.width.saturating_sub(4),
                height: 1,
            };

            let ratio = (ctx.budget_used_pct / 100.0) as f64;
            let gauge_color = if ratio >= 0.9 {
                Color::Rgb(239, 68, 68)  // red - near limit
            } else if ratio >= 0.7 {
                Color::Rgb(245, 158, 11) // amber - warning
            } else {
                Color::Rgb(34, 197, 94)  // green - ok
            };

            let gauge = Gauge::new(ratio)
                .fill_color(gauge_color)
                .label("Budget")
                .show_percent(true);

            gauge.render(gauge_area, buf);
        }

        // Sources summary
        if area.height > 2 {
            let sources_count = ctx.sources.len();
            let excluded_count = ctx.excluded.len();

            let sources_line = Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("Sources: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", sources_count),
                    Style::default().fg(Color::Rgb(34, 197, 94)),
                ),
                Span::styled(" included", Style::default().fg(Color::DarkGray)),
                if excluded_count > 0 {
                    Span::styled(
                        format!(", {} excluded", excluded_count),
                        Style::default().fg(Color::Rgb(107, 114, 128)),
                    )
                } else {
                    Span::styled("", Style::default())
                },
            ]);

            let paragraph = Paragraph::new(sources_line);
            let line_area = Rect {
                x: area.x,
                y: area.y + 2,
                width: area.width,
                height: 1,
            };
            paragraph.render(line_area, buf);
        }
    }

    /// Render server status
    fn render_servers(&self, area: Rect, buf: &mut Buffer) {
        // Get unique servers from MCP calls
        let mut servers: std::collections::HashSet<&str> = std::collections::HashSet::new();
        for call in &self.state.mcp_calls {
            servers.insert(&call.server);
        }

        if servers.is_empty() {
            buf.set_string(
                area.x + 2,
                area.y,
                "(no MCP servers)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Server badges
        let mut spans = vec![Span::styled("  Servers: ", Style::default().fg(Color::DarkGray))];

        for (i, server) in servers.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" ", Style::default()));
            }
            spans.push(Span::styled(
                format!("[{}]", server),
                Style::default()
                    .fg(Color::Rgb(139, 92, 246))
                    .add_modifier(Modifier::BOLD),
            ));
        }

        let line = Line::from(spans);
        let paragraph = Paragraph::new(line);
        paragraph.render(area, buf);
    }
}

impl Widget for ContextPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border
        let border_style = self.theme.border_style(self.focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" ⊛ NOVANET STATION ")
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 6 || inner.width < 20 {
            return;
        }

        // Layout: MCP Header | MCP Log | Context | Servers
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),  // MCP Header
                Constraint::Min(3),     // MCP Log
                Constraint::Length(3),  // Context Assembly
                Constraint::Length(1),  // Servers
            ])
            .split(inner);

        self.render_mcp_header(chunks[0], buf);
        self.render_mcp_log(chunks[1], buf);
        self.render_context(chunks[2], buf);
        self.render_servers(chunks[3], buf);
    }
}

/// Format number with thousands separator
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::TuiState;
    use crate::tui::theme::Theme;

    #[test]
    fn test_context_panel_creation() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = ContextPanel::new(&state, &theme).focused(true);
        assert!(panel.focused);
    }

    #[test]
    fn test_build_mcp_entries_empty() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = ContextPanel::new(&state, &theme);
        let entries = panel.build_mcp_entries();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }
}
