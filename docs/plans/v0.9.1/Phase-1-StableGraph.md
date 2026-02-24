# Phase 1: StableGraph + ChatWorkflow Infrastructure

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Migrate FlowGraph to petgraph::StableGraph and create ChatWorkflow struct that wraps it.

**Architecture:** StableGraph preserves NodeIndex after deletion (critical for @1, @2 references). ChatWorkflow owns the DAG, DataStore, and EventLog for a chat session.

**Tech Stack:** petgraph 0.6, parking_lot, rustc-hash

**Skills:** @rust-core, @test-driven-development

---

## Task 1.1: Add petgraph Dependency

**Files:**
- Modify: `Cargo.toml`

**Step 1: Edit Cargo.toml**

```toml
[dependencies]
petgraph = "0.6"
```

**Step 2: Verify compilation**

Run: `cargo check`
Expected: Compiles without errors

**Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "chore(deps): add petgraph 0.6 for StableGraph"
```

---

## Task 1.2: Write Failing Test for StableGraph Node Stability

**Files:**
- Modify: `src/dag/flow.rs` (add test module)
- Test: In-file `#[cfg(test)]` module

**Step 1: Write the failing test**

```rust
// src/dag/flow.rs - add to #[cfg(test)] mod tests
#[test]
fn test_stablegraph_preserves_indices_after_removal() {
    use petgraph::stable_graph::StableGraph;
    use petgraph::Directed;

    let mut graph: StableGraph<&str, (), Directed> = StableGraph::new();

    let n0 = graph.add_node("task-0");
    let n1 = graph.add_node("task-1");
    let n2 = graph.add_node("task-2");

    // Add edges
    graph.add_edge(n0, n1, ());
    graph.add_edge(n1, n2, ());

    // Remove middle node
    graph.remove_node(n1);

    // CRITICAL: n0 and n2 must still be valid
    assert_eq!(graph[n0], "task-0");
    assert_eq!(graph[n2], "task-2");

    // n1 should not exist
    assert!(graph.node_weight(n1).is_none());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_stablegraph_preserves_indices_after_removal --lib`
Expected: PASS (petgraph StableGraph already works this way - this is a documentation test)

**Step 3: Commit**

```bash
git add src/dag/flow.rs
git commit -m "test(dag): add StableGraph index stability test"
```

---

## Task 1.3: Create FlowGraph Type Alias for StableGraph

**Files:**
- Modify: `src/dag/flow.rs:1-30`

**Step 1: Write the failing test**

```rust
#[test]
fn test_flowgraph_is_stablegraph() {
    let graph = FlowGraph::new();
    // Should compile - FlowGraph is a StableGraph
    let _: &StableGraph<Arc<str>, (), Directed> = graph.inner();
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_flowgraph_is_stablegraph --lib`
Expected: FAIL with "method `inner` not found"

**Step 3: Write minimal implementation**

```rust
// src/dag/flow.rs
use std::sync::Arc;
use petgraph::stable_graph::{StableGraph, NodeIndex};
use petgraph::Directed;
use rustc_hash::FxHashMap;

/// DAG representation using StableGraph for stable node indices.
/// Critical for @1, @2, @last references in chat - indices don't change after deletion.
pub struct FlowGraph {
    /// The underlying stable graph
    graph: StableGraph<Arc<str>, (), Directed>,
    /// Map from task ID to node index (for O(1) lookup)
    id_to_node: FxHashMap<Arc<str>, NodeIndex>,
}

impl FlowGraph {
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            id_to_node: FxHashMap::default(),
        }
    }

    /// Get reference to inner StableGraph
    pub fn inner(&self) -> &StableGraph<Arc<str>, (), Directed> {
        &self.graph
    }
}

impl Default for FlowGraph {
    fn default() -> Self {
        Self::new()
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_flowgraph_is_stablegraph --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/flow.rs
git commit -m "feat(dag): migrate FlowGraph to petgraph::StableGraph"
```

---

## Task 1.4: Implement FlowGraph::add_node()

**Files:**
- Modify: `src/dag/flow.rs:40-70`

**Step 1: Write the failing test**

```rust
#[test]
fn test_add_node_returns_index() {
    let mut graph = FlowGraph::new();

    let idx1 = graph.add_node("task-1");
    let idx2 = graph.add_node("task-2");

    assert_ne!(idx1, idx2);
    assert_eq!(graph.node_count(), 2);
    assert_eq!(graph.get_task_id(idx1), Some("task-1"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_add_node_returns_index --lib`
Expected: FAIL with "method `add_node` not found"

**Step 3: Write minimal implementation**

