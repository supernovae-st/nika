// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! InferBox Widget
//!
//! LLM text generation box with streaming support.
//! Shows model, prompt, response, and token metrics.
//!
//! MatrixDecrypt integration for streaming text reveal effect.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, Widget},
};

use super::{BoxState, RenderMode, StreamingContext, TokenVelocity, VerbColor};
use crate::tokens::compat;
use crate::unicode::display_width;
use crate::widgets::matrix_decrypt::{DecryptVerb, StreamingDecrypt};
use crate::widgets::micro;

/// InferBox data and rendering
#[derive(Debug, Clone)]
pub struct InferBox {
    /// LLM model name
    pub model: String,
    /// Input prompt
    pub prompt: String,
    /// Generated response (streamed)
    pub response: String,
    /// Input tokens
    pub tokens_in: u64,
    /// Output tokens
    pub tokens_out: u64,
    /// Thinking tokens (Claude extended thinking)
    pub thinking_tokens: Option<u64>,
    /// Execution state
    pub state: BoxState,
    /// Is prompt section expanded
    pub expanded_prompt: bool,
    /// Is response section expanded
    pub expanded_response: bool,
    /// Show streaming cursor
    pub streaming_cursor: bool,
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
    /// Token velocity tracker for sparkline display
    pub velocity: TokenVelocity,
    /// Estimated cost in USD
    pub cost: Option<f64>,
    /// Matrix decrypt effect for streaming response
    pub decrypt: StreamingDecrypt,
    /// Whether to use decrypt effect for streaming
    pub use_decrypt_effect: bool,
    /// Cached first line of response for collapsed mode (avoids alloc per frame)
    response_first_line: String,
}

