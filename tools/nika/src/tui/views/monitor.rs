//! Monitor View - Real-time workflow execution monitoring
//!
//! Displays 4-panel layout for workflow execution visibility:
//!
//! ```text
//! ┏━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
//! ┃  NIKA RUNNER                                               │ ⌘K Help         ┃
//! ┠─────────────────────────────────────────────────────────────────────────────────────┨
//! ┃  [h] Home   [c] Chat   [s] Studio   [r] Runner ◀━━          claude-sonnet-4-6 🧠  ┃
//! ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
//! ┃  ╔══════════════════════════════════════════════╦═══════════════════════════════╗  ┃
//! ┃  ║  ◉ MISSION CONTROL                    [1]   ║  ◎ DAG EXECUTION         [2]  ║  ┃
//! ┃  ║  ┌─────────────────────────────────────┐    ║  ┌─────────────────────────┐   ║  ┃
//! ┃  ║  │ ⚡ generate │ ◐ Running │ 156 tok   │    ║  │  generate ──► process   │   ║  ┃
//! ┃  ║  │ ▓▓▓▓▓▓░░░░░ │ ~2s  │ $0.003        │    ║  │      ↓           ↓      │   ║  ┃
//! ┃  ║  └─────────────────────────────────────┘    ║  │   compile ◄── validate  │   ║  ┃
//! ┃  ║  ┌─────────────────────────────────────┐    ║  └─────────────────────────┘   ║  ┃
//! ┃  ║  │ 📟 compile  │ ○ Queued │ ~1s        │    ║                                ║  ┃
//! ┃  ║  └─────────────────────────────────────┘    ║   4 tasks │ 3 layers           ║  ┃
//! ┃  ╠══════════════════════════════════════════════╬═══════════════════════════════╣  ┃
//! ┃  ║  ◎ TASK DETAIL                        [3]   ║  ◎ AGENT REASONING       [4]  ║  ┃
//! ┃  ║  ┌ PROMPT ─────────────────────────────┐    ║  Turn 1: Thinking...          ║  ┃
//! ┃  ║  │ Generate landing page content...    │    ║  💭 I should first analyze... ║  ┃
//! ┃  ║  └─────────────────────────────────────┘    ║  Turn 2: Tool call            ║  ┃
//! ┃  ║  ┌ RESPONSE ───────────────────────────┐    ║  → novanet_describe           ║  ┃
//! ┃  ║  │ Welcome to QR Code AI...            │    ║  Turn 3: Complete [142T]      ║  ┃
//! ┃  ║  └─────────────────────────────────────┘    ║                                ║  ┃
//! ┃  ╚══════════════════════════════════════════════╩═══════════════════════════════╝  ┃
//! ┣━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
//! ┃  Runner │ ◐ Running │ 2/5 │ 40% │ 00:14.4 │ 3,237 tok │ $0.024 │ 🔌 novanet ✓   ┃
//! ┗━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
//! ```
//!

use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::view_trait::View;
use super::{DagTab, MissionTab, NovanetTab, ReasoningTab, TuiView, ViewAction};
use crate::tui::focus::PanelId;
use crate::tui::state::TuiState;
use crate::tui::theme::{MissionPhase, TaskStatus, Theme, VerbColor};
use crate::tui::unicode::truncate_to_width;
use crate::tui::widgets::{task_box::RenderMode, DagAscii, NodeBoxData, NodeBoxMode};

/// Monitor View state
///
/// Real-time workflow execution monitoring with 4-panel layout:
/// - Panel 1: Mission Control (TaskBox widgets)
/// - Panel 2: DAG Execution (DagAscii with Sugiyama layout)
/// - Panel 3: Task Detail (full TaskBox for selected task)
/// - Panel 4: Agent Reasoning (agent turns)
///
/// Major redesign with DagAscii and TaskBox integration.
/// - DagAscii replaces custom DAG rendering
/// - TaskBox widgets replace simple List items
/// - Cost tracking per task
/// - TokenVelocity sparkline
pub struct MonitorView {
    /// Currently focused panel
    pub focus: PanelId,
    /// Scroll offset per panel
    pub scroll: [usize; 4],
    /// Animation frame counter
    pub frame: u8,
    /// Selected task index for detail panel (Panel 3)
    pub selected_task: usize,
    /// TaskBox render mode (Compact/Expanded/Full)
    pub render_mode: RenderMode,
    /// DAG node box mode (Minimal/Expanded)
    pub dag_mode: NodeBoxMode,
    /// Cached DAG nodes (rebuilt only when dag_cache_version changes)
    cached_dag_nodes: Vec<NodeBoxData>,
    /// Cached DAG dependencies (rebuilt only when dag_cache_version changes)
    cached_dag_deps: HashMap<String, Vec<String>>,
    /// Version counter for cache invalidation
    dag_cache_version: u32,
    /// Cached formatted JSON for selected task input
    cached_task_input_json: String,
    /// Cached formatted JSON for selected task output
    cached_task_output_json: String,
    /// Cached formatted JSON for selected MCP response
    cached_mcp_response_json: String,
    /// Task ID that the cached task JSON belongs to
    cached_json_task_id: Option<String>,
    /// MCP call index that the cached MCP JSON belongs to
    cached_json_mcp_idx: Option<usize>,
}

impl MonitorView {
    /// Runner panels in order for navigation
    const PANELS: [PanelId; 4] = [
        PanelId::RunnerMission,
        PanelId::RunnerDag,
        PanelId::RunnerNovanet,
        PanelId::RunnerReasoning,
    ];

    /// Create a new MonitorView
    pub fn new() -> Self {
        Self {
            focus: PanelId::RunnerMission,
            scroll: [0; 4],
            frame: 0,
            selected_task: 0,
            render_mode: RenderMode::Expanded,
            dag_mode: NodeBoxMode::Minimal,
            cached_dag_nodes: Vec::new(),
            cached_dag_deps: HashMap::new(),
            dag_cache_version: 0,
            cached_task_input_json: String::new(),
            cached_task_output_json: String::new(),
            cached_mcp_response_json: String::new(),
            cached_json_task_id: None,
            cached_json_mcp_idx: None,
        }
    }

