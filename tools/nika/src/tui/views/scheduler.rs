//! Scheduler View — Cron/queue management for Nika workflows (v0.12)
//!
//! # Layout
//!
//! ```text
//! ┌───────────────────────────────────┬─────────────────────────────────────────┐
//! │         PANEL A                   │              PANEL B                    │
//! │     SCHEDULE LIST                 │           TIMELINE VIEW                 │  50%
//! │          40%                      │               60%                       │
//! ├───────────────────────────────────┼─────────────────────────────────────────┤
//! │         PANEL C                   │              PANEL D                    │
//! │      RUN HISTORY                  │          CRON EDITOR                    │  40%
//! │          40%                      │               60%                       │
//! └───────────────────────────────────┴─────────────────────────────────────────┘
//! ```
//!
//! # Features (v0.12 MVP)
//!
//! - Schedule list with cron expressions
//! - Timeline visualization
//! - Run history with status
//! - Cron editor with validation

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::{TuiView, View, ViewAction};
use crate::tui::state::TuiState;
use crate::tui::theme::Theme;

/// Scheduler view state
///
/// Wired in App (v0.11.3) with full View trait implementation.
/// Accessed via TuiView::Scheduler ([5] or 's' shortcut).
#[derive(Debug)]
pub struct SchedulerView {
    /// Selected schedule index
    selected: usize,
    /// Active panel (A, B, C, D)
    active_panel: SchedulerPanel,
    /// Scheduled workflows (placeholder)
    schedules: Vec<ScheduleEntry>,
}

/// Panels in Scheduler view (A/B/C/D 2x2 grid)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SchedulerPanel {
    #[default]
    ScheduleList,
    Timeline,
    History,
    CronEditor,
}

impl SchedulerPanel {
    pub fn next(&self) -> Self {
        match self {
            SchedulerPanel::ScheduleList => SchedulerPanel::Timeline,
            SchedulerPanel::Timeline => SchedulerPanel::History,
            SchedulerPanel::History => SchedulerPanel::CronEditor,
            SchedulerPanel::CronEditor => SchedulerPanel::ScheduleList,
        }
    }

    pub fn prev(&self) -> Self {
        match self {
            SchedulerPanel::ScheduleList => SchedulerPanel::CronEditor,
            SchedulerPanel::Timeline => SchedulerPanel::ScheduleList,
            SchedulerPanel::History => SchedulerPanel::Timeline,
            SchedulerPanel::CronEditor => SchedulerPanel::History,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            SchedulerPanel::ScheduleList => "A",
            SchedulerPanel::Timeline => "B",
            SchedulerPanel::History => "C",
            SchedulerPanel::CronEditor => "D",
        }
    }
}

/// A scheduled workflow entry
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScheduleEntry {
    pub name: String,
    pub cron: String,
    pub enabled: bool,
    pub last_run: Option<String>,
    pub next_run: Option<String>,
}

impl Default for SchedulerView {
    fn default() -> Self {
        Self::new()
    }
}

impl SchedulerView {
    pub fn new() -> Self {
        Self {
            selected: 0,
            active_panel: SchedulerPanel::default(),
            // Placeholder schedules for demo
            schedules: vec![
                ScheduleEntry {
                    name: "seo-pipeline".to_string(),
                    cron: "*/6 * * * *".to_string(),
                    enabled: true,
                    last_run: Some("08:00:00".to_string()),
                    next_run: Some("14:00:00".to_string()),
                },
                ScheduleEntry {
                    name: "novanet-sync".to_string(),
                    cron: "30 14 * * *".to_string(),
                    enabled: true,
                    last_run: Some("yesterday".to_string()),
                    next_run: Some("14:30:00".to_string()),
                },
            ],
        }
    }