impl InferBox {
    /// Create a new InferBox
    pub fn new(model: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            model: model.into(),
            prompt: prompt.into(),
            response: String::new(),
            tokens_in: 0,
            tokens_out: 0,
            thinking_tokens: None,
            state: BoxState::default(),
            expanded_prompt: false,
            expanded_response: false,
            streaming_cursor: false,
            pulse_intensity: 0.0,
            render_mode: RenderMode::default(),
            velocity: TokenVelocity::new(32),
            cost: None,
            decrypt: StreamingDecrypt::new().with_verb(DecryptVerb::Infer),
            use_decrypt_effect: true, // Enable by default
            response_first_line: String::new(),
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Set response text
    pub fn with_response(mut self, response: impl Into<String>) -> Self {
        self.response = response.into();
        self
    }

    /// Set token counts
    pub fn with_tokens(mut self, input: u64, output: u64) -> Self {
        self.tokens_in = input;
        self.tokens_out = output;
        self
    }

    /// Set thinking tokens
    pub fn with_thinking(mut self, tokens: u64) -> Self {
        self.thinking_tokens = Some(tokens);
        self
    }

    /// Enable streaming cursor
    pub fn with_streaming_cursor(mut self, enabled: bool) -> Self {
        self.streaming_cursor = enabled;
        self
    }

    /// Set pulse intensity for border animation (clamped to 0.0-1.0)
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Set velocity tracker
    pub fn with_velocity(mut self, velocity: TokenVelocity) -> Self {
        self.velocity = velocity;
        self
    }

    /// Push a token velocity sample (tokens/sec)
    pub fn push_velocity_sample(&mut self, tokens_per_sec: f32) {
        self.velocity.push(tokens_per_sec);
    }

    /// Calculate cost based on token counts and model
    pub fn calculate_cost(input_tokens: u64, output_tokens: u64, model: &str) -> f64 {
        let (input_rate, output_rate) = match model {
            m if m.contains("claude") => (3.0, 15.0), // $3/M in, $15/M out
            m if m.contains("gpt-4") => (5.0, 15.0),
            m if m.contains("gpt-3.5") => (0.5, 1.5),
            m if m.contains("mistral") => (2.0, 6.0),
            _ => (1.0, 3.0), // Default fallback
        };

        let input_cost = (input_tokens as f64 / 1_000_000.0) * input_rate;
        let output_cost = (output_tokens as f64 / 1_000_000.0) * output_rate;
        input_cost + output_cost
    }

    /// Update cost based on current token counts
    pub fn update_cost(&mut self) {
        self.cost = Some(Self::calculate_cost(
            self.tokens_in,
            self.tokens_out,
            &self.model,
        ));
    }

    /// Append to response (streaming)
    /// Also pushes text to decrypt effect for Matrix animation
    pub fn append_response(&mut self, text: &str) {
        self.response.push_str(text);
        // Update first-line cache (avoids replace('\n'," ") alloc every frame)
        if self.response_first_line.len() < 200 {
            let end = self.response.find('\n').unwrap_or(self.response.len());
            let end = end.min(200);
            self.response_first_line = self.response[..end].to_string();
        }
        // Push to decrypt for Matrix animation effect
        if self.use_decrypt_effect {
            self.decrypt.push_text(text);
        }
    }

    /// Advance the decrypt animation by one frame
    /// Call this on each TUI tick (e.g., 60fps)
    pub fn tick(&mut self) {
        if self.use_decrypt_effect {
            self.decrypt.tick();
        }
    }

    /// Force complete reveal (skip animation)
    pub fn reveal_response(&mut self) {
        self.decrypt.reveal_all();
    }

    /// Set render mode
    pub fn with_render_mode(mut self, mode: RenderMode) -> Self {
        self.render_mode = mode;
        self
    }

    /// Enable/disable decrypt effect
    pub fn with_decrypt_effect(mut self, enabled: bool) -> Self {
        self.use_decrypt_effect = enabled;
        self
    }

    /// Check if decrypt animation is complete
    pub fn is_decrypt_complete(&self) -> bool {
        !self.use_decrypt_effect || self.decrypt.is_complete()
    }

    /// Clear response and reset decrypt state
    pub fn clear_response(&mut self) {
        self.response.clear();
        self.response_first_line.clear();
        self.decrypt.clear();
    }

    /// Toggle prompt expansion
    pub fn toggle_prompt(&mut self) {
        self.expanded_prompt = !self.expanded_prompt;
    }

    /// Toggle response expansion
    pub fn toggle_response(&mut self) {
        self.expanded_response = !self.expanded_response;
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        let mut height: u16 = 5; // Header + model + prompt header + response header + footer

        // Prompt lines
        let prompt_lines = if self.expanded_prompt {
            self.prompt.lines().count().min(10) as u16
        } else {
            1
        };
        height += prompt_lines;

        // Response lines
        let response_lines = if self.expanded_response {
            self.response.lines().count().min(20) as u16
        } else if !self.response.is_empty() {
            2
        } else {
            1
        };
        height += response_lines;

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

    /// Get provider icon from model name
    fn provider_icon(model: &str) -> &'static str {
        if model.contains("claude") {
            "🧠"
        } else if model.contains("gpt") {
            "🤖"
        } else if model.contains("mistral") {
            "🌀"
        } else if model.contains("llama") {
            "🦙"
        } else if model.contains("deepseek") {
            "🔍"
        } else if model.contains("qwen") {
            "🌐"
        } else if model.contains("gemini") {
            "✨"
        } else if model.contains("grok") {
            "⚡"
        } else {
            "💬"
        }
    }

    /// Convert to list items for scrollable List widget
    pub fn to_list_items(&self, ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        let verb = VerbColor::Infer;
        let verb_color = verb.rgb();
        let dim_style = Style::default().fg(compat::SLATE_500);

        // Compact mode: single line summary
        if self.render_mode == RenderMode::Compact {
            let status = self.state.icon();
            let suffix = self.state.suffix();
            let line = Line::from(vec![
                Span::styled(
                    format!("{}: ", verb.icon_label()),
                    Style::default().fg(verb_color),
                ),
                Span::styled(
                    InferBox::truncate(&self.model, 20),
                    Style::default().fg(compat::SLATE_400),
                ),
                Span::styled(format!("  {} ", status), Style::default().fg(verb_color)),
                Span::styled(suffix.into_owned(), dim_style),
                Span::styled(
                    format!(" │ {}→{} tok", self.tokens_in, self.tokens_out),
                    dim_style,
                ),
            ]);
            return vec![ListItem::new(line)];
        }

        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let content_style = Style::default().fg(compat::SLATE_200);

        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        let mut items: Vec<ListItem<'static>> = Vec::new();

        // Header line: ╭─ ⚡ INFER ─────────── ✅ 1.4s ───╮
        let header = Line::from(vec![
            Span::styled("╭─ ", border_style),
            Span::styled(verb.icon_label(), border_style),
            Span::styled(" ─── ", border_style),
            Span::styled(format!("{} {}", status_icon, status_suffix), border_style),
            Span::styled(" ─╮", border_style),
        ]);
        items.push(ListItem::new(header));

        // Model line
        let provider_icon = InferBox::provider_icon(&self.model);
        let model_line = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(
                format!("model: {} {}", provider_icon, self.model),
                dim_style,
            ),
        ]);
        items.push(ListItem::new(model_line));

