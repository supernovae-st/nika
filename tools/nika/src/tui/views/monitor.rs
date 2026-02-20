//! Monitor View
//!
//! Real-time execution monitoring with 4 panels and tabs.
//!
//! # Scroll Limits
//!
//! Scroll values are bounded to prevent infinite scrolling.
//! `MAX_SCROLL_LINES` defines the maximum scroll offset.
//!
//! # Layout
//!
//! ```text
//! ╭──────────────────────────────────────────────────────────────────────────────────╮
//! │  🚀 NIKA EXECUTION                invoke.nika.yaml                   ⏱ 00:04.2  │
//! │  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ │
//! │  ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  2/4  50%  │
//! ├───────────────────────────────────┬──────────────────────────────────────────────┤
//! │  ⚡ MISSION CONTROL               │  🔷 DAG VIEW                                 │
//! │  [Progress] IO  Output            │  Graph [YAML]                                │
//! │  ─────────────────────────────────│──────────────────────────────────────────────│
//! │                                   │                                              │
//! │  ✅ task1         1.2s    ━━━━━━ │           ╭──────────╮                       │
//! │     infer                         │           │ task1 ✓  │                       │
//! │                                   │           ╰────┬─────╯                       │
//! │  🔄 task2         ···    ━━━░░░░ │                │                             │
//! │     invoke:novanet                │                ▼                             │
//! │                                   │           ╭──────────╮                       │
//! │  ○ task3          -      ░░░░░░░ │           │ task2 🔄 │                       │
//! │     infer (pending)               │           ╰──────────╯                       │
//! │                                   │                                              │
//! ├───────────────────────────────────┼──────────────────────────────────────────────┤
//! │  🌐 NOVANET MCP                   │  🧠 AGENT REASONING                          │
//! │  Summary [Full JSON]              │  [Turns] Thinking                            │
//! │  ─────────────────────────────────│──────────────────────────────────────────────│
//! │                                   │                                              │
//! │  #1 ✅ novanet_describe   847b    │  Turn 1 ─────────────────────────────        │
//! │  ┌────────────────────────────┐   │  💭 "I'll query the schema..."               │
//! │  │ → entity: "qr-code"        │   │                                              │
//! │  │ ← 847 chars, 12ms          │   │  📤 Called: novanet_describe                 │
//! │  └────────────────────────────┘   │     entity: "qr-code"                        │
//! │                                   │                                              │
//! │  #2 🔄 novanet_traverse   ···     │  📥 Response: 847 chars                      │
//! │                                   │                                              │
//! ╰───────────────────────────────────┴──────────────────────────────────────────────╯
//! ```

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ratatui::prelude::*;
use ratatui::style::Color;
use ratatui::widgets::{Block, Borders, Gauge, Paragraph};

use crate::tui::state::TuiState;
use crate::tui::theme::{MissionPhase, TaskStatus, Theme};

/// Maximum scroll offset to prevent unbounded scrolling
const MAX_SCROLL_LINES: u16 = 1000;

/// Tab state for Mission Control panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissionTab {
    #[default]
    Progress,
    TaskIO,
    Output,
}

impl MissionTab {
    pub fn next(&self) -> Self {
        match self {
            MissionTab::Progress => MissionTab::TaskIO,
            MissionTab::TaskIO => MissionTab::Output,
            MissionTab::Output => MissionTab::Progress,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            MissionTab::Progress => "Progress",
            MissionTab::TaskIO => "IO",
            MissionTab::Output => "Output",
        }
    }
}

/// Tab state for DAG panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DagTab {
    #[default]
    Graph,
    Yaml,
}

impl DagTab {
    pub fn next(&self) -> Self {
        match self {
            DagTab::Graph => DagTab::Yaml,
            DagTab::Yaml => DagTab::Graph,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            DagTab::Graph => "Graph",
            DagTab::Yaml => "YAML",
        }
    }
}

/// Tab state for NovaNet panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NovanetTab {
    #[default]
    Summary,
    FullJson,
}

impl NovanetTab {
    pub fn next(&self) -> Self {
        match self {
            NovanetTab::Summary => NovanetTab::FullJson,
            NovanetTab::FullJson => NovanetTab::Summary,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            NovanetTab::Summary => "Summary",
            NovanetTab::FullJson => "Full JSON",
        }
    }
}

/// Tab state for Reasoning panel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ReasoningTab {
    #[default]
    Turns,
    Thinking,
}

