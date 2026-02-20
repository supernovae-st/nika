//! DAG Widget
//!
//! Visualizes task dependency graph with status indicators.
//! Enhanced with verb-specific icons and animated edges for execution flow.

use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier, Style},
    widgets::Widget,
};

use crate::tui::theme::TaskStatus;

/// Verb type for task icon display
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerbType {
    #[default]
    Unknown,
    Infer,
    Exec,
    Fetch,
    Invoke,
    Agent,
}

impl VerbType {
    /// Get the icon for this verb type
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Unknown => "📋",
            Self::Infer => "🤖",  // Robot for AI inference
            Self::Exec => "🖥️",   // Computer for shell exec
            Self::Fetch => "🌐",  // Globe for HTTP fetch
            Self::Invoke => "🔧", // Wrench for MCP tool invoke
            Self::Agent => "🤝",  // Handshake for agent loop
        }
    }

    /// Parse verb type from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "infer" => Self::Infer,
            "exec" => Self::Exec,
            "fetch" => Self::Fetch,
            "invoke" => Self::Invoke,
            "agent" => Self::Agent,
            _ => Self::Unknown,
        }
    }
}

/// Edge animation state for DAG visualization
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EdgeState {
    #[default]
    Inactive,
    /// Data flowing along this edge (shown during execution)
    Active,
    /// Data transfer complete
    Complete,
    /// Edge transfer failed
    Failed,
}

impl EdgeState {
    /// Get animated edge characters based on frame
    pub fn chars(&self, frame: u8) -> (&'static str, Color) {
        match self {
            Self::Inactive => ("│", Color::DarkGray),
            Self::Active => {
                // Animated flow characters cycling
                let flow_chars = ["│", "┃", "║", "┃"];
                let idx = (frame / 4) as usize % flow_chars.len();
                (flow_chars[idx], Color::Rgb(245, 158, 11)) // Amber
            }
            Self::Complete => ("┃", Color::Rgb(34, 197, 94)), // Green
            Self::Failed => ("╳", Color::Rgb(239, 68, 68)),   // Red
        }
    }

    /// Get horizontal edge characters
    pub fn horizontal_chars(&self, frame: u8) -> (&'static str, Color) {
        match self {
            Self::Inactive => ("─", Color::DarkGray),
            Self::Active => {
                let flow_chars = ["─", "━", "═", "━"];
                let idx = (frame / 4) as usize % flow_chars.len();
                (flow_chars[idx], Color::Rgb(245, 158, 11))
            }
            Self::Complete => ("━", Color::Rgb(34, 197, 94)),
            Self::Failed => ("╌", Color::Rgb(239, 68, 68)),
        }
    }

    /// Get data flow indicator (shows direction)
    pub fn flow_indicator(&self, frame: u8) -> Option<(&'static str, Color)> {
        match self {
            Self::Active => {
                let indicators = ["▼", "▽", "▼", "▽"];
                let idx = (frame / 3) as usize % indicators.len();
                Some((indicators[idx], Color::Rgb(245, 158, 11)))
            }
            _ => None,
        }
    }
}

/// Node in the DAG visualization
#[derive(Debug, Clone)]
pub struct DagNode {
    /// Task ID
    pub id: String,
    /// Task status
    pub status: TaskStatus,
    /// Task type (infer, exec, fetch, invoke, agent)
    pub task_type: Option<String>,
    /// Verb type (parsed from task_type for icon display)
    pub verb_type: VerbType,
    /// Dependencies (task IDs this depends on)
    pub dependencies: Vec<String>,
    /// Is this the currently executing task?
    pub is_current: bool,
    /// Duration in ms (if completed)
    pub duration_ms: Option<u64>,
    /// Edge state for incoming edges
    pub incoming_edge_state: EdgeState,
}

impl DagNode {
    pub fn new(id: impl Into<String>, status: TaskStatus) -> Self {
        Self {
            id: id.into(),
            status,
            task_type: None,
            verb_type: VerbType::Unknown,
            dependencies: Vec::new(),
            is_current: false,
            duration_ms: None,
            incoming_edge_state: EdgeState::Inactive,
        }
    }