    /// Convert task type string to VerbColor
    fn verb_from_task_type(task_type: Option<&str>) -> VerbColor {
        match task_type {
            Some("infer") => VerbColor::Infer,
            Some("exec") => VerbColor::Exec,
            Some("fetch") => VerbColor::Fetch,
            Some("invoke") => VerbColor::Invoke,
            Some("agent") => VerbColor::Agent,
            _ => VerbColor::Infer, // default
        }
    }

    /// Build NodeBoxData from TuiState for DagAscii
    fn build_dag_nodes(state: &TuiState) -> Vec<NodeBoxData> {
        state
            .task_order
            .iter()
            .filter_map(|task_id| {
                state.tasks.get(task_id).map(|task| {
                    let verb = Self::verb_from_task_type(task.task_type.as_deref());
                    let estimate = task
                        .duration_ms
                        .map(|ms| format!("{}ms", ms))
                        .unwrap_or_else(|| "~?s".to_string());

                    let mut node = NodeBoxData::new(task_id.clone(), verb)
                        .with_status(task.status)
                        .with_estimate(&estimate);
                    if let Some(ref model) = task.model {
                        node = node.with_model(model);
                    }
                    node
                })
            })
            .collect()
    }

    /// Build dependencies map from TuiState for DagAscii
    fn build_dag_dependencies(state: &TuiState) -> HashMap<String, Vec<String>> {
        let mut deps = HashMap::new();
        for (task_id, task) in &state.tasks {
            if !task.dependencies.is_empty() {
                deps.insert(task_id.clone(), task.dependencies.clone());
            }
        }
        deps
    }

    /// Select next task in mission panel
    pub fn select_next_task(&mut self, task_count: usize) {
        if task_count > 0 {
            self.selected_task = (self.selected_task + 1) % task_count;
        }
    }

    /// Select previous task in mission panel
    pub fn select_prev_task(&mut self, task_count: usize) {
        if task_count > 0 {
            self.selected_task = self.selected_task.checked_sub(1).unwrap_or(task_count - 1);
        }
    }

    /// Cycle render mode for TaskBoxes
    pub fn cycle_render_mode(&mut self) {
        self.render_mode = self.render_mode.cycle();
    }

    /// Toggle DAG mode between Minimal and Expanded
    pub fn toggle_dag_mode(&mut self) {
        self.dag_mode = match self.dag_mode {
            NodeBoxMode::Minimal => NodeBoxMode::Expanded,
            NodeBoxMode::Expanded => NodeBoxMode::Minimal,
        };
    }

    /// Refresh DAG cache if version changed
    ///
    /// Call this before rendering the DAG panel. Only rebuilds
    /// if state.dag_version() differs from cached version.
    pub fn refresh_dag_cache(&mut self, state: &TuiState) {
        let current_version = state.dag_version();
        if self.dag_cache_version != current_version {
            self.cached_dag_nodes = Self::build_dag_nodes(state);
            self.cached_dag_deps = Self::build_dag_dependencies(state);
            self.dag_cache_version = current_version;
        }
    }

    /// Refresh cached JSON for selected task
    ///
    /// Call before rendering Mission TaskIO/Output tabs to avoid
    /// serde_json::to_string_pretty in the render path.
    pub fn refresh_task_json_cache(&mut self, state: &TuiState) {
        let selected_id = state.task_order.get(self.selected_task).cloned();

        // Refresh if selected task changed OR task data updated
        if self.cached_json_task_id == selected_id && !state.dirty.progress {
            return;
        }

        self.cached_json_task_id = selected_id.clone();

        if let Some(ref task_id) = selected_id {
            if let Some(task) = state.tasks.get(task_id) {
                self.cached_task_input_json = task
                    .input
                    .as_ref()
                    .map(|v| {
                        serde_json::to_string_pretty(v.as_ref())
                            .unwrap_or_else(|_| "{}".to_string())
                    })
                    .unwrap_or_else(|| "No input".to_string());

                self.cached_task_output_json = task
                    .output
                    .as_ref()
                    .map(|v| {
                        serde_json::to_string_pretty(v.as_ref())
                            .unwrap_or_else(|_| "{}".to_string())
                    })
                    .unwrap_or_else(|| "No output yet".to_string());
                return;
            }
        }

        self.cached_task_input_json = "No task selected".to_string();
        self.cached_task_output_json = "No task selected".to_string();
    }

