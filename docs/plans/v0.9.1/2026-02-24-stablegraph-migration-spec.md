# StableGraph Migration Specification

**Date:** 2026-02-24
**Status:** Draft
**Authors:** Claude (Architecture)
**Target:** v0.9.1 Batch B1.1
**Effort:** ~350 LOC changes | 25 tests | 6-8 hours

---

## Executive Summary

Migrate `FlowGraph` from custom `FxHashMap`-based adjacency lists to `petgraph::StableGraph` for stable node indices. This enables:

- **@mention stability** — `@1`, `@2` references remain valid after message deletion
- **Reusable algorithms** — `is_cyclic_directed()`, `toposort()` from petgraph
- **Future DAG mutations** — Support for `decompose:` runtime expansion
- **Type safety** — `NodeIndex` prevents string confusion

---

## Part 1: Current Implementation Analysis

### Current FlowGraph Structure

**File:** `src/dag/flow.rs` (432 lines)

```rust
pub struct FlowGraph {
    // task_id -> list of successor task_ids
    adjacency: FxHashMap<Arc<str>, DepVec>,

    // task_id -> list of predecessor task_ids (reverse lookup)
    predecessors: FxHashMap<Arc<str>, DepVec>,

    // All task IDs (for iteration)
    task_ids: Vec<Arc<str>>,

    // Quick lookup for task existence
    task_set: FxHashSet<Arc<str>>,
}

pub(crate) type DepVec = SmallVec<[Arc<str>; 4]>;
```

### Current Methods

| Method | Purpose | Complexity |
|--------|---------|------------|
| `from_workflow()` | Build graph from flows | O(n) |
| `get_dependencies()` | Get predecessors | O(1) hash |
| `get_successors()` | Get successors | O(1) hash |
| `get_final_tasks()` | Tasks with no successors | O(n) |
| `contains()` | Check task exists | O(1) hash |
| `has_path()` | BFS path check | O(V+E) |
| `detect_cycles()` | DFS three-color | O(V+E) |

### Pain Points

1. **Bidirectional maintenance** — Must sync `adjacency` and `predecessors`
2. **190+ lines for cycle detection** — Manual three-color DFS
3. **No structural indices** — All lookups via string hashing
4. **Duplicate topological sort** — `dag_layout.rs` reimplements

---

## Part 2: Target Implementation

### New FlowGraph Structure

```rust
use petgraph::stable_graph::{StableGraph, NodeIndex};
use petgraph::Directed;
use rustc_hash::FxHashMap;
use std::sync::Arc;

/// Node payload stored in StableGraph
#[derive(Debug, Clone)]
pub struct TaskNode {
    /// Task ID (interned)
    pub id: Arc<str>,
    /// Original task index in workflow (for ordering)
    pub order: u32,
}

/// FlowGraph using petgraph::StableGraph for stable indices
pub struct FlowGraph {
    /// The actual graph structure
    graph: StableGraph<TaskNode, (), Directed>,

    /// task_id -> NodeIndex mapping (O(1) lookup)
    id_to_node: FxHashMap<Arc<str>, NodeIndex>,

    /// String interner for task IDs
    interner: FxHashMap<Arc<str>, Arc<str>>,
}
```

### Why StableGraph?

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  STABLEGRAPH INDEX BEHAVIOR (from petgraph docs)                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Non-Stable Graph (after removing node 1):                                  │
│  NodeIndex: [0, 1, 2]  ← Node 3 shifted to index 1!                         │
│                                                                             │
│  StableGraph (after removing node 1):                                       │
│  NodeIndex: [0, 2, 3]  ← Indices remain stable!                             │
│                                                                             │
│  CRITICAL FOR CHAT:                                                         │
│  @1 = msg-001 (NodeIndex 0)                                                 │
│  @2 = msg-002 (NodeIndex 1)  ← DELETED                                      │
│  @3 = msg-003 (NodeIndex 2)  ← Still NodeIndex 2, not shifted!              │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Core Methods Implementation

