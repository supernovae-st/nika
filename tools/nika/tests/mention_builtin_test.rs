// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Binding Tests: @mention + BuiltinToolRouter
//!
//! Verifies: @mentions convert to BindingSpec, builtin tools route correctly.

use nika::binding::mention::{
    has_parallel_marker, mentions_to_bindings, parse_mentions, resolve_mention, text_to_bindings,
    Mention, ResolvedMention,
};
use nika::runtime::builtin::BuiltinToolRouter;
use nika::runtime::chat_workflow::{ChatWorkflow, Role};

// ═══════════════════════════════════════════════════════════════════════════
// @mention Parsing
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn parse_number_mention() {
    let text = "Summarize @1";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Number(1)]);

    let text = "Based on @1 and @2";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Number(1), Mention::Number(2)]);
}

#[test]
fn parse_last_mention() {
    let text = "Continue from @last";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Last]);
}

#[test]
fn parse_all_mention() {
    let text = "Summarize @all";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::All]);
}

#[test]
fn parse_range_mention() {
    let text = "Combine @1..3";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Range { start: 1, end: 3 }]);
}

#[test]
fn resolve_mentions() {
    let resolved = resolve_mention(&Mention::Last, 5).unwrap();
    assert_eq!(resolved, ResolvedMention::Single(5));

    let resolved = resolve_mention(&Mention::Number(2), 5).unwrap();
    assert_eq!(resolved, ResolvedMention::Single(2));

    let resolved = resolve_mention(&Mention::All, 4).unwrap();
    assert_eq!(resolved, ResolvedMention::Multiple(vec![1, 2, 3, 4]));

    let resolved = resolve_mention(&Mention::Range { start: 1, end: 3 }, 5).unwrap();
    assert_eq!(resolved, ResolvedMention::Multiple(vec![1, 2, 3]));
}

#[test]
fn mentions_to_binding_spec() {
    let spec = mentions_to_bindings(&ResolvedMention::Single(2));
    assert!(spec.contains_key("ref_2"));
    assert_eq!(spec["ref_2"].path, "msg-002.output");

    let spec = mentions_to_bindings(&ResolvedMention::Multiple(vec![1, 2, 3]));
    assert_eq!(spec.len(), 3);
    assert_eq!(spec["ref_1"].path, "msg-001.output");
    assert_eq!(spec["ref_2"].path, "msg-002.output");
    assert_eq!(spec["ref_3"].path, "msg-003.output");

    let spec = mentions_to_bindings(&ResolvedMention::Empty);
    assert!(spec.is_empty());
}

#[test]
fn text_to_bindings_end_to_end() {
    let spec = text_to_bindings("Based on @1 and @2", 5).unwrap();
    assert_eq!(spec.len(), 2);
    assert!(spec.contains_key("ref_1"));
    assert!(spec.contains_key("ref_2"));

    let spec = text_to_bindings("Summarize @all", 4).unwrap();
    assert_eq!(spec.len(), 4);
}

