//! Message Bubble Widget - Chat messages with role distinction
//!
//! Displays messages with visual distinction between:
//! - User messages (right-aligned, cyan accent)
//! - Nika/Assistant messages (left-aligned, violet accent)
//! - System messages (centered, muted)
//! - Tool results (special formatting)
//!
//! Features:
//! - Thinking blocks (collapsible)
//! - Inline MCP call boxes
//! - YAML view toggle
//! - Timestamp display

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};
use std::time::Instant;

use super::{InferStreamData, McpCallData};

// Solarized-inspired colors
const COLOR_USER: Color = Color::Rgb(42, 161, 152); // Cyan (user messages)
const COLOR_NIKA: Color = Color::Rgb(108, 113, 196); // Violet (assistant messages)
const COLOR_SYSTEM: Color = Color::Rgb(88, 110, 117); // Muted (system messages)
const COLOR_TOOL: Color = Color::Rgb(133, 153, 0); // Green (tool results)
const COLOR_THINKING: Color = Color::Rgb(245, 158, 11); // Amber (thinking blocks)
const COLOR_MUTED: Color = Color::Rgb(88, 110, 117); // Gray
const COLOR_HEADER: Color = Color::Rgb(250, 204, 21); // Gold

/// Message role in conversation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BubbleRole {
    User,
    Nika,
    System,
    Tool,
}

impl BubbleRole {
    pub fn label(&self) -> &'static str {
        match self {
            BubbleRole::User => "You",
            BubbleRole::Nika => "Nika",
            BubbleRole::System => "System",
            BubbleRole::Tool => "Tool",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            BubbleRole::User => "👤",
            BubbleRole::Nika => "🐔",
            BubbleRole::System => "⚙️",
            BubbleRole::Tool => "🔧",
        }
    }

    pub fn color(&self) -> Color {
        match self {
            BubbleRole::User => COLOR_USER,
            BubbleRole::Nika => COLOR_NIKA,
            BubbleRole::System => COLOR_SYSTEM,
            BubbleRole::Tool => COLOR_TOOL,
        }
    }
}

/// Thinking block content (for extended thinking mode)
#[derive(Debug, Clone)]
pub struct ThinkingBlock {
    pub content: String,
    pub tokens: u64,
    pub collapsed: bool,
}

impl ThinkingBlock {
    pub fn new(content: impl Into<String>, tokens: u64) -> Self {
        Self {
            content: content.into(),
            tokens,
            collapsed: true, // Collapsed by default
        }
    }
}

/// Message bubble widget for chat view
pub struct MessageBubble<'a> {
    /// Message role
    role: BubbleRole,
    /// Main content
    content: &'a str,
    /// Optional thinking block
    thinking: Option<&'a ThinkingBlock>,
    /// MCP calls made during this message
    mcp_calls: Vec<&'a McpCallData>,
    /// Streaming inference data
    infer_stream: Option<&'a InferStreamData>,
    /// Message timestamp
    timestamp: Instant,
    /// Show as YAML instead of text
    show_yaml: bool,
    /// Maximum width for content
    max_width: u16,
    /// Is this message currently selected
    selected: bool,
}

impl<'a> MessageBubble<'a> {
    pub fn new(role: BubbleRole, content: &'a str, timestamp: Instant) -> Self {
        Self {
            role,
            content,
            thinking: None,
            mcp_calls: Vec::new(),
            infer_stream: None,
            timestamp,
            show_yaml: false,
            max_width: 80,
            selected: false,
        }
    }

    pub fn thinking(mut self, thinking: Option<&'a ThinkingBlock>) -> Self {
        self.thinking = thinking;
        self
    }

    pub fn mcp_calls(mut self, calls: Vec<&'a McpCallData>) -> Self {
        self.mcp_calls = calls;
        self
    }

    pub fn infer_stream(mut self, stream: Option<&'a InferStreamData>) -> Self {
        self.infer_stream = stream;
        self
    }

    pub fn show_yaml(mut self, show: bool) -> Self {
        self.show_yaml = show;
        self
    }

    pub fn max_width(mut self, width: u16) -> Self {
        self.max_width = width;
        self
    }

    pub fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Format content as YAML task
    fn format_as_yaml(&self) -> String {
        match self.role {
            BubbleRole::User => {
                format!(
                    "- id: user_message\n  infer: |\n    {}",
                    self.content.lines().collect::<Vec<_>>().join("\n    ")
                )
            }
            BubbleRole::Nika => {
                format!("# Response\n{}", self.content)
            }
            BubbleRole::Tool => {
                format!("# Tool Result\n```json\n{}\n```", self.content)
            }
            BubbleRole::System => {
                format!("# System\n{}", self.content)
            }
        }
    }

