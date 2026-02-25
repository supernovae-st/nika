//! Monitor View - Real-time workflow execution monitoring (v0.11)
//!
//! Displays 4-panel layout for workflow execution visibility:
//!
//! ```text
//! ╭─────────────────────────────────────────────────────────────────────────────────╮
//! │  NIKA MONITOR                                      [Tab] Switch [?] Help [Esc] │
//! ├─────────────────────────────────────────────────────────────────────────────────┤
//! │  ┌─ MISSION CONTROL ─────────┐ ┌─ DAG EXECUTION ─────────┐                      │
//! │  │  ● T1 [■■■■■■■■■■] 100%   │ │  workflow.yaml          │                      │
//! │  │  ● T2 [■■■■■░░░░░]  50%   │ │  ├── task1 ✓            │                      │
//! │  │  ○ T3 [░░░░░░░░░░]   0%   │ │  ├── task2 ►            │                      │
//! │  │                           │ │  └── task3 ○            │                      │
//! │  └───────────────────────────┘ └───────────────────────────────────────────────┘│
//! │  ┌─ NOVANET STATION ─────────┐ ┌─ AGENT REASONING ───────┐                      │
//! │  │  novanet_generate [✓]     │ │  Turn 1: Starting...    │                      │
//! │  │  params: {...}            │ │  Turn 2: Tool call...   │                      │
//! │  │  response: {...}          │ │  Turn 3: Complete       │                      │
//! │  └───────────────────────────┘ └───────────────────────────────────────────────┘│
//! │  [1] Progress [2] DAG [3] NovaNet [4] Reasoning  │  Tokens: 1.2K  Cost: $0.03  │
//! ╰─────────────────────────────────────────────────────────────────────────────────╯
//! ```

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::{TuiView, ViewAction};
use crate::tui::state::{PanelId, TuiState};
use crate::tui::theme::{MissionPhase, TaskStatus, Theme};

/// Monitor View state
///
/// Real-time workflow execution monitoring with 4-panel layout:
/// - Panel 1: Mission Control (task progress)
/// - Panel 2: DAG Execution (dependency graph)
/// - Panel 3: NovaNet Station (MCP calls)
/// - Panel 4: Agent Reasoning (agent turns)
pub struct MonitorView {
    /// Currently focused panel
    pub focus: PanelId,
    /// Scroll offset per panel
    pub scroll: [usize; 4],
    /// Animation frame counter
    pub frame: u8,
}

impl MonitorView {
    /// Create a new MonitorView
    pub fn new() -> Self {
        Self {
            focus: PanelId::Progress,
            scroll: [0; 4],
            frame: 0,
        }
    }

    /// Focus next panel (Tab)
    pub fn focus_next(&mut self) {
        self.focus = self.focus.next();
    }

    /// Focus previous panel (Shift+Tab)
    pub fn focus_prev(&mut self) {
        self.focus = self.focus.prev();
    }

    /// Get scroll offset for current panel
    fn scroll_offset(&self, panel: PanelId) -> usize {
        self.scroll[panel.number() as usize - 1]
    }

    /// Scroll current panel down
    fn scroll_down(&mut self) {
        let idx = self.focus.number() as usize - 1;
        self.scroll[idx] = self.scroll[idx].saturating_add(1);
    }

    /// Scroll current panel up
    fn scroll_up(&mut self) {
        let idx = self.focus.number() as usize - 1;
        self.scroll[idx] = self.scroll[idx].saturating_sub(1);
    }

    /// Get icon for task status
    fn status_icon(status: &TaskStatus) -> &'static str {
        match status {
            TaskStatus::Pending => "○",
            TaskStatus::Running => "►",
            TaskStatus::Success => "✓",
            TaskStatus::Failed => "✗",
            TaskStatus::Paused => "⏸",
        }
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