    pub fn with_type(mut self, task_type: impl Into<String>) -> Self {
        let type_str = task_type.into();
        self.verb_type = VerbType::from_str(&type_str);
        self.task_type = Some(type_str);
        self
    }

    pub fn with_verb(mut self, verb: VerbType) -> Self {
        self.verb_type = verb;
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.dependencies = deps;
        self
    }

    pub fn current(mut self) -> Self {
        self.is_current = true;
        self
    }

    pub fn with_duration(mut self, ms: u64) -> Self {
        self.duration_ms = Some(ms);
        self
    }

    pub fn with_edge_state(mut self, state: EdgeState) -> Self {
        self.incoming_edge_state = state;
        self
    }
}

/// Animated spinner frames for running tasks
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// DAG visualization widget
pub struct Dag<'a> {
    nodes: &'a [DagNode],
    /// Selected node for details
    selected: Option<usize>,
    /// Compact mode (just icons)
    compact: bool,
    /// Animation frame (for spinners)
    frame: u8,
}

impl<'a> Dag<'a> {
    pub fn new(nodes: &'a [DagNode]) -> Self {
        Self {
            nodes,
            selected: None,
            compact: false,
            frame: 0,
        }
    }

    /// Set animation frame for spinners
    pub fn with_frame(mut self, frame: u8) -> Self {
        self.frame = frame;
        self
    }

    pub fn selected(mut self, index: usize) -> Self {
        self.selected = Some(index);
        self
    }

    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Get current spinner character
    fn spinner(&self) -> &'static str {
        let idx = (self.frame / 6) as usize % SPINNER_FRAMES.len();
        SPINNER_FRAMES[idx]
    }

    /// Get status color
    fn status_color(status: TaskStatus) -> Color {
        match status {
            TaskStatus::Pending => Color::Rgb(107, 114, 128), // gray
            TaskStatus::Running => Color::Rgb(245, 158, 11),  // amber
            TaskStatus::Success => Color::Rgb(34, 197, 94),   // green
            TaskStatus::Failed => Color::Rgb(239, 68, 68),    // red
            TaskStatus::Paused => Color::Rgb(6, 182, 212),    // cyan
        }
    }

    /// Get status icon (static version for non-running tasks)
    fn status_icon_static(status: TaskStatus, is_current: bool) -> &'static str {
        if is_current && status != TaskStatus::Running {
            return "▶";
        }
        match status {
            TaskStatus::Pending => "○",
            TaskStatus::Running => "◉", // Will be replaced by spinner
            TaskStatus::Success => "●",
            TaskStatus::Failed => "⊗",
            TaskStatus::Paused => "◎",
        }
    }

    /// Get status icon (animated version - uses spinner for running)
    fn status_icon(&self, status: TaskStatus, is_current: bool) -> &str {
        if status == TaskStatus::Running {
            return self.spinner();
        }
        Self::status_icon_static(status, is_current)
    }

    /// Get task type icon (uses VerbType for consistent display)
    fn type_icon(verb_type: VerbType) -> &'static str {
        verb_type.icon()
    }

    /// Get edge characters for rendering with animation
    fn edge_chars(&self, state: EdgeState) -> (&'static str, Color) {
        state.chars(self.frame)
    }

    /// Calculate node positions for layout
    fn calculate_layout(&self, _width: u16, _height: u16) -> Vec<(u16, u16)> {
        let node_count = self.nodes.len();
        if node_count == 0 {
            return Vec::new();
        }

        // Simple vertical list layout for now
        // Each node gets a row (TODO: variable height based on available space)
        let row_height = 1;

        self.nodes
            .iter()
            .enumerate()
            .map(|(i, _)| (2, i as u16 * row_height))
            .collect()
    }
}