```rust
impl FlowGraph {
    /// Create empty graph
    pub fn new() -> Self {
        Self {
            graph: StableGraph::new(),
            id_to_node: FxHashMap::default(),
            interner: FxHashMap::default(),
        }
    }

    /// Build from workflow flows
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let mut fg = Self::new();

        // Add all task nodes
        for (i, task) in workflow.tasks.iter().enumerate() {
            fg.add_task(&task.id, i as u32);
        }

        // Add edges from flows
        for flow in &workflow.flows {
            fg.add_edge(&flow.source, &flow.target);
        }

        fg
    }

    /// Add a task node (returns NodeIndex)
    pub fn add_task(&mut self, task_id: &str, order: u32) -> NodeIndex {
        // Intern the task ID
        let id = self.intern(task_id);

        // Create node payload
        let node = TaskNode { id: id.clone(), order };

        // Add to graph
        let idx = self.graph.add_node(node);

        // Track in lookup map
        self.id_to_node.insert(id, idx);

        idx
    }

    /// Add edge between tasks
    pub fn add_edge(&mut self, source: &str, target: &str) {
        if let (Some(&src_idx), Some(&tgt_idx)) =
            (self.id_to_node.get(source), self.id_to_node.get(target))
        {
            self.graph.add_edge(src_idx, tgt_idx, ());
        }
    }

    /// Remove a task (stable - other indices unchanged)
    pub fn remove_task(&mut self, task_id: &str) -> Option<TaskNode> {
        let idx = self.id_to_node.remove(task_id)?;
        self.graph.remove_node(idx)
    }

    /// Get dependencies (predecessors) of a task
    pub fn get_dependencies(&self, task_id: &str) -> Vec<Arc<str>> {
        let Some(&idx) = self.id_to_node.get(task_id) else {
            return Vec::new();
        };

        self.graph
            .neighbors_directed(idx, petgraph::Direction::Incoming)
            .filter_map(|n| self.graph.node_weight(n).map(|w| w.id.clone()))
            .collect()
    }

    /// Get successors of a task
    pub fn get_successors(&self, task_id: &str) -> Vec<Arc<str>> {
        let Some(&idx) = self.id_to_node.get(task_id) else {
            return Vec::new();
        };

        self.graph
            .neighbors_directed(idx, petgraph::Direction::Outgoing)
            .filter_map(|n| self.graph.node_weight(n).map(|w| w.id.clone()))
            .collect()
    }

    /// Get tasks with no successors (final tasks)
    pub fn get_final_tasks(&self) -> Vec<Arc<str>> {
        self.graph
            .node_indices()
            .filter(|&idx| {
                self.graph.neighbors_directed(idx, petgraph::Direction::Outgoing).count() == 0
            })
            .filter_map(|idx| self.graph.node_weight(idx).map(|w| w.id.clone()))
            .collect()
    }

    /// Check if task exists
    pub fn contains(&self, task_id: &str) -> bool {
        self.id_to_node.contains_key(task_id)
    }

    /// Check if path exists from source to target
    pub fn has_path(&self, from: &str, to: &str) -> bool {
        let (Some(&from_idx), Some(&to_idx)) =
            (self.id_to_node.get(from), self.id_to_node.get(to))
        else {
            return false;
        };

        petgraph::algo::has_path_connecting(&self.graph, from_idx, to_idx, None)
    }

    /// Detect cycles in the graph
    pub fn detect_cycles(&self) -> Result<(), NikaError> {
        if petgraph::algo::is_cyclic_directed(&self.graph) {
            // Find a cycle for error message
            let cycle = self.find_cycle_path();
            return Err(NikaError::CycleDetected { path: cycle });
        }
        Ok(())
    }

    /// Topological sort (for execution order)
    pub fn topological_order(&self) -> Vec<Arc<str>> {
        match petgraph::algo::toposort(&self.graph, None) {
            Ok(order) => order
                .into_iter()
                .filter_map(|idx| self.graph.node_weight(idx).map(|w| w.id.clone()))
                .collect(),
            Err(_) => Vec::new(), // Cycle detected
        }
    }

    /// Node count
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Edge count
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    /// Get NodeIndex for a task ID (for direct graph access)
    pub fn node_index(&self, task_id: &str) -> Option<NodeIndex> {
        self.id_to_node.get(task_id).copied()
    }

    /// Intern a task ID string
    fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(interned) = self.interner.get(s) {
            return interned.clone();
        }
        let arc: Arc<str> = s.into();
        self.interner.insert(arc.clone(), arc.clone());
        arc
    }

    /// Find cycle path for error message
    fn find_cycle_path(&self) -> String {
        // Use DFS to find a cycle
        use petgraph::visit::{depth_first_search, DfsEvent, Control};

        let mut path = Vec::new();
        let mut in_stack = FxHashSet::default();

        depth_first_search(&self.graph, self.graph.node_indices(), |event| {
            match event {
                DfsEvent::Discover(n, _) => {
                    in_stack.insert(n);
                    path.push(n);
                }
                DfsEvent::BackEdge(_, to) if in_stack.contains(&to) => {
                    // Found cycle
                    return Control::Break(to);
                }
                DfsEvent::Finish(n, _) => {
                    in_stack.remove(&n);
                    path.pop();
                }
                _ => {}
            }
            Control::Continue
        });

        // Format cycle as "a -> b -> c"
        path.iter()
            .filter_map(|&idx| self.graph.node_weight(idx).map(|w| w.id.as_ref()))
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}
```

