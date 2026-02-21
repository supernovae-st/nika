//! DAG Execution Panel
//!
//! Displays task dependency graph with:
//! - Visual DAG representation
//! - Task status indicators
//! - Dependency connections
//! - Execution progress
//! - Task details on selection

use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::tui::state::TuiState;
use crate::tui::theme::{TaskStatus, Theme};
use crate::tui::widgets::{Dag, DagNode, ScrollIndicator};

/// DAG Execution panel (Panel 2)
pub struct GraphPanel<'a> {
    state: &'a TuiState,
    theme: &'a Theme,
    focused: bool,
}

impl<'a> GraphPanel<'a> {
    pub fn new(state: &'a TuiState, theme: &'a Theme) -> Self {
        Self {
            state,
            theme,
            focused: false,
        }
    }

    pub fn focused(mut self, focused: bool) -> Self {
        self.focused = focused;
        self
    }

    /// Build DAG nodes from state
    fn build_dag_nodes(&self) -> Vec<DagNode> {
        self.state
            .task_order
            .iter()
            .filter_map(|id| {
                self.state.tasks.get(id).map(|task| {
                    let mut node = DagNode::new(&task.id, task.status)
                        .with_dependencies(task.dependencies.clone());

                    if let Some(ref task_type) = task.task_type {
                        node = node.with_type(task_type);
                    }

                    if let Some(ms) = task.duration_ms {
                        node = node.with_duration(ms);
                    }

                    if self.state.current_task.as_ref() == Some(&task.id) {
                        node = node.current();
                    }

                    // Add breakpoint marker (v0.5.2+)
                    if self.state.has_breakpoint(&task.id) {
                        node = node.with_breakpoint(true);
                    }

                    node
                })
            })
            .collect()
    }

    /// Render DAG statistics header
    fn render_stats(&self, area: Rect, buf: &mut Buffer) {
        let total = self.state.tasks.len();
        let completed = self
            .state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Success)
            .count();
        let failed = self
            .state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Failed)
            .count();
        let running = self
            .state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Running)
            .count();
        let pending = self
            .state
            .tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .count();

        // Stats line: ● 3 ○ 2 ◉ 1 ⊗ 0
        let stats_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("●", Style::default().fg(Color::Rgb(34, 197, 94))),
            Span::styled(
                format!(" {} ", completed),
                Style::default().fg(Color::White),
            ),
            Span::styled("○", Style::default().fg(Color::Rgb(107, 114, 128))),
            Span::styled(format!(" {} ", pending), Style::default().fg(Color::White)),
            Span::styled("◉", Style::default().fg(Color::Rgb(245, 158, 11))),
            Span::styled(format!(" {} ", running), Style::default().fg(Color::White)),
            Span::styled("⊗", Style::default().fg(Color::Rgb(239, 68, 68))),
            Span::styled(format!(" {}", failed), Style::default().fg(Color::White)),
            Span::styled(
                format!("  │ Total: {}", total),
                Style::default().fg(Color::DarkGray),
            ),
        ]);

        let paragraph = Paragraph::new(stats_line);
        paragraph.render(area, buf);
    }

    /// Render the DAG visualization with scroll indicator
    fn render_dag(&self, area: Rect, buf: &mut Buffer) {
        let nodes = self.build_dag_nodes();
        let total_nodes = nodes.len();

        // Get scroll offset for this panel
        let scroll = self
            .state
            .scroll
            .get(&crate::tui::state::PanelId::Dag)
            .copied()
            .unwrap_or(0);

        // Calculate visible count (each node takes ~2 lines in the DAG)
        let visible_count = (area.height as usize / 2).max(1);

        // Split area: DAG content | scroll indicator (1 char width)
        let has_scroll = total_nodes > visible_count;
        let (dag_area, scroll_area) = if has_scroll && area.width > 3 {
            let dag_width = area.width.saturating_sub(2);
            (
                Rect::new(area.x, area.y, dag_width, area.height),
                Some(Rect::new(area.x + dag_width + 1, area.y, 1, area.height)),
            )
        } else {
            (area, None)
        };

        // Apply scroll to nodes (skip first N)
        let visible_nodes: Vec<DagNode> = nodes.into_iter().skip(scroll).collect();

        // Pass animation frame for animated spinners on running tasks
        let dag = Dag::new(&visible_nodes).with_frame(self.state.frame);
        dag.render(dag_area, buf);

        // Render scroll indicator if scrollable
        if let Some(scroll_rect) = scroll_area {
            let indicator = ScrollIndicator::new()
                .position(scroll, total_nodes, visible_count)
                .thumb_style(Style::default().fg(Color::Rgb(59, 130, 246))) // blue
                .track_style(Style::default().fg(Color::DarkGray));
            indicator.render(scroll_rect, buf);
        }
    }

    /// Render dependency legend
    fn render_legend(&self, area: Rect, buf: &mut Buffer) {
        let legend = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled("○", Style::default().fg(Color::Rgb(107, 114, 128))),
            Span::styled(" pending ", Style::default().fg(Color::DarkGray)),
            Span::styled("◉", Style::default().fg(Color::Rgb(245, 158, 11))),
            Span::styled(" running ", Style::default().fg(Color::DarkGray)),
            Span::styled("●", Style::default().fg(Color::Rgb(34, 197, 94))),
            Span::styled(" done ", Style::default().fg(Color::DarkGray)),
            Span::styled("⊗", Style::default().fg(Color::Rgb(239, 68, 68))),
            Span::styled(" failed", Style::default().fg(Color::DarkGray)),
        ]);

        let paragraph = Paragraph::new(legend);
        paragraph.render(area, buf);
    }
}

