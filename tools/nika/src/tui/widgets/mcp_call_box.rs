//! MCP Call Box Widget
//!
//! Renders inline MCP tool call visualization with params, result, and timing.

use std::time::Duration;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

/// Status of an MCP call
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum McpCallStatus {
    #[default]
    Running,
    Success,
    Failed,
}

impl McpCallStatus {
    pub fn indicator(&self, frame: u8) -> (&'static str, Color) {
        match self {
            Self::Running => {
                let spinners = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];
                let idx = (frame as usize) % spinners.len();
                (spinners[idx], Color::Rgb(250, 204, 21)) // Yellow
            }
            Self::Success => ("✅", Color::Rgb(34, 197, 94)), // Green
            Self::Failed => ("❌", Color::Rgb(239, 68, 68)),  // Red
        }
    }
}

/// Data for rendering an MCP call box
#[derive(Debug, Clone, Default)]
pub struct McpCallData {
    /// Tool name (e.g., "novanet_describe")
    pub tool: String,
    /// MCP server name
    pub server: String,
    /// Input parameters (JSON string, truncated)
    pub params: String,
    /// Result (JSON string, truncated) - None if still running
    pub result: Option<String>,
    /// Error message if failed
    pub error: Option<String>,
    /// Call duration
    pub duration: Duration,
    /// Call status
    pub status: McpCallStatus,
    /// Whether result is expanded
    pub expanded: bool,
    /// Animation frame for spinner
    pub frame: u8,
    /// Retry count (v0.5.2+)
    pub retry_count: u8,
    /// Maximum retries allowed
    pub max_retries: u8,
}

/// Maximum default retries for MCP calls
pub const DEFAULT_MAX_RETRIES: u8 = 3;

impl McpCallData {
    pub fn new(tool: impl Into<String>, server: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            server: server.into(),
            params: String::new(),
            result: None,
            error: None,
            duration: Duration::ZERO,
            status: McpCallStatus::Running,
            expanded: false,
            frame: 0,
            retry_count: 0,
            max_retries: DEFAULT_MAX_RETRIES,
        }
    }

    /// Check if this call can be retried
    pub fn can_retry(&self) -> bool {
        self.status == McpCallStatus::Failed && self.retry_count < self.max_retries
    }

    /// Mark call for retry - resets to Running state
    pub fn mark_for_retry(&mut self) {
        if self.can_retry() {
            self.retry_count += 1;
            self.status = McpCallStatus::Running;
            self.error = None;
            self.result = None;
            self.duration = Duration::ZERO;
        }
    }

    /// Get retry info string for display
    pub fn retry_info(&self) -> Option<String> {
        if self.retry_count > 0 {
            Some(format!("(retry {}/{})", self.retry_count, self.max_retries))
        } else {
            None
        }
    }

    pub fn with_params(mut self, params: impl Into<String>) -> Self {
        self.params = params.into();
        self
    }

    pub fn with_result(mut self, result: impl Into<String>) -> Self {
        self.result = Some(result.into());
        self.status = McpCallStatus::Success;
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self.status = McpCallStatus::Failed;
        self
    }

    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    /// Update frame for animation
    pub fn tick(&mut self) {
        self.frame = self.frame.wrapping_add(1);
    }
}

/// MCP call box widget
pub struct McpCallBox<'a> {
    data: &'a McpCallData,
}

impl<'a> McpCallBox<'a> {
    pub fn new(data: &'a McpCallData) -> Self {
        Self { data }
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        let mut height = 3; // borders + header
        if !self.data.params.is_empty() {
            height += 1;
        }
        if self.data.result.is_some()
            || self.data.error.is_some()
            || self.data.status == McpCallStatus::Running
        {
            height += 1;
        }
        if self.data.expanded && self.data.result.is_some() {
            height += 3; // Extra lines for expanded content
        }
        height
    }

    /// Truncate string to fit width
    fn truncate(s: &str, max_len: usize) -> String {
        if s.len() > max_len {
            format!("{}...", &s[..max_len.saturating_sub(3)])
        } else {
            s.to_string()
        }
    }
}

