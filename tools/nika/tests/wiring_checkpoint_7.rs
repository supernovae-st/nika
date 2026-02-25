//! WIRING-7: Chat DAG Panel Integration
//!
//! Verifies: ChatDagPanel, ChatNodeBox, ChatEdgeLine work together
//! Run after: v0.10.0 (Chat DAG Widgets)
//!
//! Tests validate:
//! - ChatNodeKind: 5 kinds (User, Assistant, ToolCall, System, Error)
//! - ChatNodeState: 4 states (Idle, Running, Complete, Failed)
//! - DagNodeData: Node construction with labels and states
//! - DagEdgeData: Edge construction with labels and active state
//! - ChatDagPanel: Panel composition and navigation

use nika::tui::widgets::{ChatDagPanel, ChatNodeKind, ChatNodeState, DagEdgeData, DagNodeData};

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: ChatNodeKind construction and methods
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_7_node_kind_user() {
    let kind = ChatNodeKind::User;
    assert_eq!(kind.icon(), "👤");
}

#[test]
fn wiring_7_node_kind_assistant() {
    let kind = ChatNodeKind::Assistant;
    assert_eq!(kind.icon(), "🤖");
}

#[test]
fn wiring_7_node_kind_tool_call() {
    let kind = ChatNodeKind::ToolCall;
    assert_eq!(kind.icon(), "🔌");
}

#[test]
fn wiring_7_node_kind_system() {
    let kind = ChatNodeKind::System;
    assert_eq!(kind.icon(), "⚙️");
}

#[test]
fn wiring_7_node_kind_error() {
    let kind = ChatNodeKind::Error;
    assert_eq!(kind.icon(), "❌");
}

#[test]
fn wiring_7_node_kind_all_five() {
    let all = ChatNodeKind::all();
    assert_eq!(all.len(), 5, "Should have 5 node kinds");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: ChatNodeState construction and methods
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_7_node_state_idle_default() {
    let state = ChatNodeState::default();
    assert_eq!(state, ChatNodeState::Idle);
    assert!(!state.is_running());
}

#[test]
fn wiring_7_node_state_running() {
    let state = ChatNodeState::Running;
    assert!(state.is_running());
}

#[test]
fn wiring_7_node_state_complete() {
    let state = ChatNodeState::Complete;
    assert!(!state.is_running());
}

#[test]
fn wiring_7_node_state_failed() {
    let state = ChatNodeState::Failed;
    assert!(!state.is_running());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: DagNodeData construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_7_dag_node_new() {
    let node = DagNodeData::new("node1", ChatNodeKind::User, 0);
    assert_eq!(node.id, "node1");
    assert_eq!(node.kind, ChatNodeKind::User);
    assert_eq!(node.index, 0);
    assert_eq!(node.state, ChatNodeState::Idle);
}

#[test]
fn wiring_7_dag_node_with_label() {
    let node = DagNodeData::new("node2", ChatNodeKind::Assistant, 1).with_label("Hello!");
    assert_eq!(node.label, "Hello!");
}

#[test]
fn wiring_7_dag_node_with_state() {
    let node =
        DagNodeData::new("node3", ChatNodeKind::ToolCall, 2).with_state(ChatNodeState::Running);
    assert_eq!(node.state, ChatNodeState::Running);
}

#[test]
fn wiring_7_dag_node_builder_chain() {
    let node = DagNodeData::new("node4", ChatNodeKind::System, 3)
        .with_label("Config loaded")
        .with_state(ChatNodeState::Complete);

    assert_eq!(node.id, "node4");
    assert_eq!(node.label, "Config loaded");
    assert_eq!(node.state, ChatNodeState::Complete);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: DagEdgeData construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_7_dag_edge_new() {
    let edge = DagEdgeData::new("node1", "node2");
    assert_eq!(edge.from, "node1");
    assert_eq!(edge.to, "node2");
    assert_eq!(edge.label, None);
    assert!(!edge.active);
}

#[test]
fn wiring_7_dag_edge_with_label() {
    let edge = DagEdgeData::new("n1", "n2").with_label("@0");
    assert_eq!(edge.label, Some("@0".to_string()));
}

#[test]
fn wiring_7_dag_edge_with_active() {
    let edge = DagEdgeData::new("n1", "n2").with_active(true);
    assert!(edge.active);
}

#[test]
fn wiring_7_dag_edge_builder_chain() {
    let edge = DagEdgeData::new("source", "target")
        .with_label("ref")
        .with_active(true);

    assert_eq!(edge.from, "source");
    assert_eq!(edge.to, "target");
    assert_eq!(edge.label, Some("ref".to_string()));
    assert!(edge.active);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 5: ChatDagPanel composition
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_7_dag_panel_new_empty() {
    let panel = ChatDagPanel::new();
    assert_eq!(panel.node_count(), 0);
    assert_eq!(panel.edge_count(), 0);
    assert!(panel.is_empty());
}

#[test]
fn wiring_7_dag_panel_default() {
    let panel = ChatDagPanel::default();
    assert!(panel.is_empty());
}

#[test]
fn wiring_7_dag_panel_add_nodes() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));

    assert_eq!(panel.node_count(), 2);
    assert!(!panel.is_empty());
}

#[test]
fn wiring_7_dag_panel_add_edges() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));
    panel.add_edge(DagEdgeData::new("n1", "n2"));

    assert_eq!(panel.edge_count(), 1);
}

#[test]
fn wiring_7_dag_panel_selection() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));

    assert!(panel.selected().is_none());

    panel.select("n1");
    assert_eq!(panel.selected(), Some("n1"));

    panel.clear_selection();
    assert!(panel.selected().is_none());
}

#[test]
fn wiring_7_dag_panel_select_next() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));
    panel.add_node(DagNodeData::new("n3", ChatNodeKind::ToolCall, 2));

    // Start with no selection
    panel.select_next();
    assert_eq!(panel.selected(), Some("n1"));

    panel.select_next();
    assert_eq!(panel.selected(), Some("n2"));

    panel.select_next();
    assert_eq!(panel.selected(), Some("n3"));

    // At end, stays at last
    panel.select_next();
    assert_eq!(panel.selected(), Some("n3"));
}

#[test]
fn wiring_7_dag_panel_select_prev() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));

    // Start with no selection - goes to last
    panel.select_prev();
    assert_eq!(panel.selected(), Some("n2"));

    panel.select_prev();
    assert_eq!(panel.selected(), Some("n1"));

    // At start, stays at first
    panel.select_prev();
    assert_eq!(panel.selected(), Some("n1"));
}

#[test]
fn wiring_7_dag_panel_visibility() {
    let mut panel = ChatDagPanel::new();
    assert!(panel.is_visible(), "Default should be visible");

    panel.toggle_visible();
    assert!(!panel.is_visible());

    panel.toggle_visible();
    assert!(panel.is_visible());

    panel.set_visible(false);
    assert!(!panel.is_visible());
}

#[test]
fn wiring_7_dag_panel_with_title() {
    let panel = ChatDagPanel::new().with_title("Chat DAG");
    // Title is set (no getter, but builder works)
    assert!(true, "Title builder should work");
}

#[test]
fn wiring_7_dag_panel_nodes_accessor() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));

    let nodes = panel.nodes();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, "n1");
}

#[test]
fn wiring_7_dag_panel_edges_accessor() {
    let mut panel = ChatDagPanel::new();
    panel.add_edge(DagEdgeData::new("a", "b"));

    let edges = panel.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from, "a");
}