    /// Render schedule list panel (A)
    fn render_schedule_list(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let is_active = self.active_panel == SchedulerPanel::ScheduleList;
        let border_color = if is_active {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" 📋 SCHEDULED WORKFLOWS [A] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let items: Vec<ListItem> = self
            .schedules
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let status = if s.enabled { "▶" } else { "⏸" };
                let style = if i == self.selected && is_active {
                    Style::default()
                        .fg(theme.highlight)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.text_primary)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("{} ", status), style),
                    Span::styled(&s.name, style),
                    Span::styled(
                        format!(" │ {} ", s.cron),
                        Style::default().fg(theme.text_muted),
                    ),
                ]))
            })
            .collect();

        let list = List::new(items).block(block);
        frame.render_widget(list, area);
    }

    /// Render timeline panel (B)
    fn render_timeline(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let is_active = self.active_panel == SchedulerPanel::Timeline;
        let border_color = if is_active {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" TIMELINE [B] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let text = vec![
            Line::from("     12:00      14:00      16:00      18:00      20:00"),
            Line::from("       │          │          │          │          │"),
            Line::from("       ├──────────┤                                  "),
            Line::from("       │seo-pipe  │                                  "),
            Line::from("       ├──────────┼──────────┤                       "),
            Line::from("                  │novanet   │                       "),
            Line::from("       ─────────────────────────────────────────── now"),
        ];

        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(theme.text_primary));
        frame.render_widget(paragraph, area);
    }

    /// Render history panel (C)
    fn render_history(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let is_active = self.active_panel == SchedulerPanel::History;
        let border_color = if is_active {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" RUN HISTORY [C] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let text = vec![
            Line::from(Span::styled(
                "Today:",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw(" 08:00 │ seo-pipeline    │ "),
                Span::styled("✓", Style::default().fg(Color::Green)),
                Span::raw(" 45s │ $0.12"),
            ]),
            Line::from(vec![
                Span::raw(" 06:00 │ novanet-sync    │ "),
                Span::styled("✓", Style::default().fg(Color::Green)),
                Span::raw(" 12s │ $0.02"),
            ]),
        ];

        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(theme.text_primary));
        frame.render_widget(paragraph, area);
    }

    /// Render cron editor panel (D)
    fn render_cron_editor(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let is_active = self.active_panel == SchedulerPanel::CronEditor;
        let border_color = if is_active {
            theme.highlight
        } else {
            theme.border_normal
        };

        let block = Block::default()
            .title(" CRON EDITOR [D] ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color));

        let text = if let Some(schedule) = self.schedules.get(self.selected) {
            vec![
                Line::from(format!("Editing: {}", schedule.name)),
                Line::from(""),
                Line::from(format!("Cron: {}", schedule.cron)),
                Line::from(""),
                Line::from("= Every 6 hours (00:00, 06:00, 12:00, 18:00)"),
            ]
        } else {
            vec![Line::from("No schedule selected")]
        };

        let paragraph = Paragraph::new(text)
            .block(block)
            .style(Style::default().fg(theme.text_primary));
        frame.render_widget(paragraph, area);
    }
}

