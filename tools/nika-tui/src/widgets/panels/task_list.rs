// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task List Panel - Selectable list of workflow tasks
//!
//! Shows all tasks in a workflow with their status icons.
//! Selection syncs with TaskBoxFlow for detail view.
//!
//! ## Layout
//!
//! ```text
//! ┌─ Task List ───────────┐
//! │ ⚡ generate    ✓      │
//! │ 📟 compile     ◐      │  ← selected
//! │ 🛰️ fetch       ○      │
//! │ 🔌 invoke      ○      │
//! │ 🐔 agent       ○      │
//! └───────────────────────┘
//! ```

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState},
    Frame,
};

use crate::state::TuiState;
use crate::theme::{Theme, VerbColor};

/// Task List Panel action
#[derive(Debug, Clone)]
pub enum TaskListAction {
    /// No action
    None,
    /// Selection changed to task at index
    SelectTask(usize),
}

/// Task List Panel - Shows all tasks in workflow
pub struct TaskListPanel {
    /// List widget state (for selection)
    list_state: ListState,
    /// Number of tasks
    task_count: usize,
}

impl TaskListPanel {
    /// Create a new TaskListPanel
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            task_count: 0,
        }
    }

    /// Get currently selected task index
    pub fn selected(&self) -> Option<usize> {
        self.list_state.selected()
    }

    /// Set selected task index
    pub fn select(&mut self, index: usize) {
        if index < self.task_count {
            self.list_state.select(Some(index));
        }
    }

    /// Render the task list panel
    pub fn render(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
        is_focused: bool,
    ) {
        let style = if is_focused {
            Style::default()
                .fg(theme.border_focused)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let focus_indicator = if is_focused { "●" } else { " " };
        let title = format!(" {} Tasks ", focus_indicator);

        let block = Block::default()
            .title(Span::styled(title, style))
            .borders(Borders::ALL)
            .border_style(style);

        // Build list items from state
        let items: Vec<ListItem> = state
            .task_order
            .iter()
            .filter_map(|task_id| {
                state.tasks.get(task_id).map(|task| {
                    let verb = Self::verb_from_task_type(task.task_type.as_deref());
                    let icon = Self::verb_icon(verb);
                    let status_icon = Self::status_icon(task.status);
                    let status_color = Self::status_color(task.status, theme);

                    // Format: "⚡ task_name   ✓" (UTF-8 safe truncation)
                    let name = if task_id.chars().count() > 12 {
                        let truncated: String = task_id.chars().take(10).collect();
                        format!("{}..", truncated)
                    } else {
                        task_id.clone()
                    };

                    let line = Line::from(vec![
                        Span::styled(
                            format!("{} ", icon),
                            Style::default().fg(Self::verb_color(verb, theme)),
                        ),
                        Span::styled(name, Style::default().fg(theme.text_primary)),
                        Span::raw("  "),
                        Span::styled(status_icon, Style::default().fg(status_color)),
                    ]);

                    ListItem::new(line)
                })
            })
            .collect();

        self.task_count = items.len();

        // Clamp selection to valid range or select first item
        if let Some(selected) = self.list_state.selected() {
            if selected >= self.task_count {
                // Selection is out of bounds - clamp to last valid index
                if self.task_count > 0 {
                    self.list_state.select(Some(self.task_count - 1));
                } else {
                    self.list_state.select(None);
                }
            }
        } else if !items.is_empty() {
            // Nothing selected, select first
            self.list_state.select(Some(0));
        }

        let list = List::new(items)
            .block(block)
            .highlight_style(
                Style::default()
                    .bg(theme.selection)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol("▶ ");

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: KeyEvent) -> TaskListAction {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                if let Some(selected) = self.list_state.selected() {
                    if selected > 0 {
                        self.list_state.select(Some(selected - 1));
                        return TaskListAction::SelectTask(selected - 1);
                    }
                }
                TaskListAction::None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if let Some(selected) = self.list_state.selected() {
                    if selected + 1 < self.task_count {
                        self.list_state.select(Some(selected + 1));
                        return TaskListAction::SelectTask(selected + 1);
                    }
                }
                TaskListAction::None
            }
            KeyCode::Home | KeyCode::Char('g') => {
                if self.task_count > 0 {
                    self.list_state.select(Some(0));
                    return TaskListAction::SelectTask(0);
                }
                TaskListAction::None
            }
            KeyCode::End | KeyCode::Char('G') => {
                if self.task_count > 0 {
                    let last = self.task_count - 1;
                    self.list_state.select(Some(last));
                    return TaskListAction::SelectTask(last);
                }
                TaskListAction::None
            }
            _ => TaskListAction::None,
        }
    }

    // === Helper methods ===

    fn verb_from_task_type(task_type: Option<&str>) -> VerbColor {
        match task_type {
            Some("infer") => VerbColor::Infer,
            Some("exec") => VerbColor::Exec,
            Some("fetch") => VerbColor::Fetch,
            Some("invoke") => VerbColor::Invoke,
            Some("agent") => VerbColor::Agent,
            _ => VerbColor::Infer,
        }
    }

    fn verb_icon(verb: VerbColor) -> &'static str {
        verb.icon()
    }

    fn verb_color(verb: VerbColor, theme: &Theme) -> ratatui::style::Color {
        theme.verb_color(verb)
    }

    fn status_icon(status: crate::theme::TaskStatus) -> &'static str {
        use crate::theme::TaskStatus;
        match status {
            TaskStatus::Queued => "○",
            TaskStatus::Pending => "◦",
            TaskStatus::Running => "◐",
            TaskStatus::Success => "✓",
            TaskStatus::Failed => "✗",
            TaskStatus::Paused => "⏸",
            TaskStatus::Skipped => "⊘",
        }
    }

    fn status_color(status: crate::theme::TaskStatus, theme: &Theme) -> ratatui::style::Color {
        use crate::theme::TaskStatus;
        match status {
            TaskStatus::Queued => theme.text_muted,
            TaskStatus::Pending => theme.text_muted,
            TaskStatus::Running => theme.status_running,
            TaskStatus::Success => theme.status_success,
            TaskStatus::Failed => theme.status_failed,
            TaskStatus::Paused => theme.text_muted,
            TaskStatus::Skipped => theme.text_muted,
        }
    }
}

impl Default for TaskListPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_list_new() {
        let panel = TaskListPanel::new();
        assert!(panel.selected().is_none());
    }

    #[test]
    fn test_navigation() {
        let mut panel = TaskListPanel::new();
        panel.task_count = 5;
        panel.list_state.select(Some(2));

        // Up
        let key = KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE);
        let action = panel.handle_key(key);
        assert!(matches!(action, TaskListAction::SelectTask(1)));
        assert_eq!(panel.selected(), Some(1));

        // Down
        let key = KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE);
        let action = panel.handle_key(key);
        assert!(matches!(action, TaskListAction::SelectTask(2)));
    }
}
