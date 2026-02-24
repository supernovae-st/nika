//! InvokeBox Widget
//!
//! MCP tool call box with params/result visualization.
//! Shows tool name, server, parameters, and result.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use super::{BoxState, VerbColor};

/// InvokeBox data and rendering
#[derive(Debug, Clone)]
pub struct InvokeBox {
    /// Tool name (e.g., "novanet_describe")
    pub tool: String,
    /// MCP server name
    pub server: String,
    /// Input parameters (JSON)
    pub params: serde_json::Value,
    /// Result (JSON) - None if still running
    pub result: Option<serde_json::Value>,
    /// Error message if failed
    pub error: Option<String>,
    /// Execution state
    pub state: BoxState,
    /// Is params section expanded
    pub expanded_params: bool,
    /// Is result section expanded
    pub expanded_result: bool,
}

impl InvokeBox {
    /// Create a new InvokeBox
    pub fn new(tool: impl Into<String>, server: impl Into<String>) -> Self {
        Self {
            tool: tool.into(),
            server: server.into(),
            params: serde_json::Value::Null,
            result: None,
            error: None,
            state: BoxState::default(),
            expanded_params: false,
            expanded_result: false,
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Set parameters
    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        self.params = params;
        self
    }

    /// Set parameters from string
    pub fn with_params_str(mut self, params: &str) -> Self {
        self.params = serde_json::from_str(params).unwrap_or(serde_json::Value::Null);
        self
    }

    /// Set result
    pub fn with_result(mut self, result: serde_json::Value) -> Self {
        self.result = Some(result);
        self
    }

    /// Set result from string
    pub fn with_result_str(mut self, result: &str) -> Self {
        self.result = serde_json::from_str(result).ok();
        self
    }

    /// Set error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Toggle params expansion
    pub fn toggle_params(&mut self) {
        self.expanded_params = !self.expanded_params;
    }

    /// Toggle result expansion
    pub fn toggle_result(&mut self) {
        self.expanded_result = !self.expanded_result;
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        let mut height: u16 = 5; // Header + server + params header + result header + bottom

        // Params section
        if self.expanded_params && !self.params.is_null() {
            let params_str = serde_json::to_string_pretty(&self.params).unwrap_or_default();
            height += params_str.lines().count().min(5) as u16;
        }

        // Result section
        if self.expanded_result && self.result.is_some() {
            let result_str = self
                .result
                .as_ref()
                .map(|r| serde_json::to_string_pretty(r).unwrap_or_default())
                .unwrap_or_default();
            height += result_str.lines().count().min(5) as u16;
        }

        height
    }

    /// Truncate string preserving UTF-8
    fn truncate(s: &str, max_len: usize) -> String {
        let char_count = s.chars().count();
        if char_count > max_len && max_len > 3 {
            let truncated: String = s.chars().take(max_len - 3).collect();
            format!("{}...", truncated)
        } else {
            s.to_string()
        }
    }

    /// Format JSON value for single-line display
    fn format_json_oneline(value: &serde_json::Value, max_len: usize) -> String {
        let json_str = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
        Self::truncate(&json_str, max_len)
    }
}

impl Widget for InvokeBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 5 {
            return;
        }

