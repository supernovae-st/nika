//! InvokeBox Widget
//!
//! MCP tool call box with params/result visualization.
//! Shows tool name, server, parameters, and result.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{ListItem, Widget},
};

use super::{BoxState, RenderMode, StreamingContext, VerbColor};

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
    // === PERF: JSON Cache Fields (avoid serde in render loop) ===
    // WARNING: These caches are ONLY updated via with_params()/with_result() builders.
    // Direct mutation of self.params/self.result will NOT invalidate the cache.
    // ALWAYS use builders or set_params()/set_result() methods to keep cache in sync.
    /// Cached one-line JSON for params (render collapsed mode)
    params_oneline_cached: Option<String>,
    /// Cached pretty JSON for params (render expanded mode)
    params_pretty_cached: Option<String>,
    /// Cached one-line JSON for result (render collapsed mode)
    result_oneline_cached: Option<String>,
    /// Cached pretty JSON for result (render expanded mode)
    result_pretty_cached: Option<String>,
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
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
            params_oneline_cached: None,
            params_pretty_cached: None,
            result_oneline_cached: None,
            result_pretty_cached: None,
            pulse_intensity: 0.0,
            render_mode: RenderMode::default(),
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Set parameters (pre-caches JSON strings for render performance)
    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        // PERF: Pre-compute JSON strings to avoid serde in render loop
        if !params.is_null() {
            self.params_oneline_cached = serde_json::to_string(&params).ok();
            self.params_pretty_cached = serde_json::to_string_pretty(&params).ok();
        }
        self.params = params;
        self
    }

    /// Set parameters from string (pre-caches JSON strings)
    pub fn with_params_str(self, params: &str) -> Self {
        let parsed = serde_json::from_str(params).unwrap_or(serde_json::Value::Null);
        self.with_params(parsed)
    }

    /// Set result (pre-caches JSON strings for render performance)
    pub fn with_result(mut self, result: serde_json::Value) -> Self {
        // PERF: Pre-compute JSON strings to avoid serde in render loop
        self.result_oneline_cached = serde_json::to_string(&result).ok();
        self.result_pretty_cached = serde_json::to_string_pretty(&result).ok();
        self.result = Some(result);
        self
    }

    /// Set result from string (pre-caches JSON strings)
    pub fn with_result_str(self, result: &str) -> Self {
        if let Ok(parsed) = serde_json::from_str(result) {
            self.with_result(parsed)
        } else {
            self
        }
    }

    /// Set error
    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    /// Set pulse intensity for border animation (clamped to 0.0-1.0)
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
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
    /// PERF: Uses cached JSON strings to avoid serde in layout calculation
    pub fn required_height(&self) -> u16 {
        // Compact mode is always 1 line
        if self.render_mode == RenderMode::Compact {
            return 1;
        }

        let mut height: u16 = 5; // Header + server + params header + result header + bottom

        // Params section - PERF: use cached pretty JSON
        if self.expanded_params && !self.params.is_null() {
            let lines = self
                .params_pretty_cached
                .as_ref()
                .map(|s| s.lines().count())
                .unwrap_or(0);
            height += lines.min(5) as u16;
        }

        // Result section - PERF: use cached pretty JSON
        if self.expanded_result && self.result.is_some() {
            let lines = self
                .result_pretty_cached
                .as_ref()
                .map(|s| s.lines().count())
                .unwrap_or(0);
            height += lines.min(5) as u16;
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
    #[allow(dead_code)]
    fn format_json_oneline(value: &serde_json::Value, max_len: usize) -> String {
        let json_str = serde_json::to_string(value).unwrap_or_else(|_| "null".to_string());
        Self::truncate(&json_str, max_len)
    }

    /// Render in compact mode (single line)
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        let verb = VerbColor::Invoke;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let line_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));

        let status_icon = self.state.icon();
        let server_info = format!("server: {}", self.server);

        // Format: 🔌 INVOKE: novanet_describe  ✅ server: novanet
        let prefix = format!("{}: {} ", verb.icon_label(), self.tool);
        let suffix = format!("  {} {}", status_icon, server_info);
        let available = (area.width as usize)
            .saturating_sub(prefix.chars().count())
            .saturating_sub(suffix.chars().count());

        // Build line with proper spacing
        let padding = " ".repeat(available);
        let line = format!("{}{}{}", prefix, padding, suffix);
        buf.set_string(area.x, area.y, &line, line_style);

        // Dim the server info portion
        let suffix_x = area.x + (line.chars().count() - suffix.chars().count()) as u16;
        buf.set_string(suffix_x, area.y, &suffix, dim_style);
    }

    /// Convert to ListItem for scrollable List widget
    pub fn to_list_items(&self, _ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        let verb = VerbColor::Invoke;
        let verb_color = verb.rgb();
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));
        let success_style = Style::default().fg(Color::Rgb(34, 197, 94));
        let error_style = Style::default().fg(Color::Rgb(239, 68, 68));

        // Compact mode: single line
        if self.render_mode == RenderMode::Compact {
            let status_icon = self.state.icon();
            let line = Line::from(vec![
                Span::styled(
                    format!("{}: {} ", verb.icon_label(), self.tool),
                    Style::default().fg(verb_color),
                ),
                Span::styled(status_icon, Style::default().fg(verb_color)),
                Span::styled(format!(" server: {}", self.server), dim_style),
            ]);
            return vec![ListItem::new(line)];
        }

        // Expanded mode: multi-line box
        let mut items = Vec::new();
        let border_color = self
            .state
            .border_color_with_pulse(verb_color, self.pulse_intensity);
        let border_style = Style::default().fg(border_color);

        // Top border with verb icon and status
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix().into_owned();
        items.push(ListItem::new(Line::from(vec![
            Span::styled("┌─ ", border_style),
            Span::styled(verb.icon_label(), Style::default().fg(verb_color)),
            Span::styled(
                format!(" {} ", status_icon),
                Style::default().fg(verb_color),
            ),
            Span::styled(status_suffix, dim_style),
        ])));

        // Tool and server line
        items.push(ListItem::new(Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled("Tool: ", dim_style),
            Span::styled(self.tool.clone(), Style::default().fg(Color::White)),
            Span::styled(" @ ", dim_style),
            Span::styled(self.server.clone(), Style::default().fg(Color::Cyan)),
        ])));

        // PARAMS section
        let params_indicator = if self.expanded_params { "▼" } else { "▶" };
        if self.expanded_params && !self.params.is_null() {
            // Expanded params - show pretty JSON
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(format!("{} PARAMS:", params_indicator), dim_style),
            ])));
            if let Some(ref pretty) = self.params_pretty_cached {
                for line in pretty.lines().take(5) {
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│   ", border_style),
                        Span::styled(line.to_string(), dim_style),
                    ])));
                }
            }
        } else {
            // Collapsed params - show one-line
            let params_preview = self
                .params_oneline_cached
                .as_ref()
                .map(|s| Self::truncate(s, 50))
                .unwrap_or_else(|| "null".to_string());
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(format!("{} PARAMS: ", params_indicator), dim_style),
                Span::styled(params_preview, dim_style),
            ])));
        }

        // RESULT section (if present)
        if let Some(ref _result) = self.result {
            let result_indicator = if self.expanded_result { "▼" } else { "▶" };
            if self.expanded_result {
                // Expanded result
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(format!("{} RESULT:", result_indicator), success_style),
                ])));
                if let Some(ref pretty) = self.result_pretty_cached {
                    for line in pretty.lines().take(5) {
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│   ", border_style),
                            Span::styled(line.to_string(), success_style),
                        ])));
                    }
                }
            } else {
                // Collapsed result
                let result_preview = self
                    .result_oneline_cached
                    .as_ref()
                    .map(|s| Self::truncate(s, 50))
                    .unwrap_or_else(|| "...".to_string());
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(format!("{} RESULT: ", result_indicator), success_style),
                    Span::styled(result_preview, success_style),
                ])));
            }
        }

        // ERROR section (if present)
        if let Some(ref error) = self.error {
            items.push(ListItem::new(Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled("❌ ERROR: ", error_style),
                Span::styled(Self::truncate(error, 60), error_style),
            ])));
        }

        // Bottom border
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "└─────────────────────────────────────────",
            border_style,
        )])));

        items
    }
}