impl ReasoningTab {
    pub fn next(&self) -> Self {
        match self {
            ReasoningTab::Turns => ReasoningTab::Thinking,
            ReasoningTab::Thinking => ReasoningTab::Turns,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            ReasoningTab::Turns => "Turns",
            ReasoningTab::Thinking => "Thinking",
        }
    }
}

/// Focused panel in the monitor view
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MonitorPanel {
    #[default]
    Mission,
    Dag,
    Novanet,
    Reasoning,
}

impl MonitorPanel {
    pub fn next(&self) -> Self {
        match self {
            MonitorPanel::Mission => MonitorPanel::Dag,
            MonitorPanel::Dag => MonitorPanel::Novanet,
            MonitorPanel::Novanet => MonitorPanel::Reasoning,
            MonitorPanel::Reasoning => MonitorPanel::Mission,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            MonitorPanel::Mission => MonitorPanel::Reasoning,
            MonitorPanel::Dag => MonitorPanel::Mission,
            MonitorPanel::Novanet => MonitorPanel::Dag,
            MonitorPanel::Reasoning => MonitorPanel::Novanet,
        }
    }

    pub fn number(&self) -> u8 {
        match self {
            MonitorPanel::Mission => 1,
            MonitorPanel::Dag => 2,
            MonitorPanel::Novanet => 3,
            MonitorPanel::Reasoning => 4,
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            MonitorPanel::Mission => "MISSION CONTROL",
            MonitorPanel::Dag => "DAG VIEW",
            MonitorPanel::Novanet => "NOVANET MCP",
            MonitorPanel::Reasoning => "AGENT REASONING",
        }
    }

    pub fn icon(&self) -> &'static str {
        match self {
            MonitorPanel::Mission => "⚡",
            MonitorPanel::Dag => "🔷",
            MonitorPanel::Novanet => "🌐",
            MonitorPanel::Reasoning => "🧠",
        }
    }
}

/// Execution status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExecutionStatus {
    #[default]
    Idle,
    Running,
    Completed,
    Failed,
}

/// Monitor view state
#[derive(Debug)]
pub struct MonitorView {
    /// Workflow file path
    pub workflow_path: PathBuf,
    /// Workflow YAML content
    pub workflow_yaml: String,
    /// Focused panel
    pub focused_panel: MonitorPanel,
    /// Tab states for each panel
    pub mission_tab: MissionTab,
    pub dag_tab: DagTab,
    pub novanet_tab: NovanetTab,
    pub reasoning_tab: ReasoningTab,
    /// Execution status
    pub status: ExecutionStatus,
    /// Start time
    pub start_time: Option<Instant>,
    /// Scroll offsets for various views
    pub yaml_scroll: u16,
    pub json_scroll: u16,
    pub thinking_scroll: u16,
}

impl MonitorView {
    /// Create a new monitor view
    pub fn new(workflow_path: PathBuf, workflow_yaml: String) -> Self {
        Self {
            workflow_path,
            workflow_yaml,
            focused_panel: MonitorPanel::Mission,
            mission_tab: MissionTab::default(),
            dag_tab: DagTab::default(),
            novanet_tab: NovanetTab::default(),
            reasoning_tab: ReasoningTab::default(),
            status: ExecutionStatus::Idle,
            start_time: None,
            yaml_scroll: 0,
            json_scroll: 0,
            thinking_scroll: 0,
        }
    }

    /// Start execution
    pub fn start_execution(&mut self) {
        self.status = ExecutionStatus::Running;
        self.start_time = Some(Instant::now());
    }

    /// Mark execution as completed
    pub fn complete_execution(&mut self) {
        self.status = ExecutionStatus::Completed;
    }

    /// Mark execution as failed
    pub fn fail_execution(&mut self) {
        self.status = ExecutionStatus::Failed;
    }

    /// Check if execution is idle (can switch back to browser)
    pub fn is_idle(&self) -> bool {
        matches!(
            self.status,
            ExecutionStatus::Idle | ExecutionStatus::Completed | ExecutionStatus::Failed
        )
    }

    /// Get elapsed time
    pub fn elapsed(&self) -> Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    /// Format elapsed time as MM:SS.T
    pub fn elapsed_display(&self) -> String {
        let elapsed = self.elapsed();
        let secs = elapsed.as_secs();
        let mins = secs / 60;
        let secs = secs % 60;
        let tenths = elapsed.subsec_millis() / 100;
        format!("{:02}:{:02}.{}", mins, secs, tenths)
    }

