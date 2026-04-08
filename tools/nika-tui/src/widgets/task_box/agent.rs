// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! AgentBox Widget
//!
//! Container for multi-turn agent execution with nested child boxes.
//! Shows turn progress, token metrics, nested tool calls, and final response.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{ListItem, Widget},
};

use super::StreamingContext;

use super::{BoxState, RenderMode, TaskBox, TokenVelocity, VerbColor};
use crate::tokens::compat;
use crate::unicode::display_width;
use crate::utils::format_number_compact;
use crate::widgets::micro;

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
    pub tokens_in: u64,
    /// Output tokens
    pub tokens_out: u64,
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
    /// Token velocity tracker for sparkline display
    pub velocity: TokenVelocity,
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
            velocity: TokenVelocity::new(32),
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
    pub fn with_tokens(mut self, input: u64, output: u64) -> Self {
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

    /// Push a token velocity sample (tokens/sec)
    pub fn push_velocity_sample(&mut self, tokens_per_sec: f32) {
        self.velocity.push(tokens_per_sec);
    }

    /// Get turn progress as ratio (0.0 to 1.0)
    pub fn turn_progress_ratio(&self) -> f32 {
        if self.max_turns == 0 {
            return 0.0;
        }
        (self.turn as f32 / self.max_turns as f32).min(1.0)
    }

    /// Generate a progress bar string for turn progress
    /// Format: "[████████████░░░░░░░░] 60% (turn 3/5)"
    pub fn turn_progress_bar(&self, width: usize) -> String {
        let ratio = self.turn_progress_ratio();
        let percent = (ratio * 100.0).round() as u32;
        let filled = (ratio * width as f32).round() as usize;
        let empty = width.saturating_sub(filled);

        format!(
            "[{}{}] {}% (turn {}/{})",
            "█".repeat(filled),
            "░".repeat(empty),
            percent,
            self.turn,
            self.max_turns
        )
    }

    /// Get status icons for all children (compact display)
    /// Returns icons like "✅✅✅⣾❌" where each icon represents a child's state
    pub fn children_status_icons(&self) -> String {
        const MAX_ICONS: usize = 10;

        if self.children.is_empty() {
            return String::new();
        }

        let mut icons = String::new();
        let display_count = self.children.len().min(MAX_ICONS);

        for child in self.children.iter().take(display_count) {
            icons.push_str(child.state().icon());
        }

        // Add overflow indicator if more children than displayed
        if self.children.len() > MAX_ICONS {
            icons.push_str(&format!("+{}", self.children.len() - MAX_ICONS));
        }

        icons
    }

    /// Generate a compact single-line representation for collapsed view
    /// Format: "🐔 turn 3/5 [✅✅⣾] Research competitors..."
    pub fn compact_line(&self, max_width: usize) -> String {
        let icon = self.icon();
        let state_icon = self.state.icon();
        let turn_info = format!("{}/{}", self.turn, self.max_turns);
        let children_icons = self.children_status_icons();

        // Build fixed prefix: "🐔 ⣾ 3/5"
        let prefix = format!("{} {} {}", icon, state_icon, turn_info);

        // Add children icons if present: "[✅✅]"
        let children_part = if children_icons.is_empty() {
            String::new()
        } else {
            format!(" [{}]", children_icons)
        };

        // Add response preview or prompt
        let text = if let Some(ref response) = self.final_response {
            AgentBox::truncate(response.lines().next().unwrap_or(""), 30)
        } else {
            AgentBox::truncate(&self.prompt, 30)
        };

        let full_line = format!("{}{} {}", prefix, children_part, text);

        // Truncate to max width
        AgentBox::truncate(&full_line, max_width)
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

    /// Convert to ListItem for scrollable List widget rendering
    ///
    /// Returns multiple lines based on render mode:
    /// - Compact: Single line with icon, turn info, children icons, prompt preview
    /// - Expanded/Full: Multiple lines with borders, metrics, children, response
    pub fn to_list_items(&self, _ctx: &StreamingContext) -> Vec<ListItem<'static>> {
        // Use Spawn color for subagents (lighter rose), Agent color for parent
        let verb = if self.is_subagent {
            VerbColor::Spawn
        } else {
            VerbColor::Agent
        };
        let verb_color = verb.rgb();
        let border_color = self
            .state
            .border_color_with_pulse(verb.rgb(), self.pulse_intensity);
        let border_style = Style::default().fg(border_color);
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);
        let metric_style = Style::default().fg(compat::SLATE_400);

        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();
        let agent_icon = self.icon();

        match self.render_mode {
            RenderMode::Compact => {
                // Single line: "🐔 ⣾ 3/5 [✅✅] Research competitors..."
                let compact = self.compact_line(80);
                vec![ListItem::new(Line::from(vec![Span::styled(
                    compact,
                    content_style,
                )]))]
            }
            RenderMode::Expanded | RenderMode::Full => {
                let mut items = Vec::new();

                // Header label based on agent type
                let header_label = if self.is_subagent {
                    if self.depth > 1 {
                        format!("{} SUBAGENT (d{})", agent_icon, self.depth)
                    } else {
                        format!("{} SUBAGENT", agent_icon)
                    }
                } else {
                    format!("{} AGENT", agent_icon)
                };

                // Top border with label
                let prompt_preview = AgentBox::truncate(&self.prompt, 40);
                items.push(ListItem::new(Line::from(vec![
                    Span::styled("╭─ ", border_style),
                    Span::styled(header_label, Style::default().fg(verb_color)),
                    Span::styled(" ", border_style),
                    Span::styled(prompt_preview, content_style),
                    Span::styled(" ", border_style),
                    Span::styled(status_icon, border_style),
                    Span::styled(" ", border_style),
                    Span::styled(status_suffix.to_string(), dim_style),
                    Span::styled(" ─╮", border_style),
                ])));

                // Metrics bar: turn progress, tokens, cost, tool count
                let turn_info = format!("turn {}/{}", self.turn, self.max_turns);
                let tokens_str = format!(
                    "{}↓ {}↑",
                    format_number_compact(self.tokens_in),
                    format_number_compact(self.tokens_out)
                );
                let cost_str = format!("${:.4}", self.cost);
                let tools_str = format!("🔧{}", self.tool_calls);
                let velocity_sparkline = self.velocity.sparkline_chars().to_owned();

                items.push(ListItem::new(Line::from(vec![
                    Span::styled("│ ", border_style),
                    Span::styled(turn_info, metric_style),
                    Span::styled(" │ ", dim_style),
                    Span::styled(tokens_str, metric_style),
                    Span::styled(" │ ", dim_style),
                    Span::styled(cost_str, metric_style),
                    Span::styled(" │ ", dim_style),
                    Span::styled(tools_str, metric_style),
                    Span::styled(" │ ", dim_style),
                    Span::styled(velocity_sparkline, metric_style),
                    Span::styled(" │", border_style),
                ])));

                // Cost accumulator micro-widget
                if self.cost > 0.0 {
                    let cost_line = micro::cost_accumulator(self.cost, 0.0, None);
                    let mut spans = vec![Span::styled("│ ", border_style)];
                    spans.extend(cost_line.spans);
                    items.push(ListItem::new(Line::from(spans)));
                }

                // Children section
                if !self.children.is_empty() {
                    let children_header = if self.expanded_children {
                        format!("├─ Children ({}) ▼", self.children.len())
                    } else {
                        let icons = self.children_status_icons();
                        format!("├─ Children ({}) ▶ [{}]", self.children.len(), icons)
                    };
                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        children_header,
                        dim_style,
                    )])));

                    // If expanded, show child summaries
                    if self.expanded_children {
                        for child in &self.children {
                            let child_line = format!(
                                "│   {} {} {}",
                                child.verb_color().icon(),
                                child.state().icon(),
                                AgentBox::truncate(child.task_id().unwrap_or("task"), 50)
                            );
                            items.push(ListItem::new(Line::from(vec![Span::styled(
                                child_line, dim_style,
                            )])));
                        }
                    }
                }

                // Response section
                if let Some(ref response) = self.final_response {
                    let response_header = if self.expanded_response {
                        "├─ Response ▼".to_string()
                    } else {
                        let preview = AgentBox::truncate(response.lines().next().unwrap_or(""), 40);
                        format!("├─ Response ▶ {}", preview)
                    };
                    items.push(ListItem::new(Line::from(vec![Span::styled(
                        response_header,
                        dim_style,
                    )])));

                    // If expanded, show full response lines
                    if self.expanded_response {
                        for line in response.lines() {
                            let response_line = format!("│   {}", line);
                            items.push(ListItem::new(Line::from(vec![Span::styled(
                                response_line,
                                content_style,
                            )])));
                        }
                        // Handle empty response case
                        if response.is_empty() {
                            items.push(ListItem::new(Line::from(vec![Span::styled(
                                "│   (empty)",
                                dim_style,
                            )])));
                        }
                    }
                }

                // Bottom border
                items.push(ListItem::new(Line::from(vec![Span::styled(
                    "╰─────────────────────────────────────────────────────────╯",
                    border_style,
                )])));

                items
            }
        }
    }
}

