// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG panel rendering for the Studio view.
//!
//! Contains `render_dag_panel()` on `StudioView`, plus `render_structure()`,
//! `render_dag_structure()`, `render_validation()`, `render_complexity_meter()`,
//! and dependency/verb helpers on `YamlEditorPanel`.

use rustc_hash::FxHashMap;

use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

use super::types::StudioFocus;
use super::{StudioView, YamlEditorPanel};
use crate::theme::{TaskStatus, Theme, VerbColor};
use crate::widgets::{DagAscii, NodeBoxData, NodeBoxMode};
use nika_engine::ast::{Task, TaskAction, Workflow};

// ═══════════════════════════════════════════════════════════════════════════════
// StudioView: DAG panel in 3-panel layout
// ═══════════════════════════════════════════════════════════════════════════════

impl StudioView {
    /// Render the DAG panel (delegates to editor's DAG rendering)
    pub(super) fn render_dag_panel(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let style = self.border_style(StudioFocus::Dag, theme);

        // UX: Add focus indicator to show which panel is active
        let focus_indicator = if self.focus == StudioFocus::Dag {
            "●"
        } else {
            " "
        };

        let block = Block::default()
            .title(Span::styled(
                format!(" {} DAG Preview ", focus_indicator),
                style,
            ))
            .borders(Borders::ALL)
            .border_style(style);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Show placeholder when no file is loaded
        if self.editor.path.is_none() {
            let content_lines = 4u16;
            let pad_top = inner.height.saturating_sub(content_lines) / 2;

            let mut lines = vec![Line::from(""); pad_top as usize];
            lines.extend(vec![
                Line::from(Span::styled(
                    "No workflow loaded",
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Open a .nika.yaml file",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(Span::styled(
                    "to see task dependencies",
                    Style::default().fg(theme.text_muted),
                )),
            ]);

            let paragraph = Paragraph::new(lines).alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, inner);
            return;
        }

