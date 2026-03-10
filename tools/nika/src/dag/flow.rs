//! Dag - DAG structure built from workflow flows (optimized)
//!
//! Performance optimizations:
//! - `Arc<str>` for zero-cost cloning of task IDs
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
pub(crate) type DepVec = SmallVec<[Arc<str>; 4]>;

/// Graph of task dependencies built from flows
///
/// Uses `Arc<str>` + FxHashMap + SmallVec for maximum performance.
pub struct Dag {
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

impl Dag {
    /// Build a DAG from a workflow.
    ///
    /// # Errors
    ///
    /// Returns `Err(NikaError::DuplicateTaskId)` if duplicate task IDs are found.
    pub fn from_workflow(workflow: &Workflow) -> Result<Self, NikaError> {
        let capacity = workflow.tasks.len();
        let mut adjacency: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut predecessors: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut task_ids: Vec<Arc<str>> = Vec::with_capacity(capacity);
        let mut task_set: FxHashSet<Arc<str>> =
            FxHashSet::with_capacity_and_hasher(capacity, Default::default());

        // Intern task IDs once, reuse everywhere (single allocation per unique ID)
        // BUG-001 FIX: Detect duplicate task IDs before insertion
        for task in &workflow.tasks {
            let id = intern(&task.id); // Interned Arc<str>
            if task_set.contains(&id) {
                return Err(NikaError::DuplicateTaskId {
                    task_id: task.id.clone(),
                });
            }
            task_ids.push(Arc::clone(&id));
            task_set.insert(Arc::clone(&id));
            adjacency.insert(Arc::clone(&id), DepVec::new());
            predecessors.insert(id, DepVec::new());
        }

        // Build from workflow-level flows: (lookup Arc from set or intern)
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

        // Build from task-level flow: (v0.1+)
        // flow: [dep1, dep2] means dep1 -> this_task, dep2 -> this_task
        for task in &workflow.tasks {
            if let Some(ref deps) = task.flow {
                let tgt_arc = task_set
                    .get(task.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| intern(&task.id));

                for dep in deps {
                    let src_arc = task_set
                        .get(dep.as_str())
                        .cloned()
                        .unwrap_or_else(|| intern(dep));

                    adjacency
                        .entry(Arc::clone(&src_arc))
                        .or_default()
                        .push(Arc::clone(&tgt_arc));
                    predecessors
                        .entry(Arc::clone(&tgt_arc))
                        .or_default()
                        .push(src_arc);
                }
            }
        }

        // ═══════════════════════════════════════════════════════════════════════════
        // BUG-003 FIX: Build implicit edges from task-level use: wiring (v0.22.4)
        // use: { data: step1 } creates implicit dependency: step1 -> this_task
        // ═══════════════════════════════════════════════════════════════════════════
        for task in &workflow.tasks {
            if let Some(ref wiring) = task.use_wiring {
                let tgt_arc = task_set
                    .get(task.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| intern(&task.id));

                for entry in wiring.values() {
                    let dep_task_id = entry.task_id();

                    // Skip self-references
                    if dep_task_id == task.id {
                        continue;
                    }

                    // Skip non-task paths: context.*, inputs.*, etc.
                    // Only create edges for actual task references
                    if !task_set.contains(dep_task_id) {
                        continue;
                    }

                    let src_arc = task_set
                        .get(dep_task_id)
                        .cloned()
                        .unwrap_or_else(|| intern(dep_task_id));

                    // Add edge: dependency -> current_task (avoid duplicates)
                    let adj_entry = adjacency.entry(Arc::clone(&src_arc)).or_default();
                    if !adj_entry.iter().any(|x| x.as_ref() == tgt_arc.as_ref()) {
                        adj_entry.push(Arc::clone(&tgt_arc));
                    }

                    let pred_entry = predecessors.entry(Arc::clone(&tgt_arc)).or_default();
                    if !pred_entry.iter().any(|x| x.as_ref() == src_arc.as_ref()) {
                        pred_entry.push(src_arc);
                    }
                }
            }
        }

        Ok(Self {
            adjacency,
            predecessors,
            task_ids,
            task_set,
        })
    }

    /// Get dependencies of a task (returns `Arc<str>` slice)
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
    /// Returns `Arc<str>` for zero-cost cloning by caller.
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

    // ═══════════════════════════════════════════════════════════════════════════
    // BUG-004 FIX: Get deepest terminal task for output selection (v0.22.4)
    // ═══════════════════════════════════════════════════════════════════════════

