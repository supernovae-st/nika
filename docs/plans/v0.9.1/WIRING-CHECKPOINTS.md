# Nika v0.9.1 — WIRING CHECKPOINTS

> **For Claude:** Run these tests AFTER completing each phase. WIRING = where components connect. Most bugs live here.

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                           ⚠️  CRITICAL WARNING  ⚠️                             ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║  WIRING bugs are the #1 source of integration failures.                       ║
║                                                                               ║
║  Each checkpoint verifies that components ACTUALLY CONNECT, not just          ║
║  that they exist in isolation. Unit tests pass ≠ integration works.           ║
║                                                                               ║
║  DO NOT SKIP THESE CHECKPOINTS. Run them before proceeding to next phase.     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

---

## Checkpoint Overview

| Checkpoint | After Phase | Verifies | Command |
|------------|-------------|----------|---------|
| WIRING-0 | v0.9.0 | StableDag ↔ Dag | `cargo test wiring_checkpoint_0` |
| WIRING-1 | v0.9.1 | Dag ↔ ChatWorkflow | `cargo test wiring_checkpoint_1` |
| WIRING-2 | v0.9.2 | ChatWorkflow ↔ @mention Bindings | `cargo test wiring_checkpoint_2` |
| WIRING-3 | v0.9.3 | BuiltinRouter ↔ Executor | `cargo test wiring_checkpoint_3` |
| WIRING-4 | v0.9.4 | ChatDagPanel ↔ EventLog | `cargo test wiring_checkpoint_4` |
| WIRING-5 | v0.9.5 | Session ↔ DAG Restore | `cargo test wiring_checkpoint_5` |

---

## WIRING-0: StableDag Foundation

**After:** v0.9.0 (StableGraph Migration)
**Verifies:** StableDag provides stable NodeIndex after deletion

```rust
// tests/wiring_checkpoint_0.rs

#[test]
fn wiring_checkpoint_0_stable_node_index() {
    use nika::dag::stable::StableDag;

    // Test 1: Create StableDag
    let mut graph: StableDag<String> = StableDag::new();

    // Test 2: Add nodes and track indices
    let idx1 = graph.add_node("Node 1".to_string());
    let idx2 = graph.add_node("Node 2".to_string());
    let idx3 = graph.add_node("Node 3".to_string());

    // Test 3: NodeIndex values are stable after removal
    graph.remove_node(idx2);

    // idx1 and idx3 should still be valid
    assert_eq!(graph.node_weight(idx1), Some(&"Node 1".to_string()));
    assert_eq!(graph.node_weight(idx3), Some(&"Node 3".to_string()));

    // idx2 is now invalid (removed)
    assert_eq!(graph.node_weight(idx2), None);

    // Test 4: Node count reflects removal
    assert_eq!(graph.node_count(), 2);
}

#[test]
fn wiring_checkpoint_0_stable_edges() {
    use nika::dag::stable::StableDag;

    let mut graph: StableDag<&str> = StableDag::new();

    let a = graph.add_node("A");
    let b = graph.add_node("B");
    let c = graph.add_node("C");

    // Add edges
    graph.add_edge(a, b);
    graph.add_edge(b, c);

    // Test: Edges exist
    assert!(graph.has_edge(a, b));
    assert!(graph.has_edge(b, c));
    assert!(!graph.has_edge(a, c));

    // Remove middle node
    graph.remove_node(b);

    // Edges involving b should be gone
    assert!(!graph.has_edge(a, b));
    assert!(!graph.has_edge(b, c));

    // But a and c still valid
    assert_eq!(graph.node_weight(a), Some(&"A"));
    assert_eq!(graph.node_weight(c), Some(&"C"));
}
```

**Run:**
```bash
cargo test wiring_checkpoint_0 --test wiring_checkpoint_0
```

**Expected:** All 2 tests pass.

---

## WIRING-1: StableDag ↔ ChatWorkflow

**After:** v0.9.1 (ChatWorkflow)
**Verifies:** ChatWorkflow wraps StableDag correctly, auto-edges work

