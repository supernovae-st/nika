//! FlowGraph - DAG structure built from workflow flows (optimized)
//!
//! Performance optimizations:
//! - Arc<str> for zero-cost cloning of task IDs
//! - FxHashMap for faster hashing (non-crypto, ~2x faster)
//! - SmallVec for stack-allocated small dependency lists (0-4 items)
//!
//! DAG Validation:
//! - Cycle detection using DFS three-color algorithm

use std::collections::VecDeque;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::ast::Workflow;
use crate::error::NikaError;
use crate::util::intern;

/// Stack-allocated deps: most tasks have 0-4 dependencies
pub type DepVec = SmallVec<[Arc<str>; 4]>;

/// Graph of task dependencies built from flows
///
/// Uses Arc<str> + FxHashMap + SmallVec for maximum performance.
pub struct FlowGraph {
    /// task_id -> list of successor task_ids (SmallVec: stack-allocated for ≤4)
    adjacency: FxHashMap<Arc<str>, DepVec>,
    /// task_id -> list of predecessor task_ids (SmallVec: stack-allocated for ≤4)
    predecessors: FxHashMap<Arc<str>, DepVec>,
    /// All task IDs (for iteration)
    task_ids: Vec<Arc<str>>,
    /// Quick lookup for task existence (FxHashSet: faster hashing)
    #[allow(dead_code)] // Used in from_workflow for Arc<str> reuse
    task_set: FxHashSet<Arc<str>>,
}

impl FlowGraph {
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let capacity = workflow.tasks.len();
        let mut adjacency: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut predecessors: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut task_ids: Vec<Arc<str>> = Vec::with_capacity(capacity);
        let mut task_set: FxHashSet<Arc<str>> =
            FxHashSet::with_capacity_and_hasher(capacity, Default::default());

        // Intern task IDs once, reuse everywhere (single allocation per unique ID)
        for task in &workflow.tasks {
            let id = intern(&task.id); // Interned Arc<str>
            task_ids.push(Arc::clone(&id));
            task_set.insert(Arc::clone(&id));
            adjacency.insert(Arc::clone(&id), DepVec::new());
            predecessors.insert(id, DepVec::new());
        }

        // Build from flows (lookup Arc from set or intern)
        for flow in &workflow.flows {
            let sources = flow.source.as_vec();
            let targets = flow.target.as_vec();

            for source in &sources {
                for target in &targets {
                    // Find existing Arc<str> or intern new (shouldn't happen if task exists)
                    let src_arc = task_set
                        .get(*source)
                        .cloned()
                        .unwrap_or_else(|| intern(source));
                    let tgt_arc = task_set
                        .get(*target)
                        .cloned()
                        .unwrap_or_else(|| intern(target));

                    adjacency
                        .entry(Arc::clone(&src_arc))
                        .or_default()
                        .push(Arc::clone(&tgt_arc));
                    predecessors.entry(tgt_arc).or_default().push(src_arc);
                }
            }
        }