    /// Compute topological depth for each node (longest path from any root)
    fn compute_depths(&self) -> FxHashMap<Arc<str>, usize> {
        let mut depths: FxHashMap<Arc<str>, usize> =
            FxHashMap::with_capacity_and_hasher(self.task_ids.len(), Default::default());

        // Initialize roots (no predecessors) with depth 0
        for task_id in &self.task_ids {
            if self
                .predecessors
                .get(task_id.as_ref())
                .is_none_or(SmallVec::is_empty)
            {
                depths.insert(Arc::clone(task_id), 0);
            }
        }

        // Process remaining nodes in topological order
        let mut remaining: FxHashSet<Arc<str>> = self
            .task_ids
            .iter()
            .filter(|id| !depths.contains_key(id.as_ref()))
            .cloned()
            .collect();

        while !remaining.is_empty() {
            let mut progress = false;
            let mut to_remove = Vec::new();

            for task_id in &remaining {
                let preds = self.get_dependencies(task_id.as_ref());
                if preds.iter().all(|p| depths.contains_key(p.as_ref())) {
                    let max_pred_depth = preds
                        .iter()
                        .filter_map(|p| depths.get(p.as_ref()).copied())
                        .max()
                        .unwrap_or(0);
                    depths.insert(Arc::clone(task_id), max_pred_depth + 1);
                    to_remove.push(Arc::clone(task_id));
                    progress = true;
                }
            }

            for id in to_remove {
                remaining.remove(&id);
            }

            // Break if no progress (cycle or issue)
            if !progress {
                break;
            }
        }

        depths
    }

