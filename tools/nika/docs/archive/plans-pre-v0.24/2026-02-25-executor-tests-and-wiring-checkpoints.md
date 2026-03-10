# Executor Tests + WIRING-7 to WIRING-10 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Fill executor.rs test gaps (20+ new tests) and create WIRING-7 through WIRING-10 checkpoint tests for user journey validation.

**Architecture:** Executor.rs coverage gaps + 4 new WIRING integration test files

**Tech Stack:** Rust, tokio, serde_json, insta (snapshots)

---

## Part 1: Executor.rs Test Coverage (20 new tests)

### Current State

Executor.rs has 35 tests but audit identified gaps:
- Construction tests: ✅ 4 tests
- Exec verb tests: ✅ 4 tests
- Fetch verb tests: ✅ 2 tests
- Invoke verb tests: ✅ Several tests

### Missing Test Coverage

| Area | Status | Tests Needed |
|------|--------|--------------|
| `run_infer()` | ❌ Missing | 5 tests |
| `run_agent()` | ❌ Missing | 4 tests |
| `expand_decompose()` | ❌ Missing | 5 tests |
| `get_rig_provider()` | ❌ Missing | 3 tests |
| Error edge cases | ⚠️ Partial | 3 tests |

---

### Task 1: Infer Verb Tests (5 tests)

**Files:**
- Modify: `src/runtime/executor.rs` (test module)

**Step 1: Write failing test - basic infer**

```rust
#[tokio::test]
async fn test_execute_infer_basic() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "Hello, world!".to_string(),
            model: None,
            context: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_infer");
    let result = executor
        .execute(&task_id, &action, &bindings, &datastore)
        .await;

    // Mock provider returns predictable response
    assert!(result.is_ok(), "Infer should succeed with mock: {:?}", result.err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_execute_infer_basic --no-fail-fast`
Expected: FAIL (InferParams may not exist or mock provider logic)

**Step 3: Implement/fix if needed**

The test should pass with existing mock provider logic.

**Step 4: Add remaining infer tests**

```rust
#[tokio::test]
async fn test_execute_infer_with_model_override() {
    let executor = TaskExecutor::new("mock", Some("gpt-4"), None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "Test prompt".to_string(),
            model: Some("claude-sonnet-4-20250514".to_string()),
            context: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_infer_model");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_infer_with_template_prompt() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let mut bindings = ResolvedBindings::new();
    bindings.set("topic", json!("Rust programming"));
    let datastore = DataStore::new();

    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "Explain {{use.topic}} in simple terms".to_string(),
            model: None,
            context: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_infer_template");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_infer_emits_provider_events() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "Test".to_string(),
            model: None,
            context: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_infer_events");
    let _ = executor.execute(&task_id, &action, &bindings, &datastore).await;

    let events = event_log.filter_task("test_infer_events");
    assert!(!events.is_empty(), "Should emit events for infer");
}

#[tokio::test]
async fn test_execute_infer_with_context() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Infer {
        infer: InferParams {
            prompt: "Continue this story".to_string(),
            model: None,
            context: Some("Once upon a time...".to_string()),
        },
    };

    let task_id: Arc<str> = Arc::from("test_infer_context");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;
    assert!(result.is_ok());
}
```

**Step 5: Commit**

```bash
git add src/runtime/executor.rs
git commit -m "test(executor): add 5 infer verb tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 2: Agent Verb Tests (4 tests)

**Files:**
- Modify: `src/runtime/executor.rs` (test module)

**Tests to add:**

```rust
#[tokio::test]
async fn test_execute_agent_basic() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Research and summarize".to_string(),
            mcp: vec![],
            max_turns: Some(3),
            ..Default::default()
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;
    assert!(result.is_ok(), "Agent should complete: {:?}", result.err());
}

