//! WIRING-2: ChatWorkflow <-> @mention Bindings
//!
//! Verifies: @mentions are converted to valid WiringSpec bindings, ChatWorkflow integrates with mention parsing.
//! Run after: v0.9.2 (Mention Bindings)

use nika::binding::mention::{
    has_parallel_marker, mentions_to_wiring, parse_mentions, resolve_mention, text_to_wiring,
    Mention, ResolvedMention,
};
use nika::runtime::chat_workflow::{ChatWorkflow, Role};

/// Test that @N mentions are parsed correctly.
#[test]
fn wiring_checkpoint_2_parse_number_mention() {
    // Test 1: Parse @1 mention
    let text = "Summarize @1";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Number(1)]);

    // Test 2: Parse multiple @N mentions
    let text = "Based on @1 and @2";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Number(1), Mention::Number(2)]);
}

/// Test that @last mention is parsed correctly.
#[test]
fn wiring_checkpoint_2_parse_last_mention() {
    let text = "Continue from @last";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Last]);
}

/// Test that @all mention is parsed correctly.
#[test]
fn wiring_checkpoint_2_parse_all_mention() {
    let text = "Summarize @all";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::All]);
}

/// Test that @N..M range mentions are parsed correctly.
#[test]
fn wiring_checkpoint_2_parse_range_mention() {
    let text = "Combine @1..3";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Range { start: 1, end: 3 }]);
}

/// Test mention resolution to indices.
#[test]
fn wiring_checkpoint_2_resolve_mentions() {
    // @last with 5 messages -> @5
    let resolved = resolve_mention(&Mention::Last, 5).unwrap();
    assert_eq!(resolved, ResolvedMention::Single(5));

    // @2 with 5 messages -> @2
    let resolved = resolve_mention(&Mention::Number(2), 5).unwrap();
    assert_eq!(resolved, ResolvedMention::Single(2));

    // @all with 4 messages -> [1, 2, 3, 4]
    let resolved = resolve_mention(&Mention::All, 4).unwrap();
    assert_eq!(resolved, ResolvedMention::Multiple(vec![1, 2, 3, 4]));

    // @1..3 -> [1, 2, 3]
    let resolved = resolve_mention(&Mention::Range { start: 1, end: 3 }, 5).unwrap();
    assert_eq!(resolved, ResolvedMention::Multiple(vec![1, 2, 3]));
}

/// Test that resolved mentions convert to WiringSpec.
#[test]
fn wiring_checkpoint_2_mentions_to_wiring() {
    // Single reference @2
    let wiring = mentions_to_wiring(&ResolvedMention::Single(2));
    assert!(wiring.contains_key("ref_2"));
    assert_eq!(wiring["ref_2"].path, "msg-002.output");

    // Multiple references @1..3
    let wiring = mentions_to_wiring(&ResolvedMention::Multiple(vec![1, 2, 3]));
    assert_eq!(wiring.len(), 3);
    assert!(wiring.contains_key("ref_1"));
    assert!(wiring.contains_key("ref_2"));
    assert!(wiring.contains_key("ref_3"));
    assert_eq!(wiring["ref_1"].path, "msg-001.output");
    assert_eq!(wiring["ref_2"].path, "msg-002.output");
    assert_eq!(wiring["ref_3"].path, "msg-003.output");

    // Empty resolution -> empty wiring
    let wiring = mentions_to_wiring(&ResolvedMention::Empty);
    assert!(wiring.is_empty());
}

/// Test text_to_wiring end-to-end conversion.
#[test]
fn wiring_checkpoint_2_text_to_wiring() {
    // Based on @1 and @2 with 5 messages
    let wiring = text_to_wiring("Based on @1 and @2", 5).unwrap();
    assert_eq!(wiring.len(), 2);
    assert!(wiring.contains_key("ref_1"));
    assert!(wiring.contains_key("ref_2"));

    // @all with 4 messages
    let wiring = text_to_wiring("Summarize @all", 4).unwrap();
    assert_eq!(wiring.len(), 4);
    assert!(wiring.contains_key("ref_1"));
    assert!(wiring.contains_key("ref_2"));
    assert!(wiring.contains_key("ref_3"));
    assert!(wiring.contains_key("ref_4"));
}