    /// Cycle to next panel
    pub fn next_panel(&mut self) {
        self.focused_panel = self.focused_panel.next();
    }

    /// Cycle to previous panel
    pub fn prev_panel(&mut self) {
        self.focused_panel = self.focused_panel.prev();
    }

    /// Focus a specific panel by number (1-4)
    pub fn focus_panel(&mut self, number: u8) {
        self.focused_panel = match number {
            1 => MonitorPanel::Mission,
            2 => MonitorPanel::Dag,
            3 => MonitorPanel::Novanet,
            4 => MonitorPanel::Reasoning,
            _ => self.focused_panel,
        };
    }

    /// Cycle tab in focused panel
    pub fn next_tab(&mut self) {
        match self.focused_panel {
            MonitorPanel::Mission => self.mission_tab = self.mission_tab.next(),
            MonitorPanel::Dag => self.dag_tab = self.dag_tab.next(),
            MonitorPanel::Novanet => self.novanet_tab = self.novanet_tab.next(),
            MonitorPanel::Reasoning => self.reasoning_tab = self.reasoning_tab.next(),
        }
    }

    /// Navigate up in current view
    pub fn navigate_up(&mut self) {
        match (
            self.focused_panel,
            self.dag_tab,
            self.novanet_tab,
            self.reasoning_tab,
        ) {
            (MonitorPanel::Dag, DagTab::Yaml, _, _) => {
                self.yaml_scroll = self.yaml_scroll.saturating_sub(1);
            }
            (MonitorPanel::Novanet, _, NovanetTab::FullJson, _) => {
                self.json_scroll = self.json_scroll.saturating_sub(1);
            }
            (MonitorPanel::Reasoning, _, _, ReasoningTab::Thinking) => {
                self.thinking_scroll = self.thinking_scroll.saturating_sub(1);
            }
            _ => {}
        }
    }

    /// Navigate down in current view (with bounds checking)
    pub fn navigate_down(&mut self) {
        match (
            self.focused_panel,
            self.dag_tab,
            self.novanet_tab,
            self.reasoning_tab,
        ) {
            (MonitorPanel::Dag, DagTab::Yaml, _, _) => {
                self.yaml_scroll = self.yaml_scroll.saturating_add(1).min(MAX_SCROLL_LINES);
            }
            (MonitorPanel::Novanet, _, NovanetTab::FullJson, _) => {
                self.json_scroll = self.json_scroll.saturating_add(1).min(MAX_SCROLL_LINES);
            }
            (MonitorPanel::Reasoning, _, _, ReasoningTab::Thinking) => {
                self.thinking_scroll = self.thinking_scroll.saturating_add(1).min(MAX_SCROLL_LINES);
            }
            _ => {}
        }
    }

    /// Render the header bar with progress
    pub fn render_header(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        // Title line
        let title = format!(
            " 🚀 NIKA EXECUTION                {}                   ⏱ {} ",
            self.workflow_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy(),
            self.elapsed_display()
        );

        let title_style = Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::BOLD);
        buf.set_string(area.x, area.y, &title, title_style);

