// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Info Panel - Context-aware detail panel
//!
//! Shows detailed information about the currently selected task.
//! Content adapts based on task type (verb) and state.
//!
//! ## Layout
//!
//! ```text
//! ┌─ Info ─────────────────────────────┐
//! │ TASK: generate                     │
//! │ TYPE: ⚡ infer                      │
//! │ STATUS: ◐ Running                  │
//! │                                    │
//! │ ─── PROMPT ─────────────────────   │
//! │ Generate a landing page for...     │
//! │                                    │
//! │ ─── RESPONSE ───────────────────   │
//! │ Welcome to QR Code AI...           │
//! │ [streaming...]                     │
//! │                                    │
//! │ ─── METRICS ────────────────────   │
//! │ Duration: 1.2s                     │
//! │ Tokens: 128 in / 342 out           │
//! │ Model: claude-sonnet-4-6           │
//! └────────────────────────────────────┘
//! ```

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame,
};

use crate::state::{TaskState, TuiState};
use crate::theme::{TaskStatus, Theme, VerbColor};

/// Info Panel - Shows details for selected task
pub struct InfoPanel {
    /// Scroll offset for content
    scroll_offset: u16,
    /// Selected task ID
    selected_task_id: Option<String>,
    /// Cached line count from last render (for scroll upper bound)
    rendered_line_count: u16,
}

impl InfoPanel {
    /// Create a new InfoPanel
    pub fn new() -> Self {
        Self {
            scroll_offset: 0,
            selected_task_id: None,
            rendered_line_count: 0,
        }
    }

    /// Set the currently selected task
    pub fn select_task(&mut self, task_id: Option<String>) {
        if self.selected_task_id != task_id {
            self.selected_task_id = task_id;
            self.scroll_offset = 0; // Reset scroll on selection change
        }
    }

    /// Render the info panel
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
        let title = format!(" {} Info ", focus_indicator);

        let block = Block::default()
            .title(Span::styled(title, style))
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Get task to display
        let task = self
            .selected_task_id
            .as_ref()
            .and_then(|id| state.tasks.get(id));