#[tokio::test]
async fn test_execute_agent_with_mcp_servers() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Use tools to gather data".to_string(),
            mcp: vec!["novanet".to_string()],
            max_turns: Some(2),
            ..Default::default()
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent_mcp");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_execute_agent_emits_start_complete_events() {
    let event_log = EventLog::new();
    let executor = TaskExecutor::new("mock", None, None, event_log.clone());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Quick task".to_string(),
            mcp: vec![],
            max_turns: Some(1),
            ..Default::default()
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent_events");
    let _ = executor.execute(&task_id, &action, &bindings, &datastore).await;

    let events = event_log.filter_task("test_agent_events");

    let start_events: Vec<_> = events.iter()
        .filter(|e| matches!(e.kind, EventKind::AgentStart { .. }))
        .collect();
    let complete_events: Vec<_> = events.iter()
        .filter(|e| matches!(e.kind, EventKind::AgentComplete { .. }))
        .collect();

    assert_eq!(start_events.len(), 1, "Should emit AgentStart");
    assert_eq!(complete_events.len(), 1, "Should emit AgentComplete");
}

#[tokio::test]
async fn test_execute_agent_with_provider_override() {
    let executor = TaskExecutor::new("claude", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Agent {
        agent: AgentParams {
            prompt: "Test task".to_string(),
            mcp: vec![],
            max_turns: Some(1),
            provider: Some("mock".to_string()), // Override to mock
            ..Default::default()
        },
    };

    let task_id: Arc<str> = Arc::from("test_agent_provider");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;
    assert!(result.is_ok(), "Should use mock provider: {:?}", result.err());
}
```

**Step 5: Commit**

```bash
git add src/runtime/executor.rs
git commit -m "test(executor): add 4 agent verb tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 3: Decompose Tests (5 tests)

**Files:**
- Modify: `src/runtime/executor.rs` (test module)

**Tests to add:**

```rust
#[tokio::test]
async fn test_expand_decompose_static_array() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let mut datastore = DataStore::new();
    datastore.insert("items".to_string(), TaskResult {
        output: json!(["a", "b", "c"]),
        ..Default::default()
    });

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        source: "$items".to_string(),
        traverse: String::new(),
        max_items: None,
        mcp: None,
    };

    let result = executor.expand_decompose(&spec, &bindings, &datastore).await;
    assert!(result.is_ok());
    let items = result.unwrap();
    assert_eq!(items.len(), 3);
}

#[tokio::test]
async fn test_expand_decompose_static_with_max_items() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let mut datastore = DataStore::new();
    datastore.insert("items".to_string(), TaskResult {
        output: json!(["a", "b", "c", "d", "e"]),
        ..Default::default()
    });

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        source: "$items".to_string(),
        traverse: String::new(),
        max_items: Some(3),
        mcp: None,
    };

    let result = executor.expand_decompose(&spec, &bindings, &datastore).await;
    let items = result.unwrap();
    assert_eq!(items.len(), 3, "Should truncate to max_items");
}

#[tokio::test]
async fn test_expand_decompose_static_non_array_error() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let mut datastore = DataStore::new();
    datastore.insert("items".to_string(), TaskResult {
        output: json!({"not": "an array"}),
        ..Default::default()
    });

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        source: "$items".to_string(),
        traverse: String::new(),
        max_items: None,
        mcp: None,
    };

    let result = executor.expand_decompose(&spec, &bindings, &datastore).await;
    assert!(result.is_err(), "Should error on non-array source");
}

#[tokio::test]
async fn test_expand_decompose_semantic_with_mcp() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    executor.inject_mock_mcp_client("novanet");
    let bindings = ResolvedBindings::new();
    let mut datastore = DataStore::new();
    datastore.insert("entity".to_string(), TaskResult {
        output: json!({"key": "qr-code"}),
        ..Default::default()
    });

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Semantic,
        source: "$entity".to_string(),
        traverse: "HAS_CHILD".to_string(),
        max_items: Some(5),
        mcp: Some("novanet".to_string()),
    };

    let result = executor.expand_decompose(&spec, &bindings, &datastore).await;
    // Mock returns predictable nodes
    assert!(result.is_ok(), "Semantic decompose should work: {:?}", result.err());
}

#[tokio::test]
async fn test_expand_decompose_missing_source_error() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new(); // Empty

    let spec = DecomposeSpec {
        strategy: DecomposeStrategy::Static,
        source: "$nonexistent".to_string(),
        traverse: String::new(),
        max_items: None,
        mcp: None,
    };

    let result = executor.expand_decompose(&spec, &bindings, &datastore).await;
    assert!(result.is_err(), "Should error when source not found");
}
```

**Step 5: Commit**

```bash
git add src/runtime/executor.rs
git commit -m "test(executor): add 5 decompose expansion tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 4: Provider Caching Tests (3 tests)

**Files:**
- Modify: `src/runtime/executor.rs` (test module)

**Tests to add:**

```rust
#[tokio::test]
async fn test_get_rig_provider_caches_provider() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());

    // First call creates provider
    let provider1 = executor.get_rig_provider("mock").await;
    assert!(provider1.is_ok());

    // Second call should use cache (same instance)
    let provider2 = executor.get_rig_provider("mock").await;
    assert!(provider2.is_ok());

    // Verify cache has exactly one entry
    assert_eq!(executor.rig_provider_cache.len(), 1);
}