```rust
impl FlowGraph {
    /// Add a node to the graph, returning its stable index
    pub fn add_node(&mut self, task_id: &str) -> NodeIndex {
        let id: Arc<str> = Arc::from(task_id);
        let idx = self.graph.add_node(Arc::clone(&id));
        self.id_to_node.insert(id, idx);
        idx
    }

    /// Get node count
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Get task ID from node index
    pub fn get_task_id(&self, idx: NodeIndex) -> Option<&str> {
        self.graph.node_weight(idx).map(|s| s.as_ref())
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_add_node_returns_index --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/flow.rs
git commit -m "feat(dag): implement FlowGraph::add_node with stable indices"
```

---

## Task 1.5: Implement FlowGraph::add_edge()

**Files:**
- Modify: `src/dag/flow.rs:70-100`

**Step 1: Write the failing test**

```rust
#[test]
fn test_add_edge_by_id() {
    let mut graph = FlowGraph::new();

    graph.add_node("task-1");
    graph.add_node("task-2");

    let result = graph.add_edge("task-1", "task-2");
    assert!(result.is_ok());

    assert!(graph.has_edge("task-1", "task-2"));
    assert!(!graph.has_edge("task-2", "task-1")); // Directed
}

#[test]
fn test_add_edge_unknown_node_fails() {
    let mut graph = FlowGraph::new();
    graph.add_node("task-1");

    let result = graph.add_edge("task-1", "unknown");
    assert!(result.is_err());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_add_edge --lib`
Expected: FAIL with "method `add_edge` not found"

**Step 3: Write minimal implementation**

```rust
use crate::error::NikaError;

impl FlowGraph {
    /// Add an edge between two tasks by ID
    pub fn add_edge(&mut self, source: &str, target: &str) -> Result<(), NikaError> {
        let source_idx = self.id_to_node.get(source)
            .copied()
            .ok_or_else(|| NikaError::DagError {
                message: format!("Source task not found: {}", source),
            })?;

        let target_idx = self.id_to_node.get(target)
            .copied()
            .ok_or_else(|| NikaError::DagError {
                message: format!("Target task not found: {}", target),
            })?;

        self.graph.add_edge(source_idx, target_idx, ());
        Ok(())
    }

    /// Check if edge exists
    pub fn has_edge(&self, source: &str, target: &str) -> bool {
        let source_idx = match self.id_to_node.get(source) {
            Some(idx) => *idx,
            None => return false,
        };
        let target_idx = match self.id_to_node.get(target) {
            Some(idx) => *idx,
            None => return false,
        };

        self.graph.contains_edge(source_idx, target_idx)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_add_edge --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/flow.rs
git commit -m "feat(dag): implement FlowGraph::add_edge with error handling"
```

---

## Task 1.6: Implement FlowGraph::remove_node()

**Files:**
- Modify: `src/dag/flow.rs:100-130`

**Step 1: Write the failing test**

```rust
#[test]
fn test_remove_node_preserves_other_indices() {
    let mut graph = FlowGraph::new();

    let idx1 = graph.add_node("task-1");
    let idx2 = graph.add_node("task-2");
    let idx3 = graph.add_node("task-3");

    // Remove middle node
    graph.remove_node("task-2");

    // idx1 and idx3 MUST still work (StableGraph guarantee)
    assert_eq!(graph.get_task_id(idx1), Some("task-1"));
    assert_eq!(graph.get_task_id(idx3), Some("task-3"));

    // idx2 should be gone
    assert!(graph.get_task_id(idx2).is_none());

    assert_eq!(graph.node_count(), 2);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_remove_node_preserves_other_indices --lib`
Expected: FAIL with "method `remove_node` not found"

**Step 3: Write minimal implementation**

```rust
impl FlowGraph {
    /// Remove a node by task ID. Other indices remain stable.
    pub fn remove_node(&mut self, task_id: &str) -> bool {
        if let Some(idx) = self.id_to_node.remove(task_id) {
            self.graph.remove_node(idx);
            true
        } else {
            false
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_remove_node_preserves_other_indices --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/dag/flow.rs
git commit -m "feat(dag): implement FlowGraph::remove_node with stable indices"
```

---

## 🧪 LIVE TEST CHECKPOINT 1

```bash
# Run all dag tests
cargo test dag:: --lib

# Expected: All tests pass
# If fails: Debug before proceeding to Task 1.7
```

---

## Task 1.7: Create ChatWorkflow Struct

**Files:**
- Create: `src/tui/chat_workflow.rs`
- Modify: `src/tui/mod.rs` (add module)

**Step 1: Write the failing test**

```rust
// src/tui/chat_workflow.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_creates_empty_workflow() {
        let cw = ChatWorkflow::new("test-session");

        assert_eq!(cw.workflow.workflow, "chat-session-test-session");
        assert!(cw.workflow.tasks.is_empty());
        assert_eq!(cw.dag.node_count(), 0);
        assert_eq!(cw.message_counter, 0);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_new_creates_empty_workflow --lib`