        match task {
            Some(task) => self.render_task_details(frame, inner, task, theme),
            None => self.render_empty(frame, inner, theme),
        }
    }

    /// Render empty state
    fn render_empty(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let text = Text::from(vec![
            Line::from(""),
            Line::from(Span::styled(
                "No task selected",
                Style::default().fg(theme.text_muted),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Select a task from the list",
                Style::default().fg(theme.text_muted),
            )),
        ]);

        let paragraph = Paragraph::new(text);
        frame.render_widget(paragraph, area);
    }

    /// Render task details
    fn render_task_details(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        task: &TaskState,
        theme: &Theme,
    ) {
        let mut lines: Vec<Line> = Vec::new();

        // Header section
        let verb = Self::verb_from_task_type(task.task_type.as_deref());
        let verb_icon = Self::verb_icon(verb);
        let status_icon = Self::status_icon(task.status);
        let status_color = Self::status_color(task.status, theme);

        lines.push(Line::from(vec![
            Span::styled("TASK: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                &task.id,
                Style::default()
                    .fg(theme.text_primary)
                    .add_modifier(Modifier::BOLD),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("TYPE: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{} ", verb_icon),
                Style::default().fg(theme.verb_color(verb)),
            ),
            Span::styled(
                task.task_type.as_deref().unwrap_or("unknown"),
                Style::default().fg(theme.text_primary),
            ),
        ]));

        lines.push(Line::from(vec![
            Span::styled("STATUS: ", Style::default().fg(theme.text_muted)),
            Span::styled(
                format!("{} ", status_icon),
                Style::default().fg(status_color),
            ),
            Span::styled(
                Self::status_text(task.status),
                Style::default().fg(status_color),
            ),
        ]));

        lines.push(Line::from(""));

        // Input section (from task.input)
        if let Some(ref input) = task.input {
            lines.push(Line::from(Span::styled(
                "─── INPUT ─────────────────────",
                Style::default().fg(theme.text_muted),
            )));

            // Display input JSON preview (single pass for performance)
            let input_str =
                serde_json::to_string_pretty(&**input).unwrap_or_else(|_| "...".to_string());
            let input_lines: Vec<_> = input_str.lines().collect();
            for line in input_lines.iter().take(5) {
                lines.push(Line::from(Span::styled(
                    (*line).to_string(),
                    Style::default().fg(theme.text_secondary),
                )));
            }
            if input_lines.len() > 5 {
                lines.push(Line::from(Span::styled(
                    "...",
                    Style::default().fg(theme.text_muted),
                )));
            }

            lines.push(Line::from(""));
        }

        // Output section (from task.output)
        if let Some(ref output) = task.output {
            lines.push(Line::from(Span::styled(
                "─── OUTPUT ────────────────────",
                Style::default().fg(theme.text_muted),
            )));

            // Display output JSON preview (single pass for performance)
            let output_str =
                serde_json::to_string_pretty(&**output).unwrap_or_else(|_| "...".to_string());
            let output_lines: Vec<_> = output_str.lines().collect();
            for line in output_lines.iter().take(10) {
                lines.push(Line::from(Span::styled(
                    (*line).to_string(),
                    Style::default().fg(theme.text_primary),
                )));
            }
            if output_lines.len() > 10 {
                lines.push(Line::from(Span::styled(
                    "...",
                    Style::default().fg(theme.text_muted),
                )));
            }

            lines.push(Line::from(""));
        } else if task.status == TaskStatus::Running {
            lines.push(Line::from(Span::styled(
                "─── OUTPUT ────────────────────",
                Style::default().fg(theme.text_muted),
            )));
            lines.push(Line::from(Span::styled(
                "[streaming...]",
                Style::default()
                    .fg(theme.status_running)
                    .add_modifier(Modifier::ITALIC),
            )));
            lines.push(Line::from(""));
        }

        // Metrics section
        lines.push(Line::from(Span::styled(
            "─── METRICS ───────────────────",
            Style::default().fg(theme.text_muted),
        )));

        if let Some(duration_ms) = task.duration_ms {
            let duration_str = if duration_ms >= 1000 {
                format!("{:.1}s", duration_ms as f64 / 1000.0)
            } else {
                format!("{}ms", duration_ms)
            };
            lines.push(Line::from(vec![
                Span::styled("Duration: ", Style::default().fg(theme.text_muted)),
                Span::styled(duration_str, Style::default().fg(theme.text_primary)),
            ]));
        }

        if let Some(tokens) = task.tokens {
            if tokens > 0 {
                lines.push(Line::from(vec![
                    Span::styled("Tokens: ", Style::default().fg(theme.text_muted)),
                    Span::styled(
                        format!("{}", tokens),
                        Style::default().fg(theme.text_primary),
                    ),
                ]));
            }
        }

        if let Some(ref model) = task.model {
            lines.push(Line::from(vec![
                Span::styled("Model: ", Style::default().fg(theme.text_muted)),
                Span::styled(model.clone(), Style::default().fg(theme.text_primary)),
            ]));
        }

        // Error section (if failed)
        if task.status == TaskStatus::Failed {
            if let Some(ref error) = task.error {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    "─── ERROR ─────────────────────",
                    Style::default().fg(theme.status_failed),
                )));
                lines.push(Line::from(Span::styled(
                    error.clone(),
                    Style::default().fg(theme.status_failed),
                )));
            }
        }

        let text = Text::from(lines);
        self.rendered_line_count = text.lines.len() as u16;
        let paragraph = Paragraph::new(text)
            .scroll((self.scroll_offset, 0))
            .wrap(Wrap { trim: true });

        frame.render_widget(paragraph, area);
    }

    /// Handle keyboard input
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        let max_scroll = self.rendered_line_count.saturating_sub(5);
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll_offset = (self.scroll_offset + 1).min(max_scroll);
                true
            }
            KeyCode::PageUp => {
                self.scroll_offset = self.scroll_offset.saturating_sub(10);
                true
            }
            KeyCode::PageDown => {
                self.scroll_offset = (self.scroll_offset + 10).min(max_scroll);
                true
            }
            KeyCode::Home | KeyCode::Char('g') => {
                self.scroll_offset = 0;
                true
            }
            KeyCode::End | KeyCode::Char('G') => {
                self.scroll_offset = max_scroll;
                true
            }
            _ => false,
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

    fn status_icon(status: TaskStatus) -> &'static str {
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

    fn status_text(status: TaskStatus) -> &'static str {
        match status {
            TaskStatus::Queued => "Queued",
            TaskStatus::Pending => "Pending",
            TaskStatus::Running => "Running",
            TaskStatus::Success => "Success",
            TaskStatus::Failed => "Failed",
            TaskStatus::Paused => "Paused",
            TaskStatus::Skipped => "Skipped",
        }
    }

    fn status_color(status: TaskStatus, theme: &Theme) -> ratatui::style::Color {
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

impl Default for InfoPanel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_info_panel_new() {
        let panel = InfoPanel::new();
        assert!(panel.selected_task_id.is_none());
        assert_eq!(panel.scroll_offset, 0);
    }

    #[test]
    fn test_select_task_resets_scroll() {
        let mut panel = InfoPanel::new();
        panel.scroll_offset = 10;
        panel.select_task(Some("task1".to_string()));
        assert_eq!(panel.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_navigation() {
        let mut panel = InfoPanel::new();
        panel.rendered_line_count = 50; // simulate having content

        let key = KeyEvent::new(KeyCode::Down, crossterm::event::KeyModifiers::NONE);
        panel.handle_key(key);
        assert_eq!(panel.scroll_offset, 1);

        let key = KeyEvent::new(KeyCode::Up, crossterm::event::KeyModifiers::NONE);
        panel.handle_key(key);
        assert_eq!(panel.scroll_offset, 0);

        // Can't go negative
        panel.handle_key(key);
        assert_eq!(panel.scroll_offset, 0);
    }

    #[test]
    fn test_scroll_upper_bound() {
        let mut panel = InfoPanel::new();
        panel.rendered_line_count = 10;
        // max_scroll = 10 - 5 = 5

        // Down past upper bound should cap at max_scroll
        for _ in 0..20 {
            panel.handle_key(KeyEvent::new(
                KeyCode::Down,
                crossterm::event::KeyModifiers::NONE,
            ));
        }
        assert_eq!(panel.scroll_offset, 5, "scroll must not exceed max_scroll");
    }

    #[test]
    fn test_scroll_end_and_home() {
        let mut panel = InfoPanel::new();
        panel.rendered_line_count = 20;
        // max_scroll = 20 - 5 = 15

        // End/G jumps to bottom
        panel.handle_key(KeyEvent::new(
            KeyCode::End,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(panel.scroll_offset, 15);

        // Home/g jumps to top
        panel.handle_key(KeyEvent::new(
            KeyCode::Home,
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(panel.scroll_offset, 0);

        // 'G' also jumps to bottom
        panel.handle_key(KeyEvent::new(
            KeyCode::Char('G'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert_eq!(panel.scroll_offset, 15);
    }
}
