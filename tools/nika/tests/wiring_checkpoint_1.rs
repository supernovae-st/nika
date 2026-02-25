//! WIRING-1: StableDag ↔ ChatWorkflow
//!
//! Verifies: ChatWorkflow wraps StableDag correctly, auto-edges work.
//! Run after: v0.9.1 (ChatWorkflow)

use nika::runtime::chat_workflow::{ChatWorkflow, Role};
use parking_lot::Mutex;
use std::sync::Arc;

/// Test that ChatWorkflow correctly wraps StableDag.
#[test]
fn wiring_checkpoint_1_chatworkflow_uses_stablegraph() {
    // Test 1: ChatWorkflow creates with empty DAG
    let mut chat = ChatWorkflow::new();
    assert_eq!(chat.message_count(), 0);

    // Test 2: add_message returns stable NodeIndex (first node at index 0)
    let idx1 = chat.add_message("Hello", Role::User);
    assert_eq!(idx1.index(), 0, "First node should have index 0");

    // Test 3: Message is retrievable by index
    let msg = chat.get_message_by_index(idx1).unwrap();
    assert_eq!(msg.content, "Hello");
    assert_eq!(msg.role, Role::User);

    // Test 4: Message number mapping works
    let msg_by_num = chat.get_message_by_number(1).unwrap();
    assert_eq!(msg_by_num.content, "Hello");

    // Test 5: Index lookup by number
    let idx_lookup = chat.get_index_by_number(1).unwrap();
    assert_eq!(
        idx_lookup, idx1,
        "NodeIndex should be stable and retrievable"
    );
}

/// Test that sequential edges are auto-created.
#[test]
fn wiring_checkpoint_1_auto_edge_creation() {
    let mut chat = ChatWorkflow::new();

    // Add sequential messages
    let idx1 = chat.add_message("User message 1", Role::User);
    let idx2 = chat.add_message("Assistant response", Role::Assistant);
    let idx3 = chat.add_message("User follow-up", Role::User);

    // Sequential edges should exist (1 → 2 → 3)
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

    // First message has no dependencies
    let deps_of_1 = chat.get_dependencies(idx1);
    assert!(deps_of_1.is_empty(), "msg-001 should have no dependencies");
}

/// Test that parallel messages skip auto-edge.
#[test]
fn wiring_checkpoint_1_parallel_message() {
    let mut chat = ChatWorkflow::new();

    let idx1 = chat.add_message("First message", Role::User);
    let idx2 = chat.add_message_parallel("// Parallel task", Role::User);

    // Parallel message should NOT depend on previous
    let deps = chat.get_dependencies(idx2);
    assert!(
        !deps.contains(&idx1),
        "Parallel message should not auto-depend on previous"
    );

    // But both messages exist
    assert_eq!(chat.message_count(), 2);
}

/// Test message counter increments correctly.
#[test]
fn wiring_checkpoint_1_message_counter() {
    let mut chat = ChatWorkflow::new();

    assert_eq!(chat.current_message_number(), 0);

    chat.add_message("First", Role::User);
    assert_eq!(chat.current_message_number(), 1);

    chat.add_message("Second", Role::Assistant);
    assert_eq!(chat.current_message_number(), 2);

    chat.add_message("Third", Role::User);
    assert_eq!(chat.current_message_number(), 3);
}

/// Test thread-safety with parking_lot::Mutex.
#[test]
fn wiring_checkpoint_1_thread_safety() {
    let workflow = Arc::new(Mutex::new(ChatWorkflow::new()));

    // Spawn 10 threads, each adding a message
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let wf = Arc::clone(&workflow);
            std::thread::spawn(move || {
                let mut guard = wf.lock();
                guard.add_message(&format!("Message {}", i), Role::User);
            })
        })
        .collect();

    // Wait for all threads
    for h in handles {
        h.join().expect("Thread should complete");
    }

    // Verify all messages added
    let guard = workflow.lock();
    assert_eq!(
        guard.message_count(),
        10,
        "All concurrent messages should be added"
    );
}

/// Test last_message accessors.
#[test]
fn wiring_checkpoint_1_last_message() {
    let mut chat = ChatWorkflow::new();

    // Empty workflow
    assert!(chat.last_message().is_none());
    assert!(chat.last_message_index().is_none());

    // After adding messages
    chat.add_message("First", Role::User);
    chat.add_message("Last", Role::Assistant);

    let last = chat.last_message().unwrap();
    assert_eq!(last.content, "Last");
    assert_eq!(last.role, Role::Assistant);

    let last_idx = chat.last_message_index().unwrap();
    assert_eq!(last_idx.index(), 1);
}