#[test]
fn parallel_marker() {
    assert!(has_parallel_marker("// Independent task"));
    assert!(has_parallel_marker("  // Also parallel"));
    assert!(!has_parallel_marker("Normal message"));
    assert!(!has_parallel_marker("@1 Reference"));

    let text = "// Independent task with @1 ref";
    assert!(has_parallel_marker(text));
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatWorkflow + @mention Integration
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn chatworkflow_mentions() {
    let mut chat = ChatWorkflow::new();

    chat.add_message("First message", Role::User);
    chat.add_message("Response to first", Role::Assistant);
    let idx3 = chat.add_message("Expand on @1", Role::User);

    let mentions = chat.get_mentions(idx3);
    assert_eq!(mentions, vec![Mention::Number(1)]);

    let spec = chat.get_bindings_for_message(idx3).unwrap();
    assert!(
        spec.contains_key("ref_1"),
        "Bindings should have ref_1 from @1 mention"
    );
    assert_eq!(spec["ref_1"].path, "msg-001.output");
}

#[test]
fn chatworkflow_edges_from_mentions() {
    let mut chat = ChatWorkflow::new();

    chat.add_message("First message", Role::User);
    chat.add_message("Second message", Role::Assistant);
    let idx3 = chat.add_message("Reference @1", Role::User);

    let edges_added = chat.add_edges_from_mentions(idx3).unwrap();
    assert_eq!(edges_added, 1, "Should add edge from @1 to message 3");

    let idx1 = chat.get_index_by_number(1).unwrap();
    assert!(
        chat.dag.has_edge(idx1, idx3),
        "Edge from msg-001 to msg-003 should exist"
    );
}

#[test]
fn parallel_message_no_auto_edge() {
    let mut chat = ChatWorkflow::new();

    chat.add_message("First message", Role::User);
    let idx2 = chat.add_message_parallel("// Independent task", Role::User);

    let idx1 = chat.get_index_by_number(1).unwrap();
    assert!(
        !chat.dag.has_edge(idx1, idx2),
        "Parallel message should not auto-depend on previous"
    );
    assert_eq!(chat.message_count(), 2);
}

#[test]
fn all_creates_dependencies() {
    let mut chat = ChatWorkflow::new();

    chat.add_message("Message 1", Role::User);
    chat.add_message("Message 2", Role::Assistant);
    chat.add_message("Message 3", Role::User);
    let idx4 = chat.add_message("Summarize @all", Role::User);

    let edges_added = chat.add_edges_from_mentions(idx4).unwrap();
    assert_eq!(
        edges_added, 3,
        "@all with 4 messages should add 3 edges (1,2,3 -> 4)"
    );

    for n in 1..=3 {
        let source_idx = chat.get_index_by_number(n).unwrap();
        assert!(
            chat.dag.has_edge(source_idx, idx4),
            "Edge from msg-{:03} to msg-004 should exist",
            n
        );
    }
}

#[test]
fn mixed_mentions() {
    let text = "Based on @1, @last, and @all";
    let mentions = parse_mentions(text);
    assert_eq!(
        mentions,
        vec![Mention::Number(1), Mention::Last, Mention::All]
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// BuiltinToolRouter
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn is_builtin() {
    assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
    assert!(BuiltinToolRouter::is_builtin("nika:log"));
    assert!(BuiltinToolRouter::is_builtin("nika:emit"));
    assert!(BuiltinToolRouter::is_builtin("nika:assert"));
    assert!(BuiltinToolRouter::is_builtin("nika:prompt"));
    assert!(BuiltinToolRouter::is_builtin("nika:run"));

    assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));
    assert!(!BuiltinToolRouter::is_builtin("perplexity:search"));
    assert!(!BuiltinToolRouter::is_builtin("sleep"));
    assert!(!BuiltinToolRouter::is_builtin(""));
}

#[test]
fn all_tools_registered() {
    let router = BuiltinToolRouter::new();

    assert!(router.has_tool("sleep"), "sleep tool should be registered");
    assert!(router.has_tool("log"), "log tool should be registered");
    assert!(router.has_tool("emit"), "emit tool should be registered");
    assert!(
        router.has_tool("assert"),
        "assert tool should be registered"
    );
    assert!(
        router.has_tool("prompt"),
        "prompt tool should be registered"
    );
    assert!(router.has_tool("run"), "run tool should be registered");

    let tool_names = router.tool_names();
    assert_eq!(tool_names.len(), 7, "Should have exactly 7 builtin tools");
}

#[test]
fn extract_name() {
    assert_eq!(BuiltinToolRouter::extract_name("nika:sleep"), Some("sleep"));
    assert_eq!(BuiltinToolRouter::extract_name("nika:log"), Some("log"));
    assert_eq!(BuiltinToolRouter::extract_name("nika:emit"), Some("emit"));
    assert_eq!(
        BuiltinToolRouter::extract_name("nika:assert"),
        Some("assert")
    );
    assert_eq!(
        BuiltinToolRouter::extract_name("nika:prompt"),
        Some("prompt")
    );
    assert_eq!(BuiltinToolRouter::extract_name("nika:run"), Some("run"));

    assert_eq!(BuiltinToolRouter::extract_name("novanet:describe"), None);
    assert_eq!(BuiltinToolRouter::extract_name("sleep"), None);
    assert_eq!(BuiltinToolRouter::extract_name(""), None);
}

#[tokio::test]
async fn dispatch_sleep() {
    let router = BuiltinToolRouter::new();

    let result = router
        .dispatch("nika:sleep", r#"{"duration": "1ms"}"#.to_string())
        .await;

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert!(
        response["slept_for_ms"].is_number(),
        "Should have slept_for_ms"
    );
}

#[tokio::test]
async fn dispatch_log() {
    let router = BuiltinToolRouter::new();

    let result = router
        .dispatch(
            "nika:log",
            r#"{"level": "info", "message": "Binding test"}"#.to_string(),
        )
        .await;

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["logged"], true);
    assert_eq!(response["level"], "info");
    assert_eq!(response["message"], "Binding test");
}

#[tokio::test]
async fn dispatch_emit() {
    let router = BuiltinToolRouter::new();

    let result = router
        .dispatch(
            "nika:emit",
            r#"{"name": "test_event", "payload": {"key": "value"}}"#.to_string(),
        )
        .await;

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["emitted"], true);
    assert_eq!(response["name"], "test_event");
}

