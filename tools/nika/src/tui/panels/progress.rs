//! Progress Panel - Mission Control
//!
//! Displays workflow execution status with:
//! - Mission header with phase indicator
//! - Task timeline with status markers
//! - Progress gauge
//! - Active task details
//! - Cost/token metrics

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::state::TuiState;
use crate::tui::theme::{MissionPhase, TaskStatus, Theme};
use crate::tui::utils::format_number;
use crate::tui::widgets::{Gauge, LatencySparkline, Timeline};

/// Progress panel (Panel 1: Mission Control)
pub struct ProgressPanel<'a> {
    state: &'a TuiState,
    theme: &'a Theme,
    focused: bool,
}

impl<'a> ProgressPanel<'a> {
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

    /// Get phase color
    fn phase_color(phase: MissionPhase) -> Color {
        match phase {
            MissionPhase::Preflight => Color::Rgb(107, 114, 128), // gray
            MissionPhase::Countdown => Color::Rgb(245, 158, 11),  // amber
            MissionPhase::Launch => Color::Rgb(236, 72, 153),     // pink
            MissionPhase::Orbital => Color::Rgb(59, 130, 246),    // blue
            MissionPhase::Rendezvous => Color::Rgb(139, 92, 246), // violet
            MissionPhase::MissionSuccess => Color::Rgb(34, 197, 94), // green
            MissionPhase::Abort => Color::Rgb(239, 68, 68),       // red
            MissionPhase::Pause => Color::Rgb(245, 158, 11),      // amber (paused)
        }
    }

    /// Get animated phase icon
    fn phase_icon_animated(&self, phase: MissionPhase) -> &'static str {
        // Only animate active phases
        match phase {
            MissionPhase::Countdown => {
                // Countdown timer animation: 3, 2, 1...
                const COUNTDOWN: &[&str] = &["3️⃣", "2️⃣", "1️⃣", "🔥"];
                let idx = (self.state.frame / 15) as usize % COUNTDOWN.len();
                COUNTDOWN[idx]
            }
            MissionPhase::Launch => {
                // Rocket launch animation
                const LAUNCH: &[&str] = &["🚀", "🔥", "💨", "✨"];
                let idx = (self.state.frame / 8) as usize % LAUNCH.len();
                LAUNCH[idx]
            }
            MissionPhase::Orbital => {
                // Orbital spinner
                const ORBITAL: &[&str] = &["🛰️", "📡", "🌐", "💫"];
                let idx = (self.state.frame / 15) as usize % ORBITAL.len();
                ORBITAL[idx]
            }
            MissionPhase::Rendezvous => {
                // MCP docking animation
                const DOCK: &[&str] = &["🔌", "⚡", "✨", "💫"];
                let idx = (self.state.frame / 10) as usize % DOCK.len();
                DOCK[idx]
            }
            // Static icons for terminal states
            _ => phase.icon(),
        }
    }

    /// Get animated spinner for running tasks
    fn spinner(&self) -> &'static str {
        const SPINNER: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let idx = (self.state.frame / 6) as usize % SPINNER.len();
        SPINNER[idx]
    }

    // Timeline entries are now cached in TuiState.cached_timeline_entries
    // Call state.ensure_timeline_cache() before rendering to update the cache

    /// Render mission header
    fn render_header(&self, area: Rect, buf: &mut Buffer) {
        let phase = self.state.workflow.phase;
        let phase_color = Self::phase_color(phase);

        // Phase indicator with animated icon
        let phase_icon = self.phase_icon_animated(phase);
        let phase_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                phase_icon,
                Style::default()
                    .fg(phase_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                phase.name(),
                Style::default()
                    .fg(phase_color)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        // Workflow path (truncated if needed)
        let path = &self.state.workflow.path;
        let max_path_len = (area.width as usize).saturating_sub(20);
        let display_path = if path.len() > max_path_len {
            format!("...{}", &path[path.len().saturating_sub(max_path_len)..])
        } else {
            path.clone()
        };

        let path_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("📄 ", Style::default()),
            Span::styled(display_path, Style::default().fg(Color::Gray)),
        ]);

        // Elapsed time
        let elapsed = format_duration(self.state.workflow.elapsed_ms);
        let elapsed_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("⏱  ", Style::default()),
            Span::styled(
                elapsed,
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let text = vec![phase_line, path_line, elapsed_line];
        let paragraph = Paragraph::new(text);
        paragraph.render(area, buf);
    }

    /// Render progress section
    fn render_progress(&self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 {
            return;
        }

        // Progress label
        let label = format!(
            "Tasks: {}/{}",
            self.state.workflow.tasks_completed, self.state.workflow.task_count
        );
        buf.set_string(
            area.x + 2,
            area.y,
            &label,
            Style::default().fg(Color::White),
        );

        // Progress gauge
        let gauge_area = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: 1,
        };

        let ratio = self.state.workflow.progress_pct() / 100.0;
        let gauge = Gauge::new(ratio as f64)
            .fill_color(if ratio >= 1.0 {
                Color::Rgb(34, 197, 94) // green
            } else if ratio > 0.0 {
                Color::Rgb(59, 130, 246) // blue
            } else {
                Color::Rgb(107, 114, 128) // gray
            })
            .show_percent(true)
            .label("");

        gauge.render(gauge_area, buf);
    }

    /// Render timeline section
    fn render_timeline(&self, area: Rect, buf: &mut Buffer) {
        // Use cached timeline entries (call state.ensure_timeline_cache() before rendering)
        let entries = &self.state.cached_timeline_entries;

        // Section label
        buf.set_string(
            area.x + 2,
            area.y,
            "Timeline",
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

        // Timeline widget
        let timeline_area = Rect {
            x: area.x + 2,
            y: area.y + 1,
            width: area.width.saturating_sub(4),
            height: area.height.saturating_sub(1),
        };

        let timeline = Timeline::new(entries)
            .elapsed(self.state.workflow.elapsed_ms)
            .with_frame(self.state.frame);
        timeline.render(timeline_area, buf);
    }

    /// Render current task details
    fn render_current_task(&self, area: Rect, buf: &mut Buffer) {
        let Some(task_id) = &self.state.current_task else {
            buf.set_string(
                area.x + 2,
                area.y,
                "(no active task)",
                Style::default().fg(Color::DarkGray),
            );
            return;
        };

        let Some(task) = self.state.tasks.get(task_id) else {
            return;
        };

        // Task ID with status icon (animated for running)
        let status_color = match task.status {
            TaskStatus::Pending => Color::Gray,
            TaskStatus::Running => Color::Rgb(245, 158, 11),
            TaskStatus::Success => Color::Rgb(34, 197, 94),
            TaskStatus::Failed => Color::Rgb(239, 68, 68),
            TaskStatus::Paused => Color::Cyan,
        };

        let status_icon: &str = match task.status {
            TaskStatus::Pending => "○",
            TaskStatus::Running => self.spinner(), // Animated!
            TaskStatus::Success => "✓",
            TaskStatus::Failed => "⊗",
            TaskStatus::Paused => "⏸",
        };

        let task_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                status_icon,
                Style::default()
                    .fg(status_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" ", Style::default()),
            Span::styled(
                task_id,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        // Task type if known
        let type_line = if let Some(task_type) = &task.task_type {
            Line::from(vec![
                Span::styled("    Type: ", Style::default().fg(Color::DarkGray)),
                Span::styled(task_type, Style::default().fg(Color::Cyan)),
            ])
        } else {
            Line::from("")
        };

        // Tokens if available
        let tokens_line = if let Some(tokens) = task.tokens {
            Line::from(vec![
                Span::styled("    Tokens: ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{}", tokens),
                    Style::default().fg(Color::Rgb(139, 92, 246)),
                ),
            ])
        } else {
            Line::from("")
        };

        let text: Vec<Line> = vec![task_line, type_line, tokens_line]
            .into_iter()
            .filter(|l| !l.spans.is_empty())
            .collect();

        let paragraph = Paragraph::new(text);
        paragraph.render(area, buf);
    }

    /// Render metrics summary
    fn render_metrics(&self, area: Rect, buf: &mut Buffer) {
        let metrics = &self.state.metrics;

        // Total tokens
        let tokens_str = format!("Tokens: {}", format_number(metrics.total_tokens));
        buf.set_string(
            area.x + 2,
            area.y,
            &tokens_str,
            Style::default().fg(Color::Rgb(139, 92, 246)), // violet
        );

        // Cost
        let cost_str = format!("Cost: ${:.4}", metrics.cost_usd);
        let cost_x = area.x + area.width.saturating_sub(cost_str.len() as u16 + 2);
        buf.set_string(cost_x, area.y, &cost_str, Style::default().fg(Color::Green));

        // MCP calls
        if area.height > 1 {
            let mcp_count: usize = metrics.mcp_calls.values().sum();
            let mcp_str = format!("MCP: {} calls", mcp_count);
            buf.set_string(
                area.x + 2,
                area.y + 1,
                &mcp_str,
                Style::default().fg(Color::Rgb(59, 130, 246)), // blue
            );

            // Latency sparkline if we have data
            if !metrics.latency_history.is_empty() && area.width > 30 {
                let sparkline_area = Rect {
                    x: area.x + 20,
                    y: area.y + 1,
                    width: area.width.saturating_sub(22),
                    height: 1,
                };
                let sparkline = LatencySparkline::new(&metrics.latency_history)
                    .warn_threshold(500)
                    .error_threshold(2000);
                sparkline.render(sparkline_area, buf);
            }
        }
    }
}

