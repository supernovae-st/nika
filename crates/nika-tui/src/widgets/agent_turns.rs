//! Agent Turns Widget
//!
//! Displays agent execution turns with status and tool calls.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// Agent turn entry for display
#[derive(Debug, Clone)]
pub struct TurnEntry {
    /// Turn index (0-based)
    pub index: u32,
    /// Turn status (thinking, tool_use, response, etc.)
    pub status: String,
    /// Token count for this turn
    pub tokens: Option<u32>,
    /// Tool calls made this turn
    pub tool_calls: Vec<String>,
    /// Is this the current turn?
    pub is_current: bool,
}

impl TurnEntry {
    pub fn new(index: u32, status: impl Into<String>) -> Self {
        Self {
            index,
            status: status.into(),
            tokens: None,
            tool_calls: Vec::new(),
            is_current: false,
        }
    }

    pub fn with_tokens(mut self, tokens: u32) -> Self {
        self.tokens = Some(tokens);
        self
    }

    pub fn with_tool_calls(mut self, calls: Vec<String>) -> Self {
        self.tool_calls = calls;
        self
    }

    pub fn current(mut self) -> Self {
        self.is_current = true;
        self
    }
}

/// Agent turns widget
pub struct AgentTurns<'a> {
    entries: &'a [TurnEntry],
    max_turns: Option<u32>,
    /// Show most recent first
    reverse: bool,
}

impl<'a> AgentTurns<'a> {
    pub fn new(entries: &'a [TurnEntry]) -> Self {
        Self {
            entries,
            max_turns: None,
            reverse: false,
        }
    }

    pub fn max_turns(mut self, max: u32) -> Self {
        self.max_turns = Some(max);
        self
    }

    pub fn reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    /// Get status icon for agent turn events
    fn status_icon(status: &str) -> &'static str {
        match status {
            "thinking" => "🤔",
            "tool_use" => "🔧",
            "tool_result" => "📋",
            "response" => "✨",
            "complete" => "✅",
            "error" => "❌",
            _ => "❓", // Unknown status
        }
    }

    /// Get status color
    fn status_color(status: &str) -> Color {
        match status {
            "thinking" => Color::Rgb(245, 158, 11),    // amber
            "tool_use" => Color::Rgb(59, 130, 246),    // blue
            "tool_result" => Color::Rgb(139, 92, 246), // violet
            "response" => Color::Rgb(34, 197, 94),     // green
            "complete" => Color::Rgb(34, 197, 94),     // green
            "error" => Color::Rgb(239, 68, 68),        // red
            _ => Color::White,
        }
    }
}

impl Widget for AgentTurns<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 15 {
            return;
        }

        if self.entries.is_empty() {
            buf.set_string(
                area.x,
                area.y,
                "(no agent active)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Get entries to display
        let entries: Vec<&TurnEntry> = if self.reverse {
            self.entries.iter().rev().collect()
        } else {
            self.entries.iter().collect()
        };

        for (i, entry) in entries.iter().enumerate() {
            if i as u16 >= area.height {
                break;
            }

            let y = area.y + i as u16;
            let color = Self::status_color(&entry.status);
            let icon = Self::status_icon(&entry.status);

            // Turn number
            let turn_str = format!("T{}", entry.index + 1);
            let turn_style = if entry.is_current {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            buf.set_string(area.x, y, &turn_str, turn_style);

            // Status icon
            buf.set_string(area.x + 3, y, icon, Style::default());

            // Status text
            let max_status_len = (area.width as usize).saturating_sub(15);
            let display_status = if entry.status.len() > max_status_len {
                format!("{}…", &entry.status[..max_status_len.saturating_sub(1)])
            } else {
                entry.status.clone()
            };

            let status_style = if entry.is_current {
                Style::default().fg(color).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(color)
            };
            buf.set_string(area.x + 6, y, &display_status, status_style);

            // Token count on right
            if let Some(tokens) = entry.tokens {
                let token_str = format_tokens(tokens);
                let token_x = area.x + area.width - token_str.len() as u16;
                if token_x > area.x + 6 + display_status.len() as u16 {
                    buf.set_string(
                        token_x,
                        y,
                        &token_str,
                        Style::default().fg(Color::Rgb(139, 92, 246)), // violet
                    );
                }
            }
        }
    }
}

/// Format token count
fn format_tokens(tokens: u32) -> String {
    if tokens < 1000 {
        format!("{}tk", tokens)
    } else {
        format!("{:.1}k", tokens as f64 / 1000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turn_entry_creation() {
        let entry = TurnEntry::new(0, "thinking")
            .with_tokens(1500)
            .with_tool_calls(vec!["read_file".to_string()])
            .current();

        assert_eq!(entry.index, 0);
        assert_eq!(entry.status, "thinking");
        assert_eq!(entry.tokens, Some(1500));
        assert!(entry.is_current);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(500), "500tk");
        assert_eq!(format_tokens(1500), "1.5k");
        assert_eq!(format_tokens(10000), "10.0k");
    }

    #[test]
    fn test_status_icons() {
        assert_eq!(AgentTurns::status_icon("thinking"), "🤔");
        assert_eq!(AgentTurns::status_icon("tool_use"), "🔧");
        assert_eq!(AgentTurns::status_icon("response"), "✨");
        assert_eq!(AgentTurns::status_icon("unknown"), "❓");
    }
}