    /// Render Mission Control panel (Panel 1)
    fn render_mission_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        let border_color = if focused {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(format!(
                " {} MISSION CONTROL ",
                Self::phase_icon(&state.workflow.phase)
            ))
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        // Build task list
        let items: Vec<ListItem> = state
            .task_order
            .iter()
            .enumerate()
            .filter_map(|(i, task_id)| {
                state.tasks.get(task_id).map(|task| {
                    let icon = Self::status_icon(&task.status);
                    let progress = match &task.status {
                        TaskStatus::Success => "[■■■■■■■■■■] 100%".to_string(),
                        TaskStatus::Running => {
                            let pct = ((self.frame as usize * 10 / 60) % 10) + 1;
                            format!(
                                "[{}{}] {}0%",
                                "■".repeat(pct),
                                "░".repeat(10 - pct),
                                pct
                            )
                        }
                        TaskStatus::Failed => "[✗✗✗✗✗✗✗✗✗✗] ERR".to_string(),
                        _ => "[░░░░░░░░░░]   0%".to_string(),
                    };

                    let style = match &task.status {
                        TaskStatus::Running => Style::default().fg(theme.highlight),
                        TaskStatus::Success => Style::default().fg(theme.status_success),
                        TaskStatus::Failed => Style::default().fg(theme.status_failed),
                        _ => Style::default().fg(theme.text_muted),
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{} ", icon), style),
                        Span::styled(format!("{:<12} ", task_id), Style::default().fg(theme.text_primary)),
                        Span::styled(progress, style),
                    ]))
                    .style(if i == self.scroll_offset(PanelId::Progress) && focused {
                        Style::default().bg(theme.highlight)
                    } else {
                        Style::default()
                    })
                })
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    /// Render DAG Execution panel (Panel 2)
    fn render_dag_panel(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        focused: bool,
    ) {
        let border_color = if focused {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" ⎔ DAG EXECUTION ")
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        // Build DAG tree view
        let mut lines: Vec<Line> = vec![];
        lines.push(Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                state.workflow.path.split('/').last().unwrap_or("workflow"),
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        for (i, task_id) in state.task_order.iter().enumerate() {
            let prefix = if i == state.task_order.len() - 1 {
                "  └── "
            } else {
                "  ├── "
            };

            let status = state
                .tasks
                .get(task_id)
                .map(|t| &t.status)
                .unwrap_or(&TaskStatus::Pending);
            let icon = Self::status_icon(status);

            let style = match status {
                TaskStatus::Running => Style::default().fg(theme.highlight),
                TaskStatus::Success => Style::default().fg(theme.status_success),
                TaskStatus::Failed => Style::default().fg(theme.status_failed),
                _ => Style::default().fg(theme.text_muted),
            };

            lines.push(Line::from(vec![
                Span::styled(prefix, Style::default().fg(theme.text_muted)),
                Span::styled(task_id, Style::default().fg(theme.text_primary)),
                Span::styled(format!(" {}", icon), style),
            ]));
        }

        let paragraph = Paragraph::new(lines).block(block);
        frame.render_widget(paragraph, area);
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
        let border_color = if focused {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" ⊛ NOVANET STATION ")
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        // Build MCP call list
        let items: Vec<ListItem> = state
            .mcp_calls
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
                .style(if i == self.scroll_offset(PanelId::NovaNet) && focused {
                    Style::default().bg(theme.highlight)
                } else {
                    Style::default()
                })
            })
            .collect();

        if items.is_empty() {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                "  No MCP calls yet",
                Style::default().fg(theme.text_muted),
            )]))
            .block(block);
            frame.render_widget(empty, area);
        } else {
            let list = List::new(items).block(block);
            frame.render_widget(list, area);
        }
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
        let border_color = if focused {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" ⊕ AGENT REASONING ")
            .title_style(
                Style::default()
                    .fg(theme.highlight)
                    .add_modifier(Modifier::BOLD),
            )
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        // Build agent turn list
        let items: Vec<ListItem> = state
            .agent_turns
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

                ListItem::new(Line::from(vec![
                    Span::styled(
                        format!("Turn {}: ", turn.index + 1),
                        Style::default()
                            .fg(theme.highlight)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(&turn.status, Style::default().fg(theme.text_primary)),
                    Span::styled(tools, Style::default().fg(theme.text_muted)),
                    Span::styled(tokens, Style::default().fg(theme.status_paused)),
                ]))
                .style(if i == self.scroll_offset(PanelId::Agent) && focused {
                    Style::default().bg(theme.highlight)
                } else {
                    Style::default()
                })
            })
            .collect();

        if items.is_empty() {
            let empty = Paragraph::new(Line::from(vec![Span::styled(
                "  No agent activity",
                Style::default().fg(theme.text_muted),
            )]))
            .block(block);
            frame.render_widget(empty, area);
        } else {
            let list = List::new(items).block(block);
            frame.render_widget(list, area);
        }
    }

    /// Render metrics footer
    fn render_footer(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        let tokens = if state.metrics.total_tokens >= 1000 {
            format!("{:.1}K", state.metrics.total_tokens as f64 / 1000.0)
        } else {
            format!("{}", state.metrics.total_tokens)
        };

        let cost = format!("${:.3}", state.metrics.cost_usd);

        let footer = Paragraph::new(Line::from(vec![
            Span::styled("[1]", Style::default().fg(theme.highlight)),
            Span::styled(" Progress ", Style::default().fg(if self.focus == PanelId::Progress { theme.text_primary } else { theme.text_muted })),
            Span::styled("[2]", Style::default().fg(theme.highlight)),
            Span::styled(" DAG ", Style::default().fg(if self.focus == PanelId::Dag { theme.text_primary } else { theme.text_muted })),
            Span::styled("[3]", Style::default().fg(theme.highlight)),
            Span::styled(" NovaNet ", Style::default().fg(if self.focus == PanelId::NovaNet { theme.text_primary } else { theme.text_muted })),
            Span::styled("[4]", Style::default().fg(theme.highlight)),
            Span::styled(" Reasoning ", Style::default().fg(if self.focus == PanelId::Agent { theme.text_primary } else { theme.text_muted })),
            Span::styled("  │  ", Style::default().fg(theme.border_normal)),
            Span::styled("Tokens: ", Style::default().fg(theme.text_muted)),
            Span::styled(&tokens, Style::default().fg(theme.highlight)),
            Span::styled("  Cost: ", Style::default().fg(theme.text_muted)),
            Span::styled(&cost, Style::default().fg(theme.status_success)),
        ]));

        frame.render_widget(footer, area);
    }
}

