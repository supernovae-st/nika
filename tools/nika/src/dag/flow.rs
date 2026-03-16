//! Dag - DAG structure built from workflow definitions
//!
//! Performance optimizations:
//! - `Arc<str>` for zero-cost cloning of task IDs
//! - FxHashMap for faster hashing (non-crypto, ~2x faster)
//! - SmallVec for stack-allocated small dependency lists (0-4 items)
//!
//! DAG Validation:
//! - Cycle detection using DFS three-color algorithm
//!
//! Two entry points:
//! - `from_workflow(&Workflow)` — runtime path (runner, CLI, TUI)
//! - `from_analyzed(&AnalyzedWorkflow)` — analyzer pipeline

use std::collections::VecDeque;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::ast::analyzed::AnalyzedWorkflow;
use crate::ast::Workflow;
use crate::error::NikaError;
use crate::util::intern;

/// Stack-allocated deps: most tasks have 0-4 dependencies
pub(crate) type DepVec = SmallVec<[Arc<str>; 4]>;

/// Graph of task dependencies built from analyzed workflow
///
/// Uses `Arc<str>` + FxHashMap + SmallVec for maximum performance.
///
/// Built from `AnalyzedWorkflow` where dependencies are pre-computed:
/// - `task.depends_on: Vec<TaskId>` — explicit ordering edges
/// - `task.implicit_deps: Vec<TaskId>` — auto-extracted from `with:` bindings
#[derive(Debug)]
pub struct Dag {
    /// task_id -> list of successor task_ids (SmallVec: stack-allocated for ≤4)
    adjacency: FxHashMap<Arc<str>, DepVec>,
    /// task_id -> list of predecessor task_ids (SmallVec: stack-allocated for ≤4)
    predecessors: FxHashMap<Arc<str>, DepVec>,
    /// All task IDs (for iteration)
    task_ids: Vec<Arc<str>>,
    /// Quick lookup for task existence (FxHashSet: faster hashing)
    task_set: FxHashSet<Arc<str>>,
}