```rust
// tests/wiring_checkpoint_1.rs

#[test]
fn wiring_checkpoint_1_flowgraph_to_chatworkflow() {
    use nika::runtime::chat_workflow::{ChatWorkflow, Role};

    // Test 1: ChatWorkflow wraps StableDag
    let mut chat = ChatWorkflow::new();

    // Test 2: add_message updates internal DAG and returns stable NodeIndex
    let node_idx = chat.add_message("Hello", Role::User);
    assert!(node_idx.index() >= 0);

    // Test 3: NodeIndex is stable (StableGraph guarantees)
    chat.add_message("Response", Role::Assistant);
    let original_idx = chat.get_index_by_number(1).unwrap();
    assert_eq!(original_idx, node_idx, "NodeIndex should be stable");

    // Test 4: Sequential edges are auto-created
    let idx2 = chat.get_index_by_number(2).unwrap();
    assert!(chat.dag.has_edge(node_idx, idx2), "Sequential edge should exist");

    // Test 5: message_counter increments via accessor
    assert_eq!(chat.current_message_number(), 2);
}

#[test]
fn wiring_checkpoint_1_thread_safety() {
    use std::sync::Arc;
    use parking_lot::Mutex;
    use nika::runtime::chat_workflow::{ChatWorkflow, Role};

    let workflow = Arc::new(Mutex::new(ChatWorkflow::new()));

    // Simulate concurrent access
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
        h.join().unwrap();
    }

    let guard = workflow.lock();
    assert_eq!(guard.message_count(), 10);
}
```

**Run:**
```bash
cargo test wiring_checkpoint_1 --test wiring_checkpoint_1
```

**Expected:** All 2 tests pass.

---

## WIRING-2: ChatWorkflow ↔ ChatAgent

**After:** Phase 2 (Binding System)
**Verifies:** @mentions are converted to valid WiringSpec bindings

```rust
// tests/wiring_checkpoint_2.rs

#[test]
fn wiring_checkpoint_2_mentions_to_wiring() {
    use nika::binding::mention::{parse_mentions, mentions_to_wiring, Mention};
    use nika::binding::WiringSpec;

    // Test 1: Parse @1 mention
    let text = "Summarize @1";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Number(1)]);

    // Test 2: Parse @last mention
    let text = "Continue from @last";
    let mentions = parse_mentions(text);
    assert_eq!(mentions, vec![Mention::Last]);

    // Test 3: Convert to WiringSpec
    let wiring = mentions_to_wiring("Based on @1 and @2", 5, Some("msg-004"));
    assert!(!wiring.use_entries.is_empty());
    assert!(wiring.use_entries.iter().any(|e| e.alias == "msg_001"));
    assert!(wiring.use_entries.iter().any(|e| e.alias == "msg_002"));

    // Test 4: Parallel marker (//) skips wiring
    let wiring = mentions_to_wiring("// Independent task", 5, Some("msg-004"));
    assert!(wiring.use_entries.is_empty());

    // Test 5: @all creates dependency on all previous
    let wiring = mentions_to_wiring("Summarize @all", 5, Some("msg-004"));
    assert_eq!(wiring.use_entries.len(), 4); // msg-001 through msg-004
}

#[test]
fn wiring_checkpoint_2_chat_agent_receives_bindings() {
    use nika::runtime::chat_workflow::{ChatWorkflow, Role};
    use nika::runtime::chat_agent::ChatAgent;

    let mut chat = ChatWorkflow::new();

    // Add messages
    chat.add_message("First message", Role::User);
    chat.add_message("Response to first", Role::Assistant);
    chat.add_message("Expand on @1", Role::User);

    // ChatAgent should see bindings from @1 mention
    let agent = ChatAgent::new(&chat);
    let task = agent.current_task().unwrap();

    // Verify bindings exist (from @1 mention in message 3)
    assert!(!task.wiring.use_entries.is_empty());
}
```

**Run:**
```bash
cargo test wiring_checkpoint_2 --test wiring_checkpoint_2
```

**Expected:** All 2 tests pass.

---

## WIRING-3: BuiltinRouter ↔ Executor

**After:** Phase 3 (Builtin Tools)
**Verifies:** `nika:*` prefix routes to builtin tools, others to MCP