        Self {
            adjacency,
            predecessors,
            task_ids,
            task_set,
        }
    }

    /// Get dependencies of a task (returns Arc<str> slice)
    #[inline]
    pub fn get_dependencies(&self, task_id: &str) -> &[Arc<str>] {
        static EMPTY: &[Arc<str>] = &[];
        self.predecessors
            .get(task_id)
            .map_or(EMPTY, SmallVec::as_slice)
    }

    /// Get successors of a task
    #[inline]
    #[allow(dead_code)] // Used for future DAG traversal
    pub fn get_successors(&self, task_id: &str) -> &[Arc<str>] {
        static EMPTY: &[Arc<str>] = &[];
        self.adjacency
            .get(task_id)
            .map_or(EMPTY, SmallVec::as_slice)
    }

    /// Find tasks with no successors (final tasks)
    ///
    /// Returns Arc<str> for zero-cost cloning by caller.
    pub fn get_final_tasks(&self) -> Vec<Arc<str>> {
        self.task_ids
            .iter()
            .filter(|id| {
                self.adjacency
                    .get(id.as_ref())
                    .is_none_or(SmallVec::is_empty)
            })
            .cloned() // Arc::clone is O(1)
            .collect()
    }

    /// Check if task exists
    #[inline]
    #[allow(dead_code)] // Used for future validation
    pub fn contains(&self, task_id: &str) -> bool {
        self.task_set.contains(task_id)
    }

    /// Check if there's a path from `from` to `to` (BFS)
    pub fn has_path(&self, from: &str, to: &str) -> bool {
        if from == to {
            return true;
        }

        // Use FxHashSet for faster visited checks
        let mut visited: FxHashSet<&str> = FxHashSet::default();
        let mut queue: VecDeque<&str> = VecDeque::new();

        queue.push_back(from);
        visited.insert(from);

        while let Some(current) = queue.pop_front() {
            if let Some(neighbors) = self.adjacency.get(current) {
                for neighbor in neighbors {
                    if neighbor.as_ref() == to {
                        return true;
                    }
                    if !visited.contains(neighbor.as_ref()) {
                        visited.insert(neighbor.as_ref());
                        queue.push_back(neighbor.as_ref());
                    }
                }
            }
        }

        false
    }

    /// Detect cycles in the DAG using DFS with three-color marking.
    ///
    /// Returns `Ok(())` if acyclic, `Err(NikaError::CycleDetected)` with cycle path if cycle found.
    ///
    /// Uses standard three-color algorithm:
    /// - White: unvisited
    /// - Gray: currently in DFS stack (visiting)
    /// - Black: fully processed (all descendants visited)
    ///
    /// A cycle is detected when we encounter a Gray node while traversing.
    pub fn detect_cycles(&self) -> Result<(), NikaError> {
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Color {
            White,
            Gray,
            Black,
        }

        let mut colors: FxHashMap<Arc<str>, Color> = self
            .task_ids
            .iter()
            .map(|id| (Arc::clone(id), Color::White))
            .collect();
        let mut stack: Vec<Arc<str>> = Vec::new();

        fn dfs(
            node: Arc<str>,
            adjacency: &FxHashMap<Arc<str>, DepVec>,
            colors: &mut FxHashMap<Arc<str>, Color>,
            stack: &mut Vec<Arc<str>>,
        ) -> Result<(), String> {
            colors.insert(Arc::clone(&node), Color::Gray);
            stack.push(Arc::clone(&node));

            if let Some(neighbors) = adjacency.get(&node) {
                for neighbor in neighbors {
                    match colors.get(neighbor) {
                        Some(Color::Gray) => {
                            // Found cycle - build path from stack
                            // SAFETY: neighbor is Gray means it's in the current DFS path (stack)
                            let cycle_start = stack
                                .iter()
                                .position(|x| x.as_ref() == neighbor.as_ref())
                                .unwrap_or(0); // Defensive: default to start if invariant fails
                            let cycle: Vec<&str> =
                                stack[cycle_start..].iter().map(|s| s.as_ref()).collect();
                            return Err(format!("{} → {}", cycle.join(" → "), neighbor));
                        }
                        Some(Color::White) | None => {
                            dfs(Arc::clone(neighbor), adjacency, colors, stack)?;
                        }
                        Some(Color::Black) => {} // Already processed
                    }
                }
            }

            stack.pop();
            colors.insert(node, Color::Black);
            Ok(())
        }

        for task_id in &self.task_ids {
            if colors.get(task_id) == Some(&Color::White) {
                if let Err(cycle) = dfs(
                    Arc::clone(task_id),
                    &self.adjacency,
                    &mut colors,
                    &mut stack,
                ) {
                    return Err(NikaError::CycleDetected { cycle });
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // CYCLE DETECTION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_detect_cycle_simple() {
        // A → B → C → A (cycle)
        let yaml = r#"
schema: nika/workflow@0.1
id: cycle_test
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-020"));
    }

    #[test]
    fn test_no_cycle_linear() {
        // A → B → C (no cycle)
        let yaml = r#"
schema: nika/workflow@0.1
id: linear_test
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(graph.detect_cycles().is_ok());
    }

    #[test]
    fn test_self_loop_is_cycle() {
        // A → A (self-loop)
        let yaml = r#"
schema: nika/workflow@0.1
id: self_loop
tasks:
  - id: a
    infer:
      prompt: "A"
flows:
  - source: a
    target: a
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-020"));
    }

    #[test]
    fn test_diamond_no_cycle() {
        // Diamond: A → B, A → C, B → D, C → D (no cycle)
        let yaml = r#"
schema: nika/workflow@0.1
id: diamond
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: [b, c]
  - source: [b, c]
    target: d
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(graph.detect_cycles().is_ok());
        assert_eq!(graph.get_final_tasks().len(), 1);
        assert!(graph.has_path("a", "d"));
    }

    #[test]
    fn test_disconnected_no_cycle() {
        // Two disconnected chains: A → B, C → D (no cycle)
        let yaml = r#"
schema: nika/workflow@0.1
id: disconnected
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
  - id: d
    infer:
      prompt: "D"
flows:
  - source: a
    target: b
  - source: c
    target: d
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        assert!(graph.detect_cycles().is_ok());
        assert_eq!(graph.get_final_tasks().len(), 2);
    }

    #[test]
    fn test_cycle_path_includes_all_nodes() {
        // A → B → C → A: cycle path should show the cycle
        let yaml = r#"
schema: nika/workflow@0.1
id: cycle_path
tasks:
  - id: a
    infer:
      prompt: "A"
  - id: b
    infer:
      prompt: "B"
  - id: c
    infer:
      prompt: "C"
flows:
  - source: a
    target: b
  - source: b
    target: c
  - source: c
    target: a
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let graph = FlowGraph::from_workflow(&workflow);

        let result = graph.detect_cycles();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Should contain cycle path
        assert!(err_msg.contains("→"));
    }
}