impl Dag {
    /// Build a DAG from an analyzed workflow.
    ///
    /// The `AnalyzedWorkflow` has already validated unique task IDs and
    /// pre-computed both explicit (`depends_on`) and implicit (`implicit_deps`)
    /// dependencies during the analysis phase.
    ///
    /// # Errors
    ///
    /// Returns `Err(NikaError::DuplicateTaskId)` if duplicate task IDs are
    /// found (defense-in-depth; analyzer should have caught this).
    pub fn from_analyzed(workflow: &AnalyzedWorkflow) -> Result<Self, NikaError> {
        let capacity = workflow.tasks.len();
        let mut adjacency: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut predecessors: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut task_ids: Vec<Arc<str>> = Vec::with_capacity(capacity);
        let mut task_set: FxHashSet<Arc<str>> =
            FxHashSet::with_capacity_and_hasher(capacity, Default::default());

        // Intern task IDs once, reuse everywhere (single allocation per unique ID)
        // Defense-in-depth: duplicate check (analyzer already validates this)
        for task in &workflow.tasks {
            let id = intern(&task.name);
            if task_set.contains(&id) {
                return Err(NikaError::DuplicateTaskId {
                    task_id: task.name.clone(),
                });
            }
            task_ids.push(Arc::clone(&id));
            task_set.insert(Arc::clone(&id));
            adjacency.insert(Arc::clone(&id), DepVec::new());
            predecessors.insert(id, DepVec::new());
        }

        // Build edges from pre-computed dependencies (depends_on + implicit_deps).
        // Both are Vec<TaskId> resolved by the analyzer.
        for task in &workflow.tasks {
            let tgt_arc = task_set
                .get(task.name.as_str())
                .cloned()
                .unwrap_or_else(|| intern(&task.name));

            // Collect all dependency TaskIds, deduplicating across depends_on and implicit_deps
            let mut seen_deps: FxHashSet<&str> = FxHashSet::default();

            // Process explicit depends_on edges
            for dep_id in &task.depends_on {
                let dep_name = workflow.task_table.get_name(*dep_id).ok_or_else(|| {
                    NikaError::MissingDependency {
                        task_id: task.name.clone(),
                        dep_id: format!("TaskId({})", dep_id.0),
                    }
                })?;

                if dep_name == task.name {
                    continue; // Skip self-references
                }
                if !seen_deps.insert(dep_name) {
                    continue; // Already processed
                }

                let src_arc = task_set
                    .get(dep_name)
                    .cloned()
                    .unwrap_or_else(|| intern(dep_name));

                adjacency
                    .entry(Arc::clone(&src_arc))
                    .or_default()
                    .push(Arc::clone(&tgt_arc));
                predecessors
                    .entry(Arc::clone(&tgt_arc))
                    .or_default()
                    .push(src_arc);
            }

            // Process implicit dependencies (from with: bindings)
            for dep_id in &task.implicit_deps {
                let dep_name = workflow.task_table.get_name(*dep_id).ok_or_else(|| {
                    NikaError::MissingDependency {
                        task_id: task.name.clone(),
                        dep_id: format!("TaskId({})", dep_id.0),
                    }
                })?;

                if dep_name == task.name {
                    continue; // Skip self-references
                }
                if !seen_deps.insert(dep_name) {
                    continue; // Already in depends_on, skip duplicate
                }

                let src_arc = task_set
                    .get(dep_name)
                    .cloned()
                    .unwrap_or_else(|| intern(dep_name));

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
    // Get deepest terminal task for output selection
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
                            let cycle_start = stack
                                .iter()
                                .position(|x| x.as_ref() == neighbor.as_ref())
                                .unwrap_or(0);
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

    // ═══════════════════════════════════════════════════════════════
    // Workflow → Dag (runtime path)
    // ═══════════════════════════════════════════════════════════════

    /// Build a DAG from a `Workflow` struct.
    ///
    /// Used by the runtime runner, CLI check command, and TUI standalone.
    /// New code targeting the analyzer pipeline should use `Dag::from_analyzed()`.
    pub fn from_workflow(workflow: &Workflow) -> Result<Self, NikaError> {
        let capacity = workflow.tasks.len();
        let mut adjacency: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut predecessors: FxHashMap<Arc<str>, DepVec> =
            FxHashMap::with_capacity_and_hasher(capacity, Default::default());
        let mut task_ids: Vec<Arc<str>> = Vec::with_capacity(capacity);
        let mut task_set: FxHashSet<Arc<str>> =
            FxHashSet::with_capacity_and_hasher(capacity, Default::default());

        // Register all task IDs
        for task in &workflow.tasks {
            let id = intern(&task.id);
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

        // Track seen edges to deduplicate between task.flow and with_spec implicit edges.
        let mut seen_edges: FxHashSet<(Arc<str>, Arc<str>)> = FxHashSet::default();

        /// Insert an edge if not already seen, updating adjacency and predecessors.
        fn insert_edge(
            src: &Arc<str>,
            tgt: &Arc<str>,
            seen: &mut FxHashSet<(Arc<str>, Arc<str>)>,
            adjacency: &mut FxHashMap<Arc<str>, DepVec>,
            predecessors: &mut FxHashMap<Arc<str>, DepVec>,
        ) {
            let key = (Arc::clone(src), Arc::clone(tgt));
            if seen.insert(key) {
                adjacency
                    .entry(Arc::clone(src))
                    .or_default()
                    .push(Arc::clone(tgt));
                predecessors
                    .entry(Arc::clone(tgt))
                    .or_default()
                    .push(Arc::clone(src));
            }
        }

        // Build edges from task-level flow/depends_on
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
                    insert_edge(
                        &src_arc,
                        &tgt_arc,
                        &mut seen_edges,
                        &mut adjacency,
                        &mut predecessors,
                    );
                }
            }

            // Add implicit edges from with: binding references
            if let Some(ref with_spec) = task.with_spec {
                let tgt_arc = task_set
                    .get(task.id.as_str())
                    .cloned()
                    .unwrap_or_else(|| intern(&task.id));
                for entry in with_spec.values() {
                    let Some(from_task) = entry.task_id() else {
                        continue; // Context/Input/Env/LoopVar — not a task ref
                    };
                    if from_task == task.id || !task_set.contains(from_task) {
                        continue;
                    }
                    let src_arc = task_set
                        .get(from_task)
                        .cloned()
                        .unwrap_or_else(|| intern(from_task));
                    insert_edge(
                        &src_arc,
                        &tgt_arc,
                        &mut seen_edges,
                        &mut adjacency,
                        &mut predecessors,
                    );
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::{
        AnalyzedForEach, AnalyzedInferAction, AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow,
        TaskId, TaskTable,
    };
    use crate::binding::WithSpec;
    use crate::source::Span;

    /// Helper: build an AnalyzedWorkflow from a list of task descriptors.
    ///
    /// Each descriptor is (name, depends_on_names, implicit_dep_names).
    fn build_workflow(descriptors: &[(&str, &[&str], &[&str])]) -> AnalyzedWorkflow {
        let mut task_table = TaskTable::new();
        let mut tasks = Vec::new();

        // First pass: insert all task names into the table
        for (name, _, _) in descriptors {
            task_table.insert(name);
        }

        // Second pass: build AnalyzedTask structs with resolved IDs
        for (name, depends_on_names, implicit_dep_names) in descriptors {
            let id = task_table.get_id(name).unwrap();
            let depends_on: Vec<TaskId> = depends_on_names
                .iter()
                .filter_map(|n| task_table.get_id(n))
                .collect();
            let implicit_deps: Vec<TaskId> = implicit_dep_names
                .iter()
                .filter_map(|n| task_table.get_id(n))
                .collect();

            tasks.push(AnalyzedTask {
                id,
                name: name.to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on,
                implicit_deps,
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            });
        }

        AnalyzedWorkflow {
            task_table,
            tasks,
            ..Default::default()
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // IMPLICIT DEPENDENCIES FROM with: BINDINGS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_implicit_dep_creates_edge() {
        // with: { data: step1.result } → implicit_deps=[step1] → step1 -> step2 edge
        let workflow = build_workflow(&[("step1", &[], &[]), ("step2", &[], &["step1"])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deps = dag.get_dependencies("step2");
        assert!(
            deps.iter().any(|d| d.as_ref() == "step1"),
            "step2 should have implicit dependency on step1 from with: binding"
        );
    }

    #[test]
    fn test_no_duplicate_edges() {
        // When both depends_on and implicit_deps reference same task, only 1 edge
        let workflow = build_workflow(&[("step1", &[], &[]), ("step2", &["step1"], &["step1"])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deps = dag.get_dependencies("step2");
        let step1_count = deps.iter().filter(|d| d.as_ref() == "step1").count();
        assert_eq!(
            step1_count, 1,
            "Should have exactly 1 edge to step1, not duplicated"
        );
    }

    #[test]
    fn test_no_deps_for_context_only_tasks() {
        // Tasks with no depends_on and no implicit_deps should have no predecessors
        let workflow = build_workflow(&[("step1", &[], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deps = dag.get_dependencies("step1");
        assert!(
            deps.is_empty(),
            "Task with no deps should have no predecessors"
        );
    }

    #[test]
    fn test_multiple_implicit_deps() {
        // Multiple with: entries create multiple edges
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &[], &[]), ("c", &[], &["a", "b"])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

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
    fn test_mixed_depends_on_and_implicit() {
        // depends_on=[a], implicit_deps=[b] → both edges present, no duplicates
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &[], &[]), ("c", &["a"], &["b"])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deps = dag.get_dependencies("c");
        assert_eq!(deps.len(), 2, "Should have 2 dependencies (a + b)");
        assert!(deps.iter().any(|d| d.as_ref() == "a"));
        assert!(deps.iter().any(|d| d.as_ref() == "b"));
    }

    // ═══════════════════════════════════════════════════════════════
    // DEEPEST TERMINAL TASK SELECTION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_deepest_final_task_simple_chain() {
        // a -> b -> c (depths: 0, 1, 2) — c should be deepest terminal
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(deepest.unwrap().as_ref(), "c");
    }

    #[test]
    fn test_deepest_final_task_branching() {
        // source -> branch_a (depth 1, terminal)
        //        -> branch_b -> final (depth 2, terminal)
        // final should be selected (depth 2 > depth 1)
        let workflow = build_workflow(&[
            ("source", &[], &[]),
            ("branch_a", &["source"], &[]),
            ("branch_b", &["source"], &[]),
            ("final", &["branch_b"], &[]),
        ]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(deepest.unwrap().as_ref(), "final");
    }

    #[test]
    fn test_deepest_final_task_parallel_same_depth() {
        // a -> b (depth 1)
        // a -> c (depth 1)
        // Both terminals at same depth, pick last defined (c)
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &["a"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(
            deepest.unwrap().as_ref(),
            "c",
            "Should pick last defined on tie"
        );
    }

    #[test]
    fn test_deepest_final_task_single() {
        let workflow = build_workflow(&[("only", &[], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let deepest = dag.get_deepest_final_task();
        assert_eq!(deepest.unwrap().as_ref(), "only");
    }

    // ═══════════════════════════════════════════════════════════════
    // CYCLE DETECTION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_detect_cycle_simple() {
        // A → B → C → A (cycle via depends_on)
        let workflow =
            build_workflow(&[("a", &["c"], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let result = dag.detect_cycles();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-020"));
    }

    #[test]
    fn test_no_cycle_linear() {
        // A → B → C (no cycle)
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.detect_cycles().is_ok());
    }

    #[test]
    fn test_diamond_no_cycle() {
        // Diamond: A → B, A → C, B → D, C → D (no cycle)
        let workflow = build_workflow(&[
            ("a", &[], &[]),
            ("b", &["a"], &[]),
            ("c", &["a"], &[]),
            ("d", &["b", "c"], &[]),
        ]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.detect_cycles().is_ok());
        assert_eq!(dag.get_final_tasks().len(), 1);
        assert!(dag.has_path("a", "d"));
    }

    #[test]
    fn test_disconnected_no_cycle() {
        // Two disconnected chains: A → B, C → D (no cycle)
        let workflow = build_workflow(&[
            ("a", &[], &[]),
            ("b", &["a"], &[]),
            ("c", &[], &[]),
            ("d", &["c"], &[]),
        ]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.detect_cycles().is_ok());
        assert_eq!(dag.get_final_tasks().len(), 2);
    }

    #[test]
    fn test_cycle_path_includes_all_nodes() {
        // A → B → C → A: cycle path should show the cycle
        let workflow =
            build_workflow(&[("a", &["c"], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let result = dag.detect_cycles();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("→"));
    }

    #[test]
    fn test_cycle_via_implicit_deps() {
        // Cycle via implicit_deps: a has implicit dep on c, c depends on b, b depends on a
        let workflow =
            build_workflow(&[("a", &[], &["c"]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let result = dag.detect_cycles();
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // DUPLICATE TASK ID DETECTION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_duplicate_task_id_detection() {
        // Manually construct workflow with duplicate task names
        // (normally analyzer prevents this, but test defense-in-depth)
        let mut task_table = TaskTable::new();
        let id0 = task_table.insert("fetch");
        let id1 = task_table.insert("process");

        let tasks = vec![
            AnalyzedTask {
                id: id0,
                name: "fetch".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: Vec::new(),
                implicit_deps: Vec::new(),
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
            // Duplicate name — same "fetch" string but different TaskId
            AnalyzedTask {
                id: TaskId::new(99),
                name: "fetch".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: Vec::new(),
                implicit_deps: Vec::new(),
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
            AnalyzedTask {
                id: id1,
                name: "process".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: Vec::new(),
                implicit_deps: Vec::new(),
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
        ];

        let workflow = AnalyzedWorkflow {
            task_table,
            tasks,
            ..Default::default()
        };

        let result = Dag::from_analyzed(&workflow);
        assert!(result.is_err(), "Should detect duplicate task name 'fetch'");
        let err = result.unwrap_err();
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
        let workflow = build_workflow(&[
            ("fetch", &[], &[]),
            ("process", &[], &[]),
            ("report", &[], &[]),
        ]);
        let result = Dag::from_analyzed(&workflow);
        assert!(result.is_ok(), "Unique task IDs should succeed");
    }

    // ═══════════════════════════════════════════════════════════════
    // GRAPH QUERY METHODS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_get_final_tasks() {
        // a -> b, c (standalone) → final tasks = b, c
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &[], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let finals = dag.get_final_tasks();
        assert_eq!(finals.len(), 2);
        let names: Vec<&str> = finals.iter().map(|f| f.as_ref()).collect();
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));
    }

    #[test]
    fn test_has_path() {
        // a -> b -> c
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &["b"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.has_path("a", "c"));
        assert!(dag.has_path("a", "b"));
        assert!(dag.has_path("b", "c"));
        assert!(!dag.has_path("c", "a"));
        assert!(!dag.has_path("b", "a"));
    }

    #[test]
    fn test_contains() {
        let workflow = build_workflow(&[("alpha", &[], &[]), ("beta", &[], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.contains("alpha"));
        assert!(dag.contains("beta"));
        assert!(!dag.contains("gamma"));
    }

    #[test]
    fn test_get_successors() {
        // a -> b, a -> c
        let workflow = build_workflow(&[("a", &[], &[]), ("b", &["a"], &[]), ("c", &["a"], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        let succs = dag.get_successors("a");
        assert_eq!(succs.len(), 2);
        let names: Vec<&str> = succs.iter().map(|s| s.as_ref()).collect();
        assert!(names.contains(&"b"));
        assert!(names.contains(&"c"));

        // Terminal nodes have no successors
        assert!(dag.get_successors("b").is_empty());
        assert!(dag.get_successors("c").is_empty());
    }

    #[test]
    fn test_empty_workflow() {
        let workflow = build_workflow(&[]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.get_final_tasks().is_empty());
        assert!(dag.get_deepest_final_task().is_none());
        assert!(dag.detect_cycles().is_ok());
    }

    #[test]
    fn test_single_task_no_deps() {
        let workflow = build_workflow(&[("solo", &[], &[])]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.get_dependencies("solo").is_empty());
        assert!(dag.get_successors("solo").is_empty());
        assert_eq!(dag.get_final_tasks().len(), 1);
        assert_eq!(dag.get_deepest_final_task().unwrap().as_ref(), "solo");
        assert!(dag.detect_cycles().is_ok());
    }

    #[test]
    fn test_complex_dag_6_tasks() {
        // Complex DAG:
        //   a → b → d → f
        //   a → c → e → f
        //   b → e (cross-edge)
        let workflow = build_workflow(&[
            ("a", &[], &[]),
            ("b", &["a"], &[]),
            ("c", &["a"], &[]),
            ("d", &["b"], &[]),
            ("e", &["c", "b"], &[]),
            ("f", &["d", "e"], &[]),
        ]);
        let dag = Dag::from_analyzed(&workflow).unwrap();

        assert!(dag.detect_cycles().is_ok());
        assert_eq!(dag.get_final_tasks().len(), 1);
        assert_eq!(dag.get_deepest_final_task().unwrap().as_ref(), "f");
        assert!(dag.has_path("a", "f"));
        assert!(dag.has_path("b", "e"));
        assert!(!dag.has_path("d", "e"));
    }

    // ═══════════════════════════════════════════════════════════════
    // DANGLING DEPENDS_ON VALIDATION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_dangling_depends_on_returns_error() {
        // Build workflow where task "b" has a depends_on pointing to a TaskId
        // that doesn't exist in the TaskTable (simulating internal invariant violation).
        let mut task_table = TaskTable::new();
        task_table.insert("a");
        task_table.insert("b");

        let id_a = task_table.get_id("a").unwrap();
        let id_b = task_table.get_id("b").unwrap();

        // TaskId(99) doesn't exist in the table — this is the dangling reference
        let dangling_id = TaskId(99);

        let tasks = vec![
            AnalyzedTask {
                id: id_a,
                name: "a".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: vec![],
                implicit_deps: vec![],
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
            AnalyzedTask {
                id: id_b,
                name: "b".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: vec![dangling_id], // dangling!
                implicit_deps: vec![],
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
        ];

        let workflow = AnalyzedWorkflow {
            task_table,
            tasks,
            ..Default::default()
        };

        let result = Dag::from_analyzed(&workflow);
        assert!(result.is_err(), "Dangling depends_on should be rejected");
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("unknown"),
            "Error should mention unknown dependency, got: {}",
            err
        );
    }

    #[test]
    fn test_dangling_implicit_dep_returns_error() {
        // Same as above but for implicit_deps (from with: bindings)
        let mut task_table = TaskTable::new();
        task_table.insert("x");
        task_table.insert("y");

        let id_x = task_table.get_id("x").unwrap();
        let id_y = task_table.get_id("y").unwrap();

        let dangling_id = TaskId(42);

        let tasks = vec![
            AnalyzedTask {
                id: id_x,
                name: "x".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: vec![],
                implicit_deps: vec![],
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
            AnalyzedTask {
                id: id_y,
                name: "y".to_string(),
                description: None,
                action: AnalyzedTaskAction::Infer(AnalyzedInferAction::default()),
                provider: None,
                model: None,
                with_spec: WithSpec::default(),
                depends_on: vec![],
                implicit_deps: vec![dangling_id], // dangling!
                output: None,
                for_each: None,
                retry: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                span: Span::dummy(),
            },
        ];

        let workflow = AnalyzedWorkflow {
            task_table,
            tasks,
            ..Default::default()
        };

        let result = Dag::from_analyzed(&workflow);
        assert!(result.is_err(), "Dangling implicit_dep should be rejected");
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH SAFETY: Prove no false rejections
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn test_for_each_workflow_dag_construction_succeeds() {
        // Prove that for_each tasks with depends_on pass through Dag::from_analyzed
        // without triggering false MissingDependency errors.
        // for_each expansion happens at RUNTIME (runner.rs), not here.
        let mut task_table = TaskTable::new();
        task_table.insert("produce");
        task_table.insert("process");
        task_table.insert("aggregate");

        let id_produce = task_table.get_id("produce").unwrap();
        let id_process = task_table.get_id("process").unwrap();

        let workflow = AnalyzedWorkflow {
            tasks: vec![
                AnalyzedTask {
                    name: "produce".to_string(),
                    ..Default::default()
                },
                AnalyzedTask {
                    name: "process".to_string(),
                    depends_on: vec![id_produce],
                    for_each: Some(AnalyzedForEach {
                        items: r#"["a", "b", "c"]"#.to_string(),
                        as_var: "item".to_string(),
                        parallel: Some(3),
                        fail_fast: true,
                        span: Span::dummy(),
                    }),
                    ..Default::default()
                },
                AnalyzedTask {
                    name: "aggregate".to_string(),
                    depends_on: vec![id_process],
                    ..Default::default()
                },
            ],
            task_table,
            ..Default::default()
        };

        let result = Dag::from_analyzed(&workflow);
        assert!(
            result.is_ok(),
            "for_each workflow should build DAG without false rejections: {:?}",
            result.err()
        );

        let dag = result.unwrap();
        assert!(dag.has_path("produce", "process"));
        assert!(dag.has_path("process", "aggregate"));
    }
}
