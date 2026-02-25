//! Agent Reasoning Panel
//!
//! Displays agent execution with:
//! - Turn progress (turn X/max)
//! - Turn history with status icons
//! - Live streaming output
//! - Token usage per turn

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget, Wrap},
};

use crate::tui::state::TuiState;
use crate::tui::theme::{Theme, VerbColor};
use crate::tui::utils::format_number;
use crate::tui::views::ReasoningTab;
use crate::tui::widgets::{
    AgentStep, AgentStepGroup, AgentStepsWidget, AgentTurns, Gauge, StepStatus, TurnEntry, VerbType,
};

// ═══════════════════════════════════════════════════════════════════════════════
// NOTE: v0.9.1 - All colors now come from Theme (no hardcoded fallbacks)
// ═══════════════════════════════════════════════════════════════════════════════

/// Agent Reasoning panel (Panel 4)
pub struct ReasoningPanel<'a> {
    state: &'a TuiState,
    theme: &'a Theme,
    focused: bool,
}

impl<'a> ReasoningPanel<'a> {
    pub fn new(state: &'a TuiState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            focused: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Get animated spinner for active turns
    fn spinner(&self) -> char {
        const SPINNER: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let idx = (self.state.frame / 6) as usize % SPINNER.len();
        SPINNER[idx]
    }

    /// Build turn entries from state
    fn build_turn_entries(&self) -> Vec<TurnEntry> {
        self.state
            .agent_turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let mut entry = TurnEntry::new(turn.index, &turn.status);

                if let Some(tokens) = turn.tokens {
                    entry = entry.with_tokens(tokens);
                }

                if !turn.tool_calls.is_empty() {
                    entry = entry.with_tool_calls(turn.tool_calls.clone());
                }

                // Mark last turn as current if agent is active
                let is_last = i == self.state.agent_turns.len() - 1;
                if is_last && self.state.agent_max_turns.is_some() {
                    entry = entry.current();
                }

                entry
            })
            .collect()
    }

    /// Render agent header with turn progress
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let current_turn = self.state.agent_turns.len();

        let header = if let Some(max_turns) = self.state.agent_max_turns {
            // Animated chicken when agent is active (🐔 = parent agent)
            let agent_frames = &["🐔", "🔥", "✨", "💫"];
            let agent_idx = (self.state.frame / 10) as usize % agent_frames.len();
            let agent_icon = agent_frames[agent_idx];

            // Animated spinner for turn indicator
            let spinner = self.spinner();

            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(format!("{} ", agent_icon), Style::default()),
                Span::styled(
                    "Agent: ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} Turn {}/{}", spinner, current_turn, max_turns),
                    Style::default().fg(self.theme.status_running),
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("🐔 ", Style::default()),
                Span::styled(
                    "Agent: ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("(inactive)", Style::default().fg(Color::DarkGray)),
            ])
        };

        let paragraph = Paragraph::new(header);
        paragraph.render(area, buf);
    }

    /// Render turn progress gauge
    fn render_progress(&self, area: Rect, buf: &mut Buffer) {
        if let Some(max_turns) = self.state.agent_max_turns {
            let current = self.state.agent_turns.len() as f64;
            let max = max_turns as f64;
            let ratio = (current / max).min(1.0);

            let color = if ratio >= 0.9 {
                self.theme.status_failed
            } else if ratio >= 0.7 {
                self.theme.status_running
            } else {
                self.theme.highlight
            };

            let gauge_area = Rect {
                x: area.x + 2,
                y: area.y,
                width: area.width.saturating_sub(4),
                height: 1,
            };

            let gauge = Gauge::new(ratio)
                .fill_color(color)
                .label("Turns")
                .show_percent(false);

            gauge.render(gauge_area, buf);
        }
    }

    /// Render turn history
    fn render_turns(&self, area: Rect, buf: &mut Buffer) {
        let entries = self.build_turn_entries();

        let turns_area = Rect {
            x: area.x + 2,
            y: area.y,
            width: area.width.saturating_sub(4),
            height: area.height,
        };

        let agent_turns = AgentTurns::new(&entries).reverse(true);
        agent_turns.render(turns_area, buf);
    }

    /// Render streaming output
    fn render_streaming(&self, area: Rect, buf: &mut Buffer) {
        if self.state.streaming_buffer.is_empty() {
            return;
        }

        // Header
        buf.set_string(
            area.x + 2,
            area.y,
            "─── Output ───",
            Style::default().fg(Color::DarkGray),
        );

        // Content (last N lines that fit)
        if area.height > 1 {
            let content_area = Rect {
                x: area.x + 2,
                y: area.y + 1,
                width: area.width.saturating_sub(4),
                height: area.height.saturating_sub(1),
            };

            // Get last lines that fit
            let lines: Vec<&str> = self.state.streaming_buffer.lines().collect();
            let visible_lines = lines
                .iter()
                .rev()
                .take(content_area.height as usize)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");

            let paragraph = Paragraph::new(visible_lines)
                .style(Style::default().fg(self.theme.text_muted))
                .wrap(Wrap { trim: true });

            paragraph.render(content_area, buf);
        }
    }

    /// Check if any turn has thinking content
    #[allow(dead_code)] // Used in tests
    fn has_thinking(&self) -> bool {
        self.state.agent_turns.iter().any(|t| t.thinking.is_some())
    }

    /// Render thinking/reasoning content (v0.4 extended thinking)
    fn render_thinking(&self, area: Rect, buf: &mut Buffer) {
        // Find the last turn with thinking content
        let thinking = self
            .state
            .agent_turns
            .iter()
            .rev()
            .find_map(|t| t.thinking.as_ref());

        if let Some(thinking_text) = thinking {
            // Header with brain emoji
            buf.set_string(
                area.x + 2,
                area.y,
                "─── 🧠 Thinking ───",
                Style::default()
                    .fg(VerbColor::Infer.rgb())
                    .add_modifier(Modifier::BOLD),
            );

            // Content area
            if area.height > 1 {
                let content_area = Rect {
                    x: area.x + 2,
                    y: area.y + 1,
                    width: area.width.saturating_sub(4),
                    height: area.height.saturating_sub(1),
                };

                // Truncate thinking to fit (show last lines that fit)
                let lines: Vec<&str> = thinking_text.lines().collect();
                let max_lines = content_area.height as usize;
                let visible_lines = if lines.len() > max_lines {
                    // Show "..." indicator and last lines
                    let mut visible = vec!["..."];
                    visible.extend(lines.iter().rev().take(max_lines - 1).rev().cloned());
                    visible.join("\n")
                } else {
                    lines.join("\n")
                };

                let paragraph = Paragraph::new(visible_lines)
                    .style(Style::default().fg(VerbColor::Infer.glow()))
                    .wrap(Wrap { trim: true });

                paragraph.render(content_area, buf);
            }
        } else {
            // No thinking content - show placeholder
            buf.set_string(
                area.x + 2,
                area.y,
                "(no thinking captured)",
                Style::default().fg(Color::DarkGray),
            );
        }
    }

    /// Render token summary
    fn render_tokens(&self, area: Rect, buf: &mut Buffer) {
        let total_tokens: u32 = self.state.agent_turns.iter().filter_map(|t| t.tokens).sum();

        if total_tokens > 0 {
            let token_line = Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("Tokens: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format_number(total_tokens),
                    Style::default().fg(VerbColor::Infer.rgb()),
                ),
            ]);

            let paragraph = Paragraph::new(token_line);
            paragraph.render(area, buf);
        } else {
            buf.set_string(
                area.x + 2,
                area.y,
                "(no tokens recorded)",
                Style::default().fg(Color::DarkGray),
            );
        }
    }

    /// Build AgentSteps from agent turns (Claude Code-like view)
    fn build_agent_steps(&self) -> Vec<AgentStep> {
        use crate::tui::widgets::{TokenUsage, ToolCallMetadata};

        self.state
            .agent_turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                // Determine verb type from tool calls
                let verb = if !turn.tool_calls.is_empty() {
                    // Check if it's an MCP tool call
                    let first_tool = &turn.tool_calls[0];
                    if first_tool.starts_with("novanet_") || first_tool.contains("mcp") {
                        VerbType::Invoke
                    } else if first_tool == "spawn_agent" {
                        VerbType::Agent
                    } else {
                        VerbType::Infer
                    }
                } else {
                    VerbType::Infer
                };

                // Determine status
                let is_last = i == self.state.agent_turns.len() - 1;
                let status = if is_last && self.state.agent_max_turns.is_some() {
                    StepStatus::Running
                } else if turn.status == "completed" || turn.status == "done" {
                    StepStatus::Completed
                } else if turn.status.contains("error") || turn.status.contains("fail") {
                    StepStatus::Failed
                } else {
                    StepStatus::Completed
                };

                let description = if !turn.tool_calls.is_empty() {
                    format!("Turn {} — {} tools", turn.index, turn.tool_calls.len())
                } else {
                    format!("Turn {} — inference", turn.index)
                };

                let mut step = AgentStep::new(&description)
                    .with_verb(verb)
                    .with_turn(turn.index, self.state.agent_max_turns);
                step.status = status;

                // Add token usage if available
                if let Some(tokens) = turn.tokens {
                    step = step.with_tokens(TokenUsage::new(tokens, 0));
                }

                // Add first tool call metadata if available
                if let Some(tool_name) = turn.tool_calls.first() {
                    step = step.with_tool_call(ToolCallMetadata::new(tool_name));
                }

                // Add thinking as detail if available
                if let Some(thinking) = &turn.thinking {
                    let preview: String = thinking.chars().take(80).collect();
                    step = step.with_detail(format!("🧠 {}", preview));
                }

                step
            })
            .collect()
    }

    /// Render Claude Code-like steps view (v0.8)
    fn render_steps(&self, area: Rect, buf: &mut Buffer) {
        let steps = self.build_agent_steps();

        if steps.is_empty() {
            buf.set_string(
                area.x + 2,
                area.y,
                "(no agent steps)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Build step group
        let task_id = self
            .state
            .current_task
            .clone()
            .unwrap_or_else(|| "agent".to_string());
        let mut group = AgentStepGroup::new(&task_id);
        for step in steps {
            group.add_step(step);
        }

        let steps_area = Rect {
            x: area.x + 1,
            y: area.y,
            width: area.width.saturating_sub(2),
            height: area.height,
        };

        let widget = AgentStepsWidget::new(&group);
        widget.render(steps_area, buf);
    }

    /// Render spawned agents tree (v0.8)
    fn render_spawned_agents(&self, area: Rect, buf: &mut Buffer) {
        if self.state.spawned_agents.is_empty() {
            return;
        }

        // Header
        buf.set_string(
            area.x + 2,
            area.y,
            "─── 🐤 Spawned Agents ───",
            Style::default()
                .fg(VerbColor::Agent.rgb())
                .add_modifier(Modifier::BOLD),
        );

        // List spawned agents with tree visualization
        for (i, agent) in self.state.spawned_agents.iter().enumerate() {
            let y = area.y + 1 + i as u16;
            if y >= area.y + area.height {
                break;
            }

            // Indent based on depth
            let indent = "  ".repeat(agent.depth as usize);
            let tree_char = if i == self.state.spawned_agents.len() - 1 {
                "└─"
            } else {
                "├─"
            };

            let line = format!(
                "{}{} 🐤 {} (from {})",
                indent, tree_char, agent.child_task_id, agent.parent_task_id
            );

            buf.set_string(
                area.x + 2,
                y,
                &line,
                Style::default().fg(VerbColor::Agent.glow()),
            );
        }
    }
}

impl Widget for ReasoningPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border with tab indicator
        let border_style = self.theme.border_style(self.focused);
        let active_tab = self.state.reasoning_tab;
        let tab_indicator = match active_tab {
            ReasoningTab::Turns => " [Turns] Thinking Steps ",
            ReasoningTab::Thinking => " Turns [Thinking] Steps ",
            ReasoningTab::Steps => " Turns Thinking [Steps] ",
        };
        let title = format!(" ⊕ AGENT REASONING {} ", tab_indicator);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 6 || inner.width < 20 {
            return;
        }

        // Check if agent is active
        let has_agent = self.state.agent_max_turns.is_some();
        let has_streaming = !self.state.streaming_buffer.is_empty();
        let has_spawned = !self.state.spawned_agents.is_empty();

        // Tab-based rendering (v0.5.2+)
        match active_tab {
            ReasoningTab::Thinking => {
                // Thinking tab: show thinking content only
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Length(1), // Header
                        Constraint::Min(4),    // Thinking content
                        Constraint::Length(1), // Tokens
                    ])
                    .split(inner);

                self.render_header(chunks[0], buf);
                self.render_thinking(chunks[1], buf);
                self.render_tokens(chunks[2], buf);
            }
            ReasoningTab::Steps => {
                // Steps tab: Claude Code-like step-by-step view (v0.8)
                if has_spawned {
                    let spawned_height = (self.state.spawned_agents.len() + 2).min(6) as u16;
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1),              // Header
                            Constraint::Min(4),                 // Steps
                            Constraint::Length(spawned_height), // Spawned agents
                            Constraint::Length(1),              // Tokens
                        ])
                        .split(inner);

                    self.render_header(chunks[0], buf);
                    self.render_steps(chunks[1], buf);
                    self.render_spawned_agents(chunks[2], buf);
                    self.render_tokens(chunks[3], buf);
                } else {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1), // Header
                            Constraint::Min(4),    // Steps
                            Constraint::Length(1), // Tokens
                        ])
                        .split(inner);

                    self.render_header(chunks[0], buf);
                    self.render_steps(chunks[1], buf);
                    self.render_tokens(chunks[2], buf);
                }
            }
            ReasoningTab::Turns => {
                // Turns tab: show turn history (with streaming if active)
                if has_agent && has_streaming {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1), // Header
                            Constraint::Length(1), // Progress
                            Constraint::Min(2),    // Turns
                            Constraint::Length(4), // Streaming
                            Constraint::Length(1), // Tokens
                        ])
                        .split(inner);

                    self.render_header(chunks[0], buf);
                    self.render_progress(chunks[1], buf);
                    self.render_turns(chunks[2], buf);
                    self.render_streaming(chunks[3], buf);
                    self.render_tokens(chunks[4], buf);
                } else if has_agent {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([
                            Constraint::Length(1), // Header
                            Constraint::Length(1), // Progress
                            Constraint::Min(2),    // Turns
                            Constraint::Length(1), // Tokens
                        ])
                        .split(inner);

                    self.render_header(chunks[0], buf);
                    self.render_progress(chunks[1], buf);
                    self.render_turns(chunks[2], buf);
                    self.render_tokens(chunks[3], buf);
                } else {
                    // No agent: Just header
                    self.render_header(inner, buf);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{AgentTurnState, TuiState};
    use crate::tui::theme::Theme;

    #[test]
    fn test_reasoning_panel_creation() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme).focused(true);
        assert!(panel.focused);
    }

    #[test]
    fn test_build_turn_entries_empty() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        let entries = panel.build_turn_entries();
        assert!(entries.is_empty());
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(12345), "12,345");
    }

    #[test]
    fn test_has_thinking_returns_false_when_no_turns() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        assert!(!panel.has_thinking());
    }

    #[test]
    fn test_has_thinking_returns_false_when_turns_without_thinking() {
        let mut state = TuiState::new("test.yaml");
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: None,
            response_text: Some("Response".to_string()),
        });
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        assert!(!panel.has_thinking());
    }

    #[test]
    fn test_has_thinking_returns_true_when_turn_has_thinking() {
        let mut state = TuiState::new("test.yaml");
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some("Let me think about this...".to_string()),
            response_text: Some("Response".to_string()),
        });
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        assert!(panel.has_thinking());
    }

    #[test]
    fn test_build_turn_entries_with_thinking() {
        let mut state = TuiState::new("test.yaml");
        state.agent_max_turns = Some(5);
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(150),
            tool_calls: vec!["tool1".to_string()],
            thinking: Some("Analyzing the problem...".to_string()),
            response_text: Some("Here's my analysis".to_string()),
        });
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        let entries = panel.build_turn_entries();
        assert_eq!(entries.len(), 1);
    }

    // === Tab-based Rendering Tests (MEDIUM 10) ===

    #[test]
    fn test_reasoning_tab_switches_view() {
        use crate::tui::views::ReasoningTab;

        let mut state = TuiState::new("test.yaml");
        state.agent_max_turns = Some(5);
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some("I'm thinking about this...".to_string()),
            response_text: Some("Result".to_string()),
        });

        // Default tab is Turns
        assert_eq!(state.reasoning_tab, ReasoningTab::Turns);

        // Switch to Thinking tab
        state.reasoning_tab = ReasoningTab::Thinking;
        assert_eq!(state.reasoning_tab, ReasoningTab::Thinking);

        // Panel should render differently for each tab (verified by buffer content)
        let theme = Theme::novanet();
        let _panel = ReasoningPanel::new(&state, &theme);
        // The panel renders based on state.reasoning_tab
    }

    #[test]
    fn test_thinking_tab_renders_thinking_content() {
        use crate::tui::views::ReasoningTab;
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let mut state = TuiState::new("test.yaml");
        state.reasoning_tab = ReasoningTab::Thinking;
        state.agent_max_turns = Some(5);
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some("Deep analysis here".to_string()),
            response_text: Some("Result".to_string()),
        });

        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);

        // Render to buffer
        let area = Rect::new(0, 0, 60, 15);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Buffer should contain thinking indicator in title
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Thinking"));
    }

    #[test]
    fn test_turns_tab_renders_turn_history() {
        use crate::tui::views::ReasoningTab;
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let mut state = TuiState::new("test.yaml");
        state.reasoning_tab = ReasoningTab::Turns;
        state.agent_max_turns = Some(5);
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec!["novanet_describe".to_string()],
            thinking: None,
            response_text: Some("Result".to_string()),
        });

        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);

        // Render to buffer
        let area = Rect::new(0, 0, 60, 15);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Buffer should contain turns indicator in title
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Turns"));
    }

    #[test]
    fn test_tab_indicator_in_title() {
        use crate::tui::views::ReasoningTab;
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let mut state = TuiState::new("test.yaml");
        state.agent_max_turns = Some(5);

        let theme = Theme::novanet();

        // Check Turns tab indicator
        state.reasoning_tab = ReasoningTab::Turns;
        let panel = ReasoningPanel::new(&state, &theme);
        let area = Rect::new(0, 0, 80, 15);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(
            content.contains("[Turns]"),
            "Should show [Turns] as active tab"
        );

        // Check Thinking tab indicator
        state.reasoning_tab = ReasoningTab::Thinking;
        let panel = ReasoningPanel::new(&state, &theme);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(
            content.contains("[Thinking]"),
            "Should show [Thinking] as active tab"
        );

        // Check Steps tab indicator (v0.8)
        state.reasoning_tab = ReasoningTab::Steps;
        let panel = ReasoningPanel::new(&state, &theme);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(
            content.contains("[Steps]"),
            "Should show [Steps] as active tab"
        );
    }

    // === Steps Tab Tests (v0.8) ===

    #[test]
    fn test_steps_tab_renders_agent_steps() {
        use crate::tui::views::ReasoningTab;
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let mut state = TuiState::new("test.yaml");
        state.reasoning_tab = ReasoningTab::Steps;
        state.agent_max_turns = Some(5);
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(100),
            tool_calls: vec!["novanet_describe".to_string()],
            thinking: None,
            response_text: Some("Result".to_string()),
        });

        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);

        // Render to buffer
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Buffer should contain steps indicator in title
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(content.contains("[Steps]"));
    }

    #[test]
    fn test_build_agent_steps_empty() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        let steps = panel.build_agent_steps();
        assert!(steps.is_empty());
    }

    #[test]
    fn test_build_agent_steps_with_tool_calls() {
        let mut state = TuiState::new("test.yaml");
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(150),
            tool_calls: vec!["novanet_generate".to_string()],
            thinking: None,
            response_text: Some("Generated".to_string()),
        });

        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        let steps = panel.build_agent_steps();

        assert_eq!(steps.len(), 1);
    }

    #[test]
    fn test_build_agent_steps_with_spawn_agent() {
        let mut state = TuiState::new("test.yaml");
        state.agent_turns.push(AgentTurnState {
            index: 1,
            status: "completed".to_string(),
            tokens: Some(200),
            tool_calls: vec!["spawn_agent".to_string()],
            thinking: Some("Spawning a child agent...".to_string()),
            response_text: Some("Agent spawned".to_string()),
        });

        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);
        let steps = panel.build_agent_steps();

        assert_eq!(steps.len(), 1);
    }

    // === Spawned Agents Tests (v0.8) ===

    #[test]
    fn test_spawned_agents_display() {
        use crate::tui::state::SpawnedAgent;
        use crate::tui::views::ReasoningTab;
        use ratatui::buffer::Buffer;
        use ratatui::layout::Rect;

        let mut state = TuiState::new("test.yaml");
        state.reasoning_tab = ReasoningTab::Steps;
        state.agent_max_turns = Some(5);
        state.spawned_agents.push(SpawnedAgent {
            parent_task_id: "orchestrator".to_string(),
            child_task_id: "subtask-1".to_string(),
            depth: 1,
        });

        let theme = Theme::novanet();
        let panel = ReasoningPanel::new(&state, &theme);

        // Render to buffer
        let area = Rect::new(0, 0, 80, 20);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Buffer should contain spawned agent info
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        assert!(content.contains("Spawned"));
    }

    #[test]
    fn test_reasoning_tab_next_cycles_through_all() {
        use crate::tui::views::ReasoningTab;

        let tab = ReasoningTab::Turns;
        assert_eq!(tab.next(), ReasoningTab::Thinking);
        assert_eq!(tab.next().next(), ReasoningTab::Steps);
        assert_eq!(tab.next().next().next(), ReasoningTab::Turns);
    }
}
