//! Infer Stream Box Widget
//!
//! Renders streaming LLM inference with token counter and progress.

use std::time::Duration;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT COLORS (fallbacks for theme-aware rendering)
// ═══════════════════════════════════════════════════════════════════════════

const DEFAULT_RUNNING_COLOR: Color = Color::Rgb(250, 204, 21); // yellow
const DEFAULT_SUCCESS_COLOR: Color = Color::Rgb(34, 197, 94); // green
const DEFAULT_FAILED_COLOR: Color = Color::Rgb(239, 68, 68); // red
const DEFAULT_BORDER_COLOR: Color = Color::Rgb(139, 92, 246); // violet
const DEFAULT_MUTED_COLOR: Color = Color::Rgb(107, 114, 128); // gray
const DEFAULT_SEPARATOR_COLOR: Color = Color::Rgb(55, 65, 81); // dark gray

/// Status of inference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum InferStatus {
    #[default]
    Running,
    Complete,
    Failed,
}

impl InferStatus {
    pub fn indicator(&self, frame: u8) -> (&'static str, Color) {
        match self {
            Self::Running => {
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let idx = (frame as usize) % spinners.len();
                (spinners[idx], DEFAULT_RUNNING_COLOR)
            }
            Self::Complete => ("✅", DEFAULT_SUCCESS_COLOR),
            Self::Failed => ("❌", DEFAULT_FAILED_COLOR),
        }
    }
}

/// Data for rendering an inference stream box
#[derive(Debug, Clone, Default)]
pub struct InferStreamData {
    /// Model name
    pub model: String,
    /// Tokens input
    pub tokens_in: u64,
    /// Tokens output (so far)
    pub tokens_out: u64,
    /// Max tokens
    pub max_tokens: u64,
    /// Temperature
    pub temperature: f32,
    /// Duration
    pub duration: Duration,
    /// Status
    pub status: InferStatus,
    /// Streaming content
    pub content: String,
    /// Animation frame
    pub frame: u8,
}

impl InferStreamData {
    pub fn new(model: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            tokens_in: 0,
            tokens_out: 0,
            max_tokens: 2000,
            temperature: 0.7,
            duration: Duration::ZERO,
            status: InferStatus::Running,
            content: String::new(),
            frame: 0,
        }
    }

    pub fn with_tokens(mut self, input: u64, output: u64) -> Self {
        self.tokens_in = input;
        self.tokens_out = output;
        self
    }

    pub fn with_max_tokens(mut self, max: u64) -> Self {
        self.max_tokens = max;
        self
    }

    pub fn with_content(mut self, content: impl Into<String>) -> Self {
        self.content = content.into();
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_status(mut self, status: InferStatus) -> Self {
        self.status = status;
        self
    }

    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Append content during streaming
    pub fn append_content(&mut self, text: &str) {
        self.content.push_str(text);
    }

    /// Update tokens during streaming
    pub fn update_tokens(&mut self, output: u64) {
        self.tokens_out = output;
    }

    /// Mark as complete
    pub fn complete(&mut self) {
        self.status = InferStatus::Complete;
    }

    /// Mark as failed
    pub fn fail(&mut self) {
        self.status = InferStatus::Failed;
    }

    /// Tick animation frame
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }

    /// Progress percentage
    pub fn progress_percent(&self) -> f64 {
        if self.max_tokens == 0 {
            0.0
        } else {
            (self.tokens_out as f64 / self.max_tokens as f64) * 100.0
        }
    }
}

/// Infer stream box widget
pub struct InferStreamBox<'a> {
    data: &'a InferStreamData,
    max_content_lines: u16,
}

impl<'a> InferStreamBox<'a> {
    pub fn new(data: &'a InferStreamData) -> Self {
        Self {
            data,
            max_content_lines: 6,
        }
    }

    pub fn max_lines(mut self, lines: u16) -> Self {
        self.max_content_lines = lines;
        self
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        // borders (2) + header (1) + stats (1) + separator (1) + progress (1) = 6
        // + content lines
        let content_lines = self.data.content.lines().count() as u16;
        6 + content_lines.min(self.max_content_lines)
    }
}

