// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! FetchBox Widget
//!
//! HTTP request box with request/response details.
//! Shows method, URL, headers, body, status, and timing.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, Widget},
};

use super::{http, BoxState, RenderMode, StreamingContext, VerbColor};
use crate::tokens::compat;
use crate::unicode::display_width;

/// FetchBox data and rendering
#[derive(Debug, Clone)]
pub struct FetchBox {
    /// HTTP method (GET, POST, etc.)
    pub method: String,
    /// Request URL
    pub url: String,
    /// Request headers
    pub request_headers: HashMap<String, String>,
    /// Request body (if any)
    pub request_body: Option<String>,
    /// HTTP status code
    pub status_code: Option<u16>,
    /// Response body
    pub response_body: Option<String>,
    /// Response size in bytes
    pub response_size: u64,
    /// Time to first byte (ms)
    pub ttfb_ms: u64,
    /// Retry count
    pub retries: u32,
    /// Execution state
    pub state: BoxState,
    /// Is request section expanded
    pub expanded_request: bool,
    /// Is response section expanded
    pub expanded_response: bool,
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
}

impl FetchBox {
    /// Create a new FetchBox
    pub fn new(method: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            method: method.into(),
            url: url.into(),
            request_headers: HashMap::new(),
            request_body: None,
            status_code: None,
            response_body: None,
            response_size: 0,
            ttfb_ms: 0,
            retries: 0,
            state: BoxState::default(),
            expanded_request: false,
            expanded_response: false,
            pulse_intensity: 0.0,
            render_mode: RenderMode::default(),
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Add a request header
    pub fn with_header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.request_headers.insert(key.into(), value.into());
        self
    }

    /// Set request body
    pub fn with_request_body(mut self, body: impl Into<String>) -> Self {
        self.request_body = Some(body.into());
        self
    }

    /// Set response status code
    pub fn with_status(mut self, code: u16) -> Self {
        self.status_code = Some(code);
        self
    }

    /// Set response body
    pub fn with_response_body(mut self, body: impl Into<String>) -> Self {
        self.response_body = Some(body.into());
        self
    }

    /// Set response size
    pub fn with_response_size(mut self, size: u64) -> Self {
        self.response_size = size;
        self
    }

    /// Set TTFB
    pub fn with_ttfb(mut self, ttfb_ms: u64) -> Self {
        self.ttfb_ms = ttfb_ms;
        self
    }

    /// Set retry count
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
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

    /// Toggle request expansion
    pub fn toggle_request(&mut self) {
        self.expanded_request = !self.expanded_request;
    }

    /// Toggle response expansion
    pub fn toggle_response(&mut self) {
        self.expanded_response = !self.expanded_response;
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        // Compact mode is always 1 line
        if self.render_mode == RenderMode::Compact {
            return 1;
        }

        let mut height: u16 = 5; // Header + method/url + request header + response header + footer

        // Request section
        if self.expanded_request {
            height += self.request_headers.len() as u16;
            if self.request_body.is_some() {
                height += 1;
            }
        }

        // Response section
        if self.expanded_response && self.response_body.is_some() {
            height += 3; // Some response lines
        }

        height
    }

    /// Truncate string preserving UTF-8, using display width for terminal columns
    fn truncate(s: &str, max_len: usize) -> String {
        let width = display_width(s);
        if width > max_len && max_len > 3 {
            let target = max_len - 3;
            let mut current = 0;
            let mut result = String::new();
            for c in s.chars() {
                let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
                if current + cw > target {
                    break;
                }
                result.push(c);
                current += cw;
            }
            format!("{}...", result)
        } else {
            s.to_string()
        }
    }

    /// Format bytes for display
    fn format_bytes(bytes: u64) -> String {
        if bytes < 1024 {
            format!("{} B", bytes)
        } else if bytes < 1024 * 1024 {
            format!("{:.1} KB", bytes as f64 / 1024.0)
        } else {
            format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
        }
    }

    /// Mask sensitive header values
    fn mask_header_value(key: &str, value: &str) -> String {
        let sensitive_keys = ["authorization", "x-api-key", "cookie", "set-cookie"];
        if sensitive_keys
            .iter()
            .any(|k| key.to_lowercase().contains(k))
        {
            "***".to_string()
        } else if value.len() > 50 {
            FetchBox::truncate(value, 50)
        } else {
            value.to_string()
        }
    }