```rust
// tests/wiring_checkpoint_3.rs

#[tokio::test]
async fn wiring_checkpoint_3_builtin_routing() {
    use nika::runtime::builtin::router::BuiltinToolRouter;
    use nika::event::EventLog;
    use nika::store::DataStore;

    let log = EventLog::new();
    let store = DataStore::new();
    let router = BuiltinToolRouter::new(log.clone(), store);

    // Test 1: nika: prefix is recognized
    assert!(BuiltinToolRouter::is_builtin("nika:sleep"));
    assert!(BuiltinToolRouter::is_builtin("nika:log"));
    assert!(!BuiltinToolRouter::is_builtin("novanet:describe"));

    // Test 2: All 6 tools registered
    assert!(router.has_tool("sleep"));
    assert!(router.has_tool("log"));
    assert!(router.has_tool("emit"));
    assert!(router.has_tool("assert"));
    assert!(router.has_tool("prompt"));
    assert!(router.has_tool("run"));

    // Test 3: Dispatch works
    let result = router.dispatch("nika:sleep", r#"{"duration": "1ms"}"#.to_string()).await;
    assert!(result.is_ok());

    // Test 4: Events are emitted
    let events = log.events();
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::BuiltinInvoke { .. })));
    assert!(events.iter().any(|e| matches!(&e.kind, EventKind::BuiltinResponse { .. })));
}

#[tokio::test]
async fn wiring_checkpoint_3_executor_integration() {
    use nika::runtime::Executor;
    use nika::ast::Workflow;

    let workflow = Workflow::from_yaml(r#"
schema: nika/workflow@0.5
workflow: wiring-test
tasks:
  - id: test_sleep
    invoke:
      tool: nika:sleep
      params:
        duration: "1ms"
  - id: test_log
    invoke:
      tool: nika:log
      params:
        level: info
        message: "Wiring test"
  - id: test_assert
    invoke:
      tool: nika:assert
      params:
        condition: "true"
"#).unwrap();

    let executor = Executor::new(workflow);
    let result = executor.run().await;

    assert!(result.is_ok(), "Builtin tools should execute via Executor");
}
```

**Run:**
```bash
cargo test wiring_checkpoint_3 --test wiring_checkpoint_3
```

**Expected:** All 2 tests pass.

---

## WIRING-4: ChatDagPanel ↔ EventLog Subscription

**After:** Phase 4 (DAG Panel)
**Verifies:** ChatDagPanel updates in real-time from EventLog

```rust
// tests/wiring_checkpoint_4.rs

#[test]
fn wiring_checkpoint_4_dag_panel_subscription() {
    use nika::tui::widgets::chat_dag_panel::{ChatDagPanel, NodeKind};
    use nika::tui::views::chat::ChatView;
    use nika::event::{EventLog, EventKind};
    use ratatui::layout::Rect;
    use ratatui::buffer::Buffer;

    // Test 1: ChatView has DAG panel
    let view = ChatView::new();
    assert!(view.dag_panel.is_some());
    assert!(view.show_dag_panel);

    // Test 2: Adding messages updates DAG
    let mut view = ChatView::new();
    view.add_message("Hello", Role::User);
    view.add_message("Hi!", Role::Assistant);

    let panel = view.dag_panel.as_ref().unwrap();
    assert_eq!(panel.nodes.len(), 2);
    assert!(!panel.edges.is_empty());

    // Test 3: Node selection works
    let mut view = ChatView::new();
    view.add_message("Msg 1", Role::User);
    view.add_message("Msg 2", Role::Assistant);
    view.dag_panel.as_mut().unwrap().select("msg-001");

    assert_eq!(view.dag_panel.as_ref().unwrap().selected, Some("msg-001".to_string()));

    // Test 4: Scroll sync works
    let mut view = ChatView::new();
    for i in 0..20 {
        view.add_message(&format!("Msg {}", i), Role::User);
    }
    view.dag_panel.as_mut().unwrap().select("msg-015");
    view.sync_scroll_from_dag();
    assert!(view.scroll_offset > 0);

    // Test 5: Toggle works
    let mut view = ChatView::new();
    assert!(view.show_dag_panel);
    view.toggle_dag_panel();
    assert!(!view.show_dag_panel);

    // Test 6: Render doesn't panic
    let view = ChatView::new();
    let area = Rect::new(0, 0, 120, 40);
    let mut buf = Buffer::empty(area);
    view.render(area, &mut buf);
}
```

**Run:**
```bash
cargo test wiring_checkpoint_4 --test wiring_checkpoint_4
```

**Expected:** All 1 test (with 6 assertions) passes.

---

## WIRING-5: Session ↔ DAG State Restore

**After:** Phase 5 (Polish)
**Verifies:** DAG persists and restores across sessions

