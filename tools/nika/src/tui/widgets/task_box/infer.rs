//! InferBox Widget
//!
//! LLM text generation box with streaming support.
//! Shows model, prompt, response, and token metrics.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use super::{BoxState, VerbColor};

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
    pub tokens_in: u32,
    /// Output tokens
    pub tokens_out: u32,
    /// Thinking tokens (Claude extended thinking)
    pub thinking_tokens: Option<u32>,
    /// Execution state
    pub state: BoxState,
    /// Is prompt section expanded
    pub expanded_prompt: bool,
    /// Is response section expanded
    pub expanded_response: bool,
    /// Show streaming cursor
    pub streaming_cursor: bool,
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
    pub fn with_tokens(mut self, input: u32, output: u32) -> Self {
        self.tokens_in = input;
        self.tokens_out = output;
        self
    }

    /// Set thinking tokens
    pub fn with_thinking(mut self, tokens: u32) -> Self {
        self.thinking_tokens = Some(tokens);
        self
    }

    /// Enable streaming cursor
    pub fn with_streaming_cursor(mut self, enabled: bool) -> Self {
        self.streaming_cursor = enabled;
        self
    }

    /// Append to response (streaming)
    pub fn append_response(&mut self, text: &str) {
        self.response.push_str(text);
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
        } else {
            "💬"
        }
    }
}

impl Widget for InferBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 30 || area.height < 5 {
            return;
        }

        let verb = VerbColor::Infer;
        let border_color = self.state.border_color(verb.rgb());
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));
        let content_style = Style::default().fg(Color::Rgb(226, 232, 240));

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─ ⚡ INFER ─────────────────────────── ✅ 1.4s ───╮
        let title_prefix = format!("╭─ {} ", verb.icon_label());
        let title_suffix = format!(" {} {} ─╮", status_icon, status_suffix);
        let dash_count = inner_width
            .saturating_sub(title_prefix.chars().count())
            .saturating_sub(title_suffix.chars().count());
        let title = format!("{}{}{}", title_prefix, "─".repeat(dash_count), title_suffix);
        buf.set_string(area.x, area.y, &title, border_style);

        let mut y = area.y + 1;

        // Model line
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let provider_icon = Self::provider_icon(&self.model);
            let model_display = Self::truncate(&self.model, inner_width - 10);
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
                Self::truncate(&self.prompt.replace('\n', " "), inner_width - 4)
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
            let header_truncated = Self::truncate(response_header, inner_width - 2);
            buf.set_string(area.x + 2, y, header_truncated, dim_style);
            y += 1;
        }

        // Response content
        if y < area.y + area.height - 2 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let response_display = if self.response.is_empty() {
                if self.state.is_running() {
                    "┊ ...".to_string()
                } else {
                    "┊ (no response)".to_string()
                }
            } else {
                let text = if self.expanded_response {
                    self.response.clone()
                } else {
                    Self::truncate(&self.response.replace('\n', " "), inner_width - 6)
                };
                let cursor = if self.streaming_cursor && self.state.is_running() {
                    "█"
                } else {
                    ""
                };
                format!("┊ {}{}", text, cursor)
            };
            buf.set_string(area.x + 2, y, &response_display, content_style);
            y += 1;
        }

        // Fill remaining lines
        while y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);
            y += 1;
        }

        // Footer with metrics
        let metrics = format!(
            "│ 📊 {} in │ {} out │ {} {}{}│",
            self.tokens_in,
            self.tokens_out,
            Self::provider_icon(&self.model),
            if self.model.len() > 10 {
                &self.model[..10]
            } else {
                &self.model
            },
            self.thinking_tokens
                .map(|t| format!(" │ 💭 thinking: {} ", t))
                .unwrap_or_default()
        );
        let metrics_truncated = Self::truncate(&metrics, inner_width + 2);
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
}
