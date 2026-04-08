// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task Box Flow - Scrollable stack of TaskBox widgets
//!
//! Displays ALL TaskBox widgets stacked vertically with auto-scroll.
//! The view follows the currently running task automatically.
//! Manual scroll (j/k) pauses auto-follow; G resumes.
//!
//! ## Layout
//!
//! ```text
//! ┌─ Task Flow ───────────────────────┐
//! │ ┌─ generate ⚡────────────────┐   │
//! │ │ ✓ Success │ 1.2s │ 342 tok │   │
//! │ │ Response: Landing page...   │   │
//! │ └─────────────────────────────┘   │
//! │ ┌─ compile 📟─────────────────┐   │
//! │ │ ◐ Running │ ~2s             │   │ ← auto-scroll here
//! │ │ > npm run build             │   │
//! │ └─────────────────────────────┘   │
//! │ ┌─ deploy 🐔──────────────────┐   │
//! │ │ ○ Queued                    │   │
//! │ └─────────────────────────────┘   │
//! └───────────────────────────────────┘
//! ```

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::Span,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState},
    Frame,
};

use crate::state::TuiState;
use crate::theme::{TaskStatus, Theme, VerbColor};
use crate::widgets::task_box::RenderMode;

/// Height of each TaskBox in lines
const TASK_BOX_HEIGHT: u16 = 5;

/// Task Box Flow Panel - Scrollable stack of TaskBox widgets
pub struct TaskBoxFlow {
    /// Scroll offset (in pixels/lines)
    scroll_offset: u16,
    /// Whether auto-scroll is enabled
    auto_scroll: bool,
    /// Index of currently running task (for auto-scroll target)
    running_task_index: Option<usize>,
    /// Total content height
    content_height: u16,
    /// Visible height
    visible_height: u16,
    /// Render mode for TaskBoxes
    pub render_mode: RenderMode,
}