        // Render DAG structure from editor's cached workflow
        let yaml = self.editor.buffer.content();
        self.editor.render_dag_structure(frame, inner, &yaml, theme);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// YamlEditorPanel: Structure + DAG + Validation rendering
// ═══════════════════════════════════════════════════════════════════════════════

impl YamlEditorPanel {
    /// Render just the DAG structure (for use in StudioView 3-panel layout)
    pub(super) fn render_structure(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Title shows mode state
        let title = if self.dag_expanded {
            " STRUCTURE [E]xpanded "
        } else {
            " STRUCTURE [E]->expand "
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Parse YAML and render with DagAscii
        let content = self.buffer.content();
        self.render_dag_structure(frame, inner, &content, theme);
    }

    /// Render DAG structure using DagAscii widget (for use in StudioView 3-panel layout)
    ///
    /// PERF: Uses cached_workflow to avoid re-parsing YAML every frame (60 FPS).
    /// Only parses when content hash changes.
    fn render_dag_structure(&self, frame: &mut Frame, area: Rect, yaml: &str, theme: &Theme) {
        // PERF: Compute content hash to check if we need to re-parse
        let current_hash = Self::content_hash(yaml);
        let cached_hash = self.cached_content_hash.get();

        // Update cache if content changed
        if current_hash != cached_hash {
            *self.cached_workflow.borrow_mut() = nika_engine::ast::parse_workflow(yaml).ok();
            self.cached_content_hash.set(current_hash);
        }

        // Use cached workflow (avoids parsing every frame)
        let cached = self.cached_workflow.borrow();
        match cached.as_ref() {
            Some(wf) => {
                if wf.tasks.is_empty() {
                    let paragraph =
                        Paragraph::new("(no tasks)").style(Style::default().fg(theme.text_muted));
                    frame.render_widget(paragraph, area);
                    return;
                }

                // Build dependency map from Flow objects
                let deps = self.extract_flow_dependencies(wf);

                // Convert tasks to NodeBoxData
                let nodes: Vec<NodeBoxData> = wf
                    .tasks
                    .iter()
                    .map(|task| {
                        let verb = self.task_verb_color(task.as_ref());
                        NodeBoxData::new(&task.id, verb).with_status(TaskStatus::Pending)
                    })
                    .collect();

                let mode = if self.dag_expanded {
                    NodeBoxMode::Expanded
                } else {
                    NodeBoxMode::Minimal
                };

                // Create and render DagAscii widget
                let widget = DagAscii::new(&nodes)
                    .with_dependencies(deps)
                    .mode(mode)
                    .scroll(0, self.dag_scroll);

                // Render to buffer (DagAscii implements Widget)
                let buf = frame.buffer_mut();
                widget.render(area, buf);
            }
            None => {
                // YAML doesn't parse as workflow - show parse error state
                let paragraph =
                    Paragraph::new("⚠ Invalid workflow\n\nFix YAML errors to\nsee task structure")
                        .style(Style::default().fg(theme.status_failed));
                frame.render_widget(paragraph, area);
            }
        }
    }

    /// Extract dependencies from depends_on fields
    ///
    /// Returns a map: target_task_id -> [source_task_ids]
    pub(super) fn extract_flow_dependencies(
        &self,
        wf: &Workflow,
    ) -> FxHashMap<String, Vec<String>> {
        let mut deps: FxHashMap<String, Vec<String>> = FxHashMap::default();

        for task in &wf.tasks {
            if let Some(ref task_deps) = task.depends_on {
                let entry = deps.entry(task.id.clone()).or_default();
                for source in task_deps {
                    if !entry.contains(source) {
                        entry.push(source.clone());
                    }
                }
            }
        }

        deps
    }

    /// Get VerbColor from task action
    pub(super) fn task_verb_color(&self, task: &Task) -> VerbColor {
        match &task.action {
            TaskAction::Infer { .. } => VerbColor::Infer,
            TaskAction::Exec { .. } => VerbColor::Exec,
            TaskAction::Fetch { .. } => VerbColor::Fetch,
            TaskAction::Invoke { .. } => VerbColor::Invoke,
            TaskAction::Agent { .. } => VerbColor::Agent,
        }
    }

    /// Render the validation status bar
    pub(super) fn render_validation(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        let yaml_status = if self.validation.yaml_valid {
            Span::styled("Valid YAML", Style::default().fg(theme.status_success))
        } else {
            Span::styled("Invalid YAML", Style::default().fg(theme.status_failed))
        };

        let schema_status = if self.validation.schema_valid {
            Span::styled("Schema OK", Style::default().fg(theme.status_success))
        } else {
            Span::styled("Schema Error", Style::default().fg(theme.status_failed))
        };

        let warning_count = self.validation.warnings.len();
        let warning_status = if warning_count > 0 {
            Span::styled(
                format!("{} warning(s)", warning_count),
                Style::default().fg(theme.status_running), // Amber for warnings
            )
        } else {
            Span::styled("No warnings", Style::default().fg(theme.status_success))
        };

        // DAG Complexity Meter
        let complexity_meter = self.render_complexity_meter(theme);

        let line = Line::from(vec![
            Span::raw(" "),
            yaml_status,
            Span::raw("  |  "),
            schema_status,
            Span::raw("  |  "),
            warning_status,
            Span::raw("  |  "),
            complexity_meter,
        ]);

        let paragraph = Paragraph::new(line).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.border_normal)),
        );

        frame.render_widget(paragraph, area);
    }

    /// Render DAG complexity meter
    /// Shows visual indicator of workflow complexity (tasks x flows)
    fn render_complexity_meter(&self, theme: &Theme) -> Span<'static> {
        let cached = self.cached_workflow.borrow();
        match cached.as_ref() {
            Some(wf) => {
                let task_count = wf.tasks.len();
                let flow_count = wf.flow_count();
                // Complexity score: tasks + edges (simple heuristic)
                let score = task_count + flow_count;

                // Visual meter: 8 levels
                let (meter, color) = match score {
                    0 => ("▁", theme.text_muted),
                    1..=2 => ("▂", theme.status_success),
                    3..=5 => ("▃", theme.status_success),
                    6..=8 => ("▄", theme.highlight),
                    9..=12 => ("▅", theme.highlight),
                    13..=16 => ("▆", theme.status_running),
                    17..=20 => ("▇", theme.status_running),
                    _ => ("█", theme.status_failed),
                };

                Span::styled(
                    format!("DAG {} {}t {}e", meter, task_count, flow_count),
                    Style::default().fg(color),
                )
            }
            None => Span::styled("DAG ▁ --", Style::default().fg(theme.text_muted)),
        }
    }
}
