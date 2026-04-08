// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Wiring Tests: Graph Foundation
//!
//! Consolidated from: wiring_checkpoint_0 (StableDag) + wiring_checkpoint_1 (ChatWorkflow)
//!
//! Verifies: StableDag provides stable NodeIndex, ChatWorkflow wraps it correctly.

use nika::dag::StableDag;
use nika::runtime::chat_workflow::{ChatWorkflow, Role};
use parking_lot::Mutex;
use std::sync::Arc;

// ═══════════════════════════════════════════════════════════════════════════
// StableDag Foundation (was WIRING-0)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stable_node_index() {
    let mut graph: StableDag<String> = StableDag::new();

    let idx1 = graph.add_node("Node 1".to_string());
    let idx2 = graph.add_node("Node 2".to_string());
    let idx3 = graph.add_node("Node 3".to_string());

    assert_eq!(graph.node_count(), 3);
    assert_eq!(idx1.index(), 0);
    assert_eq!(idx2.index(), 1);
    assert_eq!(idx3.index(), 2);

    graph.remove_node(idx2);

    assert_eq!(graph.node_weight(idx1), Some(&"Node 1".to_string()));
    assert_eq!(graph.node_weight(idx3), Some(&"Node 3".to_string()));
    assert_eq!(graph.node_weight(idx2), None);

    // CRITICAL: idx3 should still have index 2, NOT shift to 1
    assert_eq!(
        idx3.index(),
        2,
        "NodeIndex must remain stable after deletion"
    );
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn stable_edges() {
    let mut graph: StableDag<&str> = StableDag::new();

    let a = graph.add_node("A");
    let b = graph.add_node("B");
    let c = graph.add_node("C");

    graph.add_edge(a, b);
    graph.add_edge(b, c);

    assert!(graph.has_edge(a, b), "Edge A→B should exist");
    assert!(graph.has_edge(b, c), "Edge B→C should exist");
    assert!(!graph.has_edge(a, c), "Edge A→C should NOT exist");

    graph.remove_node(b);

    assert!(
        !graph.has_edge(a, b),
        "Edge A→B should be removed with node B"
    );
    assert!(
        !graph.has_edge(b, c),
        "Edge B→C should be removed with node B"
    );

    assert_eq!(graph.node_weight(a), Some(&"A"));
    assert_eq!(graph.node_weight(c), Some(&"C"));
    assert_eq!(a.index(), 0, "Node A index should be stable");
    assert_eq!(c.index(), 2, "Node C index should be stable");
}

#[test]
fn index_reuse() {
    let mut graph: StableDag<i32> = StableDag::new();

    let idx0 = graph.add_node(0);
    let idx1 = graph.add_node(1);
    let idx2 = graph.add_node(2);

    assert_eq!(graph.node_weight(idx1), Some(&1));

    graph.remove_node(idx1);
    assert_eq!(graph.node_weight(idx1), None);

    let idx_new = graph.add_node(100);

    // CRITICAL: Existing indices remain stable
    assert_eq!(graph.node_weight(idx0), Some(&0));
    assert_eq!(graph.node_weight(idx2), Some(&2));
    assert_eq!(graph.node_weight(idx_new), Some(&100));
    assert_eq!(idx0.index(), 0, "idx0 should remain at index 0");
    assert_eq!(idx2.index(), 2, "idx2 should remain at index 2");
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatWorkflow (was WIRING-1)
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn chatworkflow_uses_stablegraph() {
    let mut chat = ChatWorkflow::new();
    assert_eq!(chat.message_count(), 0);

    let idx1 = chat.add_message("Hello", Role::User);
    assert_eq!(idx1.index(), 0, "First node should have index 0");

    let msg = chat.get_message_by_index(idx1).unwrap();
    assert_eq!(msg.content, "Hello");
    assert_eq!(msg.role, Role::User);

    let msg_by_num = chat.get_message_by_number(1).unwrap();
    assert_eq!(msg_by_num.content, "Hello");

    let idx_lookup = chat.get_index_by_number(1).unwrap();
    assert_eq!(
        idx_lookup, idx1,
        "NodeIndex should be stable and retrievable"
    );
}

#[test]
fn auto_edge_creation() {
    let mut chat = ChatWorkflow::new();

    let idx1 = chat.add_message("User message 1", Role::User);
    let idx2 = chat.add_message("Assistant response", Role::Assistant);
    let idx3 = chat.add_message("User follow-up", Role::User);

    let deps_of_2 = chat.get_dependencies(idx2);
    assert!(
        deps_of_2.contains(&idx1),
        "msg-002 should depend on msg-001"
    );

    let deps_of_3 = chat.get_dependencies(idx3);
    assert!(
        deps_of_3.contains(&idx2),
        "msg-003 should depend on msg-002"
    );

    let deps_of_1 = chat.get_dependencies(idx1);
    assert!(deps_of_1.is_empty(), "msg-001 should have no dependencies");
}

#[test]
fn parallel_message() {
    let mut chat = ChatWorkflow::new();

    let idx1 = chat.add_message("First message", Role::User);
    let idx2 = chat.add_message_parallel("// Parallel task", Role::User);

    let deps = chat.get_dependencies(idx2);
    assert!(
        !deps.contains(&idx1),
        "Parallel message should not auto-depend on previous"
    );
    assert_eq!(chat.message_count(), 2);
}

#[test]
fn message_counter() {
    let mut chat = ChatWorkflow::new();

    assert_eq!(chat.current_message_number(), 0);

    chat.add_message("First", Role::User);
    assert_eq!(chat.current_message_number(), 1);

    chat.add_message("Second", Role::Assistant);
    assert_eq!(chat.current_message_number(), 2);

    chat.add_message("Third", Role::User);
    assert_eq!(chat.current_message_number(), 3);
}

#[test]
fn thread_safety() {
    let workflow = Arc::new(Mutex::new(ChatWorkflow::new()));

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let wf = Arc::clone(&workflow);
            std::thread::spawn(move || {
                let mut guard = wf.lock();
                guard.add_message(&format!("Message {}", i), Role::User);
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Thread should complete");
    }

    let guard = workflow.lock();
    assert_eq!(
        guard.message_count(),
        10,
        "All concurrent messages should be added"
    );
}

#[test]
fn last_message() {
    let mut chat = ChatWorkflow::new();

    assert!(chat.last_message().is_none());
    assert!(chat.last_message_index().is_none());

    chat.add_message("First", Role::User);
    chat.add_message("Last", Role::Assistant);

    let last = chat.last_message().unwrap();
    assert_eq!(last.content, "Last");
    assert_eq!(last.role, Role::Assistant);

    let last_idx = chat.last_message_index().unwrap();
    assert_eq!(last_idx.index(), 1);
}