    /// Render in compact mode (single line)
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        let verb = VerbColor::Fetch;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let line_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);
        let method_style = Style::default().fg(compat::CYAN_400); // Cyan

        let status_icon = self.state.icon();
        let status_text = self
            .status_code
            .map(|c| format!("{}", c))
            .unwrap_or_else(|| "...".to_string());

        // Format: 🛰️ FETCH: GET https://api.example.com  ✅ 200
        let prefix = format!("{} ", verb.icon_label());
        let suffix = format!("  {} {}", status_icon, status_text);
        let available = (area.width as usize)
            .saturating_sub(display_width(&prefix))
            .saturating_sub(display_width(&suffix))
            .saturating_sub(display_width(&self.method) + 1);
        let url = FetchBox::truncate(&self.url, available);

        // Build the line
        buf.set_string(area.x, area.y, &prefix, line_style);
        let x = area.x + display_width(&prefix) as u16;
        buf.set_string(x, area.y, &self.method, method_style);
        let x = x + display_width(&self.method) as u16 + 1;
        buf.set_string(x, area.y, &url, line_style);
        let x = area.x + (area.width as usize - display_width(&suffix)) as u16;
        buf.set_string(x, area.y, &suffix, dim_style);
    }

    /// Convert to list items for scrollable display
    ///
    /// Returns a vector of ListItem representing this FetchBox as styled lines
    /// for use in a ratatui List widget.
    pub fn to_list_items(&self, _ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        let verb = VerbColor::Fetch;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);
        let method_style = Style::default().fg(compat::CYAN_400); // Cyan

        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        let mut items: Vec<ListItem<'static>> = Vec::new();

        // Compact mode: single line
        if self.render_mode == RenderMode::Compact {
            let status_text = self
                .status_code
                .map(|c| format!("{}", c))
                .unwrap_or_else(|| "...".to_string());

            let line = Line::from(vec![
                Span::styled(format!("{} ", verb.icon_label()), border_style),
                Span::styled(format!("{} ", self.method), method_style),
                Span::styled(FetchBox::truncate(&self.url, 50), content_style),
                Span::styled(format!("  {} {}", status_icon, status_text), dim_style),
            ]);
            items.push(ListItem::new(line));
            return items;
        }

        // Top border: ╭─ 🛰️ FETCH ─────────────────────────── ✅ 0.5s ───╮
        let top_border = Line::from(vec![
            Span::styled("╭─ ", border_style),
            Span::styled(format!("{} ", verb.icon_label()), border_style),
            Span::styled(
                format!("─────────────────── {} {} ─╮", status_icon, status_suffix),
                border_style,
            ),
        ]);
        items.push(ListItem::new(top_border));

        // Method + URL line: │ GET https://api.example.com │
        let url_display = FetchBox::truncate(&self.url, 60);
        let method_line = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(format!("{} ", self.method), method_style),
            Span::styled(url_display, content_style),
        ]);
        items.push(ListItem::new(method_line));

        // Separator
        let separator = Line::from(vec![Span::styled(
            "├───────────────────────────────────────┤",
            border_style,
        )]);
        items.push(ListItem::new(separator.clone()));

        // REQUEST section
        let request_line = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled("REQUEST", dim_style),
        ]);
        items.push(ListItem::new(request_line));

        // Headers (if expanded)
        if self.expanded_request {
            for (key, value) in &self.request_headers {
                let masked_value = FetchBox::mask_header_value(key, value);
                let header_line = Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(format!("┊ {}: {}", key, masked_value), dim_style),
                ]);
                items.push(ListItem::new(header_line));
            }
        } else if !self.request_headers.is_empty() {
            let collapsed_line = Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(
                    format!("┊ headers: {{ {} entries }}", self.request_headers.len()),
                    dim_style,
                ),
            ]);
            items.push(ListItem::new(collapsed_line));
        }

        // Request body (if any)
        if let Some(ref body) = self.request_body {
            let body_display = FetchBox::truncate(body, 50);
            let body_line = Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(format!("┊ body: {}", body_display), dim_style),
            ]);
            items.push(ListItem::new(body_line));
        }

        // Separator before response
        items.push(ListItem::new(separator.clone()));

        // RESPONSE section with status code
        let status_color = self
            .status_code
            .map(http::status_color)
            .unwrap_or(compat::SLATE_500);
        let status_text = self
            .status_code
            .map(|c| format!("{} {}", c, http::status_text(c)))
            .unwrap_or_else(|| "...".to_string());

        let response_line = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled("RESPONSE ", dim_style),
            Span::styled(status_text, Style::default().fg(status_color)),
        ]);
        items.push(ListItem::new(response_line));

        // Response body (if expanded and present)
        if self.expanded_response {
            if let Some(ref body) = self.response_body {
                // Show full response with line breaks when expanded
                for line in body.lines() {
                    let body_line = Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled(format!("┊ {}", line), content_style),
                    ]);
                    items.push(ListItem::new(body_line));
                }
                // Handle empty body case
                if body.is_empty() {
                    let body_line = Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled("┊ (empty)", dim_style),
                    ]);
                    items.push(ListItem::new(body_line));
                }
            }
        }

        // Footer with metrics
        let size_str = FetchBox::format_bytes(self.response_size);
        let retry_str = if self.retries > 0 {
            format!(" │ 🔄 {}", self.retries)
        } else {
            String::new()
        };

        let footer_line = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(
                format!("📦 {} │ ⏱️ {}ms{}", size_str, self.ttfb_ms, retry_str),
                dim_style,
            ),
        ]);
        items.push(ListItem::new(footer_line));

        // Bottom border
        let bottom_border = Line::from(vec![Span::styled(
            "╰───────────────────────────────────────╯",
            border_style,
        )]);
        items.push(ListItem::new(bottom_border));

        items
    }
}