impl Widget for Dag<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.height < 2 || area.width < 10 || self.nodes.is_empty() {
            if self.nodes.is_empty() {
                buf.set_string(
                    area.x + 2,
                    area.y,
                    "(no tasks scheduled)",
                    Style::default().fg(Color::DarkGray),
                );
            }
            return;
        }

        let positions = self.calculate_layout(area.width, area.height);

        // Render each node
        for (i, (node, &(x, y))) in self.nodes.iter().zip(positions.iter()).enumerate() {
            if y >= area.height {
                // Draw overflow indicator
                buf.set_string(
                    area.x + 2,
                    area.y + area.height - 1,
                    format!("... +{} more", self.nodes.len() - i),
                    Style::default().fg(Color::DarkGray),
                );
                break;
            }

            let color = Self::status_color(node.status);
            let icon = self.status_icon(node.status, node.is_current);

            // Draw dependency lines with animated edges
            if !node.dependencies.is_empty() && y > 0 {
                // Get edge character and color based on state
                let (connector, edge_color) = self.edge_chars(node.incoming_edge_state);

                if y > 0 {
                    buf.set_string(
                        area.x + x,
                        area.y + y.saturating_sub(1),
                        connector,
                        Style::default().fg(edge_color),
                    );
                }

                // Draw flow indicator for active edges
                if let Some((indicator, ind_color)) =
                    node.incoming_edge_state.flow_indicator(self.frame)
                {
                    if y > 1 && x > 0 {
                        buf.set_string(
                            area.x + x.saturating_sub(1),
                            area.y + y.saturating_sub(1),
                            indicator,
                            Style::default().fg(ind_color),
                        );
                    }
                }
            }

            // Draw node icon (animated for running, static for others)
            let icon_style = Style::default().fg(color).add_modifier(
                if node.is_current || node.status == TaskStatus::Running {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                },
            );
            buf.set_string(area.x + x, area.y + y, icon, icon_style);

            // Draw task ID
            let max_id_len = (area.width as usize).saturating_sub(x as usize + 4);
            let display_id = if node.id.len() > max_id_len {
                format!("{}…", &node.id[..max_id_len.saturating_sub(1)])
            } else {
                node.id.clone()
            };

            let id_style = if node.is_current {
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else if self.selected == Some(i) {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(color)
            };

            buf.set_string(area.x + x + 2, area.y + y, &display_id, id_style);

            // Draw type icon and duration if space allows
            if !self.compact && area.width > 30 {
                let type_x = area.x + x + 2 + display_id.len() as u16 + 1;
                if type_x < area.x + area.width - 10 {
                    let type_icon = Self::type_icon(node.verb_type);
                    buf.set_string(type_x, area.y + y, type_icon, Style::default());
                }

                // Draw duration for completed tasks
                if let Some(ms) = node.duration_ms {
                    let duration_str = format_duration_short(ms);
                    let duration_x = area.x + area.width - duration_str.len() as u16 - 1;
                    buf.set_string(
                        duration_x,
                        area.y + y,
                        &duration_str,
                        Style::default().fg(Color::DarkGray),
                    );
                }
            }
        }
    }
}

