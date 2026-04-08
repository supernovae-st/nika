// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Execution panel rendering (Panel 2)
//!
//! Uses DagAscii widget with Sugiyama layout algorithm and real edges.
//! Tab-aware rendering:
//! - Graph tab: visual DAG with NodeBox widgets
//! - Yaml tab: workflow info summary

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use super::MonitorView;
use crate::focus::PanelId;
use crate::state::TuiState;
use crate::theme::{TaskStatus, Theme};
use crate::views::DagTab;
use crate::widgets::{DagAscii, NodeBoxMode, ScrollIndicator};

impl MonitorView {
    /// Render DAG Execution panel (Panel 2) using DagAscii widget
    ///
    /// Uses DagAscii widget with Sugiyama layout algorithm and real edges.
    /// Tab-aware rendering (Graph | Yaml)
    pub(super) fn render_dag_panel(
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

    /// Render DAG Yaml tab
    /// Shows workflow info (YAML source not stored in state)
    pub(super) fn render_dag_yaml(
        &self,
        frame: &mut Frame,
        area: Rect,
        state: &TuiState,
        theme: &Theme,
    ) {
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

        let total_lines = content.lines().count();
        let visible_count = area.height as usize;
        let scroll_offset = self.scroll[Self::panel_index(PanelId::RunnerDag)];

        let paragraph = Paragraph::new(content)
            .style(Style::default().fg(theme.text_primary))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .scroll((scroll_offset as u16, 0));
        frame.render_widget(paragraph, area);

        // Scroll indicator when YAML content overflows
        if total_lines > visible_count {
            let scrollbar_area = Rect {
                x: area.x + area.width.saturating_sub(1),
                y: area.y,
                width: 1,
                height: area.height,
            };
            let indicator = ScrollIndicator::new()
                .position(scroll_offset, total_lines, visible_count)
                .thumb_style(Style::default().fg(theme.scrollbar_thumb))
                .track_style(Style::default().fg(theme.scrollbar_track))
                .show_arrows(true);
            frame.render_widget(indicator, scrollbar_area);
        }
    }
}