impl Widget for AgentBox {
    fn render(self, area: Rect, buf: &mut Buffer) {
        (&self).render(area, buf);
    }
}

impl Widget for &AgentBox {
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
        let dim_style = Style::default().fg(compat::SLATE_500);
        let content_style = Style::default().fg(compat::SLATE_200);
        let metric_style = Style::default().fg(compat::SLATE_400);

        let inner_width = (area.width - 2) as usize;
        let status_icon = self.state.icon();
        let status_suffix = self.state.suffix();

        // Top border: ╭─────────────────────────────────────────────────────────────────────╮
        // Header:     │ 🐔 AGENT: Research competitors...           ⣾ Running  00:12       │
        let prompt_truncated = AgentBox::truncate(&self.prompt, inner_width.saturating_sub(35));
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
        let title_display_width = display_width(&title);
        let title_padded = if title_display_width < inner_width + 2 {
            let dashes_needed = inner_width + 2 - title_display_width;
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

            // Build velocity sparkline if running and has samples
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
                "Turn {}/{} │ 📊 {} in / {} out │ 💰 ${:.2} │ 🔌 {} tools{}",
                self.turn,
                self.max_turns,
                format_number_compact(self.tokens_in),
                format_number_compact(self.tokens_out),
                self.cost,
                self.tool_calls,
                velocity_str
            );
            let metrics_truncated = AgentBox::truncate(&metrics, inner_width - 2);
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