```rust
// tests/wiring_checkpoint_5.rs

#[test]
fn wiring_checkpoint_5_session_persistence() {
    use tempfile::TempDir;
    use nika::tui::views::chat::ChatView;
    use nika::dag::export::{export_mermaid, export_json_pretty};

    let temp = TempDir::new().unwrap();

    // Test 1: Create session with DAG
    let mut view = ChatView::new()
        .with_session_dir(temp.path().to_path_buf());

    view.add_message("First message", Role::User);
    view.add_message("Response", Role::Assistant);
    view.add_message("Follow-up @1", Role::User);

    // Test 2: Save session
    view.save_session();
    let session_files: Vec<_> = std::fs::read_dir(temp.path())
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(!session_files.is_empty(), "Session file should exist");

    // Test 3: Restore session
    let mut restored = ChatView::new()
        .with_session_dir(temp.path().to_path_buf());
    restored.restore_session();

    let panel = restored.dag_panel.as_ref().unwrap();
    assert_eq!(panel.nodes.len(), 3, "DAG nodes should be restored");
    assert!(!panel.edges.is_empty(), "DAG edges should be restored");
    assert_eq!(restored.messages.len(), 3, "Messages should be restored");

    // Test 4: Export Mermaid
    let graph = restored.dag_panel.as_ref().unwrap().to_flow_graph();
    let mermaid = export_mermaid(&graph);
    assert!(mermaid.contains("graph TD"));
    assert!(mermaid.contains("-->"));

    // Test 5: Export JSON
    let json = export_json_pretty(&graph);
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(parsed["nodes"].is_array());
    assert_eq!(parsed["nodes"].as_array().unwrap().len(), 3);
}

#[test]
fn wiring_checkpoint_5_animations_tick() {
    use nika::tui::widgets::dag_node_box::{NodeBox, NodeKind, AnimationState};

    // Test 6: Node animation ticks
    let mut node = NodeBox::new("msg-001", NodeKind::UserMessage)
        .running(true);

    assert_eq!(node.animation_state, AnimationState::Pulsing);
    let initial = node.pulse_intensity();
    node.tick();
    let after = node.pulse_intensity();
    assert_ne!(initial, after, "Pulse should change on tick");
}
```

**Run:**
```bash
cargo test wiring_checkpoint_5 --test wiring_checkpoint_5
```

**Expected:** All 2 tests pass.

---

## Running All Checkpoints

```bash
# Run all wiring checkpoints
cargo test wiring_checkpoint --test 'wiring_checkpoint_*'

# Or individually after each phase:
cargo test wiring_checkpoint_1  # After Phase 1
cargo test wiring_checkpoint_2  # After Phase 2
cargo test wiring_checkpoint_3  # After Phase 3
cargo test wiring_checkpoint_4  # After Phase 4
cargo test wiring_checkpoint_5  # After Phase 5
```

---

## Common Wiring Bugs

| Bug | Symptom | Fix |
|-----|---------|-----|
| Missing Arc::clone | Data not shared | Use `Arc::clone(&x)` not `x.clone()` |
| Mutex not held | Race condition | Hold lock for entire operation |
| Missing event emit | TUI doesn't update | Add `emit()` call after state change |
| Wrong channel type | Compile error | Use `watch::Receiver` for broadcast |
| NodeIndex invalidated | Wrong node selected | Use StableGraph (Phase 1) |
| Binding not resolved | Empty value | Check `mentions_to_wiring()` output |
| Router prefix mismatch | Tool not found | Verify `nika:` vs `server:` prefix |

---

## Debugging Wiring Issues

```bash
# Enable trace logging
RUST_LOG=nika=trace cargo run -- chat

# Check event flow
cargo run -- trace list
cargo run -- trace show <id>

# Verify DAG state
cargo run -- debug dag --format json
```

---

## References

- [v0.9.0: StableGraph](./v0.9.0-StableGraph.md)
- [v0.9.1: ChatWorkflow](./v0.9.1-ChatWorkflow.md)
- [v0.9.2: Mention Bindings](./v0.9.2-MentionBindings.md)
- [v0.9.3: Builtin Tools](./v0.9.3-BuiltinTools.md)
- [v0.9.4: DAG Panel](./v0.9.4-DagPanel.md)
- [v0.9.5: Polish](./v0.9.5-Polish.md)
- [Master Plan](./README.md)