impl Widget for FetchBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl Widget for &FetchBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Compact mode: single line
        if self.render_mode == RenderMode::Compact {
            if area.width < 20 || area.height < 1 {
                return;
            }
            self.render_compact(area, buf);
            return;
        }

        if area.width < 30 || area.height < 5 {
            return;
        }

        let verb = VerbColor::Fetch;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ 🛰️ FETCH ─────────────────────────── ✅ 0.5s ───╮
        let title_prefix = format!("╭─ {} ", verb.icon_label());
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(display_width(&title_prefix))
            .saturating_sub(display_width(&title_suffix));
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);
        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Method + URL line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let method_style = Style::default().fg(compat::CYAN_400); // Cyan
            let url_display = FetchBox::truncate(&self.url, inner_width - self.method.len() - 4);
            buf.set_string(area.x + 2, y, &self.method, method_style);
            buf.set_string(
                area.x + 2 + self.method.len() as u16 + 1,
                y,
                &url_display,
                content_style,
            );
            y += 1;
        }

        // Separator
        if y < area.y + area.height - 1 {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // REQUEST section
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(area.x + 2, y, "REQUEST", dim_style);
            y += 1;

            // Headers (if expanded)
            if self.expanded_request {
                for (key, value) in &self.request_headers {
                    if y >= area.y + area.height - 3 {
                        break;
                    }
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let masked_value = FetchBox::mask_header_value(key, value);
                    let header_str = format!("┊ {}: {}", key, masked_value);
                    let header_display = FetchBox::truncate(&header_str, inner_width - 2);
                    buf.set_string(area.x + 2, y, &header_display, dim_style);
                    y += 1;
                }
            } else if !self.request_headers.is_empty() {
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);
                buf.set_string(
                    area.x + 2,
                    y,
                    format!("┊ headers: {{ {} entries }}", self.request_headers.len()),
                    dim_style,
                );
                y += 1;
            }

            // Body
            if let Some(ref body) = self.request_body {
                if y < area.y + area.height - 3 {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let body_display = FetchBox::truncate(body, inner_width - 10);
                    buf.set_string(
                        area.x + 2,
                        y,
                        format!("┊ body: {}", body_display),
                        dim_style,
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

        // RESPONSE section
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let status_color = self
                .status_code
                .map(http::status_color)
                .unwrap_or(compat::SLATE_500);
            let status_text = self
                .status_code
                .map(|c| format!("{} {}", c, http::status_text(c)))
                .unwrap_or_else(|| "...".to_string());

            let response_header = format!(
                "RESPONSE                                          {}",
                status_text
            );
            let header_truncated = FetchBox::truncate(&response_header, inner_width - 2);
            buf.set_string(
                area.x + 2,
                y,
                &header_truncated,
                Style::default().fg(status_color),
            );
            y += 1;

            // Response body
            if let Some(ref body) = self.response_body {
                if y < area.y + area.height - 2 {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let body_display = FetchBox::truncate(body, inner_width - 4);
                    buf.set_string(area.x + 2, y, format!("┊ {}", body_display), content_style);
                    y += 1;
                }
            }
        }

        // Fill remaining lines
        while y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
        }

        // Footer with metrics
        let size_str = FetchBox::format_bytes(self.response_size);
        let retry_str = if self.retries > 0 {
            format!(" │ 🔄 retries: {}", self.retries)
        } else {
            String::new()
        };

        let footer = format!(
            "│ 📦 {} │ ⏱️ TTFB: {}ms{}│",
            size_str, self.ttfb_ms, retry_str
        );
        let footer_truncated = FetchBox::truncate(&footer, inner_width + 2);
        buf.set_string(
            area.x,
            area.y + area.height - 2,
            &footer_truncated,
            dim_style,
        );

        // Bottom border
        let bottom = format!("╰{}╯", "─".repeat(inner_width));
        buf.set_string(area.x, area.y + area.height - 1, &bottom, border_style);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fetch_box_new() {
        let box_ = FetchBox::new("GET", "https://api.example.com/data");
        assert_eq!(box_.method, "GET");
        assert_eq!(box_.url, "https://api.example.com/data");
        assert!(box_.status_code.is_none());
    }

    #[test]
    fn test_fetch_box_with_response() {
        let box_ = FetchBox::new("POST", "https://api.example.com/data")
            .with_status(201)
            .with_response_body(r#"{"id": 123}"#)
            .with_response_size(1024)
            .with_ttfb(150);

        assert_eq!(box_.status_code, Some(201));
        assert_eq!(box_.response_body, Some(r#"{"id": 123}"#.to_string()));
        assert_eq!(box_.response_size, 1024);
        assert_eq!(box_.ttfb_ms, 150);
    }

    #[test]
    fn test_fetch_box_with_headers() {
        let box_ = FetchBox::new("GET", "https://api.example.com")
            .with_header("Authorization", "Bearer token123")
            .with_header("Accept", "application/json");

        assert_eq!(box_.request_headers.len(), 2);
        assert!(box_.request_headers.contains_key("Authorization"));
    }

    #[test]
    fn test_mask_header_value() {
        // Sensitive headers should be masked
        assert_eq!(
            FetchBox::mask_header_value("Authorization", "Bearer secret"),
            "***"
        );
        assert_eq!(FetchBox::mask_header_value("X-API-Key", "key123"), "***");
        assert_eq!(FetchBox::mask_header_value("Cookie", "session=abc"), "***");

        // Non-sensitive headers should be shown
        assert_eq!(
            FetchBox::mask_header_value("Accept", "application/json"),
            "application/json"
        );
        assert_eq!(
            FetchBox::mask_header_value("Content-Type", "text/html"),
            "text/html"
        );
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(FetchBox::format_bytes(100), "100 B");
        assert_eq!(FetchBox::format_bytes(1024), "1.0 KB");
        assert_eq!(FetchBox::format_bytes(1536), "1.5 KB");
        assert_eq!(FetchBox::format_bytes(1048576), "1.0 MB");
    }

    #[test]
    fn test_toggle_sections() {
        let mut box_ = FetchBox::new("GET", "https://example.com");
        assert!(!box_.expanded_request);
        assert!(!box_.expanded_response);

        box_.toggle_request();
        assert!(box_.expanded_request);

        box_.toggle_response();
        assert!(box_.expanded_response);
    }

    #[test]
    fn test_with_retries() {
        let box_ = FetchBox::new("GET", "https://example.com").with_retries(2);
        assert_eq!(box_.retries, 2);
    }

    #[test]
    fn test_required_height() {
        let minimal = FetchBox::new("GET", "https://example.com");
        assert!(minimal.required_height() >= 5);

        let with_headers =
            FetchBox::new("GET", "https://example.com").with_header("Accept", "application/json");
        // Should be same when not expanded
        assert!(with_headers.required_height() >= 5);
    }

    #[test]
    fn test_fetch_box_with_pulse() {
        let box_ = FetchBox::new("GET", "https://api.example.com").with_pulse_intensity(0.7);
        assert!((box_.pulse_intensity - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_fetch_box_pulse_default_zero() {
        let box_ = FetchBox::new("GET", "https://api.example.com");
        assert!((box_.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_fetch_box_pulse_clamped() {
        let box_high = FetchBox::new("GET", "https://example.com").with_pulse_intensity(1.5);
        assert!((box_high.pulse_intensity - 1.0).abs() < 0.001);

        let box_low = FetchBox::new("GET", "https://example.com").with_pulse_intensity(-0.5);
        assert!((box_low.pulse_intensity - 0.0).abs() < 0.001);
    }

    // === RenderMode tests ===

    #[test]
    fn test_fetch_box_with_render_mode() {
        let box_ =
            FetchBox::new("GET", "https://example.com").with_render_mode(RenderMode::Compact);
        assert_eq!(box_.render_mode, RenderMode::Compact);
    }

    #[test]
    fn test_fetch_box_compact_required_height() {
        let compact =
            FetchBox::new("GET", "https://example.com").with_render_mode(RenderMode::Compact);
        assert_eq!(compact.required_height(), 1);

        let expanded =
            FetchBox::new("GET", "https://example.com").with_render_mode(RenderMode::Expanded);
        assert!(expanded.required_height() >= 5);
    }

    #[test]
    fn test_fetch_box_compact_render() {
        // Verify compact mode produces a single-line output
        let box_ = FetchBox::new("GET", "https://api.example.com/data")
            .with_render_mode(RenderMode::Compact)
            .with_status(200);

        let mut buf = Buffer::empty(Rect::new(0, 0, 60, 1));
        box_.render(Rect::new(0, 0, 60, 1), &mut buf);

        // Check the buffer has content on the first line
        let line: String = (0..60)
            .map(|x| {
                buf.cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();
        assert!(line.contains("FETCH"));
        assert!(line.contains("GET"));
    }
}