        // Separator
        let sep = Line::from(vec![Span::styled(
            "├────────────────────────────────────────┤",
            border_style,
        )]);
        items.push(ListItem::new(sep));

        // Prompt header
        let prompt_header = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled("PROMPT", dim_style),
        ]);
        items.push(ListItem::new(prompt_header));

        // Prompt content - always show the actual prompt (partial_response belongs in RESPONSE)
        if self.expanded_prompt {
            // Show full prompt with line breaks when expanded
            for line in self.prompt.lines() {
                let prompt_line = Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(format!("┊ {}", line), content_style),
                ]);
                items.push(ListItem::new(prompt_line));
            }
            // Handle empty prompt case
            if self.prompt.is_empty() {
                let prompt_line = Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled("┊ ", content_style),
                ]);
                items.push(ListItem::new(prompt_line));
            }
        } else {
            // Truncated single line
            let prompt_text = InferBox::truncate(&self.prompt.replace('\n', " "), 60);
            let prompt_line = Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(format!("┊ {}", prompt_text), content_style),
            ]);
            items.push(ListItem::new(prompt_line));
        }

        // Separator
        items.push(ListItem::new(Line::from(vec![Span::styled(
            "├────────────────────────────────────────┤",
            border_style,
        )])));

        // Response header
        let response_header_text = if self.streaming_cursor && self.state.is_running() {
            "RESPONSE ▼ streaming...".to_string()
        } else if !self.response.is_empty() {
            format!("RESPONSE ▼ {} chars", self.response.len())
        } else {
            "RESPONSE".to_string()
        };
        let response_header = Line::from(vec![
            Span::styled("│ ", border_style),
            Span::styled(response_header_text, dim_style),
        ]);
        items.push(ListItem::new(response_header));

        // Response content - use decrypt effect when streaming, plain text otherwise
        if self.response.is_empty() {
            let response_text = if self.state.is_running() {
                "┊ ...".to_string()
            } else {
                "┊ (no response)".to_string()
            };
            let response_line = Line::from(vec![
                Span::styled("│ ", border_style),
                Span::styled(response_text, content_style),
            ]);
            items.push(ListItem::new(response_line));
        } else if self.use_decrypt_effect && self.state.is_running() && !self.decrypt.is_complete()
        {
            // Use Matrix decrypt effect during streaming
            let decrypt_line = self.decrypt.build_line();
            // Prepend border and ┊ prefix
            let mut spans = vec![Span::styled("│ ┊ ", border_style)];
            spans.extend(decrypt_line.spans);
            // Add blinking streaming cursor (500ms on, 500ms off at 60fps)
            if self.streaming_cursor && (ctx.frame / 30) % 2 == 0 {
                spans.push(Span::styled("█", content_style));
            }
            items.push(ListItem::new(Line::from(spans)));
        } else {
            // Plain text (completed or decrypt disabled)
            let cursor =
                if self.streaming_cursor && self.state.is_running() && (ctx.frame / 30) % 2 == 0 {
                    "█"
                } else {
                    ""
                };

            if self.expanded_response {
                // Show full response with line breaks when expanded
                let lines: Vec<&str> = self.response.lines().collect();
                for (i, line) in lines.iter().enumerate() {
                    let is_last = i == lines.len() - 1;
                    let line_cursor = if is_last { cursor } else { "" };
                    let response_line = Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled(format!("┊ {}{}", line, line_cursor), content_style),
                    ]);
                    items.push(ListItem::new(response_line));
                }
                // Handle empty response case (shouldn't happen here but be defensive)
                if lines.is_empty() {
                    let response_line = Line::from(vec![
                        Span::styled("│ ", border_style),
                        Span::styled(format!("┊ {}", cursor), content_style),
                    ]);
                    items.push(ListItem::new(response_line));
                }
            } else {
                // Truncated single line (use cached first-line — avoids alloc per frame)
                let text = InferBox::truncate(&self.response_first_line, 60);
                let response_line = Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(format!("┊ {}{}", text, cursor), content_style),
                ]);
                items.push(ListItem::new(response_line));
            }
        }

        // Metrics footer
        let cost_str = self
            .cost
            .map(|c| format!(" │ 💰 ${:.4}", c))
            .unwrap_or_default();
        let velocity_str = if self.state.is_running() && !self.velocity.is_empty() {
            format!(
                " │ {} {:.0} tok/s",
                self.velocity.sparkline_chars(),
                self.velocity.average()
            )
        } else {
            String::new()
        };
        let metrics = format!(
            "│ 📊 {} in │ {} out │ {} {}{}{}",
            self.tokens_in,
            self.tokens_out,
            InferBox::provider_icon(&self.model),
            InferBox::truncate(&self.model, 10),
            cost_str,
            velocity_str
        );
        let metrics_line = Line::from(vec![Span::styled(metrics, dim_style)]);
        items.push(ListItem::new(metrics_line));

        // Throughput micro-widget (sparkline with peak)
        if !self.velocity.is_empty() {
            let samples = self.velocity.samples();
            let throughput =
                micro::throughput_meter(self.velocity.average(), &samples, self.velocity.peak());
            let mut spans = vec![Span::styled("│ ", border_style)];
            spans.extend(throughput.spans);
            items.push(ListItem::new(Line::from(spans)));
        }

        // Bottom border
        let bottom = Line::from(vec![Span::styled(
            "╰────────────────────────────────────────╯",
            border_style,
        )]);
        items.push(ListItem::new(bottom));

        items
    }
}