---

## Part 3: Migration Steps

### Step 1: Add Dependency

**File:** `Cargo.toml`

```toml
[dependencies]
petgraph = "0.6"
```

### Step 2: Create New FlowGraph

**File:** `src/dag/flow.rs`

1. Keep old implementation as `FlowGraphLegacy` (temporarily)
2. Add new `FlowGraph` with StableGraph
3. Ensure same public API

### Step 3: Update Consumers

**Files affected:**

| File | Changes |
|------|---------|
| `src/runtime/runner.rs` | Use new `get_dependencies()` return type |
| `src/dag/validate.rs` | No changes (same API) |
| `src/main.rs` | No changes (same API) |
| `src/tui/widgets/dag_layout.rs` | Use `topological_order()` |

### Step 4: Remove Legacy Code

1. Delete `FlowGraphLegacy`
2. Delete custom cycle detection (~190 lines)
3. Delete custom topological sort in `dag_layout.rs` (~80 lines)

---

## Part 4: Test Cases

### Unit Tests (25 required)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_task() {
        let mut fg = FlowGraph::new();
        let idx = fg.add_task("task-1", 0);
        assert!(fg.contains("task-1"));
        assert_eq!(fg.node_count(), 1);
    }

    #[test]
    fn test_add_edge() {
        let mut fg = FlowGraph::new();
        fg.add_task("a", 0);
        fg.add_task("b", 1);
        fg.add_edge("a", "b");
        assert_eq!(fg.get_successors("a"), vec!["b".into()]);
        assert_eq!(fg.get_dependencies("b"), vec!["a".into()]);
    }

    #[test]
    fn test_remove_task_stability() {
        let mut fg = FlowGraph::new();
        let idx0 = fg.add_task("msg-001", 0);
        let idx1 = fg.add_task("msg-002", 1);
        let idx2 = fg.add_task("msg-003", 2);

        // Remove middle task
        fg.remove_task("msg-002");

        // Indices should be stable
        assert_eq!(fg.node_index("msg-001"), Some(idx0));
        assert_eq!(fg.node_index("msg-002"), None);
        assert_eq!(fg.node_index("msg-003"), Some(idx2)); // NOT shifted!
    }

    #[test]
    fn test_cycle_detection() {
        let mut fg = FlowGraph::new();
        fg.add_task("a", 0);
        fg.add_task("b", 1);
        fg.add_task("c", 2);
        fg.add_edge("a", "b");
        fg.add_edge("b", "c");
        fg.add_edge("c", "a"); // Creates cycle

        let result = fg.detect_cycles();
        assert!(result.is_err());
    }

    #[test]
    fn test_topological_order() {
        let mut fg = FlowGraph::new();
        fg.add_task("c", 2);
        fg.add_task("a", 0);
        fg.add_task("b", 1);
        fg.add_edge("a", "b");
        fg.add_edge("b", "c");

        let order = fg.topological_order();
        // a must come before b, b must come before c
        let pos_a = order.iter().position(|x| x.as_ref() == "a").unwrap();
        let pos_b = order.iter().position(|x| x.as_ref() == "b").unwrap();
        let pos_c = order.iter().position(|x| x.as_ref() == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_has_path() {
        let mut fg = FlowGraph::new();
        fg.add_task("a", 0);
        fg.add_task("b", 1);
        fg.add_task("c", 2);
        fg.add_task("d", 3);
        fg.add_edge("a", "b");
        fg.add_edge("b", "c");

        assert!(fg.has_path("a", "c"));
        assert!(!fg.has_path("a", "d"));
        assert!(!fg.has_path("c", "a")); // No reverse path
    }

    #[test]
    fn test_final_tasks() {
        let mut fg = FlowGraph::new();
        fg.add_task("a", 0);
        fg.add_task("b", 1);
        fg.add_task("c", 2);
        fg.add_edge("a", "b");
        fg.add_edge("a", "c");

        let finals = fg.get_final_tasks();
        assert_eq!(finals.len(), 2);
        assert!(finals.iter().any(|x| x.as_ref() == "b"));
        assert!(finals.iter().any(|x| x.as_ref() == "c"));
    }

    #[test]
    fn test_from_workflow() {
        let workflow = Workflow {
            schema: "nika/workflow@0.6".into(),
            workflow: "test".into(),
            tasks: vec![
                Task { id: "t1".into(), ..Default::default() },
                Task { id: "t2".into(), ..Default::default() },
            ],
            flows: vec![
                Flow { source: "t1".into(), target: "t2".into() },
            ],
            ..Default::default()
        };

        let fg = FlowGraph::from_workflow(&workflow);
        assert_eq!(fg.node_count(), 2);
        assert_eq!(fg.edge_count(), 1);
    }

    // ... 17 more tests for edge cases
}
```

---

## Part 5: Performance Considerations

### Memory Comparison

| Component | Before (FxHashMap) | After (StableGraph) |
|-----------|-------------------|---------------------|
| Per node | 24 bytes (Arc<str>) | 32 bytes (TaskNode + index) |
| Per edge | 48 bytes (2x Arc<str> in SmallVec) | 24 bytes (2x NodeIndex) |
| Overhead | 2x maps (adjacency + predecessors) | 1 graph + 1 lookup map |

**Result:** Similar memory footprint, slight advantage for large graphs.

### CPU Comparison

| Operation | Before | After |
|-----------|--------|-------|
| Add node | O(1) hash + insert | O(1) vec push + hash insert |
| Add edge | O(1) hash + SmallVec push | O(1) graph edge add |
| Get dependencies | O(1) hash lookup | O(k) neighbor iteration |
| Cycle detection | O(V+E) custom DFS | O(V+E) petgraph optimized |
| Topological sort | O(V+E) custom | O(V+E) petgraph optimized |

**Result:** Similar performance, petgraph algorithms more cache-friendly.

---

## Part 6: Chat Integration

### ChatWorkflow Integration

```rust
pub struct ChatWorkflow {
    /// DAG using StableGraph (stable indices for @mentions)
    pub dag: FlowGraph,

    /// Message counter (for ID generation)
    pub message_counter: AtomicU32,

    /// Result storage
    pub store: DataStore,

    /// Event log
    pub log: EventLog,
}

impl ChatWorkflow {
    /// Add a chat message as a node
    pub fn add_message(&mut self, node_type: NodeType) -> Arc<str> {
        let id = self.next_message_id();
        self.dag.add_task(&id, self.dag.node_count() as u32);
        id
    }

    /// Remove a message (NodeIndex remains stable for other @mentions)
    pub fn remove_message(&mut self, id: &str) {
        self.dag.remove_task(id);
        // Other indices unchanged!
    }

    /// Resolve @N to task ID
    pub fn resolve_mention(&self, n: u32) -> Option<Arc<str>> {
        // @N maps to msg-00N
        let id = format!("msg-{:03}", n);
        if self.dag.contains(&id) {
            Some(id.into())
        } else {
            None
        }
    }
}
```

### @mention Stability Example

```
Session:
  @1 = msg-001  (NodeIndex 0)
  @2 = msg-002  (NodeIndex 1)
  @3 = msg-003  (NodeIndex 2)

User deletes @2:
  @1 = msg-001  (NodeIndex 0)  <- unchanged
  @2 = DELETED  (NodeIndex 1)  <- removed
  @3 = msg-003  (NodeIndex 2)  <- STILL NodeIndex 2!

User sends new message:
  @4 = msg-004  (NodeIndex 3)  <- new index, not reusing 1
```

---

## Part 7: Files Changed Summary

| File | Lines Changed | Description |
|------|---------------|-------------|
| `Cargo.toml` | +1 | Add petgraph dependency |
| `src/dag/flow.rs` | -150, +200 | Replace with StableGraph |
| `src/dag/validate.rs` | 0 | No changes (same API) |
| `src/runtime/runner.rs` | +5 | Update Vec iteration |
| `src/tui/widgets/dag_layout.rs` | -80 | Remove custom toposort |
| `src/dag/flow.rs` tests | +300 | New comprehensive tests |

**Net change:** ~350 lines modified, ~25 new tests

---

## Part 8: Rollback Plan

If issues arise post-migration:

1. Keep `FlowGraphLegacy` for 1 release
2. Feature flag: `--features legacy-flowgraph`
3. Monitor performance metrics for 2 weeks
4. Remove legacy after confirmation

---

## References

- [petgraph StableGraph docs](https://docs.rs/petgraph/latest/petgraph/stable_graph/struct.StableGraph.html)
- [Context7 petgraph examples](/websites/rs_petgraph)
- INDEX.md B1.1 tasks
- chat-as-workflow-dag.md @mention syntax