Expected: FAIL with "cannot find module `chat_workflow`"

**Step 3: Write minimal implementation**

```rust
// src/tui/chat_workflow.rs

use std::sync::Arc;
use crate::ast::{Workflow, Task, Flow};
use crate::store::DataStore;
use crate::event::EventLog;
use crate::dag::FlowGraph;

/// A chat session represented as an incremental workflow DAG.
///
/// Each message becomes a Task node. @mentions create edges.
/// Uses StableGraph for stable node indices after deletion.
pub struct ChatWorkflow {
    /// Incremental workflow being built
    pub workflow: Workflow,
    /// DAG representation (StableGraph)
    pub dag: FlowGraph,
    /// Result storage
    pub store: DataStore,
    /// Event log for observability
    pub log: EventLog,
    /// Message counter for ID generation
    pub message_counter: u32,
}

impl ChatWorkflow {
    pub fn new(session_id: &str) -> Self {
        Self {
            workflow: Workflow {
                schema: Arc::from("nika/workflow@0.6"),
                workflow: Arc::from(format!("chat-session-{}", session_id).as_str()),
                description: Some(Arc::from("Interactive chat session")),
                tasks: Vec::new(),
                flows: Vec::new(),
                mcp: None,
                context: None,
                agents: None,
                skills: None,
            },
            dag: FlowGraph::new(),
            store: DataStore::new(),
            log: EventLog::new(),
            message_counter: 0,
        }
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_new_creates_empty_workflow --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/chat_workflow.rs src/tui/mod.rs
git commit -m "feat(tui): create ChatWorkflow struct for chat-as-DAG"
```

---

## Task 1.8: Implement ChatWorkflow::next_message_id()

**Files:**
- Modify: `src/tui/chat_workflow.rs:40-60`

**Step 1: Write the failing test**

```rust
#[test]
fn test_next_message_id_increments() {
    let mut cw = ChatWorkflow::new("test");

    assert_eq!(cw.next_message_id(), "msg-001");
    assert_eq!(cw.next_message_id(), "msg-002");
    assert_eq!(cw.next_message_id(), "msg-003");
    assert_eq!(cw.message_counter, 3);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_next_message_id_increments --lib`
Expected: FAIL with "method `next_message_id` not found"

**Step 3: Write minimal implementation**

```rust
impl ChatWorkflow {
    /// Generate next message ID (msg-001, msg-002, ...)
    pub fn next_message_id(&mut self) -> String {
        self.message_counter += 1;
        format!("msg-{:03}", self.message_counter)
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_next_message_id_increments --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/chat_workflow.rs
git commit -m "feat(tui): implement ChatWorkflow::next_message_id()"
```

---

## Task 1.9: Implement ChatWorkflow::add_task()

**Files:**
- Modify: `src/tui/chat_workflow.rs:60-90`

**Step 1: Write the failing test**

```rust
#[test]
fn test_add_task_updates_dag_and_workflow() {
    let mut cw = ChatWorkflow::new("test");

    let task = Task {
        id: Arc::from("msg-001"),
        action: TaskAction::Infer(InferParams {
            prompt: "Hello".into(),
            ..Default::default()
        }),
        ..Default::default()
    };

    let idx = cw.add_task(task.clone());

    assert_eq!(cw.dag.node_count(), 1);
    assert_eq!(cw.workflow.tasks.len(), 1);
    assert_eq!(cw.dag.get_task_id(idx), Some("msg-001"));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_add_task_updates_dag_and_workflow --lib`
Expected: FAIL with "method `add_task` not found"

**Step 3: Write minimal implementation**

```rust
use petgraph::stable_graph::NodeIndex;

impl ChatWorkflow {
    /// Add a task to both the workflow and DAG
    pub fn add_task(&mut self, task: Task) -> NodeIndex {
        let idx = self.dag.add_node(&task.id);
        self.workflow.tasks.push(task);
        idx
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_add_task_updates_dag_and_workflow --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/chat_workflow.rs
git commit -m "feat(tui): implement ChatWorkflow::add_task()"
```

---

## Task 1.10: Implement ChatWorkflow::add_flow()

**Files:**
- Modify: `src/tui/chat_workflow.rs:90-120`

**Step 1: Write the failing test**

```rust
#[test]
fn test_add_flow_creates_edge() {
    let mut cw = ChatWorkflow::new("test");

    // Add two tasks
    cw.add_task(Task { id: Arc::from("msg-001"), ..Default::default() });
    cw.add_task(Task { id: Arc::from("msg-002"), ..Default::default() });

    // Add flow
    cw.add_flow("msg-001", "msg-002").unwrap();

    assert!(cw.dag.has_edge("msg-001", "msg-002"));
    assert_eq!(cw.workflow.flows.len(), 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_add_flow_creates_edge --lib`
