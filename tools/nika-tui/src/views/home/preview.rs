//! DAG preview and YAML syntax-highlighted preview helpers.
//!
//! Contains workflow parsing cache, dependency extraction, verb coloring,
//! and prompt preview extraction for the right-side preview panel.

use std::borrow::Cow;
use std::collections::HashMap;

use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
    Frame,
};

use super::{HomeView, PreviewMode};
use crate::theme::{TaskStatus, Theme, VerbColor};
use crate::widgets::{DagAscii, NodeBoxData, NodeBoxMode};
use nika_engine::ast::{parse_workflow, TaskAction, Workflow};

impl HomeView {
    // =========================================================================
    // DAG PREVIEW HELPERS
    // =========================================================================

    /// Compute a simple hash of content for cache invalidation
    pub(crate) fn content_hash(content: &str) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        hasher.finish()
    }

    /// Extract dependencies from depends_on fields
    ///
    /// Returns a map: target_task_id -> [source_task_ids]
    pub(crate) fn extract_flow_dependencies(wf: &Workflow) -> HashMap<String, Vec<String>> {
        let mut deps: HashMap<String, Vec<String>> = HashMap::new();

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
    pub(crate) fn task_verb_color(task: &nika_engine::ast::Task) -> VerbColor {
        match &task.action {
            TaskAction::Infer { .. } => VerbColor::Infer,
            TaskAction::Exec { .. } => VerbColor::Exec,
            TaskAction::Fetch { .. } => VerbColor::Fetch,
            TaskAction::Invoke { .. } => VerbColor::Invoke,
            TaskAction::Agent { .. } => VerbColor::Agent,
        }
    }

    /// Render the preview panel (right 60%) with DAG visualization or YAML
    ///
    /// WOW UX: Enhanced with styled mode badge and keyboard hints footer
    pub(crate) fn render_preview(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // WOW UX: Build styled title with mode badge
        let title: Line = match self.preview_mode {
            PreviewMode::Dag => {
                let expand_hint = if self.dag_expanded { "━" } else { "+" };
                Line::from(vec![
                    Span::raw(" "),
                    Span::styled(
                        " DAG ",
                        Style::default()
                            .fg(Color::White)
                            .bg(Color::Indexed(99)) // Violet background
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(" "),
                    Span::styled(
                        format!("[E]{}", expand_hint),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::raw(" "),
                    Span::styled("[D]→YAML", Style::default().fg(theme.text_muted)),
                    Span::raw(" "),
                ])
            }
            PreviewMode::Yaml => Line::from(vec![
                Span::raw(" "),
                Span::styled(
                    " YAML ",
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Indexed(36)) // Cyan background
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" verb-colored "),
                Span::styled("[D]→DAG", Style::default().fg(theme.text_muted)),
                Span::raw(" "),
            ]),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .border_style(Style::default().fg(theme.border_normal));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        // Check if we have a valid file selected
        let content: &str = if let Some(entry) = self.selected_entry() {
            if entry.is_dir {
                // WOW UX: Enhanced directory selected hint with icon
                let hint_lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(
                        "📁",
                        Style::default().add_modifier(Modifier::DIM),
                    )),
                    Line::from(""),
                    Line::from(Span::styled(
                        "Select a .nika.yaml file",
                        Style::default().fg(theme.text_muted),
                    )),
                    Line::from(Span::styled(
                        "to preview task graph",
                        Style::default().fg(theme.text_muted),
                    )),
                ];
                let paragraph =
                    Paragraph::new(hint_lines).alignment(ratatui::layout::Alignment::Center);
                frame.render_widget(paragraph, inner);
                return;
            }
            &self.standalone.preview_content
        } else {
            // WOW UX: Enhanced no file selected state
            let hint_lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "📄",
                    Style::default().add_modifier(Modifier::DIM),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "No file selected",
                    Style::default().fg(theme.text_muted),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Use ↑↓ or j/k to navigate",
                    Style::default()
                        .fg(theme.text_muted)
                        .add_modifier(Modifier::DIM),
                )),
            ];
            let paragraph =
                Paragraph::new(hint_lines).alignment(ratatui::layout::Alignment::Center);
            frame.render_widget(paragraph, inner);
            return;
        };

        // YAML mode: render verb-colored text directly (no DAG parsing needed)
        if self.preview_mode == PreviewMode::Yaml {
            Self::render_yaml_preview(frame, inner, content, theme);
            return;
        }

        // PERF: Compute content hash to check if we need to re-parse
        let current_hash = Self::content_hash(content);
        let cached_hash = self.cached_content_hash.get();

        // Update cache if content changed
        if current_hash != cached_hash {
            *self.cached_workflow.borrow_mut() = parse_workflow(content).ok();
            self.cached_content_hash.set(current_hash);
        }

        // Use cached workflow (avoids parsing every frame)
        let cached = self.cached_workflow.borrow();
        match cached.as_ref() {
            Some(wf) => {
                if wf.tasks.is_empty() {
                    let paragraph = Paragraph::new("(no tasks defined)")
                        .style(Style::default().fg(theme.text_muted));
                    frame.render_widget(paragraph, inner);
                    return;
                }

                // Build dependency map from Flow objects
                let deps = Self::extract_flow_dependencies(wf);

                // Convert tasks to NodeBoxData
                let nodes: Vec<NodeBoxData> = wf
                    .tasks
                    .iter()
                    .map(|task| {
                        let verb = Self::task_verb_color(task.as_ref());
                        let mut node =
                            NodeBoxData::new(&task.id, verb).with_status(TaskStatus::Pending);

                        // Add prompt preview if available (for infer/agent)
                        if let Some(prompt) = Self::extract_prompt_preview(task.as_ref()) {
                            node = node.with_prompt_preview(prompt);
                        }

                        // Add for_each info if present
                        if let Some(for_each) = &task.for_each {
                            if let Some(arr) = for_each.as_array() {
                                node = node.with_for_each_count(arr.len());
                            }
                        }

                        node
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
                    .scroll(0, 0);

                // Render to buffer (DagAscii implements Widget)
                let buf = frame.buffer_mut();
                widget.render(inner, buf);
            }
            None => {
                // YAML doesn't parse as workflow - show text fallback with line numbers
                let lines: Vec<Line> = content
                    .lines()
                    .take(inner.height as usize) // Limit to visible area
                    .enumerate()
                    .map(|(i, line)| {
                        Line::from(vec![
                            Span::styled(
                                format!("{:4} │ ", i + 1),
                                Style::default().fg(theme.text_muted),
                            ),
                            Span::raw(line),
                        ])
                    })
                    .collect();

                let paragraph = Paragraph::new(lines);
                frame.render_widget(paragraph, inner);
            }
        }
    }

    /// Get VerbColor for YAML line highlighting
    /// Returns the VerbColor if the line starts a verb block (infer:, exec:, etc.)
    pub(crate) fn get_line_verb_color(line: &str) -> Option<VerbColor> {
        let trimmed = line.trim();

        // Check for verb declarations (exact match with VerbColor)
        if trimmed.starts_with("infer:") {
            Some(VerbColor::Infer) // ⚡ Infer - violet
        } else if trimmed.starts_with("exec:") {
            Some(VerbColor::Exec) // 📟 Exec - amber
        } else if trimmed.starts_with("fetch:") {
            Some(VerbColor::Fetch) // 🛰️ Fetch - cyan
        } else if trimmed.starts_with("invoke:") {
            Some(VerbColor::Invoke) // 🔌 Invoke - emerald
        } else if trimmed.starts_with("agent:") {
            Some(VerbColor::Agent) // 🐔 Agent - rose
        } else {
            None
        }
    }

    /// Render YAML content with verb-colored syntax highlighting
    ///
    /// Each verb (infer, exec, fetch, invoke, agent) gets its canonical VerbColor.
    /// Lines within a verb block inherit the color until a new top-level key appears.
    pub(crate) fn render_yaml_preview(frame: &mut Frame, area: Rect, content: &str, theme: &Theme) {
        // Track current verb context for indented lines
        let mut current_verb: Option<VerbColor> = None;

        let lines: Vec<Line> = content
            .lines()
            .take(area.height as usize) // Limit to visible area
            .enumerate()
            .map(|(i, line)| {
                // Check if line starts a new verb block
                if let Some(verb) = Self::get_line_verb_color(line) {
                    current_verb = Some(verb);
                } else if !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty() {
                    // Non-indented, non-empty line resets verb context
                    current_verb = None;
                }

                // Build styled line with verb coloring
                // Use Cow to avoid allocations for static strings (performance)
                let (prefix_icon, line_style): (Cow<'static, str>, Style) = match current_verb {
                    Some(verb) => {
                        // Verb icon as prefix for verb lines only
                        let icon: Cow<'static, str> = if Self::get_line_verb_color(line).is_some() {
                            Cow::Owned(format!("{} ", verb.icon()))
                        } else {
                            Cow::Borrowed("  ") // Static str, no allocation
                        };
                        (icon, Style::default().fg(verb.rgb()))
                    }
                    None => (
                        Cow::Borrowed("  "), // Static str, no allocation
                        Style::default().fg(theme.text_secondary),
                    ),
                };

                Line::from(vec![
                    Span::styled(
                        format!("{:4} │", i + 1),
                        Style::default().fg(theme.text_muted),
                    ),
                    Span::styled(prefix_icon, line_style),
                    Span::styled(line.to_string(), line_style),
                ])
            })
            .collect();

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, area);
    }

    /// Extract prompt preview from task for DAG display
    pub(crate) fn extract_prompt_preview(task: &nika_engine::ast::Task) -> Option<String> {
        match &task.action {
            TaskAction::Infer { infer } => {
                // Truncate to ~50 chars for preview
                let prompt = &infer.prompt;
                let preview: String = prompt.chars().take(50).collect();
                if prompt.len() > 50 {
                    Some(format!("{}...", preview))
                } else {
                    Some(preview)
                }
            }
            TaskAction::Agent { agent } => {
                let prompt = &agent.prompt;
                let preview: String = prompt.chars().take(50).collect();
                if prompt.len() > 50 {
                    Some(format!("{}...", preview))
                } else {
                    Some(preview)
                }
            }
            TaskAction::Exec { exec } => {
                let command = &exec.command;
                let preview: String = command.chars().take(40).collect();
                if command.len() > 40 {
                    Some(format!("$ {}...", preview))
                } else {
                    Some(format!("$ {}", command))
                }
            }
            TaskAction::Fetch { fetch } => Some(fetch.url.clone()),
            TaskAction::Invoke { invoke } => {
                invoke.tool.clone().or_else(|| invoke.resource.clone())
            }
        }
    }
}