#[tokio::test]
async fn dispatch_assert_pass() {
    let router = BuiltinToolRouter::new();

    let result = router
        .dispatch(
            "nika:assert",
            r#"{"condition": true, "message": "Should pass"}"#.to_string(),
        )
        .await;

    let response: serde_json::Value = serde_json::from_str(&result.unwrap()).unwrap();
    assert_eq!(response["passed"], true);
}

#[tokio::test]
async fn dispatch_assert_fail() {
    let router = BuiltinToolRouter::new();

    let result = router
        .dispatch(
            "nika:assert",
            r#"{"condition": false, "message": "Should fail"}"#.to_string(),
        )
        .await;

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Should fail"),
        "Error should contain the assert message"
    );
}

#[tokio::test]
async fn dispatch_non_builtin_fails() {
    let router = BuiltinToolRouter::new();

    let err = router
        .dispatch("novanet:describe", r#"{}"#.to_string())
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Not a builtin tool"),
        "Error should indicate not a builtin tool"
    );
}

#[tokio::test]
async fn dispatch_unknown_builtin_fails() {
    let router = BuiltinToolRouter::new();

    let err = router
        .dispatch("nika:unknown", r#"{}"#.to_string())
        .await
        .unwrap_err();

    assert!(
        err.to_string().contains("Unknown builtin tool"),
        "Error should indicate unknown builtin tool"
    );
}

#[tokio::test]
async fn dispatch_run_validation() {
    use std::io::Write;
    use tempfile::NamedTempFile;

    let router = BuiltinToolRouter::new();

    let mut temp_file = NamedTempFile::with_suffix(".nika.yaml").unwrap();
    writeln!(
        temp_file,
        r#"schema: nika/workflow@0.12
workflow: binding-test
tasks:
  - id: hello
    exec: "echo binding""#
    )
    .unwrap();

    let result = router
        .dispatch(
            "nika:run",
            format!(
                r#"{{"workflow": "{}"}}"#,
                temp_file.path().to_str().unwrap()
            ),
        )
        .await;
    assert!(
        result.is_ok(),
        "nika:run with valid workflow should succeed"
    );

    let result = router
        .dispatch(
            "nika:run",
            r#"{"workflow": "path/to/nonexistent.nika.yaml"}"#.to_string(),
        )
        .await;
    assert!(
        result.is_err(),
        "nika:run with non-existent file should fail"
    );

    let result = router
        .dispatch("nika:run", r#"{"workflow": "test.yaml"}"#.to_string())
        .await;
    assert!(
        result.is_err(),
        "nika:run with invalid extension should fail"
    );

    let result = router
        .dispatch("nika:run", r#"{"workflow": ""}"#.to_string())
        .await;
    assert!(result.is_err(), "nika:run with empty path should fail");
}

#[tokio::test]
async fn dispatch_prompt_validation() {
    let router = BuiltinToolRouter::new();

    let result = router.dispatch("nika:prompt", r#"{}"#.to_string()).await;
    assert!(result.is_err(), "nika:prompt without message should fail");
}

#[test]
fn router_default() {
    let router = BuiltinToolRouter::default();

    assert_eq!(router.tool_names().len(), 7);
    assert!(router.has_tool("sleep"));
    assert!(router.has_tool("log"));
}
