//! Agent Phase Indicator Rendering
//!
//! Renders the agent phase indicator box showing real-time agent activity
//! (Syncing/Planning/Invoking/Processing/Inferring/Streaming) with Matrix effect.

use ratatui::{
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

use super::{ChatView, InlineContent};
use crate::tui::theme::Theme;
use crate::tui::widgets::AgentPhase;

impl ChatView {
    /// Render agent phase indicator box when agent is active (not Idle).
    ///
    /// Shows real phases with Matrix decrypt effect and MCP tool details.
    /// Appends items to the provided vector.
    pub(super) fn render_agent_phase_items(
        &self,
        items: &mut Vec<ListItem>,
        content_width: usize,
        theme: &Theme,
    ) {
        if self.agent_phase == AgentPhase::Idle {
            return;
        }

        // Phase indicator box header - color by phase type
        let phase_color = match self.agent_phase {
            AgentPhase::Syncing => theme.status_running,
            AgentPhase::Planning => theme.status_running,
            AgentPhase::Routing => theme.status_running,
            AgentPhase::Invoking => theme.highlight,
            AgentPhase::Processing => theme.status_success,
            AgentPhase::Inferring => theme.status_running,
            AgentPhase::Composing => theme.status_running,
            AgentPhase::Streaming => theme.status_success,
            AgentPhase::Idle => theme.text_muted,
        };

        // Box top
        items.push(ListItem::new(Line::from(vec![Span::styled(
            format!(
                "┌─ 🐔 Nika {}─┐",
                "─".repeat(content_width.saturating_sub(15))
            ),
            Style::default().fg(phase_color),
        )])));

        // Phase indicator line with Matrix effect
        let phase_line = self.phase_indicator.build_line();
        let mut phase_spans = vec![Span::styled("│  ", Style::default().fg(phase_color))];
        phase_spans.extend(phase_line.spans);
        // Pad to box width
        phase_spans.push(Span::styled(
            format!("{:width$}│", "", width = content_width.saturating_sub(30)),
            Style::default().fg(phase_color),
        ));
        items.push(ListItem::new(Line::from(phase_spans)));

        // Show tool details if in Invoking/Processing phase
        if matches!(
            self.agent_phase,
            AgentPhase::Invoking | AgentPhase::Processing
        ) {
            if let Some(ref tool) = self.agent_phase_tool {
                // Get last MCP call details if available
                if let Some(InlineContent::McpCall(mcp_data)) = self.inline_content.last() {
                    // Server line
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│  ", Style::default().fg(phase_color)),
                        Span::styled("├── ", Style::default().fg(theme.text_muted)),
                        Span::styled("server: ", Style::default().fg(theme.text_muted)),
                        Span::styled(
                            mcp_data.server.clone(),
                            Style::default().fg(theme.highlight),
                        ),
                    ])));

                    // Params line (truncated)
                    if !mcp_data.params.is_empty() {
                        let params_preview = if mcp_data.params.len() > 40 {
                            format!("{}...", &mcp_data.params[..40])
                        } else {
                            mcp_data.params.clone()
                        };
                        items.push(ListItem::new(Line::from(vec![
                            Span::styled("│  ", Style::default().fg(phase_color)),
                            Span::styled("├── ", Style::default().fg(theme.text_muted)),
                            Span::styled("params: ", Style::default().fg(theme.text_muted)),
                            Span::styled(
                                params_preview,
                                Style::default().fg(theme.text_secondary),
                            ),
                        ])));
                    }

                    // Duration line
                    let elapsed_secs = mcp_data.duration.as_secs_f64();
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│  ", Style::default().fg(phase_color)),
                        Span::styled("└── ", Style::default().fg(theme.text_muted)),
                        Span::styled("⏱ ", Style::default().fg(theme.text_muted)),
                        Span::styled(
                            format!("{:.1}s", elapsed_secs),
                            Style::default().fg(if elapsed_secs > 5.0 {
                                theme.status_failed
                            } else if elapsed_secs > 2.0 {
                                theme.status_running
                            } else {
                                theme.status_success
                            }),
                        ),
                    ])));
                } else {
                    // Fallback: just show tool name
                    items.push(ListItem::new(Line::from(vec![
                        Span::styled("│  ", Style::default().fg(phase_color)),
                        Span::styled("└── ", Style::default().fg(theme.text_muted)),
                        Span::styled("tool: ", Style::default().fg(theme.text_muted)),
                        Span::styled(tool.clone(), Style::default().fg(theme.highlight)),
                    ])));
                }
            }
        }

        // Box bottom
        items.push(ListItem::new(Line::from(vec![Span::styled(
            format!("└{}┘", "─".repeat(content_width.saturating_sub(2))),
            Style::default().fg(phase_color),
        )])));

        items.push(ListItem::new("")); // spacing
    }
}
