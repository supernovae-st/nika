//! MCP Call Log Widget
//!
//! Displays MCP tool/resource calls with status indicators.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

/// MCP call entry for display
#[derive(Debug, Clone)]
pub struct McpEntry {
    /// Sequence number
    pub seq: usize,
    /// Server name
    pub server: String,
    /// Tool name (if tool call)
    pub tool: Option<String>,
    /// Resource URI (if resource read)
    pub resource: Option<String>,
    /// Call completed
    pub completed: bool,
    /// Output length in bytes
    pub output_len: Option<usize>,
}

impl McpEntry {
    pub fn new(seq: usize, server: impl Into<String>) -> Self {
        Self {
            seq,
            server: server.into(),
            tool: None,
            resource: None,
            completed: false,
            output_len: None,
        }
    }

    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.tool = Some(tool.into());
        self
    }

    pub fn with_resource(mut self, resource: impl Into<String>) -> Self {
        self.resource = Some(resource.into());
        self
    }

    pub fn completed(mut self, output_len: usize) -> Self {
        self.completed = true;
        self.output_len = Some(output_len);
        self
    }
}

/// MCP call log widget
pub struct McpLog<'a> {
    entries: &'a [McpEntry],
    /// Show most recent first
    reverse: bool,
    /// Max entries to show
    max_entries: usize,
}

impl<'a> McpLog<'a> {
    pub fn new(entries: &'a [McpEntry]) -> Self {
        Self {
            entries,
            reverse: true,
            max_entries: 10,
        }
    }

    pub fn reverse(mut self, reverse: bool) -> Self {
        self.reverse = reverse;
        self
    }

    pub fn max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }
}

impl Widget for McpLog<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height == 0 || area.width < 10 {
            return;
        }

        if self.entries.is_empty() {
            buf.set_string(
                area.x,
                area.y,
                "(no MCP calls)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Get entries to display
        let entries: Vec<&McpEntry> = if self.reverse {
            self.entries.iter().rev().take(self.max_entries).collect()
        } else {
            self.entries.iter().take(self.max_entries).collect()
        };

        for (i, entry) in entries.iter().enumerate() {
            if i as u16 >= area.height {
                break;
            }

            let y = area.y + i as u16;

            // Status icon
            let (icon, icon_color) = if entry.completed {
                ("✓", Color::Rgb(34, 197, 94))  // green
            } else {
                ("⋯", Color::Rgb(245, 158, 11)) // amber
            };

            buf.set_string(area.x, y, icon, Style::default().fg(icon_color));

            // Call type icon
            let type_icon = if entry.tool.is_some() {
                "🔌"  // tool call
            } else {
                "📖"  // resource read
            };
            buf.set_string(area.x + 2, y, type_icon, Style::default());

            // Name (tool or resource)
            let name = entry.tool.as_deref()
                .or(entry.resource.as_deref())
                .unwrap_or("unknown");

            let max_name_len = (area.width as usize).saturating_sub(12);
            let display_name = if name.len() > max_name_len {
                format!("{}…", &name[..max_name_len.saturating_sub(1)])
            } else {
                name.to_string()
            };

            let name_style = if entry.completed {
                Style::default().fg(Color::White)
            } else {
                Style::default()
                    .fg(Color::Rgb(245, 158, 11))
                    .add_modifier(Modifier::BOLD)
            };

            buf.set_string(area.x + 5, y, &display_name, name_style);

            // Output size if completed
            if let Some(len) = entry.output_len {
                let size_str = format_size(len);
                let size_x = area.x + area.width - size_str.len() as u16;
                if size_x > area.x + 5 + display_name.len() as u16 {
                    buf.set_string(
                        size_x,
                        y,
                        &size_str,
                        Style::default().fg(Color::DarkGray),
                    );
                }
            }
        }
    }
}

/// Format byte size as human readable
fn format_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{}B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1}KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1}MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_entry_creation() {
        let entry = McpEntry::new(1, "novanet")
            .with_tool("novanet_describe")
            .completed(1024);

        assert_eq!(entry.seq, 1);
        assert_eq!(entry.server, "novanet");
        assert_eq!(entry.tool, Some("novanet_describe".to_string()));
        assert!(entry.completed);
        assert_eq!(entry.output_len, Some(1024));
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(500), "500B");
        assert_eq!(format_size(1500), "1.5KB");
        assert_eq!(format_size(1500000), "1.4MB");
    }
}