impl View for SchedulerView {
    fn render(&mut self, frame: &mut Frame, area: Rect, _state: &TuiState, theme: &Theme) {
        // Split into top (50%) and bottom (40%) + status (10%)
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(50),
                Constraint::Percentage(40),
                Constraint::Percentage(10),
            ])
            .split(area);

        // Top row: Schedule List (40%) + Timeline (60%)
        let top_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(main_chunks[0]);

        // Bottom row: History (40%) + Cron Editor (60%)
        let bottom_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
            .split(main_chunks[1]);

        // Render all panels
        self.render_schedule_list(frame, top_chunks[0], theme);
        self.render_timeline(frame, top_chunks[1], theme);
        self.render_history(frame, bottom_chunks[0], theme);
        self.render_cron_editor(frame, bottom_chunks[1], theme);

        // Status bar
        let status = Line::from(vec![
            Span::styled(" 🦋 Scheduler ", Style::default().fg(theme.highlight)),
            Span::raw("│ "),
            Span::raw(format!(
                "{} active │ Panel: {} │ Tab: cycle │ Enter: select",
                self.schedules.iter().filter(|s| s.enabled).count(),
                self.active_panel.label()
            )),
        ]);
        let status_widget = Paragraph::new(status)
            .style(Style::default().fg(theme.text_muted))
            .block(Block::default().borders(Borders::TOP));
        frame.render_widget(status_widget, main_chunks[2]);
    }

    fn handle_key(&mut self, key: KeyEvent, _state: &mut TuiState) -> ViewAction {
        match (key.code, key.modifiers) {
            // View switching
            (KeyCode::Char('1'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Browse),
            (KeyCode::Char('2'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Chat),
            (KeyCode::Char('3'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Editor),
            (KeyCode::Char('4'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Runner),
            (KeyCode::Char('5'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Scheduler),
            (KeyCode::Char('6'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Settings),
            // Letter shortcuts (v0.12)
            (KeyCode::Char('e'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Browse),
            (KeyCode::Char('c'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Chat),
            (KeyCode::Char('d'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Editor),
            (KeyCode::Char('r'), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Runner),
            // 's' is Scheduler (current view)
            (KeyCode::Char(','), KeyModifiers::NONE) => ViewAction::SwitchView(TuiView::Settings),
            // Panel navigation
            (KeyCode::Tab, KeyModifiers::NONE) => {
                self.active_panel = self.active_panel.next();
                ViewAction::None
            }
            (KeyCode::BackTab, KeyModifiers::SHIFT) => {
                self.active_panel = self.active_panel.prev();
                ViewAction::None
            }
            // List navigation
            (KeyCode::Up | KeyCode::Char('k'), KeyModifiers::NONE) => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                ViewAction::None
            }
            (KeyCode::Down | KeyCode::Char('j'), KeyModifiers::NONE) => {
                if self.selected < self.schedules.len().saturating_sub(1) {
                    self.selected += 1;
                }
                ViewAction::None
            }
            // Quit
            (KeyCode::Char('q'), KeyModifiers::NONE) => ViewAction::Quit,
            _ => ViewAction::None,
        }
    }

    fn status_line(&self, _state: &TuiState) -> String {
        format!(
            "Scheduler │ {} schedules │ Panel {}",
            self.schedules.len(),
            self.active_panel.label()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_view_new() {
        let view = SchedulerView::new();
        assert_eq!(view.selected, 0);
        assert_eq!(view.active_panel, SchedulerPanel::ScheduleList);
        assert!(!view.schedules.is_empty());
    }

    #[test]
    fn test_scheduler_panel_cycle() {
        let panel = SchedulerPanel::ScheduleList;
        assert_eq!(panel.next(), SchedulerPanel::Timeline);
        assert_eq!(panel.next().next(), SchedulerPanel::History);
        assert_eq!(panel.next().next().next(), SchedulerPanel::CronEditor);
        assert_eq!(
            panel.next().next().next().next(),
            SchedulerPanel::ScheduleList
        );
    }

    #[test]
    fn test_scheduler_panel_prev() {
        let panel = SchedulerPanel::ScheduleList;
        assert_eq!(panel.prev(), SchedulerPanel::CronEditor);
    }

    #[test]
    fn test_scheduler_panel_labels() {
        assert_eq!(SchedulerPanel::ScheduleList.label(), "A");
        assert_eq!(SchedulerPanel::Timeline.label(), "B");
        assert_eq!(SchedulerPanel::History.label(), "C");
        assert_eq!(SchedulerPanel::CronEditor.label(), "D");
    }

    #[test]
    fn test_scheduler_view_status_line() {
        let view = SchedulerView::new();
        let state = TuiState::new("test");
        let status = view.status_line(&state);
        assert!(status.contains("Scheduler"));
        assert!(status.contains("schedules"));
    }

    #[test]
    fn test_scheduler_handle_key_view_switch() {
        let mut view = SchedulerView::new();
        let mut state = TuiState::new("test");

        // Number keys
        assert!(matches!(
            view.handle_key(
                KeyEvent::new(KeyCode::Char('1'), KeyModifiers::NONE),
                &mut state
            ),
            ViewAction::SwitchView(TuiView::Browse)
        ));
        assert!(matches!(
            view.handle_key(
                KeyEvent::new(KeyCode::Char('2'), KeyModifiers::NONE),
                &mut state
            ),
            ViewAction::SwitchView(TuiView::Chat)
        ));

        // Letter shortcuts
        assert!(matches!(
            view.handle_key(
                KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE),
                &mut state
            ),
            ViewAction::SwitchView(TuiView::Browse)
        ));
    }

    #[test]
    fn test_scheduler_handle_key_panel_navigation() {
        let mut view = SchedulerView::new();
        let mut state = TuiState::new("test");
        assert_eq!(view.active_panel, SchedulerPanel::ScheduleList);

        view.handle_key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE), &mut state);
        assert_eq!(view.active_panel, SchedulerPanel::Timeline);

        view.handle_key(
            KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
            &mut state,
        );
        assert_eq!(view.active_panel, SchedulerPanel::ScheduleList);
    }

    #[test]
    fn test_scheduler_handle_key_list_navigation() {
        let mut view = SchedulerView::new();
        let mut state = TuiState::new("test");
        assert_eq!(view.selected, 0);

        view.handle_key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE), &mut state);
        assert_eq!(view.selected, 1);

        view.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &mut state);
        assert_eq!(view.selected, 0);

        // Should not go below 0
        view.handle_key(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE), &mut state);
        assert_eq!(view.selected, 0);
    }
}