                    // PERF: Render child by reference (zero-copy via Widget for &TaskBox)
                    child.render(child_area, buf);
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

                    let line_display = AgentBox::truncate(line, inner_width - 4);
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
    use crate::widgets::task_box::InvokeBox;

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
        assert_eq!(format_number_compact(500), "500");
        assert_eq!(format_number_compact(1500), "1.5K");
        assert_eq!(format_number_compact(1_500_000), "1.5M");
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

    // === TokenVelocity tests ===

    #[test]
    fn test_agent_box_has_velocity() {
        let mut box_ = AgentBox::new("task-1", "prompt");
        assert_eq!(box_.velocity.len(), 0);

        box_.push_velocity_sample(10.0);
        box_.push_velocity_sample(30.0);
        box_.push_velocity_sample(50.0);

        assert_eq!(box_.velocity.len(), 3);
        assert!((box_.velocity.average() - 30.0).abs() < 0.1);
    }

    #[test]
    fn test_agent_box_velocity_sparkline() {
        let mut box_ = AgentBox::new("task-1", "prompt");
        for i in 0..8 {
            box_.push_velocity_sample(i as f32 * 10.0);
        }

        let sparkline = box_.velocity.sparkline_chars();
        assert_eq!(sparkline.chars().count(), 8);
    }

    // === Plan A Phase 10: Compact Mode Tests ===