impl TaskBoxFlow {
    /// Create a new TaskBoxFlow
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            auto_scroll: true,
            running_task_index: None,
            content_height: 0,
            visible_height: 0,
            render_mode: RenderMode::Expanded,
        }
    }

    /// Render the task flow panel
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

        // Show auto-scroll indicator
        let scroll_indicator = if self.auto_scroll { "⟳" } else { "⏸" };
        let title = format!(" {} Task Flow {} ", focus_indicator, scroll_indicator);

        let block = Block::default()
            .title(Span::styled(title, style))
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        self.visible_height = inner.height;

        // Calculate content height and find running task
        let task_count = state.task_order.len();
        self.content_height = (task_count as u16) * TASK_BOX_HEIGHT;
        self.running_task_index = None;

        // Clamp scroll offset in case content shrank (prevents blank view)
        self.scroll_offset = self.scroll_offset.min(self.max_scroll());

        // Find the currently running task index
        for (idx, task_id) in state.task_order.iter().enumerate() {
            if let Some(task) = state.tasks.get(task_id) {
                if task.status == TaskStatus::Running {
                    self.running_task_index = Some(idx);
                    break;
                }
            }
        }

        // Auto-scroll to running task
        if self.auto_scroll {
            if let Some(running_idx) = self.running_task_index {
                let target_offset = (running_idx as u16) * TASK_BOX_HEIGHT;
                // Center the running task in view
                let centered = target_offset.saturating_sub(self.visible_height / 2);
                self.scroll_offset = centered.min(self.max_scroll());
            }
        }

        // Render visible TaskBoxes
        self.render_task_boxes(frame, inner, state, theme);

        // Render scrollbar if needed
        if self.content_height > self.visible_height {
            let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .begin_symbol(Some("▲"))
                .end_symbol(Some("▼"));

            let mut scrollbar_state = ScrollbarState::new(self.content_height as usize)
                .position(self.scroll_offset as usize)
                .viewport_content_length(self.visible_height as usize);

            frame.render_stateful_widget(
                scrollbar,
                area, // Use full area for scrollbar
                &mut scrollbar_state,
            );
        }
    }

    /// Render individual TaskBoxes
    fn render_task_boxes(&self, frame: &mut Frame, area: Rect, state: &TuiState, theme: &Theme) {
        // Calculate which tasks are visible
        let start_task = (self.scroll_offset / TASK_BOX_HEIGHT) as usize;
        let visible_tasks = ((self.visible_height / TASK_BOX_HEIGHT) + 2) as usize;

        for task_idx in start_task..start_task + visible_tasks {
            if task_idx >= state.task_order.len() {
                break;
            }

            let task_id = &state.task_order[task_idx];
            if let Some(task) = state.tasks.get(task_id) {
                // Calculate Y position
                let y_offset =
                    (task_idx as u16 * TASK_BOX_HEIGHT).saturating_sub(self.scroll_offset);

                if y_offset >= area.height {
                    break;
                }

                let task_area = Rect {
                    x: area.x,
                    y: area.y + y_offset,
                    width: area.width,
                    height: TASK_BOX_HEIGHT.min(area.height.saturating_sub(y_offset)),
                };

                // Determine verb type and render appropriate TaskBox
                let verb = Self::verb_from_task_type(task.task_type.as_deref());

                self.render_task_box(frame, task_area, task_id, task, verb, theme);
            }
        }
    }

    /// Render a single TaskBox based on verb type
    fn render_task_box(
        &self,
        frame: &mut Frame,
        area: Rect,
        task_id: &str,
        task: &crate::state::TaskState,
        verb: VerbColor,
        theme: &Theme,
    ) {
        // Build a simple representation using Paragraph
        // In the future, this will use the actual TaskBox widgets
        let status_icon = match task.status {
            TaskStatus::Queued => "○",
            TaskStatus::Pending => "◦",
            TaskStatus::Running => "◐",
            TaskStatus::Success => "✓",
            TaskStatus::Failed => "✗",
            TaskStatus::Paused => "⏸",
            TaskStatus::Skipped => "⊘",
        };

        let verb_icon = verb.icon();

        let status_color = match task.status {
            TaskStatus::Queued => theme.text_muted,
            TaskStatus::Pending => theme.text_muted,
            TaskStatus::Running => theme.status_running,
            TaskStatus::Success => theme.status_success,
            TaskStatus::Failed => theme.status_failed,
            TaskStatus::Paused => theme.text_muted,
            TaskStatus::Skipped => theme.text_muted,
        };

        // Title line
        let title = format!("{} {} │ {} │", verb_icon, task_id, status_icon);

        // Duration and tokens
        let duration_str = task
            .duration_ms
            .map(|ms| format!("{}ms", ms))
            .unwrap_or_else(|| "~?s".to_string());

        let tokens_str = task
            .tokens
            .map(|t| format!("{} tok", t))
            .unwrap_or_default();

        let info_line = format!("  {} │ {}", duration_str, tokens_str);

        // Output preview (truncated)
        let preview = task
            .output
            .as_ref()
            .and_then(|o| serde_json::to_string(&**o).ok())
            .map(|s| {
                let truncated = if s.len() > 40 {
                    format!("{}...", nika_engine::util::truncate_str(&s, 40))
                } else {
                    s
                };
                format!("  {}", truncated)
            })
            .unwrap_or_default();

        let block_style = if task.status == TaskStatus::Running {
            Style::default()
                .fg(theme.status_running)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.border_normal)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(block_style)
            .title(Span::styled(
                format!(" {} ", task_id),
                Style::default().fg(status_color),
            ));

        let content = format!("{}\n{}\n{}", title, info_line, preview);
        let paragraph = Paragraph::new(content).block(block);

        frame.render_widget(paragraph, area);
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.auto_scroll = false; // Pause auto-scroll on manual navigation
                self.scroll_up(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.auto_scroll = false;
                self.scroll_down(1);
                true
            }
            KeyCode::PageUp => {
                self.auto_scroll = false;
                self.scroll_up(self.visible_height.saturating_sub(2));
                true
            }
            KeyCode::PageDown => {
                self.auto_scroll = false;
                self.scroll_down(self.visible_height.saturating_sub(2));
                true
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.auto_scroll = false;
                self.scroll_offset = 0;
                true
            }
            KeyCode::End | KeyCode::Char('G') => {
                // G resumes auto-scroll
                self.auto_scroll = true;
                self.scroll_offset = self.max_scroll();
                true
            }
            KeyCode::Char('f') => {
                // Toggle auto-follow
                self.auto_scroll = !self.auto_scroll;
                true
            }
            _ => false,
        }
    }

    fn scroll_up(&mut self, amount: u16) {
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
    }

    fn scroll_down(&mut self, amount: u16) {
        self.scroll_offset = (self.scroll_offset + amount).min(self.max_scroll());
    }

    fn max_scroll(&self) -> u16 {
        self.content_height.saturating_sub(self.visible_height)
    }

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
}

impl Default for TaskBoxFlow {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_flow_new() {
        let flow = TaskBoxFlow::new();
        assert!(flow.auto_scroll);
        assert_eq!(flow.scroll_offset, 0);
    }

    #[test]
    fn test_auto_scroll_pause_resume() {
        let mut flow = TaskBoxFlow::new();
        assert!(flow.auto_scroll);

        // Manual scroll pauses auto-scroll
        let key = KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE);
        flow.handle_key(key);
        assert!(!flow.auto_scroll);

        // G resumes
        let key = KeyEvent::new(KeyCode::Char('G'), crossterm::event::KeyModifiers::NONE);
        flow.handle_key(key);
        assert!(flow.auto_scroll);
    }

    #[test]
    fn test_scroll_bounds() {
        let mut flow = TaskBoxFlow::new();
        flow.content_height = 100;
        flow.visible_height = 20;

        // Can't scroll past max
        flow.scroll_down(200);
        assert_eq!(flow.scroll_offset, 80); // 100 - 20

        // Can't scroll negative
        flow.scroll_up(200);
        assert_eq!(flow.scroll_offset, 0);
    }
}