impl Widget for GraphPanel<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Draw border
        let border_style = self.theme.border_style(self.focused);
        let block = Block::default()
            .borders(Borders::ALL)
            .title(" ⎔ DAG EXECUTION ")
            .border_style(border_style);

        let inner = block.inner(area);
        block.render(area, buf);

        if inner.height < 4 || inner.width < 15 {
            return;
        }

        // Layout: Stats | DAG | Legend
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // Stats
                Constraint::Min(2),    // DAG
                Constraint::Length(1), // Legend
            ])
            .split(inner);

        self.render_stats(chunks[0], buf);
        self.render_dag(chunks[1], buf);
        self.render_legend(chunks[2], buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::state::{TaskState, TuiState};
    use crate::tui::theme::Theme;

    #[test]
    fn test_graph_panel_creation() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = GraphPanel::new(&state, &theme).focused(true);
        assert!(panel.focused);
    }

    #[test]
    fn test_build_dag_nodes_empty() {
        let state = TuiState::new("test.yaml");
        let theme = Theme::novanet();
        let panel = GraphPanel::new(&state, &theme);
        let nodes = panel.build_dag_nodes();
        assert!(nodes.is_empty());
    }

    // === Scroll Indicator Tests (MEDIUM 11) ===

    #[test]
    fn test_scroll_indicator_shows_when_many_tasks() {
        let mut state = TuiState::new("test.yaml");
        // Add 20 tasks (more than can fit in a small area)
        for i in 0..20 {
            let task_id = format!("task-{}", i);
            state.task_order.push(task_id.clone());
            let mut task = TaskState::new(task_id.clone(), vec![]);
            task.task_type = Some("infer".to_string());
            state.tasks.insert(task_id, task);
        }

        let theme = Theme::novanet();
        let panel = GraphPanel::new(&state, &theme);

        // Render to buffer (small height = needs scroll)
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Buffer should contain scroll indicator characters
        let content: String = buf.content.iter().map(|c| c.symbol()).collect();
        // Should have scroll indicator when many tasks
        assert!(
            content.contains("░") || content.contains("█") || content.contains("▲"),
            "Should show scroll indicator with many tasks"
        );
    }

    #[test]
    fn test_no_scroll_indicator_when_few_tasks() {
        let mut state = TuiState::new("test.yaml");
        // Add just 2 tasks
        for i in 0..2 {
            let task_id = format!("task-{}", i);
            state.task_order.push(task_id.clone());
            let mut task = TaskState::new(task_id.clone(), vec![]);
            task.task_type = Some("exec".to_string());
            state.tasks.insert(task_id, task);
        }

        let theme = Theme::novanet();
        let panel = GraphPanel::new(&state, &theme);

        // Render to buffer (large height = no scroll needed)
        let area = Rect::new(0, 0, 40, 30);
        let mut buf = Buffer::empty(area);
        panel.render(area, &mut buf);

        // Verify panel renders without error (scroll indicator hidden)
        // Just verifies no panic
    }
}