Expected: FAIL with "method `add_flow` not found"

**Step 3: Write minimal implementation**

```rust
impl ChatWorkflow {
    /// Add a flow (edge) between tasks
    pub fn add_flow(&mut self, source: &str, target: &str) -> Result<(), NikaError> {
        self.dag.add_edge(source, target)?;
        self.workflow.flows.push(Flow {
            source: Arc::from(source),
            target: Arc::from(target),
        });
        Ok(())
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_add_flow_creates_edge --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/chat_workflow.rs
git commit -m "feat(tui): implement ChatWorkflow::add_flow()"
```

---

## Task 1.11: Implement Thread-Safe ChatWorkflow Wrapper

**Files:**
- Modify: `src/tui/chat_workflow.rs:1-20`

**Step 1: Write the failing test**

```rust
#[tokio::test]
async fn test_chat_workflow_is_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Arc<parking_lot::Mutex<ChatWorkflow>>>();
}
```

**Step 2: Verify types are correct**

Run: `cargo test test_chat_workflow_is_send_sync --lib`
Expected: PASS (Rust's type system checks this at compile time)

**Step 3: Document thread-safety pattern**

```rust
// Add to chat_workflow.rs top
//! # Thread Safety
//!
//! `ChatWorkflow` is NOT thread-safe by itself. Wrap in
//! `Arc<parking_lot::Mutex<ChatWorkflow>>` for concurrent access.
//!
//! ```rust
//! use std::sync::Arc;
//! use parking_lot::Mutex;
//!
//! let workflow = Arc::new(Mutex::new(ChatWorkflow::new("session")));
//!
//! // In async context:
//! let task = {
//!     let mut wf = workflow.lock();
//!     wf.add_task(task)
//! }; // Lock released before await
//!
//! // Execute task (no lock held)
//! executor.execute(&task).await?;
//! ```
```

**Step 4: Commit**

```bash
git add src/tui/chat_workflow.rs
git commit -m "docs(tui): document ChatWorkflow thread-safety pattern"
```

---

## Task 1.12: Wire ChatWorkflow into ChatAgent

**Files:**
- Modify: `src/tui/views/chat.rs` (or `src/tui/chat_agent.rs`)

**Step 1: Write the failing test**

```rust
#[test]
fn test_chat_agent_has_workflow() {
    let agent = ChatAgent::new(/* ... */);
    assert!(agent.workflow().is_some());
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test test_chat_agent_has_workflow --lib`
Expected: FAIL with "method `workflow` not found"

**Step 3: Write minimal implementation**

```rust
// In ChatAgent or ChatView
pub struct ChatAgent {
    // ... existing fields ...
    workflow: ChatWorkflow,
}

impl ChatAgent {
    pub fn workflow(&self) -> &ChatWorkflow {
        &self.workflow
    }

    pub fn workflow_mut(&mut self) -> &mut ChatWorkflow {
        &mut self.workflow
    }
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test test_chat_agent_has_workflow --lib`
Expected: PASS

**Step 5: Commit**

```bash
git add src/tui/views/chat.rs
git commit -m "feat(tui): wire ChatWorkflow into ChatAgent"
```

---

## 🔌 WIRING CHECKPOINT 1: FlowGraph ↔ ChatWorkflow

```bash
# Verify wiring is correct
cargo test chat_workflow --lib
cargo test dag:: --lib

# Manual verification
cargo run -- chat
# In chat, send a message
# Verify: no crash, message appears
```

**Expected:** Chat view works, ChatWorkflow is created internally.

---

## 🧪 LIVE TEST: End of Phase 1

```bash
# 1. Run all Phase 1 tests
cargo test dag:: --lib
cargo test chat_workflow --lib

# 2. Manual test
cargo run -- chat
> "Hello"
# Expected: Message appears, no errors

# 3. Verify test count
cargo test --lib 2>&1 | grep "test result"
# Expected: ~35 new tests pass
```

---

## Phase 1 Deliverables

- [x] `petgraph` dependency added
- [x] `FlowGraph` uses `StableGraph<Arc<str>, ()>`
- [x] Node addition/removal methods available
- [x] `ChatWorkflow` struct created
- [x] ChatWorkflow wired into ChatAgent
- [x] 35 new tests passing
- [x] Zero clippy warnings

---

## Next Phase

After Phase 1 passes all tests and live verification:
→ Proceed to [Phase-2-Bindings.md](./Phase-2-Bindings.md)