    #[test]
    fn test_agent_box_turn_progress_ratio() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(3, 5);
        assert!((box_.turn_progress_ratio() - 0.6).abs() < 0.001);
    }

    #[test]
    fn test_agent_box_turn_progress_ratio_zero() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(0, 10);
        assert!((box_.turn_progress_ratio() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_agent_box_turn_progress_ratio_full() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(5, 5);
        assert!((box_.turn_progress_ratio() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_agent_box_turn_progress_ratio_max_zero() {
        // Edge case: max_turns = 0 should return 0.0 (avoid division by zero)
        let box_ = AgentBox::new("task-1", "prompt").with_turn(0, 0);
        assert!((box_.turn_progress_ratio() - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_agent_box_turn_progress_bar() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(3, 5);
        let bar = box_.turn_progress_bar(20);
        // Expected: "[████████████░░░░░░░░] 60% (turn 3/5)"
        assert!(bar.contains("████"));
        assert!(bar.contains("░░"));
        assert!(bar.contains("60%"));
        assert!(bar.contains("turn 3/5"));
    }

    #[test]
    fn test_agent_box_turn_progress_bar_zero() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(0, 10);
        let bar = box_.turn_progress_bar(10);
        assert!(bar.contains("░░░░░░░░░░"));
        assert!(bar.contains("0%"));
        assert!(bar.contains("turn 0/10"));
    }

    #[test]
    fn test_agent_box_turn_progress_bar_full() {
        let box_ = AgentBox::new("task-1", "prompt").with_turn(5, 5);
        let bar = box_.turn_progress_bar(10);
        assert!(bar.contains("██████████"));
        assert!(bar.contains("100%"));
        assert!(bar.contains("turn 5/5"));
    }

    #[test]
    fn test_agent_box_children_status_icons_empty() {
        let box_ = AgentBox::new("task-1", "prompt");
        assert_eq!(box_.children_status_icons(), "");
    }

    #[test]
    fn test_agent_box_children_status_icons_single() {
        let mut box_ = AgentBox::new("task-1", "prompt");
        let child =
            TaskBox::Invoke(InvokeBox::new("tool", "server").with_state(BoxState::success(100)));
        box_.add_child(child);
        assert_eq!(box_.children_status_icons(), "✓"); // checkmark
    }

    #[test]
    fn test_agent_box_children_status_icons_multiple() {
        use crate::widgets::task_box::ExecBox;

        let mut box_ = AgentBox::new("task-1", "prompt");

        // Add completed child
        let child1 =
            TaskBox::Invoke(InvokeBox::new("tool1", "server").with_state(BoxState::success(100)));
        box_.add_child(child1);

        // Add running child
        let child2 = TaskBox::Exec(ExecBox::new("ls -la").with_state(BoxState::running()));
        box_.add_child(child2);

        // Add failed child
        let child3 = TaskBox::Invoke(
            InvokeBox::new("tool2", "server")
                .with_state(BoxState::failed("error".to_string(), 100)),
        );
        box_.add_child(child3);

        let icons = box_.children_status_icons();
        assert!(icons.contains("✓")); // success (checkmark)
        assert!(icons.contains("✗")); // failed (cross)
                                      // Running icon is a spinner character
    }

    #[test]
    fn test_agent_box_children_status_icons_max_display() {
        let mut box_ = AgentBox::new("task-1", "prompt");

        // Add 12 children
        for i in 0..12 {
            let child = TaskBox::Invoke(
                InvokeBox::new(format!("tool{}", i), "server").with_state(BoxState::success(100)),
            );
            box_.add_child(child);
        }

        let icons = box_.children_status_icons();
        // Should show 10 icons + "+2" overflow
        assert!(icons.contains("+2"));
    }

    #[test]
    fn test_agent_box_compact_line() {
        let mut box_ = AgentBox::new("task-1", "Research competitors")
            .with_turn(3, 5)
            .with_state(BoxState::running());

        // Add some children
        let child1 =
            TaskBox::Invoke(InvokeBox::new("tool1", "server").with_state(BoxState::success(100)));
        box_.add_child(child1);
        let child2 =
            TaskBox::Invoke(InvokeBox::new("tool2", "server").with_state(BoxState::success(100)));
        box_.add_child(child2);

        let compact = box_.compact_line(60);

        // Should contain:
        // - Icon
        // - Turn progress
        // - Children status icons
        assert!(compact.contains("🐔")); // Agent icon
        assert!(compact.contains("3/5")); // Turn progress
        assert!(compact.contains("✓")); // Child status (checkmark)
    }

    #[test]
    fn test_agent_box_compact_line_subagent() {
        let box_ = AgentBox::new("task-1", "Sub-task")
            .with_turn(1, 3)
            .as_subagent(2)
            .with_state(BoxState::running());

        let compact = box_.compact_line(50);

        assert!(compact.contains("🐤")); // Subagent icon
        assert!(compact.contains("1/3")); // Turn progress
    }

    #[test]
    fn test_agent_box_compact_line_with_final_response() {
        let box_ = AgentBox::new("task-1", "Research")
            .with_turn(5, 5)
            .with_final_response("The analysis shows...")
            .with_state(BoxState::success(1000));

        let compact = box_.compact_line(60);

        assert!(compact.contains("✓")); // Success state (checkmark)
        assert!(compact.contains("5/5")); // Turn progress
                                          // Should show truncated response preview
        assert!(compact.contains("The analysis"));
    }
}