impl Widget for McpCallBox<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 20 || area.height < 3 {
            return;
        }

        let (status_char, _status_color) = self.data.status.indicator(self.data.frame);

        // Border color: emerald for invoke
        let border_color = Color::Rgb(16, 185, 129); // Emerald
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(107, 114, 128));

        // Top border with title
        let duration_str = format!("{:.1}s", self.data.duration.as_secs_f64());
        let inner_width = area.width.saturating_sub(2) as usize;

        // Build title: ╭─ 🔧 MCP CALL: tool_name ─────── ✅ 1.2s ─╮
        let title_prefix = format!("╭─ 🔧 MCP CALL: {} ", self.data.tool);
        let title_suffix = format!(" {} {} ─╮", status_char, duration_str);
        let dash_count = inner_width
            .saturating_sub(title_prefix.chars().count())
            .saturating_sub(title_suffix.chars().count());
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);

        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Params line
        if !self.data.params.is_empty() && y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let max_params_len = (area.width - 15) as usize;
            let params_display = Self::truncate(&self.data.params, max_params_len);
            buf.set_string(
                area.x + 2,
                y,
                format!("📥 params: {}", params_display),
                dim_style,
            );
            y += 1;
        }

        // Result, error, or running status
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            if let Some(ref error) = self.data.error {
                let max_len = (area.width - 15) as usize;
                let error_display = Self::truncate(error, max_len);
                buf.set_string(
                    area.x + 2,
                    y,
                    format!("❌ Error: {}", error_display),
                    Style::default().fg(Color::Rgb(239, 68, 68)),
                );
            } else if let Some(ref result) = self.data.result {
                let max_len = (area.width - 15) as usize;
                let result_display = Self::truncate(result, max_len);
                buf.set_string(
                    area.x + 2,
                    y,
                    format!("📤 result: {}", result_display),
                    Style::default().fg(Color::Rgb(34, 197, 94)),
                );
            } else {
                buf.set_string(
                    area.x + 2,
                    y,
                    "⏳ Running...",
                    Style::default().fg(Color::Rgb(250, 204, 21)),
                );
            }
            y += 1;
        }

        // Fill remaining lines with empty bordered lines
        while y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
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
    fn test_mcp_call_creation() {
        let call = McpCallData::new("novanet_describe", "novanet")
            .with_params(r#"{ "entity": "qr-code" }"#)
            .with_duration(Duration::from_millis(1234));

        assert_eq!(call.tool, "novanet_describe");
        assert_eq!(call.server, "novanet");
        assert_eq!(call.status, McpCallStatus::Running);
        assert_eq!(call.duration, Duration::from_millis(1234));
    }

    #[test]
    fn test_mcp_call_success() {
        let call = McpCallData::new("novanet_describe", "novanet")
            .with_result(r#"{ "display_name": "QR Code" }"#);

        assert_eq!(call.status, McpCallStatus::Success);
        assert!(call.result.is_some());
        assert!(call.error.is_none());
    }

    #[test]
    fn test_mcp_call_failure() {
        let call = McpCallData::new("novanet_traverse", "novanet").with_error("Entity not found");

        assert_eq!(call.status, McpCallStatus::Failed);
        assert!(call.error.is_some());
        assert!(call.result.is_none());
    }

    #[test]
    fn test_status_indicators() {
        let (char, color) = McpCallStatus::Success.indicator(0);
        assert_eq!(char, "✅");
        assert_eq!(color, Color::Rgb(34, 197, 94));

        let (char, color) = McpCallStatus::Failed.indicator(0);
        assert_eq!(char, "❌");
        assert_eq!(color, Color::Rgb(239, 68, 68));

        // Running shows spinner
        let (char1, _) = McpCallStatus::Running.indicator(0);
        let (char2, _) = McpCallStatus::Running.indicator(1);
        assert_ne!(char1, char2); // Different frames
    }

    #[test]
    fn test_required_height() {
        // Minimal: 3 lines
        let call = McpCallData::new("tool", "server");
        let box_widget = McpCallBox::new(&call);
        assert_eq!(box_widget.required_height(), 4); // 3 base + 1 for running status

        // With params: +1
        let call = McpCallData::new("tool", "server").with_params("params");
        let box_widget = McpCallBox::new(&call);
        assert_eq!(box_widget.required_height(), 5);

        // With result: still counts
        let call = McpCallData::new("tool", "server")
            .with_params("params")
            .with_result("result");
        let box_widget = McpCallBox::new(&call);
        assert_eq!(box_widget.required_height(), 5);
    }

    #[test]
    fn test_truncate() {
        assert_eq!(McpCallBox::truncate("short", 10), "short");
        assert_eq!(
            McpCallBox::truncate("very long string here", 10),
            "very lo..."
        );
    }

    #[test]
    fn test_tick() {
        let mut call = McpCallData::new("tool", "server");
        assert_eq!(call.frame, 0);
        call.tick();
        assert_eq!(call.frame, 1);
        call.tick();
        assert_eq!(call.frame, 2);
    }

    #[test]
    fn test_with_frame() {
        let call = McpCallData::new("tool", "server").with_frame(5);
        assert_eq!(call.frame, 5);
    }

    #[test]
    fn test_default_status() {
        let call = McpCallData::default();
        assert_eq!(call.status, McpCallStatus::Running);
    }

    // === Retry Tests (HIGH 7) ===

    #[test]
    fn test_retry_initial_state() {
        let call = McpCallData::new("tool", "server");
        assert_eq!(call.retry_count, 0);
        assert_eq!(call.max_retries, super::DEFAULT_MAX_RETRIES);
        assert!(!call.can_retry()); // Running - not retryable
    }

    #[test]
    fn test_can_retry_failed_call() {
        let call = McpCallData::new("tool", "server").with_error("Connection failed");
        assert!(call.can_retry());
    }

    #[test]
    fn test_cannot_retry_success_call() {
        let call = McpCallData::new("tool", "server").with_result("success");
        assert!(!call.can_retry());
    }

    #[test]
    fn test_mark_for_retry_resets_state() {
        let mut call = McpCallData::new("tool", "server").with_error("Failed");
        assert_eq!(call.retry_count, 0);

        call.mark_for_retry();
        assert_eq!(call.retry_count, 1);
        assert_eq!(call.status, McpCallStatus::Running);
        assert!(call.error.is_none());
    }

    #[test]
    fn test_cannot_retry_beyond_max() {
        let mut call = McpCallData::new("tool", "server").with_error("Failed");
        call.max_retries = 2;

        // First retry
        call.mark_for_retry();
        call.status = McpCallStatus::Failed;
        call.error = Some("Failed again".into());
        assert!(call.can_retry());

        // Second retry
        call.mark_for_retry();
        call.status = McpCallStatus::Failed;
        call.error = Some("Failed yet again".into());
        assert!(!call.can_retry()); // Max retries reached

        assert_eq!(call.retry_count, 2);
    }

    #[test]
    fn test_retry_info() {
        let call = McpCallData::new("tool", "server");
        assert!(call.retry_info().is_none());

        let mut call = McpCallData::new("tool", "server").with_error("Failed");
        call.mark_for_retry();
        assert_eq!(
            call.retry_info(),
            Some(format!("(retry 1/{})", super::DEFAULT_MAX_RETRIES))
        );
    }
}