        // Progress bar
        let completed = state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Success)
            .count();
        let total = state.tasks.len().max(1);
        let ratio = completed as f64 / total as f64;

        if area.height > 1 {
            let gauge_area = Rect {
                x: area.x,
                y: area.y + 1,
                width: area.width,
                height: 1,
            };

            let gauge = Gauge::default()
                .gauge_style(Style::default().fg(theme.status_success))
                .ratio(ratio)
                .label(format!("{}/{}  {:.0}%", completed, total, ratio * 100.0));

            gauge.render(gauge_area, buf);
        }
    }

    /// Render the full monitor view
    pub fn render(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        // Header takes 2 lines
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(2), Constraint::Min(0)])
            .split(area);

        self.render_header(chunks[0], buf, theme, state);

        // Split remaining area into 2x2 grid
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(chunks[1]);

        let top = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[0]);

        let bottom = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[1]);

        self.render_mission(top[0], buf, theme, state);
        self.render_dag(top[1], buf, theme, state);
        self.render_novanet(bottom[0], buf, theme, state);
        self.render_reasoning(bottom[1], buf, theme, state);
    }

    /// Render Mission Control panel
    fn render_mission(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let is_focused = self.focused_panel == MonitorPanel::Mission;
        let border_style = if is_focused {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                MonitorPanel::Mission.icon(),
                MonitorPanel::Mission.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        // Render tab content
        match self.mission_tab {
            MissionTab::Progress => self.render_progress(inner, buf, theme, state),
            MissionTab::TaskIO => self.render_task_io(inner, buf, theme, state),
            MissionTab::Output => self.render_output(inner, buf, theme, state),
        }
    }

    /// Render progress tab content
    fn render_progress(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::styled(
                "[Progress]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" IO  Output"),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        for task in state.tasks.values() {
            let status_icon = match task.status {
                TaskStatus::Success => "✅",
                TaskStatus::Running => "🔄",
                _ => "○",
            };

            let duration = match task.status {
                TaskStatus::Success => {
                    format!("{:.1}s", task.duration_ms.unwrap_or(0) as f64 / 1000.0)
                }
                TaskStatus::Running => "···".to_string(),
                _ => "-".to_string(),
            };

            lines.push(Line::from(format!(
                "{} {:12} {:>8}",
                status_icon, task.id, duration
            )));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render task I/O tab content
    fn render_task_io(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::raw("Progress "),
            Span::styled(
                "[IO]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  Output"),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        // Show current task's I/O (or selected task if we add selection later)
        let task_id = state.current_task.as_ref();

        if let Some(task_id) = task_id {
            if let Some(task) = state.tasks.get(task_id) {
                // Task header
                lines.push(Line::from(vec![
                    Span::styled("╭─── TASK: ", Style::default().fg(theme.text_muted)),
                    Span::styled(
                        task_id,
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" ───", Style::default().fg(theme.text_muted)),
                ]));

                // Status
                let status_icon = match task.status {
                    TaskStatus::Success => "✅",
                    TaskStatus::Running => "🔄",
                    TaskStatus::Failed => "❌",
                    _ => "○",
                };
                lines.push(Line::from(format!(
                    "│ Status: {}  Verb: {}",
                    status_icon,
                    task.task_type.as_deref().unwrap_or("unknown")
                )));
                lines.push(Line::from("│"));

                // Input section
                lines.push(Line::from(Span::styled(
                    "┌─── 📥 INPUT ───",
                    Style::default().fg(Color::Cyan),
                )));
                if let Some(input) = &task.input {
                    let json = serde_json::to_string_pretty(input.as_ref()).unwrap_or_default();
                    for line in json.lines().take(8) {
                        lines.push(Line::from(Span::styled(
                            format!("│ {}", line),
                            Style::default().fg(theme.text_secondary),
                        )));
                    }
                    // Truncation indicator
                    let line_count = json.lines().count();
                    if line_count > 8 {
                        lines.push(Line::from(Span::styled(
                            format!("│ ... ({} more lines)", line_count - 8),
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        "│ (no input)",
                        Style::default().fg(theme.text_muted),
                    )));
                }
                lines.push(Line::from("└───────────────"));

                // Output section
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "┌─── 📤 OUTPUT ───",
                    Style::default().fg(Color::Green),
                )));
                if let Some(output) = &task.output {
                    let json = serde_json::to_string_pretty(output.as_ref()).unwrap_or_default();
                    for line in json.lines().take(8) {
                        lines.push(Line::from(Span::styled(
                            format!("│ {}", line),
                            Style::default().fg(theme.text_secondary),
                        )));
                    }
                    let line_count = json.lines().count();
                    if line_count > 8 {
                        lines.push(Line::from(Span::styled(
                            format!("│ ... ({} more lines)", line_count - 8),
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                } else if task.status == TaskStatus::Running {
                    lines.push(Line::from(Span::styled(
                        "│ ⏳ Waiting for response...",
                        Style::default().fg(Color::Yellow),
                    )));
                } else if task.status == TaskStatus::Failed {
                    if let Some(error) = &task.error {
                        lines.push(Line::from(Span::styled(
                            format!("│ ❌ {}", error),
                            Style::default().fg(Color::Red),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        "│ (pending)",
                        Style::default().fg(theme.text_muted),
                    )));
                }
                lines.push(Line::from("└───────────────"));
            } else {
                lines.push(Line::from("No task data available"));
            }
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "(no active task)",
                Style::default().fg(theme.text_muted),
            )));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render output tab content
    fn render_output(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::raw("Progress  IO "),
            Span::styled(
                "[Output]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        match state.workflow.phase {
            MissionPhase::MissionSuccess => {
                // Success header
                lines.push(Line::from(vec![
                    Span::styled(
                        "╭─── ✅ WORKFLOW COMPLETED ",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "───────────────────────────╮",
                        Style::default().fg(Color::Green),
                    ),
                ]));

                // Duration and stats
                let duration_str = state
                    .workflow
                    .total_duration_ms
                    .map(|d| format!("{:.2}s", d as f64 / 1000.0))
                    .unwrap_or_else(|| "···".to_string());

                let mcp_count = state.mcp_calls.len();

                lines.push(Line::from(vec![
                    Span::styled("│ Duration: ", Style::default().fg(theme.text_secondary)),
                    Span::styled(duration_str, Style::default().fg(Color::Cyan)),
                    Span::styled("    Tasks: ", Style::default().fg(theme.text_secondary)),
                    Span::styled(
                        format!(
                            "{}/{}",
                            state.workflow.tasks_completed, state.workflow.task_count
                        ),
                        Style::default().fg(Color::Green),
                    ),
                    Span::styled("    MCP Calls: ", Style::default().fg(theme.text_secondary)),
                    Span::styled(mcp_count.to_string(), Style::default().fg(Color::Yellow)),
                ]));

                // Progress bar visualization
                lines.push(Line::from(Span::styled(
                    "│ ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓  100%",
                    Style::default().fg(Color::Green),
                )));
                lines.push(Line::from(Span::styled(
                    "╰───────────────────────────────────────────────────────╯",
                    Style::default().fg(Color::Green),
                )));
                lines.push(Line::from(""));

                // Final output section
                lines.push(Line::from(Span::styled(
                    "┌─── 📤 FINAL OUTPUT ───────────────────────────────────┐",
                    Style::default().fg(Color::Cyan),
                )));

                if let Some(output) = &state.workflow.final_output {
                    let json = serde_json::to_string_pretty(output.as_ref()).unwrap_or_default();
                    for line in json.lines().take(12) {
                        lines.push(Line::from(Span::styled(
                            format!("│ {}", line),
                            Style::default().fg(Color::Green),
                        )));
                    }
                    let line_count = json.lines().count();
                    if line_count > 12 {
                        lines.push(Line::from(Span::styled(
                            format!("│ ... ({} more lines)", line_count - 12),
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        "│ (no output)",
                        Style::default().fg(theme.text_muted),
                    )));
                }

                lines.push(Line::from(Span::styled(
                    "└───────────────────────────────────────────────────────┘",
                    Style::default().fg(Color::Cyan),
                )));

                // Action hints
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "[c] Copy to clipboard   [s] Save to file   [Esc] Back to Browser",
                    Style::default().fg(theme.text_muted),
                )));
            }

            MissionPhase::Abort => {
                // Failure header
                lines.push(Line::from(vec![
                    Span::styled(
                        "╭─── ❌ WORKFLOW FAILED ",
                        Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        "─────────────────────────────╮",
                        Style::default().fg(Color::Red),
                    ),
                ]));

                if let Some(error) = &state.workflow.error_message {
                    lines.push(Line::from(Span::styled(
                        format!("│ {}", error),
                        Style::default().fg(Color::Red),
                    )));
                }

                lines.push(Line::from(Span::styled(
                    "╰─────────────────────────────────────────────────────────╯",
                    Style::default().fg(Color::Red),
                )));

                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "[r] Retry   [Esc] Back to Browser",
                    Style::default().fg(theme.text_muted),
                )));
            }

            _ => {
                // Still running
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "⏳ Waiting for workflow completion...",
                    Style::default().fg(Color::Yellow),
                )));
                lines.push(Line::from(""));

                // Show progress indicator
                let progress = state.workflow.progress_pct();
                let filled = (progress / 2.0) as usize;
                let empty = 50 - filled;

                lines.push(Line::from(vec![
                    Span::styled("▓".repeat(filled), Style::default().fg(theme.highlight)),
                    Span::styled("░".repeat(empty), Style::default().fg(theme.text_muted)),
                    Span::styled(
                        format!("  {:.0}%", progress),
                        Style::default().fg(theme.text_secondary),
                    ),
                ]));

                // Show current task if any
                if let Some(task_id) = &state.current_task {
                    lines.push(Line::from(""));
                    lines.push(Line::from(vec![
                        Span::styled("Current: ", Style::default().fg(theme.text_secondary)),
                        Span::styled(
                            task_id,
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ]));
                }
            }
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render DAG panel
    fn render_dag(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let is_focused = self.focused_panel == MonitorPanel::Dag;
        let border_style = if is_focused {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                MonitorPanel::Dag.icon(),
                MonitorPanel::Dag.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        match self.dag_tab {
            DagTab::Graph => self.render_dag_graph(inner, buf, theme, state),
            DagTab::Yaml => self.render_dag_yaml(inner, buf, theme),
        }
    }

    /// Render DAG graph view with animation for running tasks
    fn render_dag_graph(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::styled(
                "[Graph]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" YAML"),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        // Braille spinner frames for animation
        const SPINNERS: [&str; 8] = ["⣾", "⣽", "⣻", "⢿", "⡿", "⣟", "⣯", "⣷"];

        // Simple vertical DAG with animation
        for task in state.tasks.values() {
            let is_running = task.status == TaskStatus::Running;
            let is_current = state.current_task.as_ref() == Some(&task.id);

            // Animated spinner for running tasks
            let icon = match task.status {
                TaskStatus::Success => "✓",
                TaskStatus::Running => SPINNERS[(state.frame as usize / 4) % 8],
                TaskStatus::Failed => "✗",
                _ => "○",
            };

            // Dynamic color for running tasks (pulse effect using frame counter)
            let style = match task.status {
                TaskStatus::Running => {
                    // Pulse brightness based on frame counter
                    let pulse = ((state.frame as f32 / 15.0).sin() * 0.5 + 0.5) * 100.0;
                    if is_current && pulse > 50.0 {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(theme.highlight)
                    }
                }
                TaskStatus::Success => Style::default().fg(theme.status_success),
                TaskStatus::Failed => Style::default().fg(Color::Red),
                _ => Style::default().fg(theme.text_muted),
            };

            // Box drawing with enhanced borders for current task
            let (top_border, bottom_border) = if is_current && is_running {
                ("╔══════════╗", "╚════╦═════╝")
            } else {
                ("╭──────────╮", "╰────┬─────╯")
            };

            // Render task box
            lines.push(Line::from(Span::styled(
                format!("     {}", top_border),
                style,
            )));

            // Task ID with icon - handle truncation for long IDs
            let task_id_display = if task.id.len() > 8 {
                format!("{}…", &task.id[..7])
            } else {
                format!("{:8}", task.id)
            };

            // Add running indicator arrow if current
            let prefix = if is_current && is_running {
                "◀── "
            } else {
                "    "
            };

            lines.push(Line::from(vec![
                Span::styled(format!("     │ {} {} │", task_id_display, icon), style),
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));

            lines.push(Line::from(Span::styled(
                format!("     {}", bottom_border),
                style,
            )));

            // Connector line with animation for running
            let connector = if is_running {
                let dots = ["│", "┊", "┆", "│"];
                format!("          {}", dots[(state.frame as usize / 8) % 4])
            } else {
                "          │".to_string()
            };
            lines.push(Line::from(Span::styled(
                connector,
                Style::default().fg(theme.text_muted),
            )));
        }

        // Remove last connector line (no need after last task)
        if !state.tasks.is_empty() {
            lines.pop();
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render DAG YAML view
    fn render_dag_yaml(&self, area: Rect, buf: &mut Buffer, theme: &Theme) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::raw("Graph "),
            Span::styled(
                "[YAML]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        // YAML content
        for line in self.workflow_yaml.lines().skip(self.yaml_scroll as usize) {
            lines.push(Line::from(Span::styled(
                line,
                Style::default().fg(theme.text_muted),
            )));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render NovaNet panel
    fn render_novanet(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let is_focused = self.focused_panel == MonitorPanel::Novanet;
        let border_style = if is_focused {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                MonitorPanel::Novanet.icon(),
                MonitorPanel::Novanet.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        match self.novanet_tab {
            NovanetTab::Summary => self.render_mcp_summary(inner, buf, theme, state),
            NovanetTab::FullJson => self.render_mcp_json(inner, buf, theme, state),
        }
    }

    /// Render MCP summary
    fn render_mcp_summary(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::styled(
                "[Summary]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Full JSON"),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        for (i, call) in state.mcp_calls.iter().enumerate() {
            let status_icon = if call.completed { "✅" } else { "🔄" };
            let tool_name = call.tool.as_deref().unwrap_or("resource");
            let output_len = call.output_len.unwrap_or(0);

            lines.push(Line::from(format!(
                "#{} {} {:20} {}b",
                i + 1,
                status_icon,
                tool_name,
                output_len
            )));
        }

        if state.mcp_calls.is_empty() {
            lines.push(Line::from("No MCP calls yet"));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render full MCP JSON
    fn render_mcp_json(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::raw("Summary "),
            Span::styled(
                "[Full JSON]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        // Get selected MCP call (default to last if none selected)
        let selected_idx = state
            .selected_mcp_idx
            .or_else(|| state.mcp_calls.len().checked_sub(1));

        if let Some(idx) = selected_idx {
            if let Some(call) = state.mcp_calls.get(idx) {
                // Header with call info
                let status_icon = if call.is_error {
                    "❌"
                } else if call.completed {
                    "✅"
                } else {
                    "🔄"
                };
                let tool_name = call.tool.as_deref().unwrap_or("resource");
                let duration_str = call
                    .duration_ms
                    .map(|d| format!("{}ms", d))
                    .unwrap_or_else(|| "···".to_string());

                lines.push(Line::from(vec![
                    Span::styled(
                        format!("╭─── #{} {} {} ", idx + 1, status_icon, tool_name),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::styled(duration_str, Style::default().fg(theme.text_secondary)),
                    Span::styled(" ───╮", Style::default().fg(theme.text_muted)),
                ]));

                // REQUEST section
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "▶ REQUEST",
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(Span::styled(
                    "═".repeat((area.width as usize).min(40)),
                    Style::default().fg(Color::Cyan),
                )));

                // Show server and tool
                lines.push(Line::from(Span::styled(
                    format!("  server: {}", call.server),
                    Style::default().fg(theme.text_secondary),
                )));
                lines.push(Line::from(Span::styled(
                    format!("  tool: {}", tool_name),
                    Style::default().fg(theme.text_secondary),
                )));

                // Show params JSON
                if let Some(params) = &call.params {
                    lines.push(Line::from(Span::styled(
                        "  params:",
                        Style::default().fg(theme.text_secondary),
                    )));
                    let json = serde_json::to_string_pretty(params).unwrap_or_default();
                    for line in json.lines().take(10) {
                        lines.push(Line::from(Span::styled(
                            format!("    {}", line),
                            Style::default().fg(Color::Yellow),
                        )));
                    }
                    let line_count = json.lines().count();
                    if line_count > 10 {
                        lines.push(Line::from(Span::styled(
                            format!("    ... ({} more lines)", line_count - 10),
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                }

                // RESPONSE section
                lines.push(Line::from(""));
                let response_style = if call.is_error {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD)
                };
                lines.push(Line::from(Span::styled("◀ RESPONSE", response_style)));
                lines.push(Line::from(Span::styled(
                    "═".repeat((area.width as usize).min(40)),
                    if call.is_error {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::Green)
                    },
                )));

                if call.completed {
                    if let Some(response) = &call.response {
                        let json = serde_json::to_string_pretty(response).unwrap_or_default();
                        for line in json.lines().skip(self.json_scroll as usize).take(10) {
                            lines.push(Line::from(Span::styled(
                                format!("  {}", line),
                                if call.is_error {
                                    Style::default().fg(Color::Red)
                                } else {
                                    Style::default().fg(Color::Green)
                                },
                            )));
                        }
                        let total_lines = json.lines().count();
                        if total_lines > 10 {
                            lines.push(Line::from(Span::styled(
                                format!(
                                    "  [↑↓] Scroll ({}/{} lines)",
                                    self.json_scroll.min(total_lines as u16) + 1,
                                    total_lines
                                ),
                                Style::default().fg(theme.text_muted),
                            )));
                        }
                    } else {
                        lines.push(Line::from(Span::styled(
                            "  (empty response)",
                            Style::default().fg(theme.text_muted),
                        )));
                    }
                } else {
                    lines.push(Line::from(Span::styled(
                        "  ⏳ Waiting for response...",
                        Style::default().fg(Color::Yellow),
                    )));
                    lines.push(Line::from(Span::styled(
                        "     ⣾⣽⣻⢿⡿⣟⣯⣷ Loading...",
                        Style::default().fg(theme.text_muted),
                    )));
                }

                // Footer
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "╰───────────────────────────────────────╯",
                    Style::default().fg(theme.text_muted),
                )));

                // Navigation hint
                if state.mcp_calls.len() > 1 {
                    lines.push(Line::from(Span::styled(
                        format!(
                            "[j/k] Navigate MCP calls ({}/{})",
                            idx + 1,
                            state.mcp_calls.len()
                        ),
                        Style::default().fg(theme.text_muted),
                    )));
                }
            }
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "No MCP calls yet",
                Style::default().fg(theme.text_muted),
            )));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render Reasoning panel
    fn render_reasoning(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let is_focused = self.focused_panel == MonitorPanel::Reasoning;
        let border_style = if is_focused {
            Style::default().fg(theme.highlight)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .title(format!(
                " {} {} ",
                MonitorPanel::Reasoning.icon(),
                MonitorPanel::Reasoning.title()
            ))
            .borders(Borders::ALL)
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        match self.reasoning_tab {
            ReasoningTab::Turns => self.render_agent_turns(inner, buf, theme, state),
            ReasoningTab::Thinking => self.render_thinking(inner, buf, theme, state),
        }
    }

    /// Render agent turns
    fn render_agent_turns(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::styled(
                "[Turns]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw(" Thinking"),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        for (i, turn) in state.agent_turns.iter().enumerate() {
            lines.push(Line::from(format!("Turn {} ─────────────────", i + 1)));
            if let Some(thinking) = &turn.thinking {
                let preview: String = thinking.chars().take(50).collect();
                lines.push(Line::from(format!("💭 \"{}...\"", preview)));
            }
        }

        if state.agent_turns.is_empty() {
            lines.push(Line::from("No agent turns yet"));
        }

        let paragraph = Paragraph::new(lines);
        paragraph.render(area, buf);
    }

    /// Render thinking view
    fn render_thinking(&self, area: Rect, buf: &mut Buffer, theme: &Theme, state: &TuiState) {
        let mut lines = Vec::new();

        // Tab indicator
        lines.push(Line::from(vec![
            Span::raw("Turns "),
            Span::styled(
                "[Thinking]",
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));
        lines.push(Line::from("─".repeat(area.width as usize)));

        // Show all thinking content
        for turn in &state.agent_turns {
            if let Some(thinking) = &turn.thinking {
                for line in thinking.lines() {
                    lines.push(Line::from(Span::styled(
                        line,
                        Style::default().fg(theme.text_muted),
                    )));
                }
                lines.push(Line::from(""));
            }
        }

        if lines.len() <= 2 {
            lines.push(Line::from("No thinking content yet"));
        }

        let paragraph = Paragraph::new(lines).scroll((self.thinking_scroll, 0));
        paragraph.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_panel_cycle() {
        let panel = MonitorPanel::Mission;
        assert_eq!(panel.next(), MonitorPanel::Dag);
        assert_eq!(panel.next().next(), MonitorPanel::Novanet);
        assert_eq!(panel.next().next().next(), MonitorPanel::Reasoning);
        assert_eq!(panel.next().next().next().next(), MonitorPanel::Mission);
    }

    #[test]
    fn test_monitor_panel_numbers() {
        assert_eq!(MonitorPanel::Mission.number(), 1);
        assert_eq!(MonitorPanel::Dag.number(), 2);
        assert_eq!(MonitorPanel::Novanet.number(), 3);
        assert_eq!(MonitorPanel::Reasoning.number(), 4);
    }

    #[test]
    fn test_mission_tab_cycle() {
        let tab = MissionTab::Progress;
        assert_eq!(tab.next(), MissionTab::TaskIO);
        assert_eq!(tab.next().next(), MissionTab::Output);
        assert_eq!(tab.next().next().next(), MissionTab::Progress);
    }

    #[test]
    fn test_dag_tab_cycle() {
        let tab = DagTab::Graph;
        assert_eq!(tab.next(), DagTab::Yaml);
        assert_eq!(tab.next().next(), DagTab::Graph);
    }

    #[test]
    fn test_execution_status_idle() {
        let mut view = MonitorView::new(PathBuf::from("test.yaml"), String::new());
        assert!(view.is_idle());

        view.start_execution();
        assert!(!view.is_idle());

        view.complete_execution();
        assert!(view.is_idle());
    }

    #[test]
    fn test_elapsed_display() {
        let view = MonitorView::new(PathBuf::from("test.yaml"), String::new());
        // Without start time, should be 00:00.0
        assert_eq!(view.elapsed_display(), "00:00.0");
    }
}