impl Widget for ProgressPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border
        let border_style = self.theme.border_style(self.focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" ◉ MISSION CONTROL ")
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 8 || inner.width < 20 {
            return;
        }

        // Layout: Header | Progress | Timeline | Current Task | Metrics
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Header
                Constraint::Length(2), // Progress
                Constraint::Length(4), // Timeline
                Constraint::Length(4), // Current task
                Constraint::Min(2),    // Metrics
            ])
            .split(inner);

        self.render_header(chunks[0], buf);
        self.render_progress(chunks[1], buf);
        self.render_timeline(chunks[2], buf);
        self.render_current_task(chunks[3], buf);
        self.render_metrics(chunks[4], buf);
    }
}

/// Format duration as HH:MM:SS or MM:SS
fn format_duration(ms: u64) -> String {
    let total_secs = ms / 1000;
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
    } else {
        format!("{:02}:{:02}", minutes, seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "00:00");
        assert_eq!(format_duration(5000), "00:05");
        assert_eq!(format_duration(65000), "01:05");
        assert_eq!(format_duration(3661000), "01:01:01");
    }

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(1234567), "1,234,567");
    }

    #[test]
    fn test_phase_colors_distinct() {
        let colors: Vec<Color> = vec![
            ProgressPanel::phase_color(MissionPhase::Preflight),
            ProgressPanel::phase_color(MissionPhase::Countdown),
            ProgressPanel::phase_color(MissionPhase::Launch),
            ProgressPanel::phase_color(MissionPhase::Orbital),
            ProgressPanel::phase_color(MissionPhase::MissionSuccess),
            ProgressPanel::phase_color(MissionPhase::Abort),
        ];

        // Verify most colors are distinct (some might overlap intentionally)
        let unique_count = colors
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert!(unique_count >= 5, "Phase colors should be mostly distinct");
    }
}