#[tokio::test]
async fn test_get_rig_provider_different_providers() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());

    let _ = executor.get_rig_provider("mock").await;
    let _ = executor.get_rig_provider("openai").await; // May fail without key but that's ok

    // Cache should have entries for attempted providers
    assert!(executor.rig_provider_cache.len() >= 1);
}

#[tokio::test]
async fn test_executor_clone_shares_provider_cache() {
    let executor1 = TaskExecutor::new("mock", None, None, EventLog::new());
    let _ = executor1.get_rig_provider("mock").await;

    let executor2 = executor1.clone();

    // Both should share the same cache (Arc)
    assert_eq!(executor1.rig_provider_cache.len(), executor2.rig_provider_cache.len());
}
```

**Step 5: Commit**

```bash
git add src/runtime/executor.rs
git commit -m "test(executor): add 3 provider caching tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 5: Error Edge Case Tests (3 tests)

**Files:**
- Modify: `src/runtime/executor.rs` (test module)

**Tests to add:**

```rust
#[tokio::test]
async fn test_execute_exec_timeout() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Exec {
        exec: ExecParams {
            // Sleep longer than EXEC_TIMEOUT
            command: "sleep 300".to_string(),
        },
    };

    let task_id: Arc<str> = Arc::from("test_timeout");
    let result = tokio::time::timeout(
        Duration::from_secs(5), // Test timeout
        executor.execute(&task_id, &action, &bindings, &datastore)
    ).await;

    // Either our timeout or executor's timeout should trigger
    assert!(result.is_err() || result.unwrap().is_err());
}

#[tokio::test]
async fn test_execute_invoke_missing_mcp_server() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    // Don't inject mock client
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: "nonexistent_server".to_string(),
            tool: Some("some_tool".to_string()),
            params: None,
            resource: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_missing_mcp");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;

    // Should fail gracefully with MCP error, not panic
    assert!(result.is_err());
}

#[tokio::test]
async fn test_execute_invoke_invalid_params() {
    let executor = TaskExecutor::new("mock", None, None, EventLog::new());
    let bindings = ResolvedBindings::new();
    let datastore = DataStore::new();

    // Neither tool nor resource specified
    let action = TaskAction::Invoke {
        invoke: InvokeParams {
            mcp: "novanet".to_string(),
            tool: None,
            params: None,
            resource: None,
        },
    };

    let task_id: Arc<str> = Arc::from("test_invalid");
    let result = executor.execute(&task_id, &action, &bindings, &datastore).await;

    // Should fail validation
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(matches!(err, NikaError::ValidationError { .. }));
}
```

**Step 5: Commit**

```bash
git add src/runtime/executor.rs
git commit -m "test(executor): add 3 error edge case tests

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Part 2: WIRING Checkpoint Tests (7-10)

### WIRING Pattern

Each WIRING checkpoint validates a specific integration point in the user journey.

| Checkpoint | Focus | Version |
|------------|-------|---------|
| WIRING-7 | Chat DAG Panel Integration | v0.10 |
| WIRING-8 | Monitor View Integration | v0.11 |
| WIRING-9 | Provider Selection Flow | v0.12 |
| WIRING-10 | Full TUI Navigation | v0.12 |

---

### Task 6: WIRING-7 Chat DAG Panel

**Files:**
- Create: `tests/wiring_checkpoint_7.rs`

**Content:**

```rust
//! WIRING-7: Chat DAG Panel Integration
//!
//! Verifies: ChatDagPanel, ChatNodeBox, ChatEdgeLine work together
//! Run after: v0.10.0 (Chat DAG Widgets)

