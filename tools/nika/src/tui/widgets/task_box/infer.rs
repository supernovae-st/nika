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

use super::{BoxState, RenderMode, TokenVelocity, VerbColor};

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
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
    /// Token velocity tracker for sparkline display
    pub velocity: TokenVelocity,
    /// Estimated cost in USD
    pub cost: Option<f64>,
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
    pub fn calculate_cost(input_tokens: u32, output_tokens: u32, model: &str) -> f64 {
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
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
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
            Self::truncate(&self.model, 10),
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

    // === TokenVelocity wiring tests ===

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
}