    /// Get the deepest terminal task (for workflow output selection)
    ///
    /// Returns the terminal task with highest topological depth.
    /// On ties, picks the one that appears last in task definition order.
    pub fn get_deepest_final_task(&self) -> Option<Arc<str>> {
        let final_tasks = self.get_final_tasks();

        // Fast path: single or no terminal
        if final_tasks.len() <= 1 {
            return final_tasks.into_iter().next();
        }

        let depths = self.compute_depths();

        final_tasks.into_iter().max_by(|a, b| {
            let depth_a = depths.get(a.as_ref()).copied().unwrap_or(0);
            let depth_b = depths.get(b.as_ref()).copied().unwrap_or(0);
            depth_a.cmp(&depth_b).then_with(|| {
                // Tiebreaker: task definition order (last defined wins)
                let pos_a = self.task_ids.iter().position(|x| x == a).unwrap_or(0);
                let pos_b = self.task_ids.iter().position(|x| x == b).unwrap_or(0);
                pos_a.cmp(&pos_b)
            })
        })
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
    use crate::serde_yaml;

    // ═══════════════════════════════════════════════════════════════
    // BUG-003: IMPLICIT DEPENDS_ON FROM USE: WIRING
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_use_wiring_creates_implicit_dependency() {
        // BUG-003: use: { data: step1 } should create step1 -> step2 edge
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_implicit_dep
tasks:
  - id: step1
    infer: "Generate data"
  - id: step2
    use:
      data: step1
    infer: "Process: {{use.data}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        // step2 should depend on step1 (implicit from use:)
        let deps = dag.get_dependencies("step2");
        assert!(
            deps.iter().any(|d| d.as_ref() == "step1"),
            "step2 should have implicit dependency on step1 from use: block"
        );
    }

    #[test]
    fn test_use_wiring_no_duplicate_edges() {
        // When both use: and depends_on: reference same task, only 1 edge
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_no_dup
tasks:
  - id: step1
    infer: "Generate"
  - id: step2
    depends_on: [step1]
    use:
      data: step1
    infer: "{{use.data}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deps = dag.get_dependencies("step2");
        // Should have exactly 1 edge, not 2 (deduped)
        let step1_count = deps.iter().filter(|d| d.as_ref() == "step1").count();
        assert_eq!(
            step1_count, 1,
            "Should have exactly 1 edge to step1, not duplicated"
        );
    }

    #[test]
    fn test_use_wiring_skips_context_refs() {
        // context.files.* paths should NOT create task dependencies
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_context_ref
tasks:
  - id: step1
    use:
      brand: context.files.brand
    infer: "{{use.brand}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deps = dag.get_dependencies("step1");
        assert!(deps.is_empty(), "context refs should not create task deps");
    }

    #[test]
    fn test_use_wiring_skips_inputs_refs() {
        // inputs.* paths should NOT create task dependencies
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_inputs_ref
inputs:
  message:
    type: string
tasks:
  - id: step1
    use:
      msg: inputs.message
    infer: "{{use.msg}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deps = dag.get_dependencies("step1");
        assert!(deps.is_empty(), "inputs refs should not create task deps");
    }

    #[test]
    fn test_use_wiring_multiple_deps() {
        // Multiple use: entries create multiple edges
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_multi_deps
tasks:
  - id: a
    infer: "A"
  - id: b
    infer: "B"
  - id: c
    use:
      data_a: a
      data_b: b
    infer: "{{use.data_a}} + {{use.data_b}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deps = dag.get_dependencies("c");
        assert!(
            deps.iter().any(|d| d.as_ref() == "a"),
            "c should depend on a"
        );
        assert!(
            deps.iter().any(|d| d.as_ref() == "b"),
            "c should depend on b"
        );
    }

    #[test]
    fn test_use_wiring_with_field_path() {
        // use: { data: task.field } should create dependency on "task"
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_field_path
tasks:
  - id: producer
    infer: "Produce"
  - id: consumer
    use:
      name: producer.name
      price: producer.price
    infer: "{{use.name}} costs {{use.price}}"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deps = dag.get_dependencies("consumer");
        // Both refs are to "producer" (task_id extracts first segment)
        let producer_count = deps.iter().filter(|d| d.as_ref() == "producer").count();
        assert_eq!(
            producer_count, 1,
            "Should have exactly 1 edge to producer (deduped)"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // BUG-004: DEEPEST TERMINAL TASK SELECTION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_deepest_final_task_simple_chain() {
        // a -> b -> c (depths: 0, 1, 2)
        // c should be selected as deepest terminal
        let yaml = r#"
schema: nika/workflow@0.9
workflow: chain
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
  - id: c
    depends_on: [b]
    infer: "C"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(deepest.unwrap().as_ref(), "c");
    }

    #[test]
    fn test_deepest_final_task_branching() {
        // source -> branch_a (depth 1, terminal)
        //        -> branch_b -> final (depth 2, terminal)
        // final should be selected (depth 2 > depth 1)
        let yaml = r#"
schema: nika/workflow@0.9
workflow: branching
tasks:
  - id: source
    infer: "Source"
  - id: branch_a
    depends_on: [source]
    infer: "A"
  - id: branch_b
    depends_on: [source]
    infer: "B"
  - id: final
    depends_on: [branch_b]
    infer: "Final"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(deepest.unwrap().as_ref(), "final");
    }

    #[test]
    fn test_deepest_final_task_parallel_same_depth() {
        // a -> b (depth 1)
        // a -> c (depth 1)
        // Both terminals at same depth, pick last defined (c)
        let yaml = r#"
schema: nika/workflow@0.9
workflow: parallel
tasks:
  - id: a
    infer: "A"
  - id: b
    depends_on: [a]
    infer: "B"
  - id: c
    depends_on: [a]
    infer: "C"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(
            deepest.unwrap().as_ref(),
            "c",
            "Should pick last defined on tie"
        );
    }

    #[test]
    fn test_deepest_final_task_single() {
        // Single task workflow
        let yaml = r#"
schema: nika/workflow@0.9
workflow: single
tasks:
  - id: only
    infer: "Only task"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let dag = Dag::from_workflow(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(deepest.unwrap().as_ref(), "only");
    }

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
        let graph = Dag::from_workflow(&workflow).unwrap();

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
        let graph = Dag::from_workflow(&workflow).unwrap();

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
        let graph = Dag::from_workflow(&workflow).unwrap();

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
        let graph = Dag::from_workflow(&workflow).unwrap();

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
        let graph = Dag::from_workflow(&workflow).unwrap();

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
        let graph = Dag::from_workflow(&workflow).unwrap();

        let result = graph.detect_cycles();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        // Should contain cycle path
        assert!(err_msg.contains("→"));
    }

    // ═══════════════════════════════════════════════════════════════
    // BUG-001: DUPLICATE TASK ID DETECTION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_duplicate_task_id_detection() {
        // BUG-001 FIX: Duplicate task IDs should return error, not silently overwrite
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_duplicate
tasks:
  - id: fetch
    infer: "First fetch"
  - id: fetch
    infer: "Second fetch (duplicate!)"
  - id: process
    infer: "Process results"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let result = Dag::from_workflow(&workflow);

        assert!(result.is_err(), "Should detect duplicate task ID 'fetch'");
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("Expected error for duplicate task ID"),
        };
        assert!(
            err.to_string().contains("NIKA-022"),
            "Should be NIKA-022 error code"
        );
        assert!(
            err.to_string().contains("fetch"),
            "Should mention the duplicate task ID"
        );
    }

    #[test]
    fn test_unique_task_ids_ok() {
        // No duplicates should pass
        let yaml = r#"
schema: nika/workflow@0.9
workflow: test_unique
tasks:
  - id: fetch
    infer: "Fetch"
  - id: process
    infer: "Process"
  - id: report
    infer: "Report"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let result = Dag::from_workflow(&workflow);

        assert!(result.is_ok(), "Unique task IDs should succeed");
    }
}
