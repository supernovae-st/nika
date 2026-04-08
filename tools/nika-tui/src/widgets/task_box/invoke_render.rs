// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! InvokeBox rendering logic
//!
//! Extracted from `invoke.rs` — contains `Widget` impl, `to_list_items()`,
//! compact rendering, builtin-hint summaries, and display helpers.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, Widget},
};

use super::{RenderMode, StreamingContext, VerbColor};
use crate::tokens::compat;
use crate::unicode::display_width;

use super::invoke::{BuiltinHint, InvokeBox};

// ---------------------------------------------------------------------------
// Free helpers (were static methods on InvokeBox)
// ---------------------------------------------------------------------------

/// Truncate string preserving UTF-8, using display width for terminal columns.
pub(super) fn truncate(s: &str, max_len: usize) -> String {
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

/// Extract a string field from a JSON value, returning None if missing or wrong type.
pub(super) fn json_str_field<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.as_str())
}

// ---------------------------------------------------------------------------
// Rendering methods on InvokeBox
// ---------------------------------------------------------------------------

impl InvokeBox {
    /// Build a compact summary line for a known builtin tool.
    /// Returns None for Generic hint (caller falls back to default display).
    pub(crate) fn builtin_compact_summary(&self) -> Option<Vec<Span<'static>>> {
        let dim = Style::default().fg(compat::SLATE_500);
        let success = Style::default().fg(compat::GREEN_500);
        let error = Style::default().fg(compat::RED_500);

        match self.builtin_hint {
            BuiltinHint::FileRead => {
                let path = json_str_field(&self.params, "path")
                    .or_else(|| json_str_field(&self.params, "pattern"))
                    .unwrap_or("?");
                let path_display = truncate(path, 40);
                let mut spans = vec![
                    Span::styled("\u{1F4D6} ", Style::default()),
                    Span::styled(path_display, Style::default().fg(compat::SLATE_200)),
                ];
                if let Some(ref result) = self.result {
                    let summary = json_str_field(result, "summary")
                        .map(|s| s.to_string())
                        .or_else(|| result.as_array().map(|a| format!("{} matches", a.len())))
                        .or_else(|| result.as_str().map(|s| format!("{} chars", s.len())))
                        .unwrap_or_else(|| "done".to_string());
                    spans.push(Span::styled(" \u{2502} ", dim));
                    spans.push(Span::styled(summary, success));
                }
                Some(spans)
            }
            BuiltinHint::FileWrite => {
                let path = json_str_field(&self.params, "path")
                    .or_else(|| json_str_field(&self.params, "file_path"))
                    .unwrap_or("?");
                let path_display = truncate(path, 40);
                let mut spans = vec![
                    Span::styled("\u{270D} ", Style::default()),
                    Span::styled(path_display, Style::default().fg(compat::SLATE_200)),
                ];
                if let Some(ref result) = self.result {
                    let size = json_str_field(result, "bytes_written")
                        .map(|s| s.to_string())
                        .or_else(|| {
                            result
                                .get("bytes_written")
                                .and_then(|v| v.as_u64())
                                .map(|n| format!("{n} bytes"))
                        })
                        .unwrap_or_else(|| "wrote".to_string());
                    spans.push(Span::styled(" \u{2502} ", dim));
                    spans.push(Span::styled(format!("wrote {size}"), success));
                }
                Some(spans)
            }
            BuiltinHint::MediaThumbnail => {
                let mut spans = vec![Span::styled("\u{2361} ", Style::default())];
                if let Some(ref result) = self.result {
                    let dims = result.get("width").and_then(|w| w.as_u64()).and_then(|w| {
                        result
                            .get("height")
                            .and_then(|h| h.as_u64())
                            .map(|h| (w, h))
                    });
                    if let Some((w, h)) = dims {
                        spans.push(Span::styled(
                            format!("{w}x{h}"),
                            Style::default().fg(compat::SLATE_200),
                        ));
                    }
                    if let Some(fmt) = json_str_field(result, "format") {
                        spans.push(Span::styled(" \u{2502} ", dim));
                        spans.push(Span::styled(
                            fmt.to_string(),
                            Style::default().fg(compat::CYAN_500),
                        ));
                    }
                    if let Some(size) = result.get("size").and_then(|v| v.as_u64()) {
                        spans.push(Span::styled(" \u{2502} ", dim));
                        let display = if size > 1_048_576 {
                            format!("{:.1} MB", size as f64 / 1_048_576.0)
                        } else if size > 1024 {
                            format!("{:.1} KB", size as f64 / 1024.0)
                        } else {
                            format!("{size} B")
                        };
                        spans.push(Span::styled(display, success));
                    }
                } else {
                    let tool_short = self.tool.strip_prefix("nika:").unwrap_or(&self.tool);
                    spans.push(Span::styled(
                        tool_short.to_string(),
                        Style::default().fg(compat::SLATE_200),
                    ));
                }
                Some(spans)
            }
            BuiltinHint::Assert => {
                if self.error.is_some() {
                    Some(vec![Span::styled("\u{2717} assertion failed", error)])
                } else if self.result.is_some() {
                    Some(vec![Span::styled("\u{2713} condition passed", success)])
                } else {
                    Some(vec![Span::styled(
                        "assert...",
                        Style::default().fg(compat::SLATE_200),
                    )])
                }
            }
            BuiltinHint::Complete => {
                let mut spans = vec![Span::styled("\u{1F3C1} completed", success)];
                if let Some(ref result) = self.result {
                    if let Some(conf) = result.get("confidence").and_then(|v| v.as_f64()) {
                        spans.push(Span::styled(" \u{2502} ", dim));
                        spans.push(Span::styled(
                            format!("conf {conf:.0}%"),
                            Style::default().fg(compat::SLATE_200),
                        ));
                    }
                }
                Some(spans)
            }
            BuiltinHint::MediaPipeline | BuiltinHint::Import | BuiltinHint::Sleep => {
                // These use the default compact rendering for now
                None
            }
            BuiltinHint::Generic => None,
        }
    }

    /// Render in compact mode (single line)
    fn render_compact(&self, area: Rect, buf: &mut Buffer) {
        let verb = VerbColor::Invoke;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let line_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);

        let status_icon = self.state.icon();
        let server_info = format!("server: {}", self.server);

        // Format: 🔌 INVOKE: novanet_describe  ✅ server: novanet
        let prefix = format!("{}: {} ", verb.icon_label(), self.tool);
        let suffix = format!("  {} {}", status_icon, server_info);
        let available = (area.width as usize)
            .saturating_sub(display_width(&prefix))
            .saturating_sub(display_width(&suffix));

        // Build line with proper spacing
        let padding = " ".repeat(available);
        let line = format!("{}{}{}", prefix, padding, suffix);
        buf.set_string(area.x, area.y, &line, line_style);

        // Dim the server info portion
        let suffix_x = area.x + (display_width(&line) - display_width(&suffix)) as u16;
        buf.set_string(suffix_x, area.y, &suffix, dim_style);
    }

    /// Convert to ListItem for scrollable List widget
    pub fn to_list_items(&self, _ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        let verb = VerbColor::Invoke;
        let verb_color = verb.rgb();
        let dim_style = Style::default().fg(compat::SLATE_500);
        let success_style = Style::default().fg(compat::GREEN_500);
        let error_style = Style::default().fg(compat::RED_500);

        // Compact mode: single line
        if self.render_mode == RenderMode::Compact {
            let status_icon = self.state.icon();

            // Try builtin-specialized compact display first
            if let Some(builtin_spans) = self.builtin_compact_summary() {
                let mut spans = vec![
                    Span::styled(
                        format!("{}: {} ", verb.icon_label(), self.tool),
                        Style::default().fg(verb_color),
                    ),
                    Span::styled(format!("{} ", status_icon), Style::default().fg(verb_color)),
                ];
                spans.extend(builtin_spans);
                return vec![ListItem::new(Line::from(spans))];
            }

            // Generic fallback (non-builtin tools)
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
            Span::styled("╭─ ", border_style),
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
            Span::styled(self.tool.clone(), Style::default().fg(compat::SLATE_200)),
            Span::styled(" @ ", dim_style),
            Span::styled(self.server.clone(), Style::default().fg(compat::CYAN_500)),
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
                .map(|s| truncate(s, 50))
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
                    .map(|s| truncate(s, 50))
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
                Span::styled(truncate(error, 60), error_style),
            ])));
        }

        // Bottom border (rounded, matching other verbs)
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "╰─────────────────────────────────────────╯",
            border_style,
        )])));

        items
    }
}

