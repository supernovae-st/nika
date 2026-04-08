// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Command View — Fusion of Chat + Runner
//!
//! The Command view merges the Chat agent and Runner execution monitoring
//! into a single unified view with two internal modes:
//!
//! - **Chat mode** (default): Conversational AI interface with inline TaskBoxes
//!   - Supports a collapsible **instruments panel** on the right (35% width)
//!   - Toggle with `[` key
//! - **Monitor mode**: Real-time workflow execution dashboard (4-panel layout)
//!
//! Users switch modes with Ctrl+M. Workflows auto-switch to Monitor mode.
//! Both views stay alive — chat history persists while monitoring.

use rustc_hash::FxHashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Widget},
    Frame,
};

use super::chat::ChatView;
use super::monitor::MonitorView;
use super::{View, ViewAction};
use crate::state::TuiState;
use crate::theme::{Theme, VerbColor};
use crate::tokens::compat;
use crate::widgets::{DagAscii, NodeBoxData, NodeBoxMode};

/// Internal mode within CommandView
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommandMode {
    /// Chat agent conversation (default)
    #[default]
    Chat,
    /// Workflow execution monitoring (auto-switched when workflow starts)
    Monitor,
}

/// Command View — Chat agent + workflow execution monitoring
pub struct CommandView {
    /// Current internal mode
    pub mode: CommandMode,
    /// Chat view (always alive — preserves conversation)
    pub chat: ChatView,
    /// Monitor view (always alive — preserves execution state)
    pub monitor: MonitorView,
    /// Whether the instruments panel is visible in Chat mode
    pub instruments_visible: bool,
}

impl Default for CommandView {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandView {
    pub fn new() -> Self {
        Self {
            mode: CommandMode::Chat,
            chat: ChatView::new(),
            monitor: MonitorView::new(),
            instruments_visible: true,
        }
    }

    /// Switch to Monitor mode (called when workflow execution starts)
    pub fn switch_to_monitor(&mut self) {
        self.mode = CommandMode::Monitor;
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

    /// Render the instruments side panel (right 35% in Chat mode)
    ///
    /// Sections rendered (top to bottom, conditional):
    /// 1. DAG -- if a workflow is running (task_count > 0)
    /// 2. Metrics -- always shown (tokens, cost, elapsed)
    /// 3. MCP -- if any MCP calls have been made
    fn render_instruments(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Outer block: LEFT border only, rounded
        let outer = Block::default()
            .borders(Borders::LEFT)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(theme.border_normal));
        let inner = outer.inner(area);
        frame.render_widget(outer, area);

        // Determine which sections are active
        let has_dag = state.workflow.task_count > 0;
        let has_mcp = !state.mcp.calls.is_empty();

        // Build vertical constraints based on active sections
        let mut constraints: Vec<Constraint> = Vec::new();
        if has_dag {
            constraints.push(Constraint::Min(6));
        }
        // Metrics always present (fixed 6 lines: header + 4 data + spacer)
        constraints.push(Constraint::Length(6));
        if has_mcp {
            constraints.push(Constraint::Min(4));
        }

        let sections = Layout::default()
            .direction(Direction::Vertical)
            .constraints(constraints)
            .split(inner);

        let mut section_idx = 0;

        // --- DAG Section ---
        if has_dag {
            self.render_instruments_dag(frame, sections[section_idx], state, theme);
            section_idx += 1;
        }

        // --- Metrics Section ---
        self.render_instruments_metrics(frame, sections[section_idx], state, theme);
        section_idx += 1;

        // --- MCP Section ---
        if has_mcp {
            self.render_instruments_mcp(frame, sections[section_idx], state, theme);
        }
        let _ = section_idx;
    }

