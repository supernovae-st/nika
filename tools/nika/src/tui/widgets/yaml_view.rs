//! YAML View Widget
//!
//! Displays YAML workflow content with syntax highlighting
//! and current task highlighting.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Paragraph, Widget},
};

/// YAML viewer with syntax highlighting and current task indicator
pub struct YamlView<'a> {
    /// YAML content to display
    yaml: &'a str,
    /// Currently executing task ID (for highlighting)
    current_task_id: Option<&'a str>,
    /// Scroll offset (lines from top)
    scroll: u16,
    /// Style for YAML keys
    key_style: Style,
    /// Style for YAML values (strings)
    string_style: Style,
    /// Style for YAML values (numbers/booleans)
    literal_style: Style,
    /// Style for comments
    comment_style: Style,
    /// Style for current task block
    current_task_style: Style,
}

impl<'a> YamlView<'a> {
    pub fn new(yaml: &'a str) -> Self {
        Self {
            yaml,
            current_task_id: None,
            scroll: 0,
            key_style: Style::default().fg(Color::Cyan),
            string_style: Style::default().fg(Color::Yellow),
            literal_style: Style::default().fg(Color::Green),
            comment_style: Style::default().fg(Color::Gray),
            current_task_style: Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        }
    }

    /// Set the currently executing task ID for highlighting
    pub fn current_task(mut self, task_id: Option<&'a str>) -> Self {
        self.current_task_id = task_id;
        self
    }

    /// Set scroll offset
    pub fn scroll(mut self, scroll: u16) -> Self {
        self.scroll = scroll;
        self
    }

    /// Highlight a line based on YAML syntax
    fn highlight_line(&self, line: &str, in_current_task: bool) -> Line<'a> {
        let base_style = if in_current_task {
            self.current_task_style
        } else {
            Style::default()
        };

        // Handle comments
        if line.trim_start().starts_with('#') {
            return Line::from(Span::styled(line.to_string(), self.comment_style));
        }

        // Handle key: value pairs
        if let Some(colon_pos) = line.find(':') {
            let (key_part, rest) = line.split_at(colon_pos);
            let value_part = &rest[1..]; // Skip the colon

            let mut spans = Vec::new();

            // Key part with leading whitespace preserved
            if in_current_task {
                spans.push(Span::styled(key_part.to_string(), self.current_task_style));
                spans.push(Span::styled(":", self.current_task_style));
            } else {
                spans.push(Span::styled(key_part.to_string(), self.key_style));
                spans.push(Span::styled(":", Style::default()));
            }

            // Value part
            let value = value_part.trim();
            if value.is_empty() {
                // Just the key, no value on this line
                spans.push(Span::raw(value_part.to_string()));
            } else if value.starts_with('"') || value.starts_with('\'') {
                // String value
                spans.push(Span::styled(value_part.to_string(), self.string_style));
            } else if value == "true"
                || value == "false"
                || value == "null"
                || value.parse::<f64>().is_ok()
            {
                // Literal value
                spans.push(Span::styled(value_part.to_string(), self.literal_style));
            } else {
                // Other value
                spans.push(Span::styled(value_part.to_string(), base_style));
            }

            return Line::from(spans);
        }

        // Handle list items (- item)
        if line.trim_start().starts_with('-') {
            if in_current_task {
                return Line::from(Span::styled(line.to_string(), self.current_task_style));
            }
            return Line::from(Span::styled(line.to_string(), base_style));
        }

        // Default styling
        Line::from(Span::styled(line.to_string(), base_style))
    }

    /// Check if a line is the start of a task with the given ID
    fn is_task_start(&self, line: &str, task_id: &str) -> bool {
        // Match patterns like "- id: task_name" or "  id: task_name"
        let trimmed = line.trim();
        if trimmed.starts_with("- id:") || trimmed.starts_with("id:") {
            if let Some(colon_pos) = trimmed.find(':') {
                let value = trimmed[colon_pos + 1..].trim();
                return value == task_id || value == format!("\"{}\"", task_id);
            }
        }
        false
    }

    /// Get the indentation level of a line
    fn get_indent(&self, line: &str) -> usize {
        line.len() - line.trim_start().len()
    }
}

impl Widget for YamlView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }

        let lines: Vec<&str> = self.yaml.lines().collect();
        let mut highlighted_lines: Vec<Line> = Vec::new();

        // Track current task block
        let mut in_current_task = false;
        let mut task_base_indent: Option<usize> = None;

        for line in &lines {
            // Check if we're entering the current task
            if let Some(task_id) = self.current_task_id {
                if self.is_task_start(line, task_id) {
                    in_current_task = true;
                    task_base_indent = Some(self.get_indent(line));
                } else if in_current_task {
                    // Check if we've exited the task block (decreased indent)
                    let current_indent = self.get_indent(line);
                    if let Some(base) = task_base_indent {
                        if !line.trim().is_empty() && current_indent <= base {
                            // New block at same or lower indent = end of task
                            in_current_task = false;
                            task_base_indent = None;
                        }
                    }
                }
            }

            highlighted_lines.push(self.highlight_line(line, in_current_task));
        }

        // Create paragraph with scroll
        let paragraph = Paragraph::new(highlighted_lines).scroll((self.scroll, 0));
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_yaml_view_creation() {
        let yaml = "key: value";
        let view = YamlView::new(yaml);
        assert!(view.current_task_id.is_none());
        assert_eq!(view.scroll, 0);
    }

    #[test]
    fn test_yaml_view_with_current_task() {
        let yaml = "tasks:\n  - id: test\n    infer: prompt";
        let view = YamlView::new(yaml).current_task(Some("test"));
        assert_eq!(view.current_task_id, Some("test"));
    }

    #[test]
    fn test_yaml_view_with_scroll() {
        let yaml = "key: value";
        let view = YamlView::new(yaml).scroll(5);
        assert_eq!(view.scroll, 5);
    }

    #[test]
    fn test_is_task_start() {
        let view = YamlView::new("");

        assert!(view.is_task_start("  - id: schema", "schema"));
        assert!(view.is_task_start("- id: schema", "schema"));
        assert!(view.is_task_start("id: schema", "schema"));
        assert!(view.is_task_start("  - id: \"schema\"", "schema"));

        assert!(!view.is_task_start("  - id: other", "schema"));
        assert!(!view.is_task_start("  infer: prompt", "schema"));
    }

    #[test]
    fn test_get_indent() {
        let view = YamlView::new("");

        assert_eq!(view.get_indent("no indent"), 0);
        assert_eq!(view.get_indent("  two spaces"), 2);
        assert_eq!(view.get_indent("    four spaces"), 4);
        assert_eq!(view.get_indent("\ttab"), 1);
    }

    #[test]
    fn test_yaml_view_render() {
        let yaml = "schema: test\ntasks:\n  - id: task1\n    infer: hello";
        let view = YamlView::new(yaml).current_task(Some("task1"));

        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 10));
        view.render(Rect::new(0, 0, 40, 10), &mut buf);

        // Buffer should have content (basic render test)
        // We verify it renders without panicking
        assert!(buf.area().width > 0);
    }
}