impl Default for MonitorView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for MonitorView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Main layout: content + footer
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints([
                Constraint::Min(1),    // Content
                Constraint::Length(1), // Footer
            ])
            .split(area);

        // 4-panel grid (2x2)
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(main_chunks[0]);

        let top_panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[0]);

        let bottom_panels = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[1]);

        // Render 4 panels
        self.render_mission_panel(
            frame,
            top_panels[0],
            state,
            theme,
            self.focus == PanelId::Progress,
        );
        self.render_dag_panel(
            frame,
            top_panels[1],
            state,
            theme,
            self.focus == PanelId::Dag,
        );
        self.render_novanet_panel(
            frame,
            bottom_panels[0],
            state,
            theme,
            self.focus == PanelId::NovaNet,
        );
        self.render_agent_panel(
            frame,
            bottom_panels[1],
            state,
            theme,
            self.focus == PanelId::Agent,
        );

        // Footer with metrics
        self.render_footer(frame, main_chunks[1], state, theme);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            // Escape returns to Home
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Home),

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
                self.focus = PanelId::Progress;
                ViewAction::None
            }
            KeyCode::Char('2') => {
                self.focus = PanelId::Dag;
                ViewAction::None
            }
            KeyCode::Char('3') => {
                self.focus = PanelId::NovaNet;
                ViewAction::None
            }
            KeyCode::Char('4') => {
                self.focus = PanelId::Agent;
                ViewAction::None
            }

            // Vim-style scrolling in focused panel
            KeyCode::Char('j') | KeyCode::Down => {
                self.scroll_down();
                ViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up();
                ViewAction::None
            }

            // ? opens Help
            KeyCode::Char('?') => ViewAction::SwitchView(TuiView::Help),

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

        format!(
            "Monitor • {} • {}/{} tasks • {:.0}%",
            phase, completed, task_count, progress
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
        assert_eq!(view.focus, PanelId::Progress);
        assert_eq!(view.scroll, [0, 0, 0, 0]);
        assert_eq!(view.frame, 0);
    }

    #[test]
    fn test_monitor_view_default() {
        let view = MonitorView::default();
        assert_eq!(view.focus, PanelId::Progress);
    }

    #[test]
    fn test_focus_next_cycles() {
        let mut view = MonitorView::new();
        assert_eq!(view.focus, PanelId::Progress);
        view.focus_next();
        assert_eq!(view.focus, PanelId::Dag);
        view.focus_next();
        assert_eq!(view.focus, PanelId::NovaNet);
        view.focus_next();
        assert_eq!(view.focus, PanelId::Agent);
        view.focus_next();
        assert_eq!(view.focus, PanelId::Progress);
    }

    #[test]
    fn test_focus_prev_cycles() {
        let mut view = MonitorView::new();
        assert_eq!(view.focus, PanelId::Progress);
        view.focus_prev();
        assert_eq!(view.focus, PanelId::Agent);
        view.focus_prev();
        assert_eq!(view.focus, PanelId::NovaNet);
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

        assert_eq!(view.scroll_offset(PanelId::Progress), 1);
        assert_eq!(view.scroll_offset(PanelId::Dag), 2);
        assert_eq!(view.scroll_offset(PanelId::NovaNet), 3);
        assert_eq!(view.scroll_offset(PanelId::Agent), 4);
    }

    #[test]
    fn test_status_icon() {
        assert_eq!(MonitorView::status_icon(&TaskStatus::Pending), "○");
        assert_eq!(MonitorView::status_icon(&TaskStatus::Running), "►");
        assert_eq!(MonitorView::status_icon(&TaskStatus::Success), "✓");
        assert_eq!(MonitorView::status_icon(&TaskStatus::Failed), "✗");
        assert_eq!(MonitorView::status_icon(&TaskStatus::Paused), "⏸");
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
    fn test_handle_key_escape_returns_home() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Home)));
    }

    #[test]
    fn test_handle_key_q_returns_home() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Home)));
    }

    #[test]
    fn test_handle_key_tab_cycles_focus() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.focus, PanelId::Dag);
    }

    #[test]
    fn test_handle_key_shift_tab_cycles_backwards() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Tab, KeyModifiers::SHIFT);
        view.handle_key(key, &mut state);
        assert_eq!(view.focus, PanelId::Agent);
    }

    #[test]
    fn test_handle_key_number_focuses_panel() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");

        view.handle_key(KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE), &mut state);
        assert_eq!(view.focus, PanelId::Dag);

        view.handle_key(KeyEvent::new(KeyCode::Char('3'), KeyModifiers::NONE), &mut state);
        assert_eq!(view.focus, PanelId::NovaNet);

        view.handle_key(KeyEvent::new(KeyCode::Char('4'), KeyModifiers::NONE), &mut state);
        assert_eq!(view.focus, PanelId::Agent);

        view.handle_key(KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE), &mut state);
        assert_eq!(view.focus, PanelId::Progress);
    }

    #[test]
    fn test_handle_key_j_scrolls_down() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll[0], 1);
    }

    #[test]
    fn test_handle_key_k_scrolls_up() {
        let mut view = MonitorView::new();
        view.scroll[0] = 5;
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('k'), KeyModifiers::NONE);
        view.handle_key(key, &mut state);
        assert_eq!(view.scroll[0], 4);
    }

    #[test]
    fn test_handle_key_question_opens_help() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Help)));
    }

    #[test]
    fn test_status_line_shows_phase() {
        let view = MonitorView::new();
        let state = TuiState::new("test");
        let status = view.status_line(&state);
        assert!(status.contains("Monitor"));
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
}