impl Widget for InvokeBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl Widget for &InvokeBox {
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
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);
        let success_style = Style::default().fg(compat::GREEN_500);
        let error_style = Style::default().fg(compat::RED_500);

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ 🔌 INVOKE: novanet_describe ────────── ✅ 0.8s ───╮
        let title_prefix = format!("╭─ {}: {} ", verb.icon_label(), self.tool);
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(display_width(&title_prefix))
            .saturating_sub(display_width(&title_suffix));
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

                        let line_display = truncate(line, inner_width - 4);
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
                        .map(|s| truncate(s, inner_width - 6))
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
            let header_truncated = truncate(&result_header, inner_width - 2);
            buf.set_string(area.x + 2, y, &header_truncated, dim_style);
            y += 1;

            // Result content
            if let Some(ref error) = self.error {
                if y < area.y + area.height - 2 {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let error_display = truncate(error, inner_width - 6);
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
                        .map(|s| truncate(s, inner_width - 6))
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
    use super::super::BoxState;
    use super::*;

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
        assert!(content.contains("✓")); // checkmark (not emoji)
    }

    #[test]
    fn test_builtin_compact_summary_file_read() {
        let box_ = InvokeBox::new("nika:read", "builtin")
            .with_params(serde_json::json!({"path": "/tmp/test.txt"}))
            .with_result(serde_json::json!("file contents here"))
            .with_render_mode(RenderMode::Compact);

        let summary = box_.builtin_compact_summary();
        assert!(summary.is_some());
    }

    #[test]
    fn test_builtin_compact_summary_assert_pass() {
        let box_ = InvokeBox::new("nika:assert", "builtin")
            .with_result(serde_json::json!({"passed": true}))
            .with_render_mode(RenderMode::Compact);

        let spans = box_.builtin_compact_summary().unwrap();
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("condition passed"));
    }

    #[test]
    fn test_builtin_compact_summary_assert_fail() {
        let box_ = InvokeBox::new("nika:assert", "builtin")
            .with_error("expected 200 got 404")
            .with_render_mode(RenderMode::Compact);

        let spans = box_.builtin_compact_summary().unwrap();
        let text: String = spans.iter().map(|s| s.content.to_string()).collect();
        assert!(text.contains("assertion failed"));
    }

    #[test]
    fn test_builtin_compact_summary_generic_returns_none() {
        let box_ =
            InvokeBox::new("novanet_describe", "novanet").with_render_mode(RenderMode::Compact);

        assert!(box_.builtin_compact_summary().is_none());
    }
}