impl Widget for InvokeBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Check for compact mode first
        if self.render_mode == RenderMode::Compact {
            self.render_compact(area, buf);
            return;
        }

        if area.width < 30 || area.height < 5 {
            return;
        }

        let verb = VerbColor::Invoke;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
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

            // Params content - PERF: uses pre-cached JSON strings
            if !self.params.is_null() {
                if self.expanded_params {
                    // Use cached pretty JSON if available
                    let params_str = self.params_pretty_cached.as_deref().unwrap_or("null");
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

                    // PERF: Use cached oneline JSON, truncate for display
                    let params_oneline = self
                        .params_oneline_cached
                        .as_ref()
                        .map(|s| Self::truncate(s, inner_width - 6))
                        .unwrap_or_else(|| "null".to_string());
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

        // RESULT section - PERF: uses pre-cached JSON strings
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            // PERF: Use cached string length instead of serializing
            let result_len = self
                .result_oneline_cached
                .as_ref()
                .map(|s| s.len())
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
            } else if self.result.is_some() {
                if y < area.y + area.height - 2 {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    // PERF: Use cached oneline JSON, truncate for display
                    let result_display = self
                        .result_oneline_cached
                        .as_ref()
                        .map(|s| Self::truncate(s, inner_width - 6))
                        .unwrap_or_else(|| "null".to_string());
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

    // === Cache Coherency Tests ===

    #[test]
    fn test_params_cache_populated_on_with_params() {
        let box_ = InvokeBox::new("tool", "server")
            .with_params(serde_json::json!({"key": "value", "num": 42}));

        // Cache should be populated
        assert!(box_.params_oneline_cached.is_some());
        assert!(box_.params_pretty_cached.is_some());

        // Cache content should match params
        let oneline = box_.params_oneline_cached.unwrap();
        assert!(oneline.contains("key"));
        assert!(oneline.contains("value"));
        assert!(oneline.contains("42"));

        // Pretty cache should have newlines (multi-line format)
        let pretty = box_.params_pretty_cached.unwrap();
        assert!(pretty.contains('\n'));
    }

    #[test]
    fn test_result_cache_populated_on_with_result() {
        let box_ = InvokeBox::new("tool", "server")
            .with_result(serde_json::json!({"entity": "qr-code", "locale": "fr-FR"}));

        // Cache should be populated
        assert!(box_.result_oneline_cached.is_some());
        assert!(box_.result_pretty_cached.is_some());

        // Cache content should match result
        let oneline = box_.result_oneline_cached.unwrap();
        assert!(oneline.contains("qr-code"));
        assert!(oneline.contains("fr-FR"));
    }

    #[test]
    fn test_empty_params_no_cache() {
        let box_ = InvokeBox::new("tool", "server");

        // No params = no cache
        assert!(box_.params_oneline_cached.is_none());
        assert!(box_.params_pretty_cached.is_none());
    }

    #[test]
    fn test_null_params_no_cache() {
        let box_ = InvokeBox::new("tool", "server").with_params(serde_json::Value::Null);

        // Null params = no cache (is_null() check in with_params)
        assert!(box_.params_oneline_cached.is_none());
        assert!(box_.params_pretty_cached.is_none());
    }

    #[test]
    fn test_params_str_uses_with_params() {
        let box_ = InvokeBox::new("tool", "server").with_params_str(r#"{"from": "string"}"#);

        // Should populate cache via with_params delegation
        assert!(box_.params_oneline_cached.is_some());
        let oneline = box_.params_oneline_cached.unwrap();
        assert!(oneline.contains("from"));
        assert!(oneline.contains("string"));
    }

    #[test]
    fn test_required_height_uses_cached_lines() {
        let mut box_ = InvokeBox::new("tool", "server").with_params(serde_json::json!({
            "line1": "value1",
            "line2": "value2",
            "line3": "value3"
        }));

        // Collapsed: base height
        let collapsed_height = box_.required_height();

        // Expanded: should use cached pretty JSON lines
        box_.expanded_params = true;
        let expanded_height = box_.required_height();

        // Expanded should be taller (adds lines from pretty JSON)
        assert!(expanded_height > collapsed_height);
    }

    #[test]
    fn test_invoke_box_with_pulse() {
        let box_ = InvokeBox::new("novanet_describe", "novanet").with_pulse_intensity(0.7);
        assert!((box_.pulse_intensity - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_invoke_box_pulse_default_zero() {
        let box_ = InvokeBox::new("novanet_describe", "novanet");
        assert!((box_.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_invoke_box_pulse_clamped() {
        let box_high = InvokeBox::new("tool", "server").with_pulse_intensity(1.5);
        assert!((box_high.pulse_intensity - 1.0).abs() < 0.001);

        let box_low = InvokeBox::new("tool", "server").with_pulse_intensity(-0.5);
        assert!((box_low.pulse_intensity - 0.0).abs() < 0.001);
    }

    // === Compact Mode Tests ===

    #[test]
    fn test_invoke_box_with_render_mode() {
        let box_ =
            InvokeBox::new("novanet_describe", "novanet").with_render_mode(RenderMode::Compact);
        assert_eq!(box_.render_mode, RenderMode::Compact);
    }

    #[test]
    fn test_invoke_box_compact_required_height() {
        let box_ =
            InvokeBox::new("novanet_describe", "novanet").with_render_mode(RenderMode::Compact);
        assert_eq!(box_.required_height(), 1);
    }

    #[test]
    fn test_invoke_box_compact_render() {
        let box_ = InvokeBox::new("novanet_describe", "novanet")
            .with_state(BoxState::Success { duration_ms: 150 })
            .with_render_mode(RenderMode::Compact);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
        box_.render(Rect::new(0, 0, 60, 1), &mut buf);

        let content: String = buf
            .content
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
            .trim_end()
            .to_string();

        // Should contain verb icon/label, tool name, and status
        assert!(content.contains("INVOKE"));
        assert!(content.contains("novanet_describe"));
        assert!(content.contains("✅"));
    }
}
