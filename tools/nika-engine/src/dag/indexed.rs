// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Vec-indexed DAG with Kahn's algorithm.
//!
//! A compact, cache-friendly DAG representation using `Vec<SmallVec<[TaskId; 4]>>`
//! adjacency lists indexed by `TaskId`. Replaces HashMap-based DAG for runtime
//! execution with O(1) lookups, zero hashing, and pre-computed topological order.
//!
//! ## Design
//!
//! - **DepVec**: `SmallVec<[TaskId; 4]>` — stack-allocated for ≤4 deps (95% of tasks)
//! - **TopoOrder**: Pre-computed topological ordering with depth info
//! - **kahn_sort**: BFS topological sort with simultaneous cycle detection + depth computation
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika::dag::indexed::IndexedDag;
//! use nika::ast::analyzed::AnalyzedWorkflow;
//!
//! let dag = IndexedDag::from_analyzed(&workflow)?;
//! for &task_id in dag.topo_order() {
//!     if dag.all_deps_done(task_id, &done) {
//!         // execute task
//!     }
//! }
//! ```

use rustc_hash::FxHashSet;
use smallvec::SmallVec;

use crate::ast::analyzed::{AnalyzedWorkflow, TaskId, TaskTable};
use crate::error::NikaError;
use crate::error_domains::DagError;

// ═══════════════════════════════════════════════════════════════════════════
// DepVec
// ═══════════════════════════════════════════════════════════════════════════

/// Dependency vector — stack-allocated for ≤4 dependencies.
///
/// 95% of workflow tasks have ≤4 deps, so this avoids heap allocation
/// in the common case while supporting arbitrary fan-out.
pub(crate) type DepVec = SmallVec<[TaskId; 4]>;

// ═══════════════════════════════════════════════════════════════════════════
// TopoOrder
// ═══════════════════════════════════════════════════════════════════════════

/// Pre-computed topological ordering with depth information.
///
/// Computed once by `kahn_sort` and stored immutably in `IndexedDag`.
/// Both arrays are `Box<[T]>` — no capacity waste.
#[derive(Debug, Clone)]
pub struct TopoOrder {
    /// Tasks in topological order (roots first).
    order: Box<[TaskId]>,
    /// Depth of each task, indexed by `TaskId.index()`.
    /// Roots have depth 0, successors have max(predecessor depths) + 1.
    depths: Box<[u32]>,
}

impl TopoOrder {
    /// Tasks in topological order.
    pub fn order(&self) -> &[TaskId] {
        &self.order
    }

    /// Depth of a task (0 = root).
    pub fn depth(&self, id: TaskId) -> u32 {
        self.depths[id.index() as usize]
    }

