// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Mission Control panel rendering (Panel 1)
//!
//! Verb-colored TaskBox style with tab-aware rendering:
//! - Progress tab: task list with status indicators
//! - TaskIO tab: selected task input/output JSON
//! - Output tab: selected task full output

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, Paragraph},
    Frame,
};

use super::MonitorView;
use crate::focus::PanelId;
use crate::state::TuiState;
use crate::theme::{TaskStatus, Theme};
use crate::unicode::truncate_to_width;
use crate::views::MissionTab;
use crate::widgets::task_box::RenderMode;
use crate::widgets::ScrollIndicator;

impl MonitorView {
    /// Render Mission Control panel (Panel 1) with verb-colored TaskBox style
    ///
    /// Uses TaskBox visual language (verb icons, colors, progress)
    /// Tab-aware rendering (Progress | TaskIO | Output)
    pub(super) fn render_mission_panel(
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
                                Style::default().bg(theme.background)
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
                        Style::default().bg(theme.background)
                    } else {
                        Style::default()
                    }))
                })
            })
            .flatten()
            .collect();

        // Apply scroll offset
        let total_items = items.len();
        let skip = self.scroll[Self::panel_index(PanelId::RunnerMission)];
        let visible_count = inner_area.height as usize;
        let visible_items: Vec<ListItem> = items.into_iter().skip(skip).collect();

        let list = List::new(visible_items);
        frame.render_widget(list, inner_area);

        // Scroll indicator when content overflows
        if total_items > visible_count {
            let scrollbar_area = Rect {
                x: inner_area.x + inner_area.width.saturating_sub(1),
                y: inner_area.y,
                width: 1,
                height: inner_area.height,
            };
            let indicator = ScrollIndicator::new()
                .position(skip, total_items, visible_count)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);
            frame.render_widget(indicator, scrollbar_area);
        }
    }

    /// Render Mission Control TaskIO tab
    /// Shows selected task's input and output data (uses cached JSON)
    pub(super) fn render_mission_task_io(
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
    pub(super) fn render_mission_output(
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
}