    /// Render DAG mini-view inside instruments panel
    fn render_instruments_dag(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        if area.height < 2 {
            return;
        }

        // Section header
        let header = Line::from(Span::styled(
            " DAG",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        ));
        frame.render_widget(
            Paragraph::new(header),
            Rect::new(area.x, area.y, area.width, 1),
        );

        let dag_area = Rect::new(
            area.x,
            area.y + 1,
            area.width,
            area.height.saturating_sub(1),
        );
        let mut nodes: Vec<NodeBoxData> = Vec::new();
        let mut deps: FxHashMap<String, Vec<String>> = FxHashMap::default();

        for task_id in &state.task_order {
            if let Some(task) = state.tasks.get(task_id) {
                let verb_color = Self::verb_from_task_type(task.task_type.as_deref());
                let mut node = NodeBoxData::new(task_id, verb_color).with_status(task.status);
                if let Some(model) = &task.model {
                    node = node.with_model(model);
                }
                nodes.push(node);
                if !task.dependencies.is_empty() {
                    deps.insert(task_id.clone(), task.dependencies.clone());
                }
            }
        }

        if !nodes.is_empty() {
            let dag = DagAscii::new(&nodes)
                .with_dependencies(deps)
                .mode(NodeBoxMode::Minimal)
                .with_theme(theme);
            dag.render(dag_area, frame.buffer_mut());
        }
    }

    /// Render metrics section inside instruments panel
    fn render_instruments_metrics(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        if area.height < 2 {
            return;
        }

        let header = Line::from(Span::styled(
            " Metrics",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        ));

        let mut lines: Vec<Line> = vec![header];

        // Elapsed time
        let elapsed = if let Some(total_ms) = state.workflow.total_duration_ms {
            format_duration_ms(total_ms)
        } else if let Some(started) = state.workflow.started_at {
            format_duration_ms(started.elapsed().as_millis() as u64)
        } else {
            "00:00.0".to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(" \u{23f1} ", Style::default().fg(theme.text_muted)),
            Span::styled(elapsed, Style::default().fg(theme.text_primary)),
        ]));

        // Task progress bar
        if state.workflow.task_count > 0 {
            let completed = state.workflow.tasks_completed;
            let total = state.workflow.task_count;
            let pct = state.workflow.progress_pct();
            let filled = ((pct / 100.0) * 5.0).round() as usize;
            let bar: String = "\u{2593}".repeat(filled.min(5))
                + &"\u{2591}".repeat(5_usize.saturating_sub(filled));
            lines.push(Line::from(vec![
                Span::styled(" Tasks  ", Style::default().fg(theme.text_muted)),
                Span::styled(bar, Style::default().fg(compat::GREEN_500)),
                Span::styled(
                    format!(" {}/{} {:>3.0}%", completed, total, pct),
                    Style::default().fg(theme.text_secondary),
                ),
            ]));
        }

        // Token usage
        let input_k = format_token_count(state.metrics.input_tokens);
        let output_k = format_token_count(state.metrics.output_tokens);
        lines.push(Line::from(vec![
            Span::styled(" Tokens ", Style::default().fg(theme.text_muted)),
            Span::styled(input_k, Style::default().fg(theme.text_primary)),
            Span::styled(" in \u{2502} ", Style::default().fg(theme.text_muted)),
            Span::styled(output_k, Style::default().fg(theme.text_primary)),
            Span::styled(" out", Style::default().fg(theme.text_muted)),
        ]));

        // Cost
        lines.push(Line::from(vec![
            Span::styled(" Cost   ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("${:.3}", state.metrics.cost_usd),
                Style::default().fg(compat::AMBER_400),
            ),
        ]));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    /// Render MCP section inside instruments panel
    fn render_instruments_mcp(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
        if area.height < 2 {
            return;
        }

        let header = Line::from(Span::styled(
            " MCP",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::DIM),
        ));

        let mut lines: Vec<Line> = vec![header];

        // Aggregate MCP calls by server: (tool_count, total_ms, calls_with_duration)
        let mut server_stats: FxHashMap<&str, (usize, u64, usize)> = FxHashMap::default();
        for call in &state.mcp.calls {
            let entry = server_stats
                .entry(call.server.as_str())
                .or_insert((0, 0, 0));
            entry.0 += 1;
            if let Some(ms) = call.duration_ms {
                entry.1 += ms;
                entry.2 += 1;
            }
        }

