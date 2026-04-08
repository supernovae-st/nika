// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat DAG Panel Widget
//!
//! Combines ChatNodeBox and ChatEdgeLine to render a complete
//! chat conversation as a directed acyclic graph.
//!
//! Chat-as-DAG architecture

use super::edge_line::{ChatEdgeLine, ChatPosition};
use super::node_box::{ChatNodeBox, ChatNodeKind, ChatNodeState};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Borders, Widget},
};
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════════════════════
// DAG NODE DATA
// ═══════════════════════════════════════════════════════════════════════════

/// Node data for the DAG panel
#[derive(Debug, Clone)]
pub struct DagNodeData {
    /// Node ID
    pub id: String,
    /// Node kind
    pub kind: ChatNodeKind,
    /// Display label (truncated message preview)
    pub label: String,
    /// Stable node index (@N reference)
    pub index: u32,
    /// Current state
    pub state: ChatNodeState,
}

impl DagNodeData {
    /// Create a new DAG node
    pub fn new(id: &str, kind: ChatNodeKind, index: u32) -> Self {
        Self {
            id: id.to_string(),
            kind,
            label: String::new(),
            index,
            state: ChatNodeState::default(),
        }
    }

    /// Set the label
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = label.to_string();
        self
    }

    /// Set the state
    pub fn with_state(mut self, state: ChatNodeState) -> Self {
        self.state = state;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// DAG EDGE DATA
// ═══════════════════════════════════════════════════════════════════════════

/// Edge data for the DAG panel
#[derive(Debug, Clone)]
pub struct DagEdgeData {
    /// Source node ID
    pub from: String,
    /// Target node ID
    pub to: String,
    /// Optional label (e.g., "@1" for mention edges)
    pub label: Option<String>,
    /// Whether this is an active/animated edge
    pub active: bool,
}

impl DagEdgeData {
    /// Create a new edge
    pub fn new(from: &str, to: &str) -> Self {
        Self {
            from: from.to_string(),
            to: to.to_string(),
            label: None,
            active: false,
        }
    }

    /// Set the label
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }

    /// Set active state
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// CHAT DAG PANEL
// ═══════════════════════════════════════════════════════════════════════════

/// Panel displaying chat conversation as a DAG
#[derive(Debug, Clone, Default)]
pub struct ChatDagPanel {
    /// Nodes in the DAG
    nodes: Vec<DagNodeData>,
    /// Edges connecting nodes
    edges: Vec<DagEdgeData>,
    /// Currently selected node ID
    selected: Option<String>,
    /// Scroll offset for vertical scrolling
    scroll_offset: u16,
    /// Animation tick for running nodes
    animation_tick: u8,
    /// Title for the panel
    title: String,
    /// Whether panel is visible
    visible: bool,
}

impl ChatDagPanel {
    /// Create a new empty DAG panel
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            selected: None,
            scroll_offset: 0,
            animation_tick: 0,
            title: "DAG".to_string(),
            visible: true,
        }
    }

    /// Set the title
    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    /// Add a node to the DAG
    pub fn add_node(&mut self, node: DagNodeData) {
        self.nodes.push(node);
    }

    /// Add an edge to the DAG
    pub fn add_edge(&mut self, edge: DagEdgeData) {
        self.edges.push(edge);
    }

    /// Get all nodes
    pub fn nodes(&self) -> &[DagNodeData] {
        &self.nodes
    }

    /// Get all edges
    pub fn edges(&self) -> &[DagEdgeData] {
        &self.edges
    }

    /// Get the number of nodes
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Get the number of edges
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Select a node by ID
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_string());
    }

    /// Clear selection
    pub fn clear_selection(&mut self) {
        self.selected = None;
    }

    /// Get selected node ID
    pub fn selected(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// Move selection up
    pub fn select_prev(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        match &self.selected {
            Some(id) => {
                if let Some(idx) = self.nodes.iter().position(|n| n.id == *id) {
                    if idx > 0 {
                        self.selected = Some(self.nodes[idx - 1].id.clone());
                    }
                }
            }
            None => {
                self.selected = self.nodes.last().map(|n| n.id.clone());
            }
        }
    }

    /// Move selection down
    pub fn select_next(&mut self) {
        if self.nodes.is_empty() {
            return;
        }

        match &self.selected {
            Some(id) => {
                if let Some(idx) = self.nodes.iter().position(|n| n.id == *id) {
                    if idx < self.nodes.len() - 1 {
                        self.selected = Some(self.nodes[idx + 1].id.clone());
                    }
                }
            }
            None => {
                self.selected = self.nodes.first().map(|n| n.id.clone());
            }
        }
    }

    /// Toggle visibility
    pub fn toggle_visible(&mut self) {
        self.visible = !self.visible;
    }

    /// Check if visible
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Set visibility
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Scroll up
    pub fn scroll_up(&mut self) {
        self.scroll_offset = self.scroll_offset.saturating_sub(1);
    }

    /// Scroll down
    pub fn scroll_down(&mut self, max_offset: u16) {
        self.scroll_offset = (self.scroll_offset + 1).min(max_offset);
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> u16 {
        self.scroll_offset
    }

    /// Advance animation tick
    pub fn tick(&mut self) {
        self.animation_tick = self.animation_tick.wrapping_add(1);
    }

    /// Set animation tick directly
    pub fn set_animation_tick(&mut self, tick: u8) {
        self.animation_tick = tick;
    }

    /// Update node state by ID
    pub fn update_node_state(&mut self, id: &str, state: ChatNodeState) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            node.state = state;
        }
    }

    /// Clear all nodes and edges
    pub fn clear(&mut self) {
        self.nodes.clear();
        self.edges.clear();
        self.selected = None;
        self.scroll_offset = 0;
    }

    /// Calculate node positions for layout
    fn calculate_positions(&self, area: Rect) -> HashMap<String, (u16, u16)> {
        let mut positions = HashMap::new();

        if self.nodes.is_empty() {
            return positions;
        }

        let node_width = 20u16;
        let node_height = 3u16;
        let vertical_gap = 2u16;

        // Center horizontally
        let center_x = area.x + (area.width.saturating_sub(node_width)) / 2;

        // Calculate vertical positions with scroll
        let mut y = area
            .y
            .saturating_sub(self.scroll_offset * (node_height + vertical_gap));

        for node in &self.nodes {
            positions.insert(node.id.clone(), (center_x, y));
            y += node_height + vertical_gap;
        }

        positions
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// WIDGET IMPLEMENTATION
// ═══════════════════════════════════════════════════════════════════════════

impl Widget for ChatDagPanel {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Don't render if not visible
        if !self.visible {
            return;
        }

        // Draw border
        let block = Block::default().borders(Borders::ALL).title(format!(
            " {} ({}) ",
            self.title,
            self.nodes.len()
        ));

        let inner = block.inner(area);
        block.render(area, buf);

        // Check for empty state
        if self.nodes.is_empty() {
            buf.set_string(
                inner.x + 1,
                inner.y,
                "No messages",
                Style::default().fg(Color::DarkGray),
            );
            return;
        }

        // Calculate positions
        let positions = self.calculate_positions(inner);

        // Render edges first (behind nodes)
        for edge in &self.edges {
            if let (Some(&(from_x, from_y)), Some(&(to_x, to_y))) =
                (positions.get(&edge.from), positions.get(&edge.to))
            {
                // Only render if both ends are in visible area
                if from_y < inner.y + inner.height && to_y < inner.y + inner.height {
                    let mut edge_line = ChatEdgeLine::new(
                        ChatPosition {
                            x: from_x + 10, // Center of node
                            y: from_y + 3,  // Bottom of node
                        },
                        ChatPosition {
                            x: to_x + 10, // Center of node
                            y: to_y,      // Top of node
                        },
                    )
                    .with_active(edge.active);

                    // Add label if present
                    if let Some(label) = &edge.label {
                        edge_line = edge_line.with_label(label);
                    }

                    edge_line.render(inner, buf);
                }
            }
        }

        // Render nodes
        for node in &self.nodes {
            if let Some(&(x, y)) = positions.get(&node.id) {
                // Only render if in visible area
                if y >= inner.y && y < inner.y + inner.height.saturating_sub(2) {
                    let is_selected = self.selected.as_ref() == Some(&node.id);

                    let mut node_box = ChatNodeBox::new(node.kind)
                        .with_label(&node.label)
                        .with_index(node.index)
                        .with_state(node.state)
                        .with_selected(is_selected);

                    // Advance tick for animation if running
                    if node.state.is_running() {
                        node_box.tick();
                    }

                    let node_area = Rect::new(x, y, 20.min(inner.width), 3);
                    node_box.render(node_area, buf);
                }
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // --- DagNodeData tests ---

    #[test]
    fn test_dag_node_data_creation() {
        let node = DagNodeData::new("msg-001", ChatNodeKind::User, 1);

        assert_eq!(node.id, "msg-001");
        assert_eq!(node.kind, ChatNodeKind::User);
        assert_eq!(node.index, 1);
        assert!(node.label.is_empty());
    }

    #[test]
    fn test_dag_node_data_with_label() {
        let node = DagNodeData::new("msg-001", ChatNodeKind::User, 1).with_label("Hello world");

        assert_eq!(node.label, "Hello world");
    }

    #[test]
    fn test_dag_node_data_with_state() {
        let node =
            DagNodeData::new("msg-001", ChatNodeKind::User, 1).with_state(ChatNodeState::Running);

        assert_eq!(node.state, ChatNodeState::Running);
    }

    // --- DagEdgeData tests ---

    #[test]
    fn test_dag_edge_data_creation() {
        let edge = DagEdgeData::new("msg-001", "msg-002");

        assert_eq!(edge.from, "msg-001");
        assert_eq!(edge.to, "msg-002");
        assert!(edge.label.is_none());
        assert!(!edge.active);
    }

    #[test]
    fn test_dag_edge_data_with_label() {
        let edge = DagEdgeData::new("msg-001", "msg-003").with_label("@1");

        assert_eq!(edge.label, Some("@1".to_string()));
    }

    #[test]
    fn test_dag_edge_data_with_active() {
        let edge = DagEdgeData::new("msg-001", "msg-002").with_active(true);

        assert!(edge.active);
    }

    // --- ChatDagPanel creation tests ---

    #[test]
    fn test_chat_dag_panel_creation() {
        let panel = ChatDagPanel::new();

        assert!(panel.nodes.is_empty());
        assert!(panel.edges.is_empty());
        assert!(panel.selected.is_none());
        assert!(panel.is_visible());
    }

    #[test]
    fn test_chat_dag_panel_with_title() {
        let panel = ChatDagPanel::new().with_title("CHAT DAG");

        assert_eq!(panel.title, "CHAT DAG");
    }

    #[test]
    fn test_chat_dag_panel_add_node() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2));

        assert_eq!(panel.node_count(), 2);
        assert_eq!(panel.nodes()[0].id, "msg-001");
        assert_eq!(panel.nodes()[1].id, "msg-002");
    }

    #[test]
    fn test_chat_dag_panel_add_edge() {
        let mut panel = ChatDagPanel::new();
        panel.add_edge(DagEdgeData::new("msg-001", "msg-002"));

        assert_eq!(panel.edge_count(), 1);
        assert_eq!(panel.edges()[0].from, "msg-001");
        assert_eq!(panel.edges()[0].to, "msg-002");
    }

    // --- Selection tests ---

    #[test]
    fn test_chat_dag_panel_selection() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));

        panel.select("msg-001");
        assert_eq!(panel.selected(), Some("msg-001"));

        panel.clear_selection();
        assert!(panel.selected().is_none());
    }

    #[test]
    fn test_chat_dag_panel_select_prev() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2));

        panel.select("msg-002");
        panel.select_prev();

        assert_eq!(panel.selected(), Some("msg-001"));
    }

    #[test]
    fn test_chat_dag_panel_select_next() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2));

        panel.select("msg-001");
        panel.select_next();

        assert_eq!(panel.selected(), Some("msg-002"));
    }

    #[test]
    fn test_chat_dag_panel_select_prev_at_start() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2));

        panel.select("msg-001");
        panel.select_prev();

        // Should stay at first node
        assert_eq!(panel.selected(), Some("msg-001"));
    }

    #[test]
    fn test_chat_dag_panel_select_next_at_end() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2));

        panel.select("msg-002");
        panel.select_next();

        // Should stay at last node
        assert_eq!(panel.selected(), Some("msg-002"));
    }

    // --- Visibility tests ---

    #[test]
    fn test_chat_dag_panel_toggle_visible() {
        let mut panel = ChatDagPanel::new();
        assert!(panel.is_visible());

        panel.toggle_visible();
        assert!(!panel.is_visible());

        panel.toggle_visible();
        assert!(panel.is_visible());
    }

    #[test]
    fn test_chat_dag_panel_set_visible() {
        let mut panel = ChatDagPanel::new();

        panel.set_visible(false);
        assert!(!panel.is_visible());

        panel.set_visible(true);
        assert!(panel.is_visible());
    }

    // --- Scroll tests ---

    #[test]
    fn test_chat_dag_panel_scroll_up() {
        let mut panel = ChatDagPanel::new();
        panel.scroll_offset = 5;

        panel.scroll_up();
        assert_eq!(panel.scroll_offset(), 4);
    }

    #[test]
    fn test_chat_dag_panel_scroll_up_at_zero() {
        let mut panel = ChatDagPanel::new();

        panel.scroll_up();
        assert_eq!(panel.scroll_offset(), 0);
    }

    #[test]
    fn test_chat_dag_panel_scroll_down() {
        let mut panel = ChatDagPanel::new();

        panel.scroll_down(10);
        assert_eq!(panel.scroll_offset(), 1);
    }

    #[test]
    fn test_chat_dag_panel_scroll_down_max() {
        let mut panel = ChatDagPanel::new();
        panel.scroll_offset = 10;

        panel.scroll_down(10);
        assert_eq!(panel.scroll_offset(), 10);
    }

    // --- State update tests ---

    #[test]
    fn test_chat_dag_panel_update_node_state() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));

        panel.update_node_state("msg-001", ChatNodeState::Complete);

        assert_eq!(panel.nodes()[0].state, ChatNodeState::Complete);
    }

    #[test]
    fn test_chat_dag_panel_tick() {
        let mut panel = ChatDagPanel::new();

        panel.tick();
        assert_eq!(panel.animation_tick, 1);

        panel.tick();
        assert_eq!(panel.animation_tick, 2);
    }

    // --- Clear tests ---

    #[test]
    fn test_chat_dag_panel_clear() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.add_edge(DagEdgeData::new("msg-001", "msg-002"));
        panel.select("msg-001");
        panel.scroll_offset = 5;

        panel.clear();

        assert!(panel.is_empty());
        assert_eq!(panel.edge_count(), 0);
        assert!(panel.selected().is_none());
        assert_eq!(panel.scroll_offset(), 0);
    }

    // --- Render tests ---

    #[test]
    fn test_chat_dag_panel_render_empty() {
        let panel = ChatDagPanel::new();
        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));

        panel.render(buf.area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("DAG"));
        assert!(content.contains("No messages"));
    }

    #[test]
    fn test_chat_dag_panel_render_with_nodes() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1).with_label("Hello"));
        panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2).with_label("Hi!"));
        panel.add_edge(DagEdgeData::new("msg-001", "msg-002"));

        let mut buf = Buffer::empty(Rect::new(0, 0, 40, 15));
        panel.render(buf.area, &mut buf);

        let content = buffer_to_string(&buf);
        assert!(content.contains("DAG"));
        assert!(content.contains("(2)")); // 2 nodes
    }

    #[test]
    fn test_chat_dag_panel_render_not_visible() {
        let mut panel = ChatDagPanel::new();
        panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
        panel.set_visible(false);

        let mut buf = Buffer::empty(Rect::new(0, 0, 30, 10));
        panel.render(buf.area, &mut buf);

        // Buffer should be empty (all spaces)
        let content = buffer_to_string(&buf);
        assert!(!content.contains("DAG"));
    }

    // --- Helper ---

    fn buffer_to_string(buf: &Buffer) -> String {
        buf.content.iter().map(|c| c.symbol()).collect()
    }
}