/// Test parallel marker detection and skipping.
#[test]
fn wiring_checkpoint_2_parallel_marker() {
    // Parallel marker is detected
    assert!(has_parallel_marker("// Independent task"));
    assert!(has_parallel_marker("  // Also parallel"));
    assert!(!has_parallel_marker("Normal message"));
    assert!(!has_parallel_marker("@1 Reference"));

    // Parallel messages don't get wiring from content (they're independent)
    // Note: The text_to_wiring function still parses mentions, but the
    // ChatWorkflow.add_message_parallel() skips auto-edge creation
    let text = "// Independent task with @1 ref";
    assert!(has_parallel_marker(text));
}

/// Test ChatWorkflow integration with @mention parsing.
#[test]
fn wiring_checkpoint_2_chatworkflow_mentions() {
    let mut chat = ChatWorkflow::new();

    // Add messages
    chat.add_message("First message", Role::User);
    chat.add_message("Response to first", Role::Assistant);
    let idx3 = chat.add_message("Expand on @1", Role::User);

    // Get mentions from message 3
    let mentions = chat.get_mentions(idx3);
    assert_eq!(mentions, vec![Mention::Number(1)]);

    // Get wiring spec for message 3
    let wiring = chat.get_wiring_for_message(idx3).unwrap();
    assert!(
        wiring.contains_key("ref_1"),
        "Wiring should have ref_1 from @1 mention"
    );
    assert_eq!(wiring["ref_1"].path, "msg-001.output");
}

/// Test ChatWorkflow creates edges from @mentions.
#[test]
fn wiring_checkpoint_2_chatworkflow_edges_from_mentions() {
    let mut chat = ChatWorkflow::new();

    // Add messages
    chat.add_message("First message", Role::User);
    chat.add_message("Second message", Role::Assistant);
    let idx3 = chat.add_message("Reference @1", Role::User);

    // Add edges from mentions
    let edges_added = chat.add_edges_from_mentions(idx3).unwrap();
    assert_eq!(edges_added, 1, "Should add edge from @1 to message 3");

    // Verify the edge exists (msg-001 -> msg-003)
    let idx1 = chat.get_index_by_number(1).unwrap();
    assert!(
        chat.dag.has_edge(idx1, idx3),
        "Edge from msg-001 to msg-003 should exist"
    );
}

/// Test parallel messages skip auto-edge.
#[test]
fn wiring_checkpoint_2_parallel_message_no_auto_edge() {
    let mut chat = ChatWorkflow::new();

    chat.add_message("First message", Role::User);
    let idx2 = chat.add_message_parallel("// Independent task", Role::User);

    // Parallel message should NOT have auto-edge from previous
    let idx1 = chat.get_index_by_number(1).unwrap();
    assert!(
        !chat.dag.has_edge(idx1, idx2),
        "Parallel message should not auto-depend on previous"
    );

    // Both messages exist
    assert_eq!(chat.message_count(), 2);
}

/// Test @all creates dependencies on all previous messages.
#[test]
fn wiring_checkpoint_2_all_creates_dependencies() {
    let mut chat = ChatWorkflow::new();

    // Add 4 messages
    chat.add_message("Message 1", Role::User);
    chat.add_message("Message 2", Role::Assistant);
    chat.add_message("Message 3", Role::User);
    let idx4 = chat.add_message("Summarize @all", Role::User);

    // Add edges from @all mention
    let edges_added = chat.add_edges_from_mentions(idx4).unwrap();
    assert_eq!(
        edges_added, 3,
        "@all with 4 messages should add 3 edges (1,2,3 -> 4)"
    );

    // Verify all edges exist
    for n in 1..=3 {
        let source_idx = chat.get_index_by_number(n).unwrap();
        assert!(
            chat.dag.has_edge(source_idx, idx4),
            "Edge from msg-{:03} to msg-004 should exist",
            n
        );
    }
}

/// Test mixed mentions (@1 and @last).
#[test]
fn wiring_checkpoint_2_mixed_mentions() {
    let text = "Based on @1, @last, and @all";
    let mentions = parse_mentions(text);
    assert_eq!(
        mentions,
        vec![Mention::Number(1), Mention::Last, Mention::All]
    );
}
