//! AgentBox Widget
//!
//! Container for multi-turn agent execution with nested child boxes.
//! Shows turn progress, token metrics, nested tool calls, and final response.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::Widget,
};

use super::{BoxState, RenderMode, TaskBox, VerbColor};

/// AgentBox data and rendering
#[derive(Debug, Clone)]
pub struct AgentBox {
    /// Task ID
    pub task_id: String,
    /// Initial prompt
    pub prompt: String,
    /// Current turn number
    pub turn: u32,
    /// Maximum turns
    pub max_turns: u32,
    /// Input tokens
    pub tokens_in: u32,
    /// Output tokens
    pub tokens_out: u32,
    /// Estimated cost (USD)
    pub cost: f64,
    /// Number of tool calls made
    pub tool_calls: u32,
    /// Nested child boxes (tool calls, sub-agents, etc.)
    pub children: Vec<TaskBox>,
    /// Final response text
    pub final_response: Option<String>,
    /// Execution state
    pub state: BoxState,
    /// Is final response expanded
    pub expanded_response: bool,
    /// Is children section expanded
    pub expanded_children: bool,
    /// Pulse intensity for border animation (0.0-1.0)
    pub pulse_intensity: f32,
    /// Whether this is a spawned subagent (vs parent agent)
    pub is_subagent: bool,
    /// Nesting depth (0 = root, 1+ = spawned)
    pub depth: u8,
    /// Render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
}

impl AgentBox {
    /// Create a new AgentBox
    pub fn new(task_id: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            task_id: task_id.into(),
            prompt: prompt.into(),
            turn: 0,
            max_turns: 10,
            tokens_in: 0,
            tokens_out: 0,
            cost: 0.0,
            tool_calls: 0,
            children: Vec::new(),
            final_response: None,
            state: BoxState::default(),
            expanded_response: false,
            expanded_children: true, // Children expanded by default
            pulse_intensity: 0.0,
            is_subagent: false,
            depth: 0,
            render_mode: RenderMode::default(),
        }
    }

    /// Set the state
    pub fn with_state(mut self, state: BoxState) -> Self {
        self.state = state;
        self
    }

    /// Set turn info
    pub fn with_turn(mut self, current: u32, max: u32) -> Self {
        self.turn = current;
        self.max_turns = max;
        self
    }

    /// Set token counts
    pub fn with_tokens(mut self, input: u32, output: u32) -> Self {
        self.tokens_in = input;
        self.tokens_out = output;
        self
    }

    /// Set cost
    pub fn with_cost(mut self, cost: f64) -> Self {
        self.cost = cost;
        self
    }

    /// Set final response
    pub fn with_final_response(mut self, response: impl Into<String>) -> Self {
        self.final_response = Some(response.into());
        self
    }

    /// Set pulse intensity for border animation (clamped to 0.0-1.0)
    pub fn with_pulse_intensity(mut self, intensity: f32) -> Self {
        self.pulse_intensity = intensity.clamp(0.0, 1.0);
        self
    }

    /// Mark as spawned subagent with depth (clamped to max 10)
    pub fn as_subagent(mut self, depth: u8) -> Self {
        self.is_subagent = true;
        self.depth = depth.min(10);
        self
    }

    /// Get the display icon based on agent type
    pub fn icon(&self) -> &'static str {
        if self.is_subagent {
            "🐤" // Chick for subagent
        } else {
            "🐔" // Chicken for parent agent
        }
    }

    /// Add a child task box
    pub fn add_child(&mut self, child: TaskBox) {
        self.children.push(child);
        self.tool_calls = self.children.len() as u32;
    }

    /// Increment turn
    pub fn next_turn(&mut self) {
        self.turn += 1;
    }

    /// Toggle response expansion
    pub fn toggle_response(&mut self) {
        self.expanded_response = !self.expanded_response;
    }

    /// Toggle children expansion
    pub fn toggle_children(&mut self) {
        self.expanded_children = !self.expanded_children;
    }

    /// Calculate required height
    pub fn required_height(&self) -> u16 {
        let mut height: u16 = 6; // Header + metrics bar + separator + response header + footer + border

        // Children height (if expanded)
        if self.expanded_children {
            for child in &self.children {
                height += child.required_height() + 1; // +1 for spacing
            }
        } else if !self.children.is_empty() {
            height += 1; // Collapsed children line
        }

        // Response lines
        if self.expanded_response {
            if let Some(ref response) = self.final_response {
                height += response.lines().count().min(10) as u16;
            }
        } else if self.final_response.is_some() {
            height += 2;
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

    /// Format tokens for display
    fn format_tokens(tokens: u32) -> String {
        if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{:.1}K", tokens as f64 / 1_000.0)
        } else {
            tokens.to_string()
        }
    }
}