    /// Refresh cached JSON for selected MCP call
    ///
    /// Call before rendering NovaNet FullJson tab to avoid
    /// serde_json::to_string_pretty in the render path.
    pub fn refresh_mcp_json_cache(&mut self, state: &TuiState) {
        let selected_idx = self.scroll_offset(PanelId::RunnerNovanet);

        // Refresh if selected MCP call changed OR MCP data updated
        if self.cached_json_mcp_idx == Some(selected_idx) && !state.dirty.novanet {
            return;
        }

        self.cached_json_mcp_idx = Some(selected_idx);

        if let Some(call) = state.mcp.calls.get(selected_idx) {
            self.cached_mcp_response_json = call
                .response
                .as_ref()
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_else(|_| "{}".to_string()))
                .unwrap_or_else(|| "No response".to_string());
        } else {
            self.cached_mcp_response_json = String::new();
        }
    }

    /// Get panel index (0-3) for scroll array
    fn panel_index(panel: PanelId) -> usize {
        match panel {
            PanelId::RunnerMission => 0,
            PanelId::RunnerDag => 1,
            PanelId::RunnerNovanet => 2,
            PanelId::RunnerReasoning => 3,
            _ => 0, // Fallback for non-Runner panels
        }
    }

    /// Focus next panel (Tab)
    pub fn focus_next(&mut self) {
        let idx = Self::panel_index(self.focus);
        self.focus = Self::PANELS[(idx + 1) % 4];
    }

    /// Focus previous panel (Shift+Tab)
    pub fn focus_prev(&mut self) {
        let idx = Self::panel_index(self.focus);
        self.focus = Self::PANELS[(idx + 3) % 4]; // +3 is same as -1 mod 4
    }

    /// Get scroll offset for current panel
    fn scroll_offset(&self, panel: PanelId) -> usize {
        self.scroll[Self::panel_index(panel)]
    }

    /// Scroll current panel down
    fn scroll_down(&mut self) {
        let idx = Self::panel_index(self.focus);
        self.scroll[idx] = self.scroll[idx].saturating_add(1);
    }

    /// Scroll current panel up
    fn scroll_up(&mut self) {
        let idx = Self::panel_index(self.focus);
        self.scroll[idx] = self.scroll[idx].saturating_sub(1);
    }

    /// Get icon for mission phase
    fn phase_icon(phase: &MissionPhase) -> &'static str {
        match phase {
            MissionPhase::Preflight => "🚀",
            MissionPhase::Countdown => "⏱",
            MissionPhase::Launch => "🔥",
            MissionPhase::Orbital => "🛸",
            MissionPhase::Rendezvous => "🎯",
            MissionPhase::MissionSuccess => "✓",
            MissionPhase::Abort => "✗",
            MissionPhase::Pause => "⏸",
        }
    }

    /// Render Mission Control panel (Panel 1) with verb-colored TaskBox style
    ///
    /// Uses TaskBox visual language (verb icons, colors, progress)
    /// Tab-aware rendering (Progress | TaskIO | Output)
    fn render_mission_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        // Tab indicator in title
        let tab_indicator = state.ui.mission_tab.title();
        let mode_indicator = match self.render_mode {
            RenderMode::Compact => "compact",
            RenderMode::Expanded => "expanded",
            RenderMode::Full => "full",
        };

        let block = Block::default()
            .title(format!(
                " {} MISSION CONTROL [{}/{}] ",
                Self::phase_icon(&state.workflow.phase),
                tab_indicator,
                mode_indicator
            ))
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused));

        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Tab-aware content rendering
        match state.ui.mission_tab {
            MissionTab::TaskIO => {
                // Show selected task's input/output
                self.render_mission_task_io(frame, inner_area, state, theme);
                return;
            }
            MissionTab::Output => {
                // Show selected task's full output
                self.render_mission_output(frame, inner_area, state, theme);
                return;
            }
            MissionTab::Progress => {
                // Fall through to default task list rendering below
            }
        }

        // Build task list with TaskBox-style rendering (Progress tab)
        let items: Vec<ListItem> = state
            .task_order
            .iter()
            .enumerate()
            .filter_map(|(i, task_id)| {
                state.tasks.get(task_id).map(|task| {
                    let verb = Self::verb_from_task_type(task.task_type.as_deref());
                    let verb_icon = verb.icon();
                    let verb_color = theme.verb_color(verb);

                    // Status indicator with BoxState-style animation
                    let (status_icon, status_color) = match &task.status {
                        TaskStatus::Queued => ("○", theme.text_muted),
                        TaskStatus::Pending => ("◦", theme.text_muted),
                        TaskStatus::Running => {
                            // Animated spinner using BoxState braille pattern
                            let spinner_chars = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
                            let idx = (self.frame as usize / 6) % spinner_chars.len();
                            let s: &'static str = match spinner_chars[idx] {
                                '⠋' => "⠋",
                                '⠙' => "⠙",
                                '⠹' => "⠹",
                                '⠸' => "⠸",
                                '⠼' => "⠼",
                                '⠴' => "⠴",
                                '⠦' => "⠦",
                                '⠧' => "⠧",
                                '⠇' => "⠇",
                                '⠏' => "⠏",
                                _ => "◐",
                            };
                            (s, verb_color)
                        }
                        TaskStatus::Success => ("✓", theme.status_success),
                        TaskStatus::Failed => ("✗", theme.status_failed),
                        TaskStatus::Paused => ("⏸", theme.status_paused),
                        TaskStatus::Skipped => ("⊘", theme.text_muted),
                    };

                    // Compact mode: single line with verb icon + task + status
                    if self.render_mode == RenderMode::Compact {
                        let is_selected = i == self.selected_task && focused;
                        return Some(
                            ListItem::new(Line::from(vec![
                                Span::styled(
                                    format!("{} ", verb_icon),
                                    Style::default().fg(verb_color),
                                ),
                                Span::styled(status_icon, Style::default().fg(status_color)),
                                Span::raw(" "),
                                Span::styled(
                                    truncate_to_width(task_id, 20),
                                    if is_selected {
                                        Style::default()
                                            .fg(theme.text_primary)
                                            .add_modifier(Modifier::BOLD)
                                    } else {
                                        Style::default().fg(theme.text_primary)
                                    },
                                ),
                            ]))
                            .style(if is_selected {
                                Style::default().bg(Color::Rgb(30, 30, 40))
                            } else {
                                Style::default()
                            }),
                        );
                    }

                    // Expanded/Full mode: two lines with progress bar
                    let progress = match &task.status {
                        TaskStatus::Success => {
                            let duration = task
                                .duration_ms
                                .map(|d| format!(" {}ms", d))
                                .unwrap_or_default();
                            format!("▓▓▓▓▓▓▓▓▓▓ 100%{}", duration)
                        }
                        TaskStatus::Running => {
                            let pct = ((self.frame as usize * 10 / 60) % 10) + 1;
                            format!("{}{} {}0%", "▓".repeat(pct), "░".repeat(10 - pct), pct)
                        }
                        TaskStatus::Failed => {
                            let err = task.error.as_deref().unwrap_or("Error");
                            format!("✗✗✗✗✗✗✗✗✗✗ {}", truncate_to_width(err, 20))
                        }
                        _ => "░░░░░░░░░░   0%".to_string(),
                    };

                    // Token count for infer tasks
                    let tokens_str = if task.task_type.as_deref() == Some("infer") {
                        task.tokens
                            .map(|t| format!(" [{}T]", t))
                            .unwrap_or_default()
                    } else {
                        String::new()
                    };

                    let is_selected = i == self.selected_task && focused;
                    let mut lines = vec![Line::from(vec![
                        Span::styled(format!("{} ", verb_icon), Style::default().fg(verb_color)),
                        Span::styled(status_icon, Style::default().fg(status_color)),
                        Span::raw(" "),
                        Span::styled(
                            task_id.clone(),
                            Style::default()
                                .fg(theme.text_primary)
                                .add_modifier(if is_selected {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                }),
                        ),
                        Span::styled(tokens_str, Style::default().fg(theme.text_muted)),
                    ])];

                    // Add progress line for Expanded/Full mode
                    if self.render_mode != RenderMode::Compact {
                        lines.push(Line::from(vec![
                            Span::raw("    "),
                            Span::styled(progress, Style::default().fg(status_color)),
                        ]));
                    }

                    Some(ListItem::new(Text::from(lines)).style(if is_selected {
                        Style::default().bg(Color::Rgb(30, 30, 40))
                    } else {
                        Style::default()
                    }))
                })
            })
            .flatten()
            .collect();

        // Apply scroll offset
        let skip = self.scroll[Self::panel_index(PanelId::RunnerMission)];
        let visible_items: Vec<ListItem> = items.into_iter().skip(skip).collect();

        let list = List::new(visible_items);
        frame.render_widget(list, inner_area);
    }

    /// Render Mission Control TaskIO tab
    /// Shows selected task's input and output data (uses cached JSON)
    fn render_mission_task_io(
        &self,
        frame: &mut Frame,
        area: Rect,
        _state: &TuiState,
        theme: &Theme,
    ) {
        // Use cached JSON (refreshed before render)
        let content = format!(
            "─── INPUT ───\n{}\n\n─── OUTPUT ───\n{}",
            truncate_to_width(&self.cached_task_input_json, area.width as usize * 3),
            truncate_to_width(&self.cached_task_output_json, area.width as usize * 3)
        );

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    /// Render Mission Control Output tab
    /// Shows selected task's full output/response (uses cached JSON)
    fn render_mission_output(
        &self,
        frame: &mut Frame,
        area: Rect,
        _state: &TuiState,
        theme: &Theme,
    ) {
        // Use cached JSON (refreshed before render)
        let content = self.cached_task_output_json.clone();

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true });
        frame.render_widget(paragraph, area);
    }

    /// Render DAG Yaml tab
    /// Shows workflow info (YAML source not stored in state)
    fn render_dag_yaml(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Build workflow summary since raw YAML isn't stored in TuiState
        let mut content = String::new();
        content.push_str("# Workflow YAML Info\n\n");
        content.push_str(&format!("Path: {}\n", state.workflow.path));
        content.push_str(&format!("Phase: {:?}\n", state.workflow.phase));
        content.push_str(&format!(
            "Tasks: {}/{}\n",
            state.workflow.tasks_completed, state.workflow.task_count
        ));

        if !state.task_order.is_empty() {
            content.push_str("\n# Task IDs:\n");
            for task_id in &state.task_order {
                if let Some(task) = state.tasks.get(task_id) {
                    let verb = task.task_type.as_deref().unwrap_or("unknown");
                    let status = match &task.status {
                        TaskStatus::Queued => "○",
                        TaskStatus::Pending => "◦",
                        TaskStatus::Running => "◐",
                        TaskStatus::Success => "✓",
                        TaskStatus::Failed => "✗",
                        TaskStatus::Paused => "⏸",
                        TaskStatus::Skipped => "⊘",
                    };
                    content.push_str(&format!("  - {}: {} {}\n", task_id, verb, status));
                }
            }
        }

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((self.scroll[Self::panel_index(PanelId::RunnerDag)] as u16, 0));
        frame.render_widget(paragraph, area);
    }

    /// Render DAG Execution panel (Panel 2) using DagAscii widget
    ///
    /// Uses DagAscii widget with Sugiyama layout algorithm and real edges.
    /// Tab-aware rendering (Graph | Yaml)
    fn render_dag_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        // Tab indicator in title
        let tab_indicator = state.ui.dag_tab.title();
        let mode_indicator = match self.dag_mode {
            NodeBoxMode::Minimal => "compact",
            NodeBoxMode::Expanded => "expanded",
        };

        let block = Block::default()
            .title(format!(
                " ⎔ DAG EXECUTION [{}/{}] ",
                tab_indicator, mode_indicator
            ))
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused));

        // Render block first
        let inner_area = block.inner(area);
        frame.render_widget(block, area);

        // Tab-aware content rendering
        match state.ui.dag_tab {
            DagTab::Yaml => {
                self.render_dag_yaml(frame, inner_area, state, theme);
                return;
            }
            DagTab::Graph => {
                // Fall through to default DAG visualization below
            }
        }

        // Use cached DAG nodes and dependencies
        // Cache is refreshed in render() before this method is called
        let nodes = &self.cached_dag_nodes;
        let deps = &self.cached_dag_deps;

        // Handle empty state
        if nodes.is_empty() {
            let msg = "No tasks loaded";
            let x = inner_area.x + (inner_area.width.saturating_sub(msg.len() as u16)) / 2;
            let y = inner_area.y + inner_area.height / 2;
            frame
                .buffer_mut()
                .set_string(x, y, msg, Style::default().fg(theme.text_muted));
            return;
        }

        // Render DagAscii widget
        let dag_widget = DagAscii::new(nodes)
            .with_theme(theme)
            .with_dependencies(deps.clone())
            .mode(self.dag_mode)
            .frame(self.frame)
            .scroll(0, self.scroll[Self::panel_index(PanelId::RunnerDag)] as u16);

        frame.render_widget(dag_widget, inner_area);
    }

    /// Render NovaNet Station panel (Panel 3)
    fn render_novanet_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        // Tab indicator in title
        let tab_indicator = state.ui.novanet_tab.title();

        let block = Block::default()
            .title(format!(" ⊛ NOVANET STATION [{}] ", tab_indicator))
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused));

        // Render block first
        let inner_area = block.inner(area);
        frame.render_widget(block.clone(), area);

        // Tab-aware content rendering
        match state.ui.novanet_tab {
            NovanetTab::FullJson => {
                self.render_novanet_full_json(frame, inner_area, state, theme);
                return;
            }
            NovanetTab::Summary => {
                // Fall through to default MCP call list below
            }
        }

        // Build MCP call list
        let items: Vec<ListItem> = state
            .mcp
            .calls
            .iter()
            .enumerate()
            .map(|(i, call)| {
                let icon = if call.is_error {
                    "✗"
                } else if call.completed {
                    "✓"
                } else {
                    "►"
                };

                let tool_name = call.tool.as_deref().unwrap_or("resource");
                let duration = call
                    .duration_ms
                    .map(|d| format!(" {}ms", d))
                    .unwrap_or_default();

                let style = if call.is_error {
                    Style::default().fg(theme.status_failed)
                } else if call.completed {
                    Style::default().fg(theme.status_success)
                } else {
                    Style::default().fg(theme.highlight)
                };

                ListItem::new(Line::from(vec![
                    Span::styled(format!("[{}] ", icon), style),
                    Span::styled(tool_name, Style::default().fg(theme.text_primary)),
                    Span::styled(duration, Style::default().fg(theme.text_muted)),
                ]))
                .style(
                    if i == self.scroll_offset(PanelId::RunnerNovanet) && focused {
                        Style::default().bg(theme.highlight)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect();

        if items.is_empty() {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                "  No MCP calls yet",
                Style::default().fg(theme.text_muted),
            )]));
            frame.render_widget(empty, inner_area);
        } else {
            let list = List::new(items);
            frame.render_widget(list, inner_area);
        }
    }

    /// Render NovaNet FullJson tab
    /// Shows full JSON response of selected MCP call (uses cached JSON)
    fn render_novanet_full_json(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        let selected_idx = self.scroll_offset(PanelId::RunnerNovanet);
        let call = state.mcp.calls.get(selected_idx);

        let content = if let Some(call) = call {
            let tool_name = call.tool.as_deref().unwrap_or("resource");
            // Use cached JSON (refreshed before render)
            format!("─── {} ───\n{}", tool_name, &self.cached_mcp_response_json)
        } else {
            "No MCP call selected".to_string()
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .scroll((
                self.scroll[Self::panel_index(PanelId::RunnerNovanet)] as u16,
                0,
            ));
        frame.render_widget(paragraph, area);
    }

    /// Render Agent Reasoning panel (Panel 4)
    fn render_agent_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        // Tab indicator in title
        let tab_indicator = state.ui.reasoning_tab.title();

        let block = Block::default()
            .title(format!(" ⊕ AGENT REASONING [{}] ", tab_indicator))
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(theme.border_style(focused));

        // Render block first
        let inner_area = block.inner(area);
        frame.render_widget(block.clone(), area);

        // Tab-aware content rendering
        match state.ui.reasoning_tab {
            ReasoningTab::Thinking => {
                self.render_agent_thinking(frame, inner_area, state, theme);
                return;
            }
            ReasoningTab::Steps => {
                self.render_agent_steps(frame, inner_area, state, theme);
                return;
            }
            ReasoningTab::Turns => {
                // Fall through to default agent turns list below
            }
        }

        // Build agent turn list with thinking display
        let items: Vec<ListItem> = state
            .agent
            .turns
            .iter()
            .enumerate()
            .map(|(i, turn)| {
                let tools = if turn.tool_calls.is_empty() {
                    "".to_string()
                } else {
                    format!(" → {}", turn.tool_calls.join(", "))
                };

                let tokens = turn
                    .tokens
                    .map(|t| format!(" [{}T]", t))
                    .unwrap_or_default();

                // Main turn line
                let main_line = Line::from(vec![
                    Span::styled(
                        format!("Turn {}: ", turn.index + 1),
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&turn.status, Style::default().fg(theme.text_primary)),
                    Span::styled(tools, Style::default().fg(theme.text_muted)),
                    Span::styled(tokens, Style::default().fg(theme.status_paused)),
                ]);

                // Build lines vec - main line plus optional thinking
                let mut lines = vec![main_line];

                // Add thinking content if present
                // Use unicode-aware truncation to avoid panic on multi-byte chars
                if let Some(ref thinking) = turn.thinking {
                    let truncated = truncate_to_width(thinking, 100);
                    lines.push(Line::from(vec![
                        Span::styled("  💭 ", Style::default().fg(theme.status_paused)),
                        Span::styled(
                            truncated,
                            Style::default()
                                .fg(theme.text_muted)
                                .add_modifier(Modifier::ITALIC),
                        ),
                    ]));
                }

                ListItem::new(Text::from(lines)).style(
                    if i == self.scroll_offset(PanelId::RunnerReasoning) && focused {
                        Style::default().bg(theme.highlight)
                    } else {
                        Style::default()
                    },
                )
            })
            .collect();

        if items.is_empty() {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                "  No agent activity",
                Style::default().fg(theme.text_muted),
            )]));
            frame.render_widget(empty, inner_area);
        } else {
            let list = List::new(items);
            frame.render_widget(list, inner_area);
        }
    }

    /// Render Agent Thinking tab
    /// Shows full thinking/reasoning content for selected turn
    fn render_agent_thinking(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        let selected_idx = self.scroll_offset(PanelId::RunnerReasoning);
        let turn = state.agent.turns.get(selected_idx);

        let content = if let Some(turn) = turn {
            if let Some(ref thinking) = turn.thinking {
                format!("─── Turn {} Thinking ───\n{}", turn.index + 1, thinking)
            } else {
                format!("Turn {} has no thinking content", turn.index + 1)
            }
        } else {
            "No agent turn selected".to_string()
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .scroll((
                self.scroll[Self::panel_index(PanelId::RunnerReasoning)] as u16,
                0,
            ));
        frame.render_widget(paragraph, area);
    }

    /// Render Agent Steps tab
    /// Shows step-by-step breakdown of agent reasoning
    fn render_agent_steps(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        let selected_idx = self.scroll_offset(PanelId::RunnerReasoning);
        let turn = state.agent.turns.get(selected_idx);

        let content = if let Some(turn) = turn {
            let mut steps = format!("─── Turn {} Steps ───\n", turn.index + 1);
            steps.push_str(&format!("Status: {}\n", turn.status));

            if !turn.tool_calls.is_empty() {
                steps.push_str("\nTool Calls:\n");
                for (i, tool) in turn.tool_calls.iter().enumerate() {
                    steps.push_str(&format!("  {}. {}\n", i + 1, tool));
                }
            }

            if let Some(tokens) = turn.tokens {
                steps.push_str(&format!("\nTokens: {}\n", tokens));
            }

            steps
        } else {
            "No agent turn selected".to_string()
        };

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: true })
            .scroll((
                self.scroll[Self::panel_index(PanelId::RunnerReasoning)] as u16,
                0,
            ));
        frame.render_widget(paragraph, area);
    }
    // Render_footer() removed - global StatusBar handles metrics now
}