        let verb = VerbColor::Invoke;
        let border_color = self.state.border_color(verb.rgb());
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));
        let content_style = Style::default().fg(Color::Rgb(226, 232, 240));
        let success_style = Style::default().fg(Color::Rgb(34, 197, 94));
        let error_style = Style::default().fg(Color::Rgb(239, 68, 68));

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ 🔌 INVOKE: novanet_describe ────────── ✅ 0.8s ───╮
        let title_prefix = format!("╭─ {}: {} ", verb.icon_label(), self.tool);
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(title_prefix.chars().count())
            .saturating_sub(title_suffix.chars().count());
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);
        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Server line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(area.x + 2, y, format!("server: {}", self.server), dim_style);
            y += 1;
        }

        // Separator
        if y < area.y + area.height - 1 {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // PARAMS section
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(area.x + 2, y, "PARAMS", dim_style);
            y += 1;

            // Params content
            if !self.params.is_null() {
                if self.expanded_params {
                    let params_str = serde_json::to_string_pretty(&self.params).unwrap_or_default();
                    for line in params_str.lines().take(5) {
                        if y >= area.y + area.height - 3 {
                            break;
                        }
                        buf.set_string(area.x, y, "│", border_style);
                        buf.set_string(area.x + area.width - 1, y, "│", border_style);

                        let line_display = Self::truncate(line, inner_width - 4);
                        buf.set_string(area.x + 2, y, format!("┊ {}", line_display), content_style);
                        y += 1;
                    }
                } else {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let params_oneline = Self::format_json_oneline(&self.params, inner_width - 6);
                    buf.set_string(
                        area.x + 2,
                        y,
                        format!("┊ {}", params_oneline),
                        content_style,
                    );
                    y += 1;
                }
            }
        }

        // Separator
        if y < area.y + area.height - 2 {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // RESULT section
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let result_len = self
                .result
                .as_ref()
                .map(|r| serde_json::to_string(r).map(|s| s.len()).unwrap_or(0))
                .unwrap_or(0);
            let result_header = if result_len > 0 {
                format!(
                    "RESULT                                        ▼ {} chars",
                    result_len
                )
            } else {
                "RESULT".to_string()
            };
            let header_truncated = Self::truncate(&result_header, inner_width - 2);
            buf.set_string(area.x + 2, y, &header_truncated, dim_style);
            y += 1;

            // Result content
            if let Some(ref error) = self.error {
                if y < area.y + area.height - 2 {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let error_display = Self::truncate(error, inner_width - 6);
                    buf.set_string(
                        area.x + 2,
                        y,
                        format!("┊ ❌ {}", error_display),
                        error_style,
                    );
                    y += 1;
                }
            } else if let Some(ref result) = self.result {
                if y < area.y + area.height - 2 {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let result_display = Self::format_json_oneline(result, inner_width - 6);
                    buf.set_string(
                        area.x + 2,
                        y,
                        format!("┊ {}", result_display),
                        success_style,
                    );
                    y += 1;
                }
            } else if self.state.is_running() && y < area.y + area.height - 2 {
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);
                buf.set_string(area.x + 2, y, "┊ ⏳ Running...", dim_style);
                y += 1;
            }
        }

        // Fill remaining lines
        while y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
        }

        // Bottom border
        let bottom = format!("╰{}╯", "─".repeat(inner_width));
        buf.set_string(area.x, area.y + area.height - 1, &bottom, border_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invoke_box_new() {
        let box_ = InvokeBox::new("novanet_describe", "novanet");
        assert_eq!(box_.tool, "novanet_describe");
        assert_eq!(box_.server, "novanet");
        assert!(box_.params.is_null());
        assert!(box_.result.is_none());
    }

    #[test]
    fn test_invoke_box_with_params() {
        let params = serde_json::json!({
            "entity": "qr-code",
            "locale": "fr-FR"
        });
        let box_ = InvokeBox::new("novanet_describe", "novanet").with_params(params.clone());

        assert_eq!(box_.params, params);
    }

    #[test]
    fn test_invoke_box_with_params_str() {
        let box_ = InvokeBox::new("tool", "server").with_params_str(r#"{"key": "value"}"#);

        assert!(box_.params.is_object());
        assert_eq!(box_.params["key"], "value");
    }

    #[test]
    fn test_invoke_box_with_result() {
        let result = serde_json::json!({
            "entity": {
                "key": "qr-code",
                "display_name": "QR Code"
            }
        });
        let box_ = InvokeBox::new("tool", "server").with_result(result.clone());

        assert_eq!(box_.result, Some(result));
    }

    #[test]
    fn test_invoke_box_with_error() {
        let box_ = InvokeBox::new("tool", "server").with_error("Entity not found");
        assert_eq!(box_.error, Some("Entity not found".to_string()));
    }

    #[test]
    fn test_toggle_sections() {
        let mut box_ = InvokeBox::new("tool", "server");
        assert!(!box_.expanded_params);
        assert!(!box_.expanded_result);

        box_.toggle_params();
        assert!(box_.expanded_params);

        box_.toggle_result();
        assert!(box_.expanded_result);
    }

    #[test]
    fn test_format_json_oneline() {
        let value = serde_json::json!({"key": "value", "num": 42});
        let formatted = InvokeBox::format_json_oneline(&value, 50);

        assert!(formatted.contains("key"));
        assert!(formatted.contains("value"));
        assert!(formatted.contains("42"));
        assert!(!formatted.contains('\n'));
    }

    #[test]
    fn test_format_json_oneline_truncation() {
        let value = serde_json::json!({
            "very_long_key": "very_long_value_that_should_be_truncated"
        });
        let formatted = InvokeBox::format_json_oneline(&value, 20);

        assert!(formatted.len() <= 20);
        assert!(formatted.ends_with("..."));
    }

    #[test]
    fn test_required_height() {
        let minimal = InvokeBox::new("tool", "server");
        assert!(minimal.required_height() >= 5);

        let with_params =
            InvokeBox::new("tool", "server").with_params(serde_json::json!({"key": "value"}));
        // Same when not expanded
        assert!(with_params.required_height() >= 5);
    }
}