    /// Calculate how many lines this message will take
    pub fn height(&self, width: u16) -> u16 {
        let content_width = width.saturating_sub(4); // Account for borders/padding
        let content = if self.show_yaml {
            self.format_as_yaml()
        } else {
            self.content.to_string()
        };

        // Header line (role + timestamp)
        let mut height = 1u16;

        // Content lines (with wrapping)
        for line in content.lines() {
            let line_chars = line.chars().count() as u16;
            if line_chars == 0 {
                height += 1;
            } else {
                height += (line_chars / content_width.max(1)) + 1;
            }
        }

        // Thinking block (if present and not collapsed)
        if let Some(thinking) = self.thinking {
            if !thinking.collapsed {
                height += 2; // Header + separator
                for line in thinking.content.lines() {
                    let line_chars = line.chars().count() as u16;
                    height += (line_chars / content_width.max(1)) + 1;
                }
            } else {
                height += 1; // Just the collapsed indicator
            }
        }

        // MCP calls (each takes ~3 lines)
        height += (self.mcp_calls.len() as u16) * 3;

        // Bottom padding
        height += 1;

        height
    }

    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let elapsed = self.timestamp.elapsed();
        let time_str = if elapsed.as_secs() < 60 {
            format!("{}s ago", elapsed.as_secs())
        } else if elapsed.as_secs() < 3600 {
            format!("{}m ago", elapsed.as_secs() / 60)
        } else {
            format!("{}h ago", elapsed.as_secs() / 3600)
        };

        let header = Line::from(vec![
            Span::raw(self.role.icon()),
            Span::raw(" "),
            Span::styled(
                self.role.label(),
                Style::default()
                    .fg(self.role.color())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" "),
            Span::styled(time_str, Style::default().fg(COLOR_MUTED)),
            if self.show_yaml {
                Span::styled(" [YAML]", Style::default().fg(COLOR_HEADER))
            } else {
                Span::raw("")
            },
        ]);

        let para = Paragraph::new(header);
        para.render(area, buf);
    }

    fn render_thinking(&self, area: Rect, buf: &mut Buffer) {
        if let Some(thinking) = self.thinking {
            let header = if thinking.collapsed {
                Line::from(vec![
                    Span::styled("💭 ", Style::default()),
                    Span::styled("Thinking", Style::default().fg(COLOR_THINKING)),
                    Span::styled(
                        format!(" ({} tokens) [collapsed]", thinking.tokens),
                        Style::default().fg(COLOR_MUTED),
                    ),
                ])
            } else {
                Line::from(vec![
                    Span::styled("💭 ", Style::default()),
                    Span::styled("Thinking", Style::default().fg(COLOR_THINKING)),
                    Span::styled(
                        format!(" ({} tokens)", thinking.tokens),
                        Style::default().fg(COLOR_MUTED),
                    ),
                ])
            };

            let para = Paragraph::new(header);
            para.render(area, buf);

            if !thinking.collapsed && area.height > 1 {
                let content_area = Rect {
                    x: area.x + 2,
                    y: area.y + 1,
                    width: area.width.saturating_sub(4),
                    height: area.height.saturating_sub(1),
                };
                let content = Paragraph::new(thinking.content.as_str())
                    .style(Style::default().fg(COLOR_MUTED))
                    .wrap(Wrap { trim: false });
                content.render(content_area, buf);
            }
        }
    }

    fn render_content(&self, area: Rect, buf: &mut Buffer) {
        let content = if self.show_yaml {
            self.format_as_yaml()
        } else {
            self.content.to_string()
        };

        let para = Paragraph::new(content)
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false });
        para.render(area, buf);
    }
}

impl Widget for MessageBubble<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 10 {
            return;
        }

        // Border style based on selection and role
        let border_style = if self.selected {
            Style::default().fg(COLOR_HEADER)
        } else {
            Style::default().fg(self.role.color())
        };

        // Draw border for non-system messages
        let inner = if self.role != BubbleRole::System {
            let block = Block::default()
                .borders(Borders::LEFT)
                .border_style(border_style);
            let inner = block.inner(area);
            block.render(area, buf);
            inner
        } else {
            area
        };

        // Split into sections
        let thinking_height = if let Some(thinking) = self.thinking {
            if thinking.collapsed {
                1
            } else {
                3.min(inner.height.saturating_sub(3))
            }
        } else {
            0
        };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),               // Header
                Constraint::Length(thinking_height), // Thinking (optional)
                Constraint::Min(1),                  // Content
            ])
            .split(inner);

        // Render header
        self.render_header(chunks[0], buf);

        // Render thinking block
        if thinking_height > 0 {
            self.render_thinking(chunks[1], buf);
        }

        // Render content
        self.render_content(chunks[2], buf);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bubble_role_colors() {
        assert_eq!(BubbleRole::User.color(), COLOR_USER);
        assert_eq!(BubbleRole::Nika.color(), COLOR_NIKA);
        assert_eq!(BubbleRole::System.color(), COLOR_SYSTEM);
        assert_eq!(BubbleRole::Tool.color(), COLOR_TOOL);
    }

    #[test]
    fn test_bubble_role_labels() {
        assert_eq!(BubbleRole::User.label(), "You");
        assert_eq!(BubbleRole::Nika.label(), "Nika");
        assert_eq!(BubbleRole::System.label(), "System");
        assert_eq!(BubbleRole::Tool.label(), "Tool");
    }

    #[test]
    fn test_thinking_block() {
        let thinking = ThinkingBlock::new("Test thinking", 100);
        assert!(thinking.collapsed);
        assert_eq!(thinking.tokens, 100);
    }

    #[test]
    fn test_message_height_calculation() {
        let bubble = MessageBubble::new(BubbleRole::User, "Hello world", Instant::now());
        let height = bubble.height(80);
        assert!(height >= 2); // At least header + content
    }

    #[test]
    fn test_yaml_format() {
        let bubble = MessageBubble::new(BubbleRole::User, "Generate a headline", Instant::now())
            .show_yaml(true);
        let yaml = bubble.format_as_yaml();
        assert!(yaml.contains("infer:"));
    }
}
