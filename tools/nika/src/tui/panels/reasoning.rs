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
use crate::tui::theme::Theme;
use crate::tui::widgets::{AgentTurns, Gauge, TurnEntry};

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
            // Animated dragon when agent is active
            let dragon_frames = &["🐉", "🔥", "✨", "💫"];
            let dragon_idx = (self.state.frame / 10) as usize % dragon_frames.len();
            let dragon = dragon_frames[dragon_idx];

            // Animated spinner for turn indicator
            let spinner = self.spinner();

            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled(format!("{} ", dragon), Style::default()),
                Span::styled(
                    "Agent: ",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{} Turn {}/{}", spinner, current_turn, max_turns),
                    Style::default().fg(Color::Rgb(245, 158, 11)), // amber
                ),
            ])
        } else {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("🐉 ", Style::default()),
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
                Color::Rgb(239, 68, 68) // red - near limit
            } else if ratio >= 0.7 {
                Color::Rgb(245, 158, 11) // amber - warning
            } else {
                Color::Rgb(59, 130, 246) // blue - ok
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
                .style(Style::default().fg(Color::Rgb(156, 163, 175))) // gray-400
                .wrap(Wrap { trim: true });

            paragraph.render(content_area, buf);
        }
    }

    /// Check if any turn has thinking content
    fn has_thinking(&self) -> bool {
        self.state
            .agent_turns
            .iter()
            .any(|t| t.thinking.is_some())
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
                    .fg(Color::Rgb(139, 92, 246)) // violet
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
                    .style(Style::default().fg(Color::Rgb(167, 139, 250))) // violet-400
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
        let total_tokens: u32 = self
            .state
            .agent_turns
            .iter()
            .filter_map(|t| t.tokens)
            .sum();

        if total_tokens > 0 {
            let token_line = Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("Tokens: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format_number(total_tokens),
                    Style::default().fg(Color::Rgb(139, 92, 246)), // violet
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
}

impl Widget for ReasoningPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border
        let border_style = self.theme.border_style(self.focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" ⊕ AGENT REASONING ")
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 6 || inner.width < 20 {
            return;
        }

        // Check if agent is active
        let has_agent = self.state.agent_max_turns.is_some();
        let has_streaming = !self.state.streaming_buffer.is_empty();

        // Layout depends on whether we have streaming output
        if has_agent && has_streaming {
            // With streaming: Header | Progress | Turns | Streaming | Tokens
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
            // Without streaming: Header | Progress | Turns | Tokens
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

/// Format number with thousands separator
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::new();
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::TuiState;
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
}