impl Widget for InferBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl Widget for &InferBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 5 {
            return;
        }

        let verb = VerbColor::Infer;
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ ⚡ INFER ─────────────────────────── ✅ 1.4s ───╮
        let title_prefix = format!("╭─ {} ", verb.icon_label());
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(display_width(&title_prefix))
            .saturating_sub(display_width(&title_suffix));
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);
        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Model line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let provider_icon = InferBox::provider_icon(&self.model);
            let model_display = InferBox::truncate(&self.model, inner_width - 10);
            buf.set_string(
                area.x + 2,
                y,
                format!("model: {} {}", provider_icon, model_display),
                dim_style,
            );
            y += 1;
        }

        // Separator
        if y < area.y + area.height - 1 {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // Prompt section
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            buf.set_string(area.x + 2, y, "PROMPT", dim_style);
            y += 1;
        }

        // Prompt content
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let prompt_display = if self.expanded_prompt {
                self.prompt.clone()
            } else {
                InferBox::truncate(&self.prompt.replace('\n', " "), inner_width - 4)
            };
            buf.set_string(
                area.x + 2,
                y,
                format!("┊ {}", prompt_display),
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

        // Response section header
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let response_header = if self.streaming_cursor && self.state.is_running() {
                "RESPONSE                                    ▼ streaming..."
            } else if !self.response.is_empty() {
                &format!(
                    "RESPONSE                                    ▼ {} chars",
                    self.response.len()
                )
            } else {
                "RESPONSE"
            };
            let header_truncated = InferBox::truncate(response_header, inner_width - 2);
            buf.set_string(area.x + 2, y, header_truncated, dim_style);
            y += 1;
        }

        // Response content - use decrypt effect when streaming, plain text otherwise
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            if self.response.is_empty() {
                let response_display = if self.state.is_running() {
                    "┊ ...".to_string()
                } else {
                    "┊ (no response)".to_string()
                };
                buf.set_string(area.x + 2, y, &response_display, content_style);
            } else if self.use_decrypt_effect
                && self.state.is_running()
                && !self.decrypt.is_complete()
            {
                // Use Matrix decrypt effect during streaming
                buf.set_string(area.x + 2, y, "┊ ", border_style);
                // Render decrypt effect directly to buffer
                let decrypt_area = Rect::new(
                    area.x + 4,
                    y,
                    (inner_width - 4).min(area.width.saturating_sub(4) as usize) as u16,
                    1,
                );
                self.decrypt.render(decrypt_area, buf);
                // Add streaming cursor
                if self.streaming_cursor {
                    let cursor_x =
                        area.x + 4 + display_width(self.decrypt.text()).min(inner_width - 6) as u16;
                    if cursor_x < area.x + area.width - 1 {
                        buf.set_string(cursor_x, y, "█", content_style);
                    }
                }
            } else {
                // Plain text (completed or decrypt disabled)
                let text = if self.expanded_response {
                    self.response.clone()
                } else {
                    InferBox::truncate(&self.response.replace('\n', " "), inner_width - 6)
                };
                let cursor = if self.streaming_cursor && self.state.is_running() {
                    "█"
                } else {
                    ""
                };
                buf.set_string(
                    area.x + 2,
                    y,
                    format!("┊ {}{}", text, cursor),
                    content_style,
                );
            }
            y += 1;
        }

        // Fill remaining lines
        while y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
        }

        // Footer with metrics
        let cost_str = self
            .cost
            .map(|c| format!(" │ 💰 ${:.4}", c))
            .unwrap_or_default();
        let velocity_str = if self.state.is_running() && !self.velocity.is_empty() {
            format!(
                " │ {} {:.0} tok/s",
                self.velocity.sparkline_chars(),
                self.velocity.average()
            )
        } else {
            String::new()
        };
        let metrics = format!(
            "│ 📊 {} in │ {} out │ {} {}{}{}{}│",
            self.tokens_in,
            self.tokens_out,
            InferBox::provider_icon(&self.model),
            InferBox::truncate(&self.model, 10),
            self.thinking_tokens
                .map(|t| format!(" │ 💭 thinking: {} ", t))
                .unwrap_or_default(),
            cost_str,
            velocity_str
        );
        let metrics_truncated = InferBox::truncate(&metrics, inner_width + 2);
        buf.set_string(
            area.x,
            area.y + area.height - 2,
            &metrics_truncated,
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
    fn test_infer_box_new() {
        let box_ = InferBox::new("claude-sonnet-4-6", "Generate a summary");
        assert_eq!(box_.model, "claude-sonnet-4-6");
        assert_eq!(box_.prompt, "Generate a summary");
        assert!(box_.response.is_empty());
        assert!(matches!(box_.state, BoxState::Queued));
    }

    #[test]
    fn test_infer_box_with_response() {
        let box_ = InferBox::new("gpt-4", "prompt")
            .with_response("Here is the response")
            .with_tokens(100, 50);

        assert_eq!(box_.response, "Here is the response");
        assert_eq!(box_.tokens_in, 100);
        assert_eq!(box_.tokens_out, 50);
    }

    #[test]
    fn test_infer_box_with_thinking() {
        let box_ = InferBox::new("claude-sonnet-4-6", "prompt").with_thinking(45);
        assert_eq!(box_.thinking_tokens, Some(45));
    }

    #[test]
    fn test_append_response() {
        let mut box_ = InferBox::new("model", "prompt");
        box_.append_response("Hello");
        box_.append_response(" World");
        assert_eq!(box_.response, "Hello World");
    }

    #[test]
    fn test_toggle_sections() {
        let mut box_ = InferBox::new("model", "prompt");
        assert!(!box_.expanded_prompt);
        assert!(!box_.expanded_response);

        box_.toggle_prompt();
        assert!(box_.expanded_prompt);

        box_.toggle_response();
        assert!(box_.expanded_response);
    }

    #[test]
    fn test_provider_icon() {
        assert_eq!(InferBox::provider_icon("claude-sonnet-4-6"), "🧠");
        assert_eq!(InferBox::provider_icon("gpt-4o"), "🤖");
        assert_eq!(InferBox::provider_icon("mistral-large"), "🌀");
        assert_eq!(InferBox::provider_icon("llama-3.2"), "🦙");
        assert_eq!(InferBox::provider_icon("deepseek-chat"), "🔍");
        assert_eq!(InferBox::provider_icon("unknown"), "💬");
    }

    #[test]
    fn test_truncate() {
        assert_eq!(InferBox::truncate("short", 10), "short");
        assert_eq!(InferBox::truncate("very long string", 10), "very lo...");
        assert_eq!(InferBox::truncate("ab", 10), "ab");
    }

    #[test]
    fn test_required_height() {
        let box_ = InferBox::new("model", "prompt");
        let height = box_.required_height();
        assert!(height >= 5);
    }

    #[test]
    fn test_streaming_cursor() {
        let box_ = InferBox::new("model", "prompt")
            .with_streaming_cursor(true)
            .with_state(BoxState::running());

        assert!(box_.streaming_cursor);
        assert!(box_.state.is_running());
    }

    #[test]
    fn test_infer_box_with_pulse() {
        let box_ =
            InferBox::new("claude-sonnet-4-6", "Generate a summary").with_pulse_intensity(0.8);
        assert!((box_.pulse_intensity - 0.8).abs() < 0.001);
    }

    #[test]
    fn test_infer_box_pulse_clamped() {
        // Test that values are clamped to 0.0-1.0 range
        let box_high = InferBox::new("model", "prompt").with_pulse_intensity(1.5);
        assert!((box_high.pulse_intensity - 1.0).abs() < 0.001);

        let box_low = InferBox::new("model", "prompt").with_pulse_intensity(-0.5);
        assert!((box_low.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_infer_box_pulse_default_zero() {
        let box_ = InferBox::new("model", "prompt");
        assert!((box_.pulse_intensity - 0.0).abs() < 0.001);
    }

    // === TokenVelocity tests ===

    #[test]
    fn test_infer_box_has_velocity() {
        let mut box_ = InferBox::new("model", "prompt");
        assert_eq!(box_.velocity.len(), 0);

        box_.push_velocity_sample(10.0);
        box_.push_velocity_sample(25.0);

        assert_eq!(box_.velocity.len(), 2);
        assert!((box_.velocity.average() - 17.5).abs() < 0.1);
    }

    #[test]
    fn test_infer_box_velocity_sparkline() {
        let mut box_ = InferBox::new("claude-sonnet-4", "prompt");
        for i in 0..8 {
            box_.push_velocity_sample(i as f32 * 10.0);
        }

        let sparkline = box_.velocity.sparkline_chars();
        assert_eq!(sparkline.chars().count(), 8);
    }

    // === Cost calculation tests ===

    #[test]
    fn test_cost_calculation_claude() {
        let cost = InferBox::calculate_cost(1000, 500, "claude-sonnet-4");
        // 1000 * $3/M + 500 * $15/M = $0.003 + $0.0075 = $0.0105
        assert!((cost - 0.0105).abs() < 0.0001);
    }

    #[test]
    fn test_cost_calculation_gpt4() {
        let cost = InferBox::calculate_cost(1000, 500, "gpt-4o");
        // 1000 * $5/M + 500 * $15/M = $0.005 + $0.0075 = $0.0125
        assert!((cost - 0.0125).abs() < 0.0001);
    }

    #[test]
    fn test_cost_calculation_gpt35() {
        let cost = InferBox::calculate_cost(10000, 5000, "gpt-3.5-turbo");
        // 10000 * $0.5/M + 5000 * $1.5/M = $0.005 + $0.0075 = $0.0125
        assert!((cost - 0.0125).abs() < 0.0001);
    }

    #[test]
    fn test_infer_box_update_cost() {
        let mut box_ = InferBox::new("claude-sonnet-4", "prompt").with_tokens(1000, 500);
        assert!(box_.cost.is_none());

        box_.update_cost();
        assert!(box_.cost.is_some());
        assert!((box_.cost.unwrap() - 0.0105).abs() < 0.0001);
    }

    // === Matrix Decrypt Integration Tests ===

    #[test]
    fn test_decrypt_effect_enabled_by_default() {
        let box_ = InferBox::new("model", "prompt");
        assert!(box_.use_decrypt_effect);
    }

    #[test]
    fn test_decrypt_effect_can_be_disabled() {
        let box_ = InferBox::new("model", "prompt").with_decrypt_effect(false);
        assert!(!box_.use_decrypt_effect);
    }

    #[test]
    fn test_append_response_pushes_to_decrypt() {
        let mut box_ = InferBox::new("model", "prompt");
        box_.append_response("Hello");

        // Both response string and decrypt state should have the text
        assert_eq!(box_.response, "Hello");
        assert_eq!(box_.decrypt.text(), "Hello");
    }

    #[test]
    fn test_tick_advances_decrypt_animation() {
        use crate::widgets::matrix_decrypt::StreamingDecrypt;

        let mut box_ = InferBox::new("model", "prompt");
        // Configure faster reveal for test (default is slow: 0.025 speed + 8 chaos frames)
        box_.decrypt = StreamingDecrypt::new()
            .with_reveal_speed(0.5) // Fast reveal
            .with_wave_factor(0.0); // No cascade delay
        box_.append_response("Test");

        // Initially not complete
        assert!(!box_.decrypt.is_complete());

        // Tick many times to reveal (8 chaos frames + reveal frames)
        for _ in 0..20 {
            box_.tick();
        }

        // Should be complete after enough ticks
        assert!(box_.is_decrypt_complete());
    }

    #[test]
    fn test_reveal_response_skips_animation() {
        let mut box_ = InferBox::new("model", "prompt");
        box_.append_response("Test text");

        // Initially not complete
        assert!(!box_.decrypt.is_complete());

        // Force reveal
        box_.reveal_response();

        // Should be immediately complete
        assert!(box_.is_decrypt_complete());
    }

    #[test]
    fn test_clear_response_resets_decrypt() {
        let mut box_ = InferBox::new("model", "prompt");
        box_.append_response("Test text");
        box_.clear_response();

        assert!(box_.response.is_empty());
        assert!(box_.decrypt.text().is_empty());
    }

    #[test]
    fn test_decrypt_verb_is_infer() {
        use crate::widgets::matrix_decrypt::DecryptVerb;
        let box_ = InferBox::new("model", "prompt");
        // The decrypt should be themed for Infer verb (violet color)
        assert_eq!(box_.decrypt.verb(), DecryptVerb::Infer);
    }
}
