// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wiring Tests: UI Widgets
//!
//! Consolidated from: wiring_checkpoint_4 (ChatDagPanel basics) +
//!                     wiring_checkpoint_5 (Session persistence) +
//!                     wiring_checkpoint_7 (Chat DAG Panel advanced)
//!
//! Verifies: ChatDagPanel, ChatNodeKind, ChatNodeState, DagNodeData,
//!           DagEdgeData, and Session infrastructure.

use nika::tui::session::{delete_session, get_latest_session, list_sessions};
use nika::tui::widgets::{ChatDagPanel, ChatNodeKind, ChatNodeState, DagEdgeData, DagNodeData};

// ═══════════════════════════════════════════════════════════════════════════
// ChatDagPanel Basics (was WIRING-4)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dag_panel_creation() {
    let mut panel = ChatDagPanel::new();

    // Add nodes representing chat messages
    panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1).with_label("Hello"));
    panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2).with_label("Hi!"));

    // Add edge for sequential flow
    panel.add_edge(DagEdgeData::new("msg-001", "msg-002"));

    assert_eq!(panel.node_count(), 2);
    assert_eq!(panel.edge_count(), 1);
}

#[test]
fn dag_panel_selection() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
    panel.add_node(DagNodeData::new("msg-002", ChatNodeKind::Assistant, 2));

    panel.select("msg-001");
    assert_eq!(panel.selected(), Some("msg-001"));

    panel.select_next();
    assert_eq!(panel.selected(), Some("msg-002"));

    panel.select_prev();
    assert_eq!(panel.selected(), Some("msg-001"));
}

#[test]
fn dag_panel_state_updates() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::ToolCall, 1));

    assert_eq!(panel.nodes()[0].state, ChatNodeState::Idle);

    panel.update_node_state("msg-001", ChatNodeState::Running);
    assert_eq!(panel.nodes()[0].state, ChatNodeState::Running);

    panel.update_node_state("msg-001", ChatNodeState::Complete);
    assert_eq!(panel.nodes()[0].state, ChatNodeState::Complete);
}

#[test]
fn dag_panel_visibility() {
    let mut panel = ChatDagPanel::new();

    assert!(panel.is_visible());
    panel.toggle_visible();
    assert!(!panel.is_visible());
    panel.toggle_visible();
    assert!(panel.is_visible());
}

#[test]
fn dag_panel_scroll() {
    let mut panel = ChatDagPanel::new();

    assert_eq!(panel.scroll_offset(), 0);
    panel.scroll_down(10);
    assert_eq!(panel.scroll_offset(), 1);
    panel.scroll_up();
    assert_eq!(panel.scroll_offset(), 0);
}

#[test]
fn dag_panel_clear() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("msg-001", ChatNodeKind::User, 1));
    panel.add_edge(DagEdgeData::new("msg-001", "msg-002"));
    panel.select("msg-001");

    panel.clear();

    assert!(panel.is_empty());
    assert_eq!(panel.edge_count(), 0);
    assert!(panel.selected().is_none());
}

#[test]
fn dag_panel_animation() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(
        DagNodeData::new("msg-001", ChatNodeKind::ToolCall, 1).with_state(ChatNodeState::Running),
    );

    // Tick animation — verify tick doesn't panic
    panel.tick();
    assert!(panel.node_count() > 0);
}

// ═══════════════════════════════════════════════════════════════════════════
// Session Persistence (was WIRING-5)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn session_list() {
    // Should return Ok even if no sessions exist
    let sessions = list_sessions().expect("list_sessions should not fail");
    // Result is a valid Vec (may be empty)
    let _ = sessions.len();
}

#[test]
fn session_get_latest() {
    let result = get_latest_session();
    // Should succeed (returns None if no sessions)
    assert!(result.is_ok());
}

#[test]
fn session_delete_nonexistent() {
    let err = delete_session("nonexistent-session-id-12345").unwrap_err();
    // Should be an error (file not found) but not panic
    let _ = err.to_string();
}

#[test]
fn session_meta_fields() {
    let sessions = list_sessions().unwrap_or_default();

    // If any sessions exist, verify the structure
    if let Some(meta) = sessions.first() {
        assert!(!meta.id.is_empty());
        let _title = &meta.title;
        let _count = meta.message_count;
    }
}