/// Format duration as compact string
fn format_duration_short(ms: u64) -> String {
    if ms < 1000 {
        format!("{}ms", ms)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        let mins = ms / 60_000;
        let secs = (ms % 60_000) / 1000;
        format!("{}m{}s", mins, secs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dag_node_creation() {
        let node = DagNode::new("task1", TaskStatus::Running)
            .with_type("infer")
            .with_dependencies(vec!["task0".to_string()])
            .current();

        assert_eq!(node.id, "task1");
        assert_eq!(node.status, TaskStatus::Running);
        assert!(node.is_current);
        assert_eq!(node.dependencies.len(), 1);
    }

    #[test]
    fn test_format_duration_short() {
        assert_eq!(format_duration_short(500), "500ms");
        assert_eq!(format_duration_short(1500), "1.5s");
        assert_eq!(format_duration_short(65000), "1m5s");
    }

    #[test]
    fn test_status_colors_distinct() {
        let colors = [
            Dag::status_color(TaskStatus::Pending),
            Dag::status_color(TaskStatus::Running),
            Dag::status_color(TaskStatus::Success),
            Dag::status_color(TaskStatus::Failed),
            Dag::status_color(TaskStatus::Paused),
        ];

        // Verify colors are distinct
        let unique_count = colors
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len();
        assert_eq!(unique_count, 5, "All status colors should be distinct");
    }

    #[test]
    fn test_verb_type_icons() {
        assert_eq!(VerbType::Infer.icon(), "🤖");
        assert_eq!(VerbType::Exec.icon(), "🖥️");
        assert_eq!(VerbType::Fetch.icon(), "🌐");
        assert_eq!(VerbType::Invoke.icon(), "🔧");
        assert_eq!(VerbType::Agent.icon(), "🤝");
        assert_eq!(VerbType::Unknown.icon(), "📋");
    }

    #[test]
    fn test_verb_type_from_str() {
        assert_eq!(VerbType::from_str("infer"), VerbType::Infer);
        assert_eq!(VerbType::from_str("INFER"), VerbType::Infer);
        assert_eq!(VerbType::from_str("exec"), VerbType::Exec);
        assert_eq!(VerbType::from_str("fetch"), VerbType::Fetch);
        assert_eq!(VerbType::from_str("invoke"), VerbType::Invoke);
        assert_eq!(VerbType::from_str("agent"), VerbType::Agent);
        assert_eq!(VerbType::from_str("unknown_verb"), VerbType::Unknown);
    }

    #[test]
    fn test_dag_node_with_verb() {
        let node = DagNode::new("task1", TaskStatus::Pending).with_verb(VerbType::Infer);
        assert_eq!(node.verb_type, VerbType::Infer);
    }

    #[test]
    fn test_dag_node_with_type_sets_verb() {
        let node = DagNode::new("task1", TaskStatus::Pending).with_type("agent");
        assert_eq!(node.verb_type, VerbType::Agent);
        assert_eq!(node.task_type, Some("agent".to_string()));
    }

    #[test]
    fn test_edge_state_chars_inactive() {
        let (ch, color) = EdgeState::Inactive.chars(0);
        assert_eq!(ch, "│");
        assert_eq!(color, Color::DarkGray);
    }

    #[test]
    fn test_edge_state_chars_active_animates() {
        let (ch0, _) = EdgeState::Active.chars(0);
        let (ch1, _) = EdgeState::Active.chars(4);

        // Characters should cycle
        assert!(ch0 == "│" || ch0 == "┃" || ch0 == "║");
        assert!(ch1 == "│" || ch1 == "┃" || ch1 == "║");
    }

    #[test]
    fn test_edge_state_complete() {
        let (ch, color) = EdgeState::Complete.chars(0);
        assert_eq!(ch, "┃");
        assert_eq!(color, Color::Rgb(34, 197, 94)); // Green
    }

    #[test]
    fn test_edge_state_failed() {
        let (ch, color) = EdgeState::Failed.chars(0);
        assert_eq!(ch, "╳");
        assert_eq!(color, Color::Rgb(239, 68, 68)); // Red
    }

    #[test]
    fn test_edge_state_flow_indicator() {
        // Active state has flow indicator
        let indicator = EdgeState::Active.flow_indicator(0);
        assert!(indicator.is_some());

        // Inactive state has no flow indicator
        let indicator = EdgeState::Inactive.flow_indicator(0);
        assert!(indicator.is_none());

        // Complete state has no flow indicator
        let indicator = EdgeState::Complete.flow_indicator(0);
        assert!(indicator.is_none());
    }

    #[test]
    fn test_dag_node_edge_state() {
        let node = DagNode::new("task1", TaskStatus::Running).with_edge_state(EdgeState::Active);
        assert_eq!(node.incoming_edge_state, EdgeState::Active);
    }
}
