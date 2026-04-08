// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Integration test for StableDag (v0.9.0)
//!
//! Tests the core invariant: NodeIndex values remain stable after node deletion.
//! This is the foundation for @mention references in Chat-as-DAG architecture.

use nika::StableDag;

/// Core invariant: indices remain stable after deletion
///
/// ```text
/// Before deletion:
/// ├── Node 0: "msg-001"
/// ├── Node 1: "msg-002"  ← DELETE THIS
/// └── Node 2: "msg-003"
///
/// After deletion:
/// ├── Node 0: "msg-001"  ← STILL index 0
/// ├── Node 1: DELETED    ← Gap in indices
/// └── Node 2: "msg-003"  ← STILL index 2 (NOT shifted to 1)
/// ```
#[test]
fn test_stable_index_after_deletion_invariant() {
    let mut graph: StableDag<String> = StableDag::new();

    // Add 3 nodes
    let idx0 = graph.add_node("msg-001".to_string());
    let idx1 = graph.add_node("msg-002".to_string());
    let idx2 = graph.add_node("msg-003".to_string());

    // Verify initial indices
    assert_eq!(idx0.index(), 0);
    assert_eq!(idx1.index(), 1);
    assert_eq!(idx2.index(), 2);
    assert_eq!(graph.node_count(), 3);

    // Delete middle node
    let removed = graph.remove_node(idx1);
    assert_eq!(removed, Some("msg-002".to_string()));

    // CRITICAL: Indices MUST NOT shift
    assert_eq!(idx0.index(), 0, "idx0 should remain 0");
    assert_eq!(idx2.index(), 2, "idx2 should remain 2, NOT shift to 1");

    // Node count decreases
    assert_eq!(graph.node_count(), 2);

    // Weights accessible by original indices
    assert_eq!(graph.node_weight(idx0), Some(&"msg-001".to_string()));
    assert_eq!(graph.node_weight(idx1), None); // Deleted
    assert_eq!(graph.node_weight(idx2), Some(&"msg-003".to_string()));
}

/// Test edges survive node deletion for unrelated nodes
#[test]
fn test_edges_survive_unrelated_node_deletion() {
    let mut graph: StableDag<String> = StableDag::new();

    let a = graph.add_node("A".to_string());
    let b = graph.add_node("B".to_string());
    let c = graph.add_node("C".to_string());
    let d = graph.add_node("D".to_string());

    // Create edge A -> B
    graph.add_edge(a, b);
    assert!(graph.has_edge(a, b));

    // Delete unrelated nodes C and D
    graph.remove_node(c);
    graph.remove_node(d);

    // Edge A -> B should still exist
    assert!(
        graph.has_edge(a, b),
        "Edge should survive unrelated deletion"
    );
    assert_eq!(graph.edge_count(), 1);
}

/// Test new nodes fill gaps from deleted nodes
#[test]
fn test_new_nodes_reuse_deleted_indices() {
    let mut graph: StableDag<String> = StableDag::new();

    let idx0 = graph.add_node("first".to_string());
    let idx1 = graph.add_node("second".to_string());

    // Delete second node
    graph.remove_node(idx1);

    // Add new node - should reuse index 1
    let idx_new = graph.add_node("third".to_string());

    // The new node gets the reused index
    assert_eq!(idx_new.index(), 1, "New node should reuse deleted index");
    assert_eq!(graph.node_weight(idx_new), Some(&"third".to_string()));

    // Original node still at index 0
    assert_eq!(graph.node_weight(idx0), Some(&"first".to_string()));
}

/// Test DagEdge with labeled edges
#[test]
fn test_flow_edge_with_label() {
    let mut graph: StableDag<String> = StableDag::new();

    let parent = graph.add_node("parent".to_string());
    let child = graph.add_node("child".to_string());

    // Add labeled edge (for @mention references)
    let edge = graph.add_edge_with_label(parent, child, "replies_to");
    assert!(edge.is_some());

    assert!(graph.has_edge(parent, child));
    assert!(!graph.has_edge(child, parent)); // Directed
}

/// Test serialization roundtrip
#[test]
fn test_stable_graph_serialization_roundtrip() {
    let mut graph: StableDag<String> = StableDag::new();

    let a = graph.add_node("node-a".to_string());
    let b = graph.add_node("node-b".to_string());
    graph.add_edge(a, b);

    // Serialize
    let json = serde_json::to_string(&graph).expect("serialize");

    // Deserialize
    let restored: StableDag<String> = serde_json::from_str(&json).expect("deserialize");

    // Verify structure preserved
    assert_eq!(restored.node_count(), 2);
    assert_eq!(restored.edge_count(), 1);
}

/// Test with struct node weights (simulating ChatMessage)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct ChatMessage {
    id: String,
    role: String,
    content: String,
}

#[test]
fn test_stable_graph_with_struct_weights() {
    let mut graph: StableDag<ChatMessage> = StableDag::new();

    let msg1 = ChatMessage {
        id: "msg-001".to_string(),
        role: "user".to_string(),
        content: "Hello".to_string(),
    };

    let msg2 = ChatMessage {
        id: "msg-002".to_string(),
        role: "assistant".to_string(),
        content: "Hi there!".to_string(),
    };

    let idx1 = graph.add_node(msg1.clone());
    let idx2 = graph.add_node(msg2);

    // Add dependency edge (assistant replies to user)
    graph.add_edge(idx1, idx2);

    // Verify
    assert_eq!(
        graph.node_weight(idx1).map(|m| &m.id),
        Some(&"msg-001".to_string())
    );
    assert!(graph.has_edge(idx1, idx2));
}