        for (server, (tool_count, total_ms, with_dur)) in &server_stats {
            let tool_label = if *tool_count == 1 { "tool" } else { "tools" };
            let latency = if *with_dur > 0 {
                format!("  {}ms", total_ms / (*with_dur as u64))
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled(" \u{25cf} ", Style::default().fg(compat::GREEN_400)),
                Span::styled(
                    format!("{:<12}", server),
                    Style::default().fg(theme.text_primary),
                ),
                Span::styled(
                    format!("{} {}", tool_count, tool_label),
                    Style::default().fg(theme.text_secondary),
                ),
                Span::styled(latency, Style::default().fg(theme.text_muted)),
            ]));
        }

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }
}

/// Format milliseconds as MM:SS.D
fn format_duration_ms(ms: u64) -> String {
    let total_secs = ms / 1000;
    let minutes = total_secs / 60;
    let seconds = total_secs % 60;
    let tenths = (ms % 1000) / 100;
    format!("{:02}:{:02}.{}", minutes, seconds, tenths)
}

/// Format token count with K suffix for large values
fn format_token_count(tokens: u64) -> String {
    if tokens >= 1000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        format!("{}", tokens)
    }
}

impl View for CommandView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        match self.mode {
            CommandMode::Chat => {
                if self.instruments_visible && area.width >= 60 {
                    // Split: 65% chat | 35% instruments
                    let chunks = Layout::default()
                        .direction(Direction::Horizontal)
                        .constraints([Constraint::Percentage(65), Constraint::Percentage(35)])
                        .split(area);

                    self.chat.render(frame, chunks[0], state, theme);
                    self.render_instruments(frame, chunks[1], state, theme);
                } else {
                    // Full-width chat (instruments hidden or terminal too narrow)
                    self.chat.render(frame, area, state, theme);
                }
            }
            CommandMode::Monitor => self.monitor.render(frame, area, state, theme),
        }
    }

    fn handle_key(&mut self, key: KeyEvent, state: &mut TuiState) -> ViewAction {
        // Ctrl+M: Toggle between Chat and Monitor modes
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('m') {
            self.mode = match self.mode {
                CommandMode::Chat => CommandMode::Monitor,
                CommandMode::Monitor => CommandMode::Chat,
            };
            return ViewAction::None;
        }

        // `[`: Toggle instruments panel (Chat mode only, no modifiers)
        if self.mode == CommandMode::Chat
            && key.code == KeyCode::Char('[')
            && key.modifiers == KeyModifiers::NONE
        {
            self.instruments_visible = !self.instruments_visible;
            return ViewAction::None;
        }

        // Esc/q in Monitor mode: switch back to Chat (not Studio)
        if self.mode == CommandMode::Monitor
            && matches!(key.code, KeyCode::Esc | KeyCode::Char('q'))
            && key.modifiers.is_empty()
        {
            self.mode = CommandMode::Chat;
            return ViewAction::None;
        }

        match self.mode {
            CommandMode::Chat => self.chat.handle_key(key, state),
            CommandMode::Monitor => self.monitor.handle_key(key, state),
        }
    }

    fn status_line(&self, state: &TuiState) -> String {
        match self.mode {
            CommandMode::Chat => self.chat.status_line(state),
            CommandMode::Monitor => self.monitor.status_line(state),
        }
    }

    fn on_enter(&mut self, state: &mut TuiState) {
        match self.mode {
            CommandMode::Chat => self.chat.on_enter(state),
            CommandMode::Monitor => self.monitor.on_enter(state),
        }
    }

    fn on_leave(&mut self, state: &mut TuiState) {
        match self.mode {
            CommandMode::Chat => self.chat.on_leave(state),
            CommandMode::Monitor => self.monitor.on_leave(state),
        }
    }

    fn tick(&mut self, state: &mut TuiState) {
        // Always tick both --- monitor tracks execution in background while
        // chat is visible, and chat may be streaming while monitor is visible
        self.chat.tick();
        self.monitor.tick(state);
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Construction Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_command_view_defaults() {
        let view = CommandView::new();
        assert_eq!(view.mode, CommandMode::Chat);
        assert!(view.instruments_visible);
    }

    #[test]
    fn test_command_mode_default_is_chat() {
        assert_eq!(CommandMode::default(), CommandMode::Chat);
    }

    // -------------------------------------------------------------------------
    // Mode Switching Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_switch_to_monitor() {
        let mut view = CommandView::new();
        view.switch_to_monitor();
        assert_eq!(view.mode, CommandMode::Monitor);
    }

    #[test]
    fn test_ctrl_m_toggles_mode() {
        let mut view = CommandView::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::new(KeyCode::Char('m'), KeyModifiers::CONTROL);

        // Chat -> Monitor
        let action = view.handle_key(key, &mut state);
        assert_eq!(view.mode, CommandMode::Monitor);
        assert_eq!(action, ViewAction::None);

        // Monitor -> Chat
        let action = view.handle_key(key, &mut state);
        assert_eq!(view.mode, CommandMode::Chat);
        assert_eq!(action, ViewAction::None);
    }

    // -------------------------------------------------------------------------
    // Instruments Toggle Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_bracket_toggles_instruments() {
        let mut view = CommandView::new();
        let mut state = TuiState::new("test.nika.yaml");
        let key = KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE);

        assert!(view.instruments_visible);

        // Toggle off
        let action = view.handle_key(key, &mut state);
        assert!(!view.instruments_visible);
        assert_eq!(action, ViewAction::None);

        // Toggle on
        let action = view.handle_key(key, &mut state);
        assert!(view.instruments_visible);
        assert_eq!(action, ViewAction::None);
    }

    #[test]
    fn test_bracket_only_works_in_chat_mode() {
        let mut view = CommandView::new();
        let mut state = TuiState::new("test.nika.yaml");
        view.switch_to_monitor();

        let key = KeyEvent::new(KeyCode::Char('['), KeyModifiers::NONE);
        // In Monitor mode, `[` should NOT toggle instruments
        view.handle_key(key, &mut state);
        assert!(view.instruments_visible);
    }

    #[test]
    fn test_bracket_with_modifiers_does_not_toggle() {
        let mut view = CommandView::new();
        let mut state = TuiState::new("test.nika.yaml");

        // Ctrl+[ should not toggle instruments
        let key = KeyEvent::new(KeyCode::Char('['), KeyModifiers::CONTROL);
        view.handle_key(key, &mut state);
        assert!(view.instruments_visible);
    }

    // -------------------------------------------------------------------------
    // Helper Function Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_duration_ms() {
        assert_eq!(format_duration_ms(0), "00:00.0");
        assert_eq!(format_duration_ms(4200), "00:04.2");
        assert_eq!(format_duration_ms(61500), "01:01.5");
        assert_eq!(format_duration_ms(125300), "02:05.3");
        assert_eq!(format_duration_ms(999), "00:00.9");
    }

    #[test]
    fn test_format_token_count() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(500), "500");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1000), "1.0k");
        assert_eq!(format_token_count(4200), "4.2k");
        assert_eq!(format_token_count(12345), "12.3k");
    }

    #[test]
    fn test_verb_from_task_type() {
        assert_eq!(
            CommandView::verb_from_task_type(Some("infer")),
            VerbColor::Infer
        );
        assert_eq!(
            CommandView::verb_from_task_type(Some("exec")),
            VerbColor::Exec
        );
        assert_eq!(
            CommandView::verb_from_task_type(Some("fetch")),
            VerbColor::Fetch
        );
        assert_eq!(
            CommandView::verb_from_task_type(Some("invoke")),
            VerbColor::Invoke
        );
        assert_eq!(
            CommandView::verb_from_task_type(Some("agent")),
            VerbColor::Agent
        );
        assert_eq!(CommandView::verb_from_task_type(None), VerbColor::Infer);
        assert_eq!(
            CommandView::verb_from_task_type(Some("unknown")),
            VerbColor::Infer
        );
    }

    // -------------------------------------------------------------------------
    // Status Line Tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_status_line_delegates_to_active_mode() {
        let view = CommandView::new();
        let state = TuiState::new("test.nika.yaml");
        let line = view.status_line(&state);
        assert!(!line.is_empty());
    }
}
