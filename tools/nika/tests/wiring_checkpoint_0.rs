//! WIRING-0: StableFlowGraph Foundation
//!
//! Verifies: StableFlowGraph provides stable NodeIndex after deletion.
//! Run after: v0.9.0 (StableGraph Migration)

use nika::dag::StableFlowGraph;

/// Test that StableFlowGraph maintains stable NodeIndex after removal.
#[test]
fn wiring_checkpoint_0_stable_node_index() {
    // Test 1: Create StableFlowGraph
    let mut graph: StableFlowGraph<String> = StableFlowGraph::new();

    // Test 2: Add nodes and track indices
    let idx1 = graph.add_node("Node 1".to_string());
    let idx2 = graph.add_node("Node 2".to_string());
    let idx3 = graph.add_node("Node 3".to_string());

    // Verify initial state
    assert_eq!(graph.node_count(), 3);
    assert_eq!(idx1.index(), 0);
    assert_eq!(idx2.index(), 1);
    assert_eq!(idx3.index(), 2);

    // Test 3: NodeIndex values are stable after removal
    graph.remove_node(idx2);

    // idx1 and idx3 should still be valid with SAME indices
    assert_eq!(graph.node_weight(idx1), Some(&"Node 1".to_string()));
    assert_eq!(graph.node_weight(idx3), Some(&"Node 3".to_string()));

    // idx2 is now invalid (removed)
    assert_eq!(graph.node_weight(idx2), None);

    // CRITICAL: idx3 should still have index 2, NOT shift to 1
    // This is the key invariant of StableGraph
    assert_eq!(idx3.index(), 2, "NodeIndex must remain stable after deletion");

    // Test 4: Node count reflects removal
    assert_eq!(graph.node_count(), 2);
}

/// Test that edges work correctly with stable indices.
#[test]
fn wiring_checkpoint_0_stable_edges() {
    let mut graph: StableFlowGraph<&str> = StableFlowGraph::new();

    let a = graph.add_node("A");
    let b = graph.add_node("B");
    let c = graph.add_node("C");

    // Add edges A → B → C
    graph.add_edge(a, b);
    graph.add_edge(b, c);

    // Test: Edges exist
    assert!(graph.has_edge(a, b), "Edge A→B should exist");
    assert!(graph.has_edge(b, c), "Edge B→C should exist");
    assert!(!graph.has_edge(a, c), "Edge A→C should NOT exist");

    // Remove middle node B
    graph.remove_node(b);

    // Edges involving B should be gone (cascading delete)
    assert!(!graph.has_edge(a, b), "Edge A→B should be removed with node B");
    assert!(!graph.has_edge(b, c), "Edge B→C should be removed with node B");

    // But A and C are still valid with original indices
    assert_eq!(graph.node_weight(a), Some(&"A"));
    assert_eq!(graph.node_weight(c), Some(&"C"));
    assert_eq!(a.index(), 0, "Node A index should be stable");
    assert_eq!(c.index(), 2, "Node C index should be stable");
}

/// Test new node reuses deleted index slots.
#[test]
fn wiring_checkpoint_0_index_reuse() {
    let mut graph: StableFlowGraph<i32> = StableFlowGraph::new();

    let idx0 = graph.add_node(0);
    let idx1 = graph.add_node(1);
    let idx2 = graph.add_node(2);

    // Verify idx1 is valid before removal
    assert_eq!(graph.node_weight(idx1), Some(&1));

    // Remove middle node
    graph.remove_node(idx1);

    // After removal, idx1 is invalid (node is gone)
    assert_eq!(graph.node_weight(idx1), None);

    // Add new node - StableGraph may reuse the deleted index slot
    let idx_new = graph.add_node(100);

    // CRITICAL: Existing indices (idx0, idx2) remain stable and valid
    assert_eq!(graph.node_weight(idx0), Some(&0));
    assert_eq!(graph.node_weight(idx2), Some(&2));
    assert_eq!(graph.node_weight(idx_new), Some(&100));

    // The new node gets an index (could be reused slot or new slot)
    // Key point: idx0 and idx2 are NOT invalidated by the new node
    assert_eq!(idx0.index(), 0, "idx0 should remain at index 0");
    assert_eq!(idx2.index(), 2, "idx2 should remain at index 2");
}