impl Default for MonitorView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for MonitorView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Minimum height guard - need at least 8 lines for 2x2 grid
        if area.height < 8 {
            // Terminal too small - render fallback message
            let msg = "↕ Terminal too small";
            let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
            let y = area.y + area.height / 2;
            frame
                .buffer_mut()
                .set_string(x, y, msg, Style::default().fg(Color::Yellow));
            return;
        }

        // Removed internal footer - global StatusBar handles this now
        // 4-panel grid (2x2) using full area
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(area);

        let top_panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);

        let bottom_panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);

        // Refresh caches before rendering (only rebuild if data changed)
        self.refresh_task_json_cache(state);
        self.refresh_mcp_json_cache(state);
        self.refresh_dag_cache(state);

        // Render 4 panels
        self.render_mission_panel(
            frame,
            top_panels[0],
            state,
            theme,
            self.focus == PanelId::RunnerMission,
        );
        self.render_dag_panel(
            frame,
            top_panels[1],
            state,
            theme,
            self.focus == PanelId::RunnerDag,
        );
        self.render_novanet_panel(
            frame,
            bottom_panels[0],
            state,
            theme,
            self.focus == PanelId::RunnerNovanet,
        );
        self.render_agent_panel(
            frame,
            bottom_panels[1],
            state,
            theme,
            self.focus == PanelId::RunnerReasoning,
        );
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        match key.code {
            // Escape returns to Studio
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Studio),

            // Tab cycles panels
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.focus_prev();
                } else {
                    self.focus_next();
                }
                ViewAction::None
            }

            // Number keys focus specific panels
            KeyCode::Char('1') => {
                self.focus = PanelId::RunnerMission;
                ViewAction::None
            }
            KeyCode::Char('2') => {
                self.focus = PanelId::RunnerDag;
                ViewAction::None
            }
            KeyCode::Char('3') => {
                self.focus = PanelId::RunnerNovanet;
                ViewAction::None
            }
            KeyCode::Char('4') => {
                self.focus = PanelId::RunnerReasoning;
                ViewAction::None
            }

            // Vim-style scrolling in focused panel
            KeyCode::Char('j') | KeyCode::Down => {
                if self.focus == PanelId::RunnerMission {
                    self.select_next_task(state.task_order.len());
                } else {
                    self.scroll_down();
                }
                ViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.focus == PanelId::RunnerMission {
                    self.select_prev_task(state.task_order.len());
                } else {
                    self.scroll_up();
                }
                ViewAction::None
            }

            // 'm' cycles TaskBox render mode (Compact/Expanded/Full)
            KeyCode::Char('m') => {
                self.cycle_render_mode();
                ViewAction::None
            }

            // 'e' toggles DAG mode (Minimal/Expanded)
            KeyCode::Char('e') => {
                self.toggle_dag_mode();
                ViewAction::None
            }

            // 't' cycles sub-tabs within the focused panel
            KeyCode::Char('t') => {
                match self.focus {
                    PanelId::RunnerMission => state.ui.mission_tab = state.ui.mission_tab.next(),
                    PanelId::RunnerDag => state.ui.dag_tab = state.ui.dag_tab.next(),
                    PanelId::RunnerNovanet => state.ui.novanet_tab = state.ui.novanet_tab.next(),
                    PanelId::RunnerReasoning => {
                        state.ui.reasoning_tab = state.ui.reasoning_tab.next()
                    }
                    _ => {}
                }
                ViewAction::None
            }

            // ? falls through to app-level Help mode
            _ => ViewAction::None,
        }
    }

    fn status_line(&self, state: &TuiState) -> String {
        let phase = match state.workflow.phase {
            MissionPhase::Preflight => "Preflight",
            MissionPhase::Countdown => "Countdown",
            MissionPhase::Launch => "Launching",
            MissionPhase::Orbital => "Running",
            MissionPhase::Rendezvous => "MCP Call",
            MissionPhase::MissionSuccess => "Complete",
            MissionPhase::Abort => "Aborted",
            MissionPhase::Pause => "Paused",
        };

        let progress = state.workflow.progress_pct();
        let task_count = state.workflow.task_count;
        let completed = state.workflow.tasks_completed;

        // Format tokens with K/M suffixes
        let tokens = state.metrics.total_tokens;
        let token_str = if tokens >= 1_000_000 {
            format!("{:.1}M", tokens as f64 / 1_000_000.0)
        } else if tokens >= 1_000 {
            format!("{:.1}K", tokens as f64 / 1_000.0)
        } else {
            format!("{}", tokens)
        };

        // Format cost
        let cost = state.metrics.cost_usd;
        let cost_str = if cost >= 1.0 {
            format!("${:.2}", cost)
        } else if cost >= 0.01 {
            format!("${:.3}", cost)
        } else {
            format!("${:.4}", cost)
        };

        // Mode indicator
        let mode_str = match self.render_mode {
            RenderMode::Compact => "C",
            RenderMode::Expanded => "E",
            RenderMode::Full => "F",
        };

        // Token velocity sparkline
        let sparkline = state.metrics.token_velocity.sparkline_chars();
        let velocity_str = if sparkline.is_empty() {
            String::new()
        } else {
            format!(" {}", sparkline)
        };

        format!(
            "Runner • {} • {}/{} • {:.0}% • {} tok{} • {} • [{}]",
            phase, completed, task_count, progress, token_str, velocity_str, cost_str, mode_str
        )
    }

    fn tick(&mut self, _state: &mut TuiState) {
        // Update animation frame (wraps at 60)
        self.frame = self.frame.wrapping_add(1);
        if self.frame >= 60 {
            self.frame = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_view_new() {
        let view = MonitorView::new();
        assert_eq!(view.focus, PanelId::RunnerMission);
        assert_eq!(view.scroll, [0, 0, 0, 0]);
        assert_eq!(view.frame, 0);
        assert_eq!(view.selected_task, 0);
        assert_eq!(view.render_mode, RenderMode::Expanded);
        assert_eq!(view.dag_mode, NodeBoxMode::Minimal);
    }

    #[test]
    fn test_monitor_view_default() {
        let view = MonitorView::default();
        assert_eq!(view.focus, PanelId::RunnerMission);
        assert_eq!(view.selected_task, 0);
        assert_eq!(view.render_mode, RenderMode::Expanded);
        assert_eq!(view.dag_mode, NodeBoxMode::Minimal);
    }

    #[test]
    fn test_select_next_task() {
        let mut view = MonitorView::new();
        view.select_next_task(3);
        assert_eq!(view.selected_task, 1);
        view.select_next_task(3);
        assert_eq!(view.selected_task, 2);
        view.select_next_task(3);
        assert_eq!(view.selected_task, 0); // wraps
    }

    #[test]
    fn test_select_prev_task() {
        let mut view = MonitorView::new();
        view.select_prev_task(3);
        assert_eq!(view.selected_task, 2); // wraps
        view.select_prev_task(3);
        assert_eq!(view.selected_task, 1);
    }

    #[test]
    fn test_cycle_render_mode() {
        let mut view = MonitorView::new();
        assert_eq!(view.render_mode, RenderMode::Expanded);
        view.cycle_render_mode();
        assert_eq!(view.render_mode, RenderMode::Full);
        view.cycle_render_mode();
        assert_eq!(view.render_mode, RenderMode::Compact);
        view.cycle_render_mode();
        assert_eq!(view.render_mode, RenderMode::Expanded);
    }

    #[test]
    fn test_toggle_dag_mode() {
        let mut view = MonitorView::new();
        assert_eq!(view.dag_mode, NodeBoxMode::Minimal);
        view.toggle_dag_mode();
        assert_eq!(view.dag_mode, NodeBoxMode::Expanded);
        view.toggle_dag_mode();
        assert_eq!(view.dag_mode, NodeBoxMode::Minimal);
    }

    #[test]
    fn test_verb_from_task_type() {
        assert_eq!(
            MonitorView::verb_from_task_type(Some("infer")),
            VerbColor::Infer
        );
        assert_eq!(
            MonitorView::verb_from_task_type(Some("exec")),
            VerbColor::Exec
        );
        assert_eq!(
            MonitorView::verb_from_task_type(Some("fetch")),
            VerbColor::Fetch
        );
        assert_eq!(
            MonitorView::verb_from_task_type(Some("invoke")),
            VerbColor::Invoke
        );
        assert_eq!(
            MonitorView::verb_from_task_type(Some("agent")),
            VerbColor::Agent
        );
        assert_eq!(MonitorView::verb_from_task_type(None), VerbColor::Infer);
    }

    #[test]
    fn test_focus_next_cycles() {
        let mut view = MonitorView::new();
        assert_eq!(view.focus, PanelId::RunnerMission);
        view.focus_next();
        assert_eq!(view.focus, PanelId::RunnerDag);
        view.focus_next();
        assert_eq!(view.focus, PanelId::RunnerNovanet);
        view.focus_next();
        assert_eq!(view.focus, PanelId::RunnerReasoning);
        view.focus_next();
        assert_eq!(view.focus, PanelId::RunnerMission);
    }

    #[test]
    fn test_focus_prev_cycles() {
        let mut view = MonitorView::new();
        assert_eq!(view.focus, PanelId::RunnerMission);
        view.focus_prev();
        assert_eq!(view.focus, PanelId::RunnerReasoning);
        view.focus_prev();
        assert_eq!(view.focus, PanelId::RunnerNovanet);
    }

    #[test]
    fn test_scroll_down() {
        let mut view = MonitorView::new();
        assert_eq!(view.scroll[0], 0);
        view.scroll_down();
        assert_eq!(view.scroll[0], 1);
        view.scroll_down();
        assert_eq!(view.scroll[0], 2);
    }

    #[test]
    fn test_scroll_up() {
        let mut view = MonitorView::new();
        view.scroll[0] = 5;
        view.scroll_up();
        assert_eq!(view.scroll[0], 4);
    }

    #[test]
    fn test_scroll_up_at_zero() {
        let mut view = MonitorView::new();
        view.scroll_up();
        assert_eq!(view.scroll[0], 0); // Stays at 0
    }

    #[test]
    fn test_scroll_offset_per_panel() {
        let mut view = MonitorView::new();
        view.scroll[0] = 1; // Progress
        view.scroll[1] = 2; // Dag
        view.scroll[2] = 3; // NovaNet
        view.scroll[3] = 4; // Agent

        assert_eq!(view.scroll_offset(PanelId::RunnerMission), 1);
        assert_eq!(view.scroll_offset(PanelId::RunnerDag), 2);
        assert_eq!(view.scroll_offset(PanelId::RunnerNovanet), 3);
        assert_eq!(view.scroll_offset(PanelId::RunnerReasoning), 4);
    }

    #[test]
    fn test_phase_icon() {
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Preflight), "🚀");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Countdown), "⏱");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Launch), "🔥");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Orbital), "🛸");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Rendezvous), "🎯");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::MissionSuccess), "✓");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Abort), "✗");
        assert_eq!(MonitorView::phase_icon(&MissionPhase::Pause), "⏸");
    }

    #[test]
    fn test_handle_key_escape_returns_studio() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Studio)));
    }

    #[test]
    fn test_handle_key_q_returns_studio() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Studio)));
    }

    #[test]
    fn test_handle_key_tab_cycles_focus() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.focus, PanelId::RunnerDag);
    }

    #[test]
    fn test_handle_key_shift_tab_cycles_backwards() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        view.handle_key(key, &mut state);
        assert_eq!(view.focus, PanelId::RunnerReasoning);
    }

    #[test]
    fn test_handle_key_number_focuses_panel() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");

        view.handle_key(
            KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
            &mut state,
        );
        assert_eq!(view.focus, PanelId::RunnerDag);

        view.handle_key(
            KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE),
            &mut state,
        );
        assert_eq!(view.focus, PanelId::RunnerNovanet);

        view.handle_key(
            KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE),
            &mut state,
        );
        assert_eq!(view.focus, PanelId::RunnerReasoning);

        view.handle_key(
            KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
            &mut state,
        );
        assert_eq!(view.focus, PanelId::RunnerMission);
    }

    #[test]
    fn test_handle_key_j_selects_task_in_mission_panel() {
        // J/k now select tasks when Mission panel is focused
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        // Add some tasks to state
        state.task_order = vec!["task1".to_string(), "task2".to_string()];

        assert_eq!(view.selected_task, 0);
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.selected_task, 1);
    }

    #[test]
    fn test_handle_key_j_scrolls_in_other_panels() {
        // J/k still scroll when other panels are focused
        let mut view = MonitorView::new();
        view.focus = PanelId::RunnerDag;
        let mut state = TuiState::new("test");

        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll[1], 1); // DAG panel scroll
    }

    #[test]
    fn test_handle_key_k_selects_prev_task_in_mission_panel() {
        // J/k now select tasks when Mission panel is focused
        let mut view = MonitorView::new();
        view.selected_task = 1;
        let mut state = TuiState::new("test");
        state.task_order = vec!["task1".to_string(), "task2".to_string()];

        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.selected_task, 0);
    }

    #[test]
    fn test_handle_key_k_scrolls_in_other_panels() {
        // J/k still scroll when other panels are focused
        let mut view = MonitorView::new();
        view.focus = PanelId::RunnerDag;
        view.scroll[1] = 5;
        let mut state = TuiState::new("test");

        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll[1], 4); // DAG panel scroll
    }

    #[test]
    fn test_handle_key_question_falls_through_to_app() {
        // '?' is NOT handled by MonitorView - it falls through to app.rs
        // which triggers TuiMode::Help. This is consistent with all other views.
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::None));
    }

    #[test]
    fn test_status_line_shows_phase() {
        let view = MonitorView::new();
        let state = TuiState::new("test");
        let status = view.status_line(&state);
        // Status line now says "Runner" instead of "Monitor"
        assert!(status.contains("Runner"));
        assert!(status.contains("Preflight"));
    }

    #[test]
    fn test_tick_increments_frame() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        assert_eq!(view.frame, 0);
        view.tick(&mut state);
        assert_eq!(view.frame, 1);
    }

    #[test]
    fn test_tick_wraps_at_60() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        view.frame = 59;
        view.tick(&mut state);
        assert_eq!(view.frame, 0);
    }

    #[test]
    fn test_implements_view_trait() {
        let view = MonitorView::new();
        let _: &dyn View = &view;
    }

    // Thinking display tests
    #[test]
    fn test_agent_turn_with_thinking_short() {
        use crate::tui::AgentTurnState;

        let mut state = TuiState::new("test");
        state.agent.turns.push(AgentTurnState {
            index: 0,
            status: "Thinking...".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some("This is a short thinking string".to_string()),
            response_text: None,
        });

        // Verify the thinking content is present in state
        assert!(state.agent.turns[0].thinking.is_some());
        let thinking = state.agent.turns[0].thinking.as_ref().unwrap();
        assert_eq!(thinking, "This is a short thinking string");
        assert!(thinking.len() <= 100); // Should not be truncated
    }

    #[test]
    fn test_agent_turn_with_thinking_truncated() {
        use crate::tui::AgentTurnState;

        let mut state = TuiState::new("test");
        let long_thinking = "A".repeat(150); // 150 characters
        state.agent.turns.push(AgentTurnState {
            index: 0,
            status: "Thinking...".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some(long_thinking.clone()),
            response_text: None,
        });

        // Verify the thinking content is long enough to require truncation
        let thinking = state.agent.turns[0].thinking.as_ref().unwrap();
        assert!(thinking.len() > 100);

        // Test truncation logic matches render_agent_panel
        let truncated = if thinking.len() > 100 {
            format!("{}...", crate::util::truncate_str(thinking, 97))
        } else {
            thinking.clone()
        };
        assert_eq!(truncated.len(), 100); // 97 chars + "..."
        assert!(truncated.ends_with("..."));
    }
}