#[test]
fn session_infrastructure() {
    // Verify session module is accessible
    let _ = list_sessions();
    let _ = get_latest_session();
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatNodeKind (was WIRING-7)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn node_kind_user() {
    assert_eq!(ChatNodeKind::User.icon(), "👤");
}

#[test]
fn node_kind_assistant() {
    assert_eq!(ChatNodeKind::Assistant.icon(), "🤖");
}

#[test]
fn node_kind_tool_call() {
    assert_eq!(ChatNodeKind::ToolCall.icon(), "🔌");
}

#[test]
fn node_kind_system() {
    assert_eq!(ChatNodeKind::System.icon(), "⚙️");
}

#[test]
fn node_kind_error() {
    assert_eq!(ChatNodeKind::Error.icon(), "❌");
}

#[test]
fn node_kind_all_five() {
    let all = ChatNodeKind::all();
    assert_eq!(all.len(), 5, "Should have 5 node kinds");
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatNodeState (was WIRING-7)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn node_state_idle_default() {
    let state = ChatNodeState::default();
    assert_eq!(state, ChatNodeState::Idle);
    assert!(!state.is_running());
}

#[test]
fn node_state_running() {
    assert!(ChatNodeState::Running.is_running());
}

#[test]
fn node_state_complete() {
    assert!(!ChatNodeState::Complete.is_running());
}

#[test]
fn node_state_failed() {
    assert!(!ChatNodeState::Failed.is_running());
}

// ═══════════════════════════════════════════════════════════════════════════
// DagNodeData (was WIRING-7)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dag_node_new() {
    let node = DagNodeData::new("node1", ChatNodeKind::User, 0);
    assert_eq!(node.id, "node1");
    assert_eq!(node.kind, ChatNodeKind::User);
    assert_eq!(node.index, 0);
    assert_eq!(node.state, ChatNodeState::Idle);
}

#[test]
fn dag_node_with_label() {
    let node = DagNodeData::new("node2", ChatNodeKind::Assistant, 1).with_label("Hello!");
    assert_eq!(node.label, "Hello!");
}

#[test]
fn dag_node_with_state() {
    let node =
        DagNodeData::new("node3", ChatNodeKind::ToolCall, 2).with_state(ChatNodeState::Running);
    assert_eq!(node.state, ChatNodeState::Running);
}

#[test]
fn dag_node_builder_chain() {
    let node = DagNodeData::new("node4", ChatNodeKind::System, 3)
        .with_label("Config loaded")
        .with_state(ChatNodeState::Complete);

    assert_eq!(node.id, "node4");
    assert_eq!(node.label, "Config loaded");
    assert_eq!(node.state, ChatNodeState::Complete);
}

// ═══════════════════════════════════════════════════════════════════════════
// DagEdgeData (was WIRING-7)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dag_edge_new() {
    let edge = DagEdgeData::new("node1", "node2");
    assert_eq!(edge.from, "node1");
    assert_eq!(edge.to, "node2");
    assert_eq!(edge.label, None);
    assert!(!edge.active);
}

#[test]
fn dag_edge_with_label() {
    let edge = DagEdgeData::new("n1", "n2").with_label("@0");
    assert_eq!(edge.label, Some("@0".to_string()));
}

#[test]
fn dag_edge_with_active() {
    let edge = DagEdgeData::new("n1", "n2").with_active(true);
    assert!(edge.active);
}

#[test]
fn dag_edge_builder_chain() {
    let edge = DagEdgeData::new("source", "target")
        .with_label("ref")
        .with_active(true);

    assert_eq!(edge.from, "source");
    assert_eq!(edge.to, "target");
    assert_eq!(edge.label, Some("ref".to_string()));
    assert!(edge.active);
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatDagPanel Advanced (was WIRING-7)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn dag_panel_new_empty() {
    let panel = ChatDagPanel::new();
    assert_eq!(panel.node_count(), 0);
    assert_eq!(panel.edge_count(), 0);
    assert!(panel.is_empty());
}

#[test]
fn dag_panel_default() {
    let panel = ChatDagPanel::default();
    assert!(panel.is_empty());
}

#[test]
fn dag_panel_add_nodes() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));

    assert_eq!(panel.node_count(), 2);
    assert!(!panel.is_empty());
}

#[test]
fn dag_panel_add_edges() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));
    panel.add_node(DagNodeData::new("n2", ChatNodeKind::Assistant, 1));
    panel.add_edge(DagEdgeData::new("n1", "n2"));

    assert_eq!(panel.edge_count(), 1);
}

#[test]
fn dag_panel_selection_advanced() {
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
fn dag_panel_select_next_cycles() {
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
fn dag_panel_select_prev_cycles() {
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
fn dag_panel_visibility_advanced() {
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
fn dag_panel_with_title() {
    let _panel = ChatDagPanel::new().with_title("Chat DAG");
    // Title builder works — reaching here means success
}

#[test]
fn dag_panel_nodes_accessor() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(DagNodeData::new("n1", ChatNodeKind::User, 0));

    let nodes = panel.nodes();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, "n1");
}

#[test]
fn dag_panel_edges_accessor() {
    let mut panel = ChatDagPanel::new();
    panel.add_edge(DagEdgeData::new("a", "b"));

    let edges = panel.edges();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].from, "a");
}