impl Widget for AgentBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width < 40 || area.height < 6 {
            return;
        }

        // Use Spawn color for subagents (lighter rose), Agent color for parent
        let verb = if self.is_subagent {
            VerbColor::Spawn
        } else {
            VerbColor::Agent
        };
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(Color::Rgb(100, 116, 139));
        let content_style = Style::default().fg(Color::Rgb(226, 232, 240));
        let metric_style = Style::default().fg(Color::Rgb(148, 163, 184));

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─────────────────────────────────────────────────────────────────────╮
        // Header:     │ 🐔 AGENT: Research competitors...           ⣾ Running  00:12       │
        let prompt_truncated = Self::truncate(&self.prompt, inner_width.saturating_sub(35));
        // Dynamic header label based on agent type
        let header_label = if self.is_subagent {
            if self.depth > 1 {
                format!("{} SUBAGENT (d{})", self.icon(), self.depth)
            } else {
                format!("{} SUBAGENT", self.icon())
            }
        } else {
            format!("{} AGENT", self.icon())
        };
        let title = format!(
            "╭─ {} {} {} {} ─╮",
            header_label, prompt_truncated, status_icon, status_suffix
        );
        // Pad with dashes to fill width
        let title_chars = title.chars().count();
        let title_padded = if title_chars < inner_width + 2 {
            let dashes_needed = inner_width + 2 - title_chars;
            let title_parts: Vec<&str> = title.splitn(2, " ─╮").collect();
            if title_parts.len() == 2 {
                format!("{}{}─╮", title_parts[0], "─".repeat(dashes_needed))
            } else {
                title
            }
        } else {
            title
        };
        buf.set_string(area.x, area.y, &title_padded, border_style);

        let mut y = area.y + 1;

        // Metrics bar: │ Turn 1/5 │ 📊 1.2K in / 456 out │ 💰 $0.02 │ 🔌 3 tools │
        if y < area.y + area.height - 1 {
            buf.set_string(area.x, y, "│", border_style);
            buf.set_string(area.x + area.width - 1, y, "│", border_style);

            let metrics = format!(
                "Turn {}/{} │ 📊 {} in / {} out │ 💰 ${:.2} │ 🔌 {} tools",
                self.turn,
                self.max_turns,
                Self::format_tokens(self.tokens_in),
                Self::format_tokens(self.tokens_out),
                self.cost,
                self.tool_calls
            );
            let metrics_truncated = Self::truncate(&metrics, inner_width - 2);
            buf.set_string(area.x + 2, y, &metrics_truncated, metric_style);
            y += 1;
        }

        // Separator
        if y < area.y + area.height - 1 {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // Children section
        if !self.children.is_empty() {
            if self.expanded_children {
                // Render each child with indentation
                for child in &self.children {
                    // PERF: Safe bounds calculation with saturating arithmetic
                    let available_height = area
                        .y
                        .saturating_add(area.height)
                        .saturating_sub(y)
                        .saturating_sub(3);
                    let child_height = child.required_height().min(available_height);

                    // Early exit if not enough space
                    if child_height < 3
                        || y.saturating_add(child_height)
                            >= area.y.saturating_add(area.height).saturating_sub(2)
                    {
                        break;
                    }

                    // Create indented area for child
                    let child_width = area.width.saturating_sub(4);
                    let child_area = Rect {
                        x: area.x + 2,
                        y,
                        width: child_width,
                        height: child_height,
                    };

                    // Skip rendering if child area too small
                    if child_area.width < 30 || child_area.height < 3 {
                        continue;
                    }

                    // Fill background for child area
                    for cy in child_area.y..child_area.y + child_area.height {
                        buf.set_string(area.x, cy, "│", border_style);
                        buf.set_string(area.x + area.width - 1, cy, "│", border_style);
                    }

                    // Render child (clone needed for Widget trait)
                    child.clone().render(child_area, buf);
                    y += child_height + 1;
                }
            } else {
                // Collapsed children summary
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);

                let children_summary = format!(
                    "  ▶ {} nested tasks (press Enter to expand)",
                    self.children.len()
                );
                buf.set_string(area.x + 2, y, &children_summary, dim_style);
                y += 1;
            }
        }

        // Separator before final response
        if y < area.y + area.height - 2 && self.final_response.is_some() {
            let sep = format!("├{}┤", "─".repeat(inner_width));
            buf.set_string(area.x, y, &sep, border_style);
            y += 1;
        }

        // FINAL RESPONSE section
        if let Some(ref response) = self.final_response {
            if y < area.y + area.height - 2 {
                buf.set_string(area.x, y, "│", border_style);
                buf.set_string(area.x + area.width - 1, y, "│", border_style);
                buf.set_string(area.x + 2, y, "FINAL RESPONSE", dim_style);
                y += 1;

                // Response content
                let response_lines: Vec<&str> = if self.expanded_response {
                    response.lines().take(10).collect()
                } else {
                    response.lines().take(2).collect()
                };

                for line in response_lines {
                    if y >= area.y + area.height - 1 {
                        break;
                    }
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);

                    let line_display = Self::truncate(line, inner_width - 4);
                    buf.set_string(area.x + 2, y, format!("┊ {}", line_display), content_style);
                    y += 1;
                }

                // Expansion hint
                if !self.expanded_response
                    && response.lines().count() > 2
                    && y < area.y + area.height - 1
                {
                    buf.set_string(area.x, y, "│", border_style);
                    buf.set_string(area.x + area.width - 1, y, "│", border_style);
                    buf.set_string(
                        area.x + 2,
                        y,
                        "┊ [truncated, press Enter to expand]",
                        dim_style,
                    );
                    y += 1;
                }
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
    use crate::tui::widgets::task_box::InvokeBox;

    #[test]
    fn test_agent_box_new() {
        let box_ = AgentBox::new("task-1", "Research competitors");
        assert_eq!(box_.task_id, "task-1");
        assert_eq!(box_.prompt, "Research competitors");
        assert_eq!(box_.turn, 0);
        assert_eq!(box_.max_turns, 10);
        assert!(box_.children.is_empty());
    }

    #[test]
    fn test_agent_box_with_turn() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(3, 5);
        assert_eq!(box_.turn, 3);
        assert_eq!(box_.max_turns, 5);
    }

    #[test]
    fn test_agent_box_with_tokens() {
        let box_ = AgentBox::new("task-1", "prompt").with_tokens(1000, 500);
        assert_eq!(box_.tokens_in, 1000);
        assert_eq!(box_.tokens_out, 500);
    }

    #[test]
    fn test_agent_box_with_cost() {
        let box_ = AgentBox::new("task-1", "prompt").with_cost(0.05);
        assert!((box_.cost - 0.05).abs() < f64::EPSILON);
    }

    #[test]
    fn test_agent_box_add_child() {
        let mut box_ = AgentBox::new("task-1", "prompt");
        assert_eq!(box_.tool_calls, 0);

        let invoke = TaskBox::Invoke(InvokeBox::new("novanet_describe", "novanet"));
        box_.add_child(invoke);

        assert_eq!(box_.children.len(), 1);
        assert_eq!(box_.tool_calls, 1);
    }

    #[test]
    fn test_agent_box_next_turn() {
        let mut box_ = AgentBox::new("task-1", "prompt");
        assert_eq!(box_.turn, 0);

        box_.next_turn();
        assert_eq!(box_.turn, 1);

        box_.next_turn();
        assert_eq!(box_.turn, 2);
    }

    #[test]
    fn test_toggle_sections() {
        let mut box_ = AgentBox::new("task-1", "prompt");
        assert!(!box_.expanded_response);
        assert!(box_.expanded_children); // Default expanded

        box_.toggle_response();
        assert!(box_.expanded_response);

        box_.toggle_children();
        assert!(!box_.expanded_children);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(AgentBox::format_tokens(500), "500");
        assert_eq!(AgentBox::format_tokens(1500), "1.5K");
        assert_eq!(AgentBox::format_tokens(1_500_000), "1.5M");
    }

    #[test]
    fn test_required_height() {
        let minimal = AgentBox::new("task-1", "prompt");
        assert!(minimal.required_height() >= 6);

        let with_response =
            AgentBox::new("task-1", "prompt").with_final_response("This is the response");
        assert!(with_response.required_height() > minimal.required_height());
    }

    #[test]
    fn test_with_final_response() {
        let box_ = AgentBox::new("task-1", "prompt").with_final_response("Based on my analysis...");

        assert_eq!(
            box_.final_response,
            Some("Based on my analysis...".to_string())
        );
    }

    #[test]
    fn test_agent_box_with_pulse() {
        let box_ = AgentBox::new("task-1", "Research competitors").with_pulse_intensity(0.7);
        assert!((box_.pulse_intensity - 0.7).abs() < 0.001);
    }

    #[test]
    fn test_agent_box_pulse_default_zero() {
        let box_ = AgentBox::new("task-1", "Research competitors");
        assert!((box_.pulse_intensity - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_agent_box_pulse_clamped() {
        let box_high = AgentBox::new("task-1", "prompt").with_pulse_intensity(1.5);
        assert!((box_high.pulse_intensity - 1.0).abs() < 0.001);

        let box_low = AgentBox::new("task-1", "prompt").with_pulse_intensity(-0.5);
        assert!((box_low.pulse_intensity - 0.0).abs() < 0.001);
    }

    // === Subagent visual tests ===

    #[test]
    fn test_agent_box_default_not_subagent() {
        let box_ = AgentBox::new("task-1", "prompt");
        assert!(!box_.is_subagent);
        assert_eq!(box_.depth, 0);
        assert_eq!(box_.icon(), "🐔");
    }

    #[test]
    fn test_agent_box_as_subagent() {
        let box_ = AgentBox::new("child-1", "Subtask").as_subagent(2);
        assert!(box_.is_subagent);
        assert_eq!(box_.depth, 2);
        assert_eq!(box_.icon(), "🐤");
    }

    #[test]
    fn test_agent_box_subagent_depth_clamped() {
        let box_ = AgentBox::new("task-1", "prompt").as_subagent(15);
        assert_eq!(box_.depth, 10); // Max depth clamped to 10
    }

    #[test]
    fn test_agent_box_icon_differs_by_type() {
        let parent = AgentBox::new("root", "Main task");
        let child = AgentBox::new("child", "Sub-task").as_subagent(1);

        assert_eq!(parent.icon(), "🐔");
        assert_eq!(child.icon(), "🐤");
        assert_ne!(parent.icon(), child.icon());
    }

    #[test]
    fn test_agent_box_subagent_depth_one_no_suffix() {
        // Depth 1 should not show (d1) in header - only depth > 1
        let sub = AgentBox::new("task", "prompt").as_subagent(1);
        assert_eq!(sub.depth, 1);
        // The header_label logic: depth > 1 shows "(dN)"
        // For depth=1, no suffix is shown
    }
}
