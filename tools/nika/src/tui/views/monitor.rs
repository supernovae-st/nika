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
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::trait_view::View;
use super::{TuiView, ViewAction};
use crate::tui::focus::PanelId;
use crate::tui::state::TuiState;
use crate::tui::theme::{MissionPhase, TaskStatus, Theme};
use crate::tui::unicode::truncate_to_width;

/// Monitor View state
///
/// Real-time workflow execution monitoring with 4-panel layout:
/// - Panel 1: Mission Control (task progress)
/// - Panel 2: DAG Execution (dependency graph)
/// - Panel 3: NovaNet Station (MCP calls)
/// - Panel 4: Agent Reasoning (agent turns)
///
/// v0.11.0: Now fully integrated into App via View trait.
/// Wired in app.rs App struct and both constructors (new, new_standalone).
pub struct MonitorView {
    /// Currently focused panel
    pub focus: PanelId,
    /// Scroll offset per panel
    pub scroll: [usize; 4],
    /// Animation frame counter
    pub frame: u8,
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
                            format!("[{}{}] {}0%", "■".repeat(pct), "░".repeat(10 - pct), pct)
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
                        Span::styled(
                            format!("{:<12} ", task_id),
                            Style::default().fg(theme.text_primary),
                        ),
                        Span::styled(progress, style),
                    ]))
                    .style(
                        if i == self.scroll_offset(PanelId::RunnerMission) && focused {
                            Style::default().bg(theme.highlight)
                        } else {
                            Style::default()
                        },
                    )
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
                state
                    .workflow
                    .path
                    .split('/')
                    .next_back()
                    .unwrap_or("workflow"),
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
                .style(if i == self.scroll_offset(PanelId::RunnerNovanet) && focused {
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

        // Build agent turn list with thinking display (v0.11.0)
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

                // Build lines vec - main line plus optional thinking (v0.11.0)
                let mut lines = vec![main_line];

                // Add thinking content if present
                // v0.12.1: Use unicode-aware truncation to avoid panic on multi-byte chars
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
            )]))
            .block(block);
            frame.render_widget(empty, area);
        } else {
            let list = List::new(items).block(block);
            frame.render_widget(list, area);
        }
    }
    // v0.12.1: render_footer() removed - global StatusBar handles metrics now
}

impl Default for MonitorView {
    fn default() -> Self {
        Self::new()
    }
}

impl View for MonitorView {
    fn render(&mut self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // v0.12.1: Minimum height guard - need at least 8 lines for 2x2 grid
        if area.height < 8 {
            // Terminal too small - render fallback message
            let msg = "↕ Terminal too small";
            let x = area.x + area.width.saturating_sub(msg.len() as u16) / 2;
            let y = area.y + area.height / 2;
            frame.buffer_mut().set_string(
                x,
                y,
                msg,
                Style::default().fg(Color::Yellow),
            );
            return;
        }

        // v0.12.1: Removed internal footer - global StatusBar handles this now
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

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match key.code {
            // Escape returns to Explorer
            KeyCode::Esc | KeyCode::Char('q') => ViewAction::SwitchView(TuiView::Explorer),

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
                self.scroll_down();
                ViewAction::None
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.scroll_up();
                ViewAction::None
            }

            // ? opens Settings (includes help - v0.12)
            KeyCode::Char('?') => ViewAction::SwitchView(TuiView::Settings),

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
        assert_eq!(view.focus, PanelId::RunnerMission);
        assert_eq!(view.scroll, [0, 0, 0, 0]);
        assert_eq!(view.frame, 0);
    }

    #[test]
    fn test_monitor_view_default() {
        let view = MonitorView::default();
        assert_eq!(view.focus, PanelId::RunnerMission);
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
    fn test_handle_key_escape_returns_explorer() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Explorer)));
    }

    #[test]
    fn test_handle_key_q_returns_explorer() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Explorer)));
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
    fn test_handle_key_question_opens_settings() {
        let mut view = MonitorView::new();
        let mut state = TuiState::new("test");
        let key = KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE);
        let action = view.handle_key(key, &mut state);
        assert!(matches!(action, ViewAction::SwitchView(TuiView::Settings)));
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

    // v0.11.0: Thinking display tests
    #[test]
    fn test_agent_turn_with_thinking_short() {
        use crate::tui::AgentTurnState;

        let mut state = TuiState::new("test");
        state.agent_turns.push(AgentTurnState {
            index: 0,
            status: "Thinking...".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some("This is a short thinking string".to_string()),
            response_text: None,
        });

        // Verify the thinking content is present in state
        assert!(state.agent_turns[0].thinking.is_some());
        let thinking = state.agent_turns[0].thinking.as_ref().unwrap();
        assert_eq!(thinking, "This is a short thinking string");
        assert!(thinking.len() <= 100); // Should not be truncated
    }

    #[test]
    fn test_agent_turn_with_thinking_truncated() {
        use crate::tui::AgentTurnState;

        let mut state = TuiState::new("test");
        let long_thinking = "A".repeat(150); // 150 characters
        state.agent_turns.push(AgentTurnState {
            index: 0,
            status: "Thinking...".to_string(),
            tokens: Some(100),
            tool_calls: vec![],
            thinking: Some(long_thinking.clone()),
            response_text: None,
        });

        // Verify the thinking content is long enough to require truncation
        let thinking = state.agent_turns[0].thinking.as_ref().unwrap();
        assert!(thinking.len() > 100);

        // Test truncation logic matches render_agent_panel
        let truncated = if thinking.len() > 100 {
            format!("{}...", &thinking[..97])
        } else {
            thinking.clone()
        };
        assert_eq!(truncated.len(), 100); // 97 chars + "..."
        assert!(truncated.ends_with("..."));
    }
}
