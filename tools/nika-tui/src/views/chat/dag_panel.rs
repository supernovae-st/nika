// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! DAG Panel for Chat View
//!
//! Contains DAG visualization, task queue management, and hints rendering.

use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    prelude::Widget,
    style::{Color, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

use super::{
    ChatDagPanel, ChatNodeKind, ChatNodeState, ChatTaskQueue, ChatTaskQueueItem, ChatTaskState,
    ChatTaskVerb, ChatView, DagEdgeData, DagNodeData, WorkflowRole,
};
use crate::Theme;

// ═══════════════════════════════════════════════════════════════════════════════
// Chat DAG Visualization
// ═══════════════════════════════════════════════════════════════════════════════

impl ChatView {
    /// Render the DAG panel showing chat as a graph + task queue
    pub(super) fn render_dag_panel(&mut self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        // Split into: DAG graph (top 60%) | Task queue (bottom 40%)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
            .split(area);

        // 1. DAG Graph Panel (nodes + edges) — cached, rebuilt only on data change
        let base_panel = self.cached_dag_panel.get_or_insert_with(|| {
            let mut panel = ChatDagPanel::new().with_title("CHAT DAG");
            for node in &self.dag_nodes {
                panel.add_node(node.clone());
            }
            for edge in &self.dag_edges {
                panel.add_edge(edge.clone());
            }
            panel
        });
        let mut dag_panel = base_panel.clone();
        if let Some(idx) = self.dag_selected {
            if let Some(node) = self.dag_nodes.get(idx) {
                dag_panel.select(&node.id);
            }
        }
        dag_panel.set_animation_tick(self.frame);
        dag_panel.render(chunks[0], frame.buffer_mut());

        // 2. Task Queue Panel (pending/running/completed)
        let mut task_queue = ChatTaskQueue::new().with_title("TASK QUEUE");
        for item in &self.task_queue {
            task_queue.add(item.clone());
        }
        task_queue.render(chunks[1], frame.buffer_mut());
    }

    /// Sync DAG state from chat messages (call when messages change)
    pub fn sync_dag_from_messages(&mut self) {
        self.dag_nodes.clear();
        self.dag_edges.clear();
        self.cached_dag_panel = None;

        // Use ChatWorkflow's DAG for proper dependency edges
        let messages = self.workflow.all_messages();

        for (i, (idx, msg)) in messages.iter().enumerate() {
            let kind = match msg.role {
                WorkflowRole::User => ChatNodeKind::User,
                WorkflowRole::Assistant => ChatNodeKind::Assistant,
                WorkflowRole::System => ChatNodeKind::System,
                WorkflowRole::Tool => ChatNodeKind::ToolCall,
            };

            // Create node with truncated label (UTF-8 safe)
            let label = if msg.content.chars().count() > 30 {
                let truncated: String = msg.content.chars().take(27).collect();
                format!("{}...", truncated)
            } else {
                msg.content.clone()
            };

            let node = DagNodeData::new(&msg.id, kind, i as u32)
                .with_label(&label)
                .with_state(ChatNodeState::Complete);

            self.dag_nodes.push(node);

            // Use actual DAG edges (includes @N mention references)
            for dep_idx in self.workflow.get_dependencies(*idx) {
                if let Some(dep_msg) = self.workflow.get_message_by_index(dep_idx) {
                    self.dag_edges.push(DagEdgeData::new(&dep_msg.id, &msg.id));
                }
            }
        }
    }

    /// Add a task to the queue (call when task execution starts)
    pub fn add_task_to_queue(&mut self, id: &str, verb: ChatTaskVerb) {
        let item = ChatTaskQueueItem::new(id, verb);
        self.task_queue.push(item);
    }

    /// Update task state in queue
    pub fn update_task_state(
        &mut self,
        id: &str,
        state: ChatTaskState,
        elapsed: Option<std::time::Duration>,
    ) {
        if let Some(task) = self.task_queue.iter_mut().find(|t| t.id() == id) {
            task.set_state(state);
            if let Some(e) = elapsed {
                task.set_elapsed(e);
            }
        }
    }

    /// Complete the last running task of a specific verb in the queue
    pub(super) fn complete_last_running_task(&mut self, verb: ChatTaskVerb, elapsed_ms: u64) {
        // Find and update the last running task of this verb type
        if let Some(task) = self
            .task_queue
            .iter_mut()
            .rev()
            .find(|t| t.verb() == verb && t.state() == ChatTaskState::Running)
        {
            task.set_state(ChatTaskState::Complete);
            task.set_elapsed(std::time::Duration::from_millis(elapsed_ms));
            task.set_progress(1.0); // Fix: Mark as 100% complete
        }
    }

    /// Fail the last running task of a specific verb in the queue
    pub(super) fn fail_last_running_task(&mut self, verb: ChatTaskVerb, elapsed_ms: u64) {
        // Find and update the last running task of this verb type
        if let Some(task) = self
            .task_queue
            .iter_mut()
            .rev()
            .find(|t| t.verb() == verb && t.state() == ChatTaskState::Running)
        {
            task.set_state(ChatTaskState::Failed);
            task.set_elapsed(std::time::Duration::from_millis(elapsed_ms));
            task.set_progress(1.0); // Fix: Failed tasks are also 100% done
        }
    }

    /// Toggle DAG panel visibility (Ctrl+D)
    pub fn toggle_dag_panel(&mut self) {
        self.show_dag_panel = !self.show_dag_panel;
        if self.show_dag_panel {
            // Sync DAG state when showing
            self.sync_dag_from_messages();
        }
    }

    /// Cycle TaskBox render mode: Compact → Expanded → Full → Compact
    /// Toggle with `m` key when not in input mode
    pub fn cycle_task_box_render_mode(&mut self) {
        self.task_box_render_mode = self.task_box_render_mode.cycle();
    }

    /// Render command hints bar
    pub(super) fn render_hints(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Fixed labels to match actual behavior
        let hints = Line::from(vec![
            Span::styled(
                " ⌘K ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" commands  "),
            Span::styled(
                " ⌘P ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" model  "),
            Span::styled(
                " ⇧P ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" providers  "),
            Span::styled(
                " 1234 ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" views  "),
            Span::styled(
                " Esc ",
                Style::default().fg(Color::Black).bg(theme.highlight),
            ),
            Span::raw(" close"),
        ]);

        let paragraph = Paragraph::new(hints);
        frame.render_widget(paragraph, area);
    }
}