impl Widget for InferStreamBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 6 {
            return;
        }

        // Border color: violet for infer
        let border_color = DEFAULT_BORDER_COLOR;
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(DEFAULT_MUTED_COLOR);

        let (status_char, _) = self.data.status.indicator(self.data.frame);

        // Top border with title
        let duration_str = format!("{:.1}s", self.data.duration.as_secs_f64());
        let inner_width = area.width.saturating_sub(2) as usize;

        let title_prefix = format!("╭─ ⚡ INFER: {} ", self.data.model);
        let title_suffix = format!(" {} {} ─╮", status_char, duration_str);
        let dash_count = inner_width
            .saturating_sub(title_prefix.chars().count())
            .saturating_sub(title_suffix.chars().count());
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);

        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Stats line: tokens
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let status_text = match self.data.status {
                InferStatus::Running => "(streaming...)",
                InferStatus::Complete => "(complete)",
                InferStatus::Failed => "(failed)",
            };

            buf.set_string(
                area.x + 2,
                y,
                format!(
                    "📊 tokens: {} in → {} out {}",
                    self.data.tokens_in, self.data.tokens_out, status_text
                ),
                dim_style,
            );
            y += 1;
        }

        // Separator
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            let separator = "─".repeat((area.width.saturating_sub(4)) as usize);
            buf.set_string(
                area.x + 2,
                y,
                &separator,
                Style::default().fg(DEFAULT_SEPARATOR_COLOR),
            );
            y += 1;
        }

        // Content lines
        let content_lines: Vec<&str> = self.data.content.lines().collect();
        let available_lines = (area.height.saturating_sub(4)).min(self.max_content_lines) as usize;
        let start = content_lines.len().saturating_sub(available_lines);

        for line in content_lines.iter().skip(start).take(available_lines) {
            if y >= area.y + area.height - 2 {
                break;
            }
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            // UTF-8 safe truncation
            let max_line_len = (area.width.saturating_sub(4)) as usize;
            let display_line: String = if line.chars().count() > max_line_len {
                line.chars().take(max_line_len).collect()
            } else {
                line.to_string()
            };
            buf.set_string(
                area.x + 2,
                y,
                display_line,
                Style::default().fg(Color::White),
            );
            y += 1;
        }

        // Blinking cursor if streaming
        if self.data.status == InferStatus::Running && !content_lines.is_empty() {
            let last_line_len = content_lines.last().map(|l| l.len()).unwrap_or(0);
            let cursor_x = area.x + 2 + (last_line_len as u16).min(area.width.saturating_sub(5));
            if y > area.y + 1 && cursor_x < area.x + area.width - 1 {
                buf.set_string(
                    cursor_x,
                    y - 1,
                    "█",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::SLOW_BLINK),
                );
            }
        }

        // Fill empty content lines
        while y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
        }

        // Progress bar
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let bar_width = (area.width.saturating_sub(30)) as usize;
            let pct = self.data.progress_percent();
            let filled = ((pct / 100.0) * bar_width as f64) as usize;
            let empty = bar_width.saturating_sub(filled);

            let bar = format!(
                "[{}{}] {}/{} tokens",
                "░".repeat(filled),
                " ".repeat(empty),
                self.data.tokens_out,
                self.data.max_tokens
            );
            buf.set_string(area.x + 2, y, &bar, dim_style);
        }

        // Bottom border
        let bottom = format!("╰{}╯", "─".repeat((area.width.saturating_sub(2)) as usize));
        buf.set_string(area.x, area.y + area.height - 1, &bottom, border_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_infer_stream_creation() {
        let data = InferStreamData::new("claude-sonnet-4")
            .with_tokens(100, 50)
            .with_max_tokens(2000);

        assert_eq!(data.model, "claude-sonnet-4");
        assert_eq!(data.tokens_in, 100);
        assert_eq!(data.tokens_out, 50);
        assert_eq!(data.max_tokens, 2000);
    }

    #[test]
    fn test_progress_percent() {
        let data = InferStreamData::new("model")
            .with_tokens(0, 500)
            .with_max_tokens(2000);
        assert_eq!(data.progress_percent(), 25.0);

        let data = InferStreamData::new("model").with_max_tokens(0);
        assert_eq!(data.progress_percent(), 0.0);
    }

    #[test]
    fn test_content_operations() {
        let mut data = InferStreamData::new("model").with_content("Hello");
        assert_eq!(data.content, "Hello");

        data.append_content(" World");
        assert_eq!(data.content, "Hello World");
    }

    #[test]
    fn test_status_transitions() {
        let mut data = InferStreamData::new("model");
        assert_eq!(data.status, InferStatus::Running);

        data.complete();
        assert_eq!(data.status, InferStatus::Complete);

        let mut data2 = InferStreamData::new("model");
        data2.fail();
        assert_eq!(data2.status, InferStatus::Failed);
    }

    #[test]
    fn test_tick() {
        let mut data = InferStreamData::new("model");
        assert_eq!(data.frame, 0);
        data.tick();
        assert_eq!(data.frame, 1);
    }

    #[test]
    fn test_status_indicators() {
        let (char, color) = InferStatus::Complete.indicator(0);
        assert_eq!(char, "✅");
        assert_eq!(color, DEFAULT_SUCCESS_COLOR);

        let (char, color) = InferStatus::Failed.indicator(0);
        assert_eq!(char, "❌");
        assert_eq!(color, DEFAULT_FAILED_COLOR);

        // Running shows spinner
        let (char1, _) = InferStatus::Running.indicator(0);
        let (char2, _) = InferStatus::Running.indicator(1);
        assert_ne!(char1, char2);
    }

    #[test]
    fn test_required_height() {
        let data = InferStreamData::new("model");
        let widget = InferStreamBox::new(&data);
        assert_eq!(widget.required_height(), 6); // Base height, no content

        let data = InferStreamData::new("model").with_content("Line1\nLine2\nLine3");
        let widget = InferStreamBox::new(&data);
        assert_eq!(widget.required_height(), 9); // 6 + 3 lines
    }

    #[test]
    fn test_max_lines() {
        let data = InferStreamData::new("model").with_content("1\n2\n3\n4\n5\n6\n7\n8\n9\n10");
        let widget = InferStreamBox::new(&data).max_lines(3);
        assert_eq!(widget.required_height(), 9); // 6 + min(10, 3) = 9
    }

    #[test]
    fn test_update_tokens() {
        let mut data = InferStreamData::new("model");
        data.update_tokens(100);
        assert_eq!(data.tokens_out, 100);
    }
}