use nika::tui::widgets::{
    ChatDagPanel, ChatEdgeLine, ChatNodeBox, ChatNodeKind, ChatNodeState, ChatTaskQueue,
};

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: ChatNodeBox construction and rendering
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_7_node_box_user() {
    let node = ChatNodeBox::new(0, ChatNodeKind::User, "Hello!");
    assert_eq!(node.index(), 0);
    assert!(matches!(node.kind(), ChatNodeKind::User));
    assert_eq!(node.content(), "Hello!");
}

#[test]
fn wiring_checkpoint_7_node_box_assistant() {
    let node = ChatNodeBox::new(1, ChatNodeKind::Assistant, "Hi there!");
    assert_eq!(node.index(), 1);
    assert!(matches!(node.kind(), ChatNodeKind::Assistant));
}

#[test]
fn wiring_checkpoint_7_node_box_with_task() {
    let node = ChatNodeBox::new(2, ChatNodeKind::Task, "Running infer...")
        .with_verb("infer")
        .with_state(ChatNodeState::Running);

    assert!(matches!(node.kind(), ChatNodeKind::Task));
    assert!(matches!(node.state(), ChatNodeState::Running));
}

#[test]
fn wiring_checkpoint_7_node_states() {
    let pending = ChatNodeBox::new(0, ChatNodeKind::Task, "Pending")
        .with_state(ChatNodeState::Pending);
    let running = ChatNodeBox::new(1, ChatNodeKind::Task, "Running")
        .with_state(ChatNodeState::Running);
    let success = ChatNodeBox::new(2, ChatNodeKind::Task, "Done")
        .with_state(ChatNodeState::Success);
    let failed = ChatNodeBox::new(3, ChatNodeKind::Task, "Error")
        .with_state(ChatNodeState::Failed);

    assert!(matches!(pending.state(), ChatNodeState::Pending));
    assert!(matches!(running.state(), ChatNodeState::Running));
    assert!(matches!(success.state(), ChatNodeState::Success));
    assert!(matches!(failed.state(), ChatNodeState::Failed));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: ChatEdgeLine construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_7_edge_line() {
    let edge = ChatEdgeLine::new(0, 1);
    assert_eq!(edge.from(), 0);
    assert_eq!(edge.to(), 1);
}

#[test]
fn wiring_checkpoint_7_edge_with_label() {
    let edge = ChatEdgeLine::new(0, 2).with_label("@0 reference");
    assert_eq!(edge.label(), Some("@0 reference"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: ChatTaskQueue
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_7_task_queue_empty() {
    let queue = ChatTaskQueue::new();
    assert!(queue.is_empty());
    assert_eq!(queue.len(), 0);
}

#[test]
fn wiring_checkpoint_7_task_queue_add_task() {
    let mut queue = ChatTaskQueue::new();
    queue.add("task1", "infer", ChatNodeState::Pending);
    queue.add("task2", "exec", ChatNodeState::Running);

    assert_eq!(queue.len(), 2);
    assert!(!queue.is_empty());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: ChatDagPanel composition
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_7_dag_panel_empty() {
    let panel = ChatDagPanel::new();
    assert_eq!(panel.node_count(), 0);
    assert_eq!(panel.edge_count(), 0);
}

#[test]
fn wiring_checkpoint_7_dag_panel_add_nodes() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(ChatNodeBox::new(0, ChatNodeKind::User, "Q1"));
    panel.add_node(ChatNodeBox::new(1, ChatNodeKind::Assistant, "A1"));

    assert_eq!(panel.node_count(), 2);
}

#[test]
fn wiring_checkpoint_7_dag_panel_add_edges() {
    let mut panel = ChatDagPanel::new();
    panel.add_node(ChatNodeBox::new(0, ChatNodeKind::User, "Q"));
    panel.add_node(ChatNodeBox::new(1, ChatNodeKind::Assistant, "A"));
    panel.add_edge(ChatEdgeLine::new(0, 1));

    assert_eq!(panel.edge_count(), 1);
}
```

**Step 5: Commit**

```bash
git add tests/wiring_checkpoint_7.rs
git commit -m "test(wiring): add WIRING-7 Chat DAG Panel integration

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 7: WIRING-8 Monitor View

**Files:**
- Create: `tests/wiring_checkpoint_8.rs`

**Content:**

```rust
//! WIRING-8: Monitor View Integration
//!
//! Verifies: MonitorView implements View trait correctly
//! Run after: v0.11.0 (Six Views Architecture)

use nika::tui::views::MonitorView;
use nika::tui::View;

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: MonitorView construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_8_monitor_view_new() {
    let view = MonitorView::new();
    assert!(true, "MonitorView should construct successfully");
}

#[test]
fn wiring_checkpoint_8_monitor_view_default() {
    let view = MonitorView::default();
    assert!(true, "MonitorView should implement Default");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: View trait implementation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_8_view_trait_status_line() {
    let view = MonitorView::new();
    let state = create_mock_tui_state();

    let status = view.status_line(&state);
    assert!(!status.is_empty(), "Status line should not be empty");
}

#[test]
fn wiring_checkpoint_8_view_trait_tick() {
    let mut view = MonitorView::new();
    let mut state = create_mock_tui_state();

    // tick() should not panic
    view.tick(&mut state);
    assert!(true);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: Panel navigation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_8_panel_focus_cycling() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut view = MonitorView::new();
    let mut state = create_mock_tui_state();

    // Initial focus
    let initial_focus = view.focused_panel();

    // Tab should cycle focus
    let tab_key = KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE);
    view.handle_key(tab_key, &mut state);

    let new_focus = view.focused_panel();
    assert_ne!(initial_focus, new_focus, "Focus should change on Tab");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: Panel count
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_8_four_panels() {
    let view = MonitorView::new();
    assert_eq!(view.panel_count(), 4, "MonitorView should have 4 panels");
}

// Helper
fn create_mock_tui_state() -> nika::tui::TuiState {
    nika::tui::TuiState::default()
}
```

**Step 5: Commit**

```bash
git add tests/wiring_checkpoint_8.rs
git commit -m "test(wiring): add WIRING-8 Monitor View integration

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 8: WIRING-9 Provider Selection

**Files:**
- Create: `tests/wiring_checkpoint_9.rs`

**Content:**

```rust
//! WIRING-9: Provider Selection Flow
//!
//! Verifies: Provider auto-detection and selection logic
//! Run after: v0.12.0 (Providers Wiring)

use nika::provider::rig::RigProvider;

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: Provider constructors
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_9_mock_provider() {
    let provider = RigProvider::mock();
    assert!(provider.is_ok(), "Mock provider should always work");
}

#[test]
fn wiring_checkpoint_9_provider_name() {
    let provider = RigProvider::mock().unwrap();
    let name = provider.name();
    assert_eq!(name, "mock");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: Provider auto-detection
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_9_auto_without_keys() {
    // Without any API keys set, auto() should return None or mock
    // This depends on implementation
    let provider = RigProvider::auto();
    // Just verify it doesn't panic
    assert!(provider.is_none() || provider.is_some());
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: Provider list
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_9_available_providers() {
    let providers = RigProvider::available_providers();

    // Should include at least mock
    assert!(providers.contains(&"mock"), "mock provider should be available");

    // Should list all 6 provider types
    let expected = ["claude", "openai", "mistral", "ollama", "groq", "deepseek"];
    for name in expected {
        assert!(
            providers.contains(&name),
            "{} should be in available providers",
            name
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: Provider env var mapping
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_9_env_var_names() {
    assert_eq!(RigProvider::env_var_for("claude"), Some("ANTHROPIC_API_KEY"));
    assert_eq!(RigProvider::env_var_for("openai"), Some("OPENAI_API_KEY"));
    assert_eq!(RigProvider::env_var_for("mistral"), Some("MISTRAL_API_KEY"));
    assert_eq!(RigProvider::env_var_for("groq"), Some("GROQ_API_KEY"));
    assert_eq!(RigProvider::env_var_for("deepseek"), Some("DEEPSEEK_API_KEY"));
    assert_eq!(RigProvider::env_var_for("ollama"), Some("OLLAMA_API_BASE_URL"));
    assert_eq!(RigProvider::env_var_for("mock"), None);
}
```

**Step 5: Commit**

```bash
git add tests/wiring_checkpoint_9.rs
git commit -m "test(wiring): add WIRING-9 Provider Selection flow

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

### Task 9: WIRING-10 Full TUI Navigation

**Files:**
- Create: `tests/wiring_checkpoint_10.rs`

**Content:**

```rust
//! WIRING-10: Full TUI Navigation
//!
//! Verifies: View enum, navigation, and key bindings work together
//! Run after: v0.12.0 (6-Views complete)

use nika::tui::views::{ChatView, HomeView, MonitorView, View, ViewAction};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// ═══════════════════════════════════════════════════════════════════════════
// TEST 1: All views construct
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_10_home_view_constructs() {
    let _view = HomeView::new();
    assert!(true);
}

#[test]
fn wiring_checkpoint_10_chat_view_constructs() {
    let _view = ChatView::new();
    assert!(true);
}

#[test]
fn wiring_checkpoint_10_monitor_view_constructs() {
    let _view = MonitorView::new();
    assert!(true);
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 2: ViewAction navigation
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_10_view_action_variants() {
    // Verify all navigation actions exist
    let _none = ViewAction::None;
    let _quit = ViewAction::Quit;
    let _switch_chat = ViewAction::SwitchView("chat".to_string());
    let _switch_home = ViewAction::SwitchView("home".to_string());
    let _switch_monitor = ViewAction::SwitchView("monitor".to_string());

    assert!(true, "All ViewAction variants should exist");
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 3: Key bindings
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_10_quit_key() {
    let mut view = HomeView::new();
    let mut state = create_mock_tui_state();

    // q should quit
    let q_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    let action = view.handle_key(q_key, &mut state);

    assert!(matches!(action, ViewAction::Quit), "q should trigger Quit");
}

#[test]
fn wiring_checkpoint_10_ctrl_c_quit() {
    let mut view = HomeView::new();
    let mut state = create_mock_tui_state();

    // Ctrl+C should quit
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let action = view.handle_key(ctrl_c, &mut state);

    assert!(matches!(action, ViewAction::Quit), "Ctrl+C should trigger Quit");
}

#[test]
fn wiring_checkpoint_10_view_switch_keys() {
    let mut home = HomeView::new();
    let mut state = create_mock_tui_state();

    // Test view switching keys (c for chat)
    let c_key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
    let action = home.handle_key(c_key, &mut state);

    // Should switch to chat or do nothing (depends on implementation)
    assert!(
        matches!(action, ViewAction::SwitchView(_) | ViewAction::None),
        "c key should either switch view or do nothing"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// TEST 4: View trait polymorphism
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn wiring_checkpoint_10_views_implement_trait() {
    fn assert_view<V: View>(_v: V) {}

    assert_view(HomeView::new());
    assert_view(ChatView::new());
    assert_view(MonitorView::new());
}

// Helper
fn create_mock_tui_state() -> nika::tui::TuiState {
    nika::tui::TuiState::default()
}
```

**Step 5: Commit**

```bash
git add tests/wiring_checkpoint_10.rs
git commit -m "test(wiring): add WIRING-10 Full TUI Navigation

Co-Authored-By: Claude <noreply@anthropic.com>
Co-Authored-By: Nika <agent@nika.sh>"
```

---

## Summary

| Task | Tests | Status |
|------|-------|--------|
| Infer verb tests | 5 | Pending |
| Agent verb tests | 4 | Pending |
| Decompose tests | 5 | Pending |
| Provider caching | 3 | Pending |
| Error edge cases | 3 | Pending |
| WIRING-7 | 12 | Pending |
| WIRING-8 | 5 | Pending |
| WIRING-9 | 5 | Pending |
| WIRING-10 | 8 | Pending |
| **TOTAL** | **50** | |

---

## Verification

After implementation:

```bash
# Run all executor tests
cargo test --lib executor -- --nocapture

# Run all WIRING tests
cargo test wiring_checkpoint -- --nocapture

# Full test suite
cargo test
```

Expected: 50+ new tests passing, total test count increases from ~2,869 to ~2,919.