    /// Maximum depth in the DAG.
    pub fn max_depth(&self) -> u32 {
        self.depths.iter().copied().max().unwrap_or(0)
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// IndexedDag
// ═══════════════════════════════════════════════════════════════════════════

/// Vec-indexed DAG for runtime execution.
///
/// All lookups are O(1) array indexing — no hashing.
/// Constructed from `AnalyzedWorkflow` via `from_analyzed()`.
#[derive(Debug, Clone)]
pub struct IndexedDag {
    /// Forward edges: task → its successors (tasks that depend on it).
    successors: Vec<DepVec>,
    /// Backward edges: task → its predecessors (tasks it depends on).
    predecessors: Vec<DepVec>,
    /// Pre-computed topological order + depths.
    topo: TopoOrder,
    /// Number of tasks.
    num_tasks: usize,
}

impl IndexedDag {
    /// Build from an AnalyzedWorkflow.
    ///
    /// Constructs adjacency lists from `depends_on` + `implicit_deps`,
    /// then runs Kahn's algorithm for topological sort + cycle detection.
    pub fn from_analyzed(wf: &AnalyzedWorkflow) -> Result<Self, NikaError> {
        let n = wf.tasks.len();
        let mut successors: Vec<DepVec> = vec![DepVec::new(); n];
        let mut predecessors: Vec<DepVec> = vec![DepVec::new(); n];
        let mut in_degree = vec![0u32; n];

        for task in &wf.tasks {
            let idx = task.id.index() as usize;
            // Deduplicate edges across depends_on and implicit_deps to prevent
            // inflated in_degree which causes false cycle detection (Bug #23).
            let mut seen_deps: FxHashSet<TaskId> = FxHashSet::default();
            // Combine explicit depends_on and implicit deps from with: bindings
            for &dep_id in task.depends_on.iter().chain(task.implicit_deps.iter()) {
                if !seen_deps.insert(dep_id) {
                    continue; // Skip duplicate edge
                }
                let dep_idx = dep_id.index() as usize;
                // dep → task (dep is predecessor of task)
                successors[dep_idx].push(task.id);
                predecessors[idx].push(dep_id);
                in_degree[idx] += 1;
            }
        }

        let topo = kahn_sort(&successors, &mut in_degree, n, &wf.task_table)?;

        Ok(Self {
            successors,
            predecessors,
            topo,
            num_tasks: n,
        })
    }

    /// Get predecessors (dependencies) of a task.
    pub fn dependencies(&self, id: TaskId) -> &[TaskId] {
        &self.predecessors[id.index() as usize]
    }

    /// Get successors (dependents) of a task.
    pub fn successors(&self, id: TaskId) -> &[TaskId] {
        &self.successors[id.index() as usize]
    }

    /// Tasks in topological order (roots first).
    pub fn topo_order(&self) -> &[TaskId] {
        self.topo.order()
    }

    /// Depth of a task in the DAG (0 = root).
    pub fn depth(&self, id: TaskId) -> u32 {
        self.topo.depth(id)
    }

    /// Maximum depth in the DAG.
    pub fn max_depth(&self) -> u32 {
        self.topo.max_depth()
    }

    /// Tasks with no successors (leaf/final tasks).
    pub fn final_tasks(&self) -> Vec<TaskId> {
        self.successors
            .iter()
            .enumerate()
            .filter(|(_, succs)| succs.is_empty())
            .map(|(i, _)| TaskId::new(i as u32))
            .collect()
    }

    /// Number of tasks in the DAG.
    pub fn len(&self) -> usize {
        self.num_tasks
    }

    /// Check if the DAG is empty.
    pub fn is_empty(&self) -> bool {
        self.num_tasks == 0
    }

    /// Check if all dependencies of a task are done.
    ///
    /// `done` is a slice indexed by `TaskId.index()`.
    pub fn all_deps_done(&self, id: TaskId, done: &[bool]) -> bool {
        self.predecessors[id.index() as usize]
            .iter()
            .all(|dep| done[dep.index() as usize])
    }

    /// Get root tasks (no predecessors).
    pub fn root_tasks(&self) -> Vec<TaskId> {
        self.predecessors
            .iter()
            .enumerate()
            .filter(|(_, preds)| preds.is_empty())
            .map(|(i, _)| TaskId::new(i as u32))
            .collect()
    }

    /// Access the topological ordering.
    pub fn topo(&self) -> &TopoOrder {
        &self.topo
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// kahn_sort
// ═══════════════════════════════════════════════════════════════════════════

/// BFS topological sort with simultaneous cycle detection and depth computation.
///
/// Uses Kahn's algorithm: repeatedly remove nodes with in-degree 0.
/// If not all nodes are processed, a cycle exists.
///
/// Returns `TopoOrder` with:
/// - `order`: tasks in topological order (roots first)
/// - `depths`: depth of each task (max of predecessor depths + 1)
fn kahn_sort(
    successors: &[DepVec],
    in_degree: &mut [u32],
    n: usize,
    task_table: &TaskTable,
) -> Result<TopoOrder, NikaError> {
    let mut order = Vec::with_capacity(n);
    let mut depths = vec![0u32; n];

    // Seed queue with roots (in-degree = 0)
    let mut queue: std::collections::VecDeque<TaskId> = in_degree
        .iter()
        .enumerate()
        .filter(|(_, &deg)| deg == 0)
        .map(|(i, _)| TaskId::new(i as u32))
        .collect();

    while let Some(node) = queue.pop_front() {
        order.push(node);
        let node_depth = depths[node.index() as usize];

        for &succ in &successors[node.index() as usize] {
            let succ_idx = succ.index() as usize;
            // Depth = max of all predecessor depths + 1
            depths[succ_idx] = depths[succ_idx].max(node_depth + 1);
            in_degree[succ_idx] -= 1;
            if in_degree[succ_idx] == 0 {
                queue.push_back(succ);
            }
        }
    }

    if order.len() != n {
        // Cycle detected — collect names of tasks still in the cycle
        let cycle_tasks: Vec<String> = in_degree
            .iter()
            .enumerate()
            .filter(|(_, &deg)| deg > 0)
            .filter_map(|(i, _)| task_table.get_name(TaskId::new(i as u32)))
            .map(|s| s.to_string())
            .collect();

        return Err(DagError::CycleDetected {
            cycle: cycle_tasks.join(" → "),
        }
        .into());
    }

    Ok(TopoOrder {
        order: order.into_boxed_slice(),
        depths: depths.into_boxed_slice(),
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::analyzed::{AnalyzedTask, AnalyzedTaskAction, AnalyzedWorkflow};
    use crate::binding::WithSpec;
    use crate::source::Span;

    /// Helper: build an AnalyzedWorkflow from task specs.
    ///
    /// Each spec is `(name, &[dependency_names])`.
    fn build_workflow(specs: &[(&str, &[&str])]) -> AnalyzedWorkflow {
        let mut wf = AnalyzedWorkflow::default();

        // First pass: register all task names
        for (name, _) in specs {
            wf.task_table.insert(name);
        }

        // Second pass: create tasks with resolved deps
        for (name, deps) in specs {
            let id = wf.task_table.get_id(name).unwrap();
            let depends_on: Vec<TaskId> = deps
                .iter()
                .map(|d| wf.task_table.get_id(d).unwrap())
                .collect();

            wf.tasks.push(AnalyzedTask {
                id,
                name: name.to_string(),
                description: None,
                action: AnalyzedTaskAction::default(),
                provider: None,
                model: None,

                with_spec: WithSpec::default(),
                depends_on,
                implicit_deps: Vec::new(),
                output: None,
                for_each: None,
                retry: None,
                on_error: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                record: None,
                context_budget: None,
                preset: None,
                routing: None,
                when: None,
                trust_elevated: false,
                span: Span::dummy(),
            });
        }

        wf
    }

    // ====================================================================
    // Empty DAG
    // ====================================================================

    #[test]
    fn empty_dag() {
        let wf = build_workflow(&[]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();
        assert_eq!(dag.len(), 0);
        assert!(dag.is_empty());
        assert!(dag.topo_order().is_empty());
        assert!(dag.final_tasks().is_empty());
        assert!(dag.root_tasks().is_empty());
    }

    // ====================================================================
    // Single task
    // ====================================================================

    #[test]
    fn single_task() {
        let wf = build_workflow(&[("solo", &[])]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        assert_eq!(dag.len(), 1);
        assert!(!dag.is_empty());
        assert_eq!(dag.topo_order().len(), 1);
        assert_eq!(dag.depth(TaskId::new(0)), 0);
        assert_eq!(dag.max_depth(), 0);
        assert_eq!(dag.final_tasks().len(), 1);
        assert_eq!(dag.root_tasks().len(), 1);
        assert!(dag.dependencies(TaskId::new(0)).is_empty());
        assert!(dag.successors(TaskId::new(0)).is_empty());
    }

    // ====================================================================
    // Linear chain: a → b → c
    // ====================================================================

    #[test]
    fn linear_chain() {
        let wf = build_workflow(&[("a", &[]), ("b", &["a"]), ("c", &["b"])]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        assert_eq!(dag.len(), 3);

        // Topo order must respect a < b < c
        let order = dag.topo_order();
        let pos_a = order.iter().position(|&id| id == TaskId::new(0)).unwrap();
        let pos_b = order.iter().position(|&id| id == TaskId::new(1)).unwrap();
        let pos_c = order.iter().position(|&id| id == TaskId::new(2)).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);

        // Depths
        assert_eq!(dag.depth(TaskId::new(0)), 0); // a = root
        assert_eq!(dag.depth(TaskId::new(1)), 1); // b
        assert_eq!(dag.depth(TaskId::new(2)), 2); // c
        assert_eq!(dag.max_depth(), 2);

        // Edges
        assert!(dag.dependencies(TaskId::new(0)).is_empty());
        assert_eq!(dag.successors(TaskId::new(0)), &[TaskId::new(1)]);
        assert_eq!(dag.dependencies(TaskId::new(1)), &[TaskId::new(0)]);
        assert_eq!(dag.successors(TaskId::new(1)), &[TaskId::new(2)]);
        assert_eq!(dag.dependencies(TaskId::new(2)), &[TaskId::new(1)]);
        assert!(dag.successors(TaskId::new(2)).is_empty());

        // Final/root tasks
        assert_eq!(dag.final_tasks(), vec![TaskId::new(2)]);
        assert_eq!(dag.root_tasks(), vec![TaskId::new(0)]);
    }

    // ====================================================================
    // Diamond: a → b, a → c, b → d, c → d
    // ====================================================================

    #[test]
    fn diamond_dag() {
        let wf = build_workflow(&[("a", &[]), ("b", &["a"]), ("c", &["a"]), ("d", &["b", "c"])]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        assert_eq!(dag.len(), 4);

        // Depths
        assert_eq!(dag.depth(TaskId::new(0)), 0); // a
        assert_eq!(dag.depth(TaskId::new(1)), 1); // b
        assert_eq!(dag.depth(TaskId::new(2)), 1); // c
        assert_eq!(dag.depth(TaskId::new(3)), 2); // d
        assert_eq!(dag.max_depth(), 2);

        // d depends on b and c
        let d_deps = dag.dependencies(TaskId::new(3));
        assert_eq!(d_deps.len(), 2);
        assert!(d_deps.contains(&TaskId::new(1)));
        assert!(d_deps.contains(&TaskId::new(2)));

        // a has two successors
        let a_succs = dag.successors(TaskId::new(0));
        assert_eq!(a_succs.len(), 2);

        // Root = a, final = d
        assert_eq!(dag.root_tasks(), vec![TaskId::new(0)]);
        assert_eq!(dag.final_tasks(), vec![TaskId::new(3)]);

        // Topo: a before b and c, both before d
        let order = dag.topo_order();
        let pos = |id: u32| order.iter().position(|&x| x == TaskId::new(id)).unwrap();
        assert!(pos(0) < pos(1));
        assert!(pos(0) < pos(2));
        assert!(pos(1) < pos(3));
        assert!(pos(2) < pos(3));
    }

    // ====================================================================
    // Parallel tasks (no edges)
    // ====================================================================

    #[test]
    fn parallel_independent() {
        let wf = build_workflow(&[("x", &[]), ("y", &[]), ("z", &[])]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        assert_eq!(dag.len(), 3);
        assert_eq!(dag.topo_order().len(), 3);

        // All at depth 0
        for i in 0..3 {
            assert_eq!(dag.depth(TaskId::new(i)), 0);
        }
        assert_eq!(dag.max_depth(), 0);

        // All are roots and finals
        assert_eq!(dag.root_tasks().len(), 3);
        assert_eq!(dag.final_tasks().len(), 3);
    }

    // ====================================================================
    // Cycle detection
    // ====================================================================

    #[test]
    fn cycle_detected() {
        // a → b → c → a (cycle)
        let mut wf = AnalyzedWorkflow::default();
        wf.task_table.insert("a");
        wf.task_table.insert("b");
        wf.task_table.insert("c");

        let id_a = wf.task_table.get_id("a").unwrap();
        let id_b = wf.task_table.get_id("b").unwrap();
        let id_c = wf.task_table.get_id("c").unwrap();

        let make_task = |id: TaskId, name: &str, deps: Vec<TaskId>| AnalyzedTask {
            id,
            name: name.to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: deps,
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        };

        wf.tasks.push(make_task(id_a, "a", vec![id_c])); // a depends on c
        wf.tasks.push(make_task(id_b, "b", vec![id_a])); // b depends on a
        wf.tasks.push(make_task(id_c, "c", vec![id_b])); // c depends on b

        let err = IndexedDag::from_analyzed(&wf).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("a") || msg.contains("b") || msg.contains("c"));
    }

    // ====================================================================
    // all_deps_done
    // ====================================================================

    #[test]
    fn all_deps_done_checks() {
        let wf = build_workflow(&[("a", &[]), ("b", &["a"]), ("c", &["a", "b"])]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        let mut done = vec![false; 3];

        // a has no deps → always ready
        assert!(dag.all_deps_done(TaskId::new(0), &done));

        // b depends on a → not ready yet
        assert!(!dag.all_deps_done(TaskId::new(1), &done));

        // Mark a done
        done[0] = true;
        assert!(dag.all_deps_done(TaskId::new(1), &done));

        // c depends on a and b → b not done yet
        assert!(!dag.all_deps_done(TaskId::new(2), &done));

        // Mark b done
        done[1] = true;
        assert!(dag.all_deps_done(TaskId::new(2), &done));
    }

    // ====================================================================
    // Implicit deps
    // ====================================================================

    #[test]
    fn implicit_deps_included() {
        let mut wf = AnalyzedWorkflow::default();
        wf.task_table.insert("src");
        wf.task_table.insert("sink");

        let id_src = wf.task_table.get_id("src").unwrap();
        let id_sink = wf.task_table.get_id("sink").unwrap();

        wf.tasks.push(AnalyzedTask {
            id: id_src,
            name: "src".to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        });

        // sink has implicit dep on src (from with: binding)
        wf.tasks.push(AnalyzedTask {
            id: id_sink,
            name: "sink".to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: vec![id_src], // implicit dep
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        });

        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        // sink should depend on src via implicit dep
        assert_eq!(dag.dependencies(id_sink), &[id_src]);
        assert_eq!(dag.successors(id_src), &[id_sink]);

        // Depths: src=0, sink=1
        assert_eq!(dag.depth(id_src), 0);
        assert_eq!(dag.depth(id_sink), 1);
    }

    // ====================================================================
    // Wide fan-out (>4 deps to test SmallVec spill)
    // ====================================================================

    #[test]
    fn wide_fanout() {
        // 6 roots all feed into a single sink
        let mut specs: Vec<(&str, &[&str])> = Vec::new();
        let names = ["r0", "r1", "r2", "r3", "r4", "r5"];
        for name in &names {
            specs.push((name, &[]));
        }
        specs.push(("sink", &["r0", "r1", "r2", "r3", "r4", "r5"]));

        let wf = build_workflow(&specs);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        assert_eq!(dag.len(), 7);
        assert_eq!(dag.dependencies(TaskId::new(6)).len(), 6); // sink has 6 deps
        assert_eq!(dag.root_tasks().len(), 6);
        assert_eq!(dag.final_tasks(), vec![TaskId::new(6)]);
        assert_eq!(dag.depth(TaskId::new(6)), 1);
    }

    // ====================================================================
    // Depth computation in complex DAG
    // ====================================================================

    #[test]
    fn depth_complex() {
        // a(0) → b(1) → d(2)
        // a(0) → c(1) → e(2) → f(3)
        // b(1) → e(2) (so e = max(1,1)+1 = 2)
        let wf = build_workflow(&[
            ("a", &[]),
            ("b", &["a"]),
            ("c", &["a"]),
            ("d", &["b"]),
            ("e", &["c", "b"]),
            ("f", &["e"]),
        ]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();

        assert_eq!(dag.depth(TaskId::new(0)), 0); // a
        assert_eq!(dag.depth(TaskId::new(1)), 1); // b
        assert_eq!(dag.depth(TaskId::new(2)), 1); // c
        assert_eq!(dag.depth(TaskId::new(3)), 2); // d
        assert_eq!(dag.depth(TaskId::new(4)), 2); // e = max(b=1, c=1) + 1
        assert_eq!(dag.depth(TaskId::new(5)), 3); // f
        assert_eq!(dag.max_depth(), 3);
    }

    // ====================================================================
    // Topo order validity
    // ====================================================================

    #[test]
    fn topo_order_valid() {
        let wf = build_workflow(&[
            ("a", &[]),
            ("b", &["a"]),
            ("c", &["a"]),
            ("d", &["b", "c"]),
            ("e", &["d"]),
        ]);
        let dag = IndexedDag::from_analyzed(&wf).unwrap();
        let order = dag.topo_order();

        // Every task appears exactly once
        assert_eq!(order.len(), 5);

        // For each task, all its deps appear before it in topo order
        for (pos, &task_id) in order.iter().enumerate() {
            for &dep in dag.dependencies(task_id) {
                let dep_pos = order.iter().position(|&x| x == dep).unwrap();
                assert!(
                    dep_pos < pos,
                    "dep {:?} should appear before {:?}",
                    dep,
                    task_id
                );
            }
        }
    }

    // ====================================================================
    // Partial cycle (not all nodes in cycle)
    // ====================================================================

    #[test]
    fn partial_cycle() {
        // a → b → c → b (b-c cycle, a is not in cycle)
        let mut wf = AnalyzedWorkflow::default();
        wf.task_table.insert("a");
        wf.task_table.insert("b");
        wf.task_table.insert("c");

        let id_a = wf.task_table.get_id("a").unwrap();
        let id_b = wf.task_table.get_id("b").unwrap();
        let id_c = wf.task_table.get_id("c").unwrap();

        let make_task = |id: TaskId, name: &str, deps: Vec<TaskId>| AnalyzedTask {
            id,
            name: name.to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: deps,
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        };

        wf.tasks.push(make_task(id_a, "a", vec![]));
        wf.tasks.push(make_task(id_b, "b", vec![id_a, id_c])); // b depends on a and c
        wf.tasks.push(make_task(id_c, "c", vec![id_b])); // c depends on b (cycle!)

        let err = IndexedDag::from_analyzed(&wf).unwrap_err();
        let msg = err.to_string();
        // b and c should be in the cycle, not a
        assert!(msg.contains("b") && msg.contains("c"));
    }

    // ====================================================================
    // Bug #23: Duplicate edges from depends_on + implicit_deps must not
    // inflate in_degree (which causes false cycle detection).
    // ====================================================================

    #[test]
    fn duplicate_dep_in_depends_on_and_implicit_deps_no_false_cycle() {
        // Task "sink" has "src" in BOTH depends_on AND implicit_deps.
        // Without deduplication, in_degree[sink] becomes 2 instead of 1,
        // causing Kahn's algorithm to never enqueue "sink" → false cycle.
        let mut wf = AnalyzedWorkflow::default();
        wf.task_table.insert("src");
        wf.task_table.insert("sink");

        let id_src = wf.task_table.get_id("src").unwrap();
        let id_sink = wf.task_table.get_id("sink").unwrap();

        wf.tasks.push(AnalyzedTask {
            id: id_src,
            name: "src".to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: Vec::new(),
            implicit_deps: Vec::new(),
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        });

        // sink has src in BOTH depends_on and implicit_deps (duplicate edge)
        wf.tasks.push(AnalyzedTask {
            id: id_sink,
            name: "sink".to_string(),
            description: None,
            action: AnalyzedTaskAction::default(),
            provider: None,
            model: None,

            with_spec: WithSpec::default(),
            depends_on: vec![id_src],
            implicit_deps: vec![id_src], // duplicate!
            output: None,
            for_each: None,
            retry: None,
            on_error: None,
            decompose: None,
            concurrency: None,
            fail_fast: None,
            artifact: None,
            log: None,
            structured: None,
            record: None,
            context_budget: None,
            preset: None,
            routing: None,
            when: None,
            trust_elevated: false,
            span: Span::dummy(),
        });

        // This should succeed — not report a false cycle
        let dag = IndexedDag::from_analyzed(&wf)
            .expect("Duplicate dep in depends_on + implicit_deps should NOT cause false cycle");

        // Verify only 1 edge exists (deduplicated)
        assert_eq!(dag.dependencies(id_sink).len(), 1);
        assert_eq!(dag.dependencies(id_sink), &[id_src]);

        // Verify depth computation is correct
        assert_eq!(dag.depth(id_src), 0);
        assert_eq!(dag.depth(id_sink), 1);
    }
}
