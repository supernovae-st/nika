//! Flow graph built from workflow flows (optimized)
//!
//! Performance optimizations:
//! - Arc<str> for zero-cost cloning of task IDs
//! - FxHashMap for faster hashing (non-crypto, ~2x faster)
//! - SmallVec for stack-allocated small dependency lists (0-4 items)

use std::collections::VecDeque;
use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

use crate::interner::intern;
use crate::workflow::Workflow;

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
                        .unwrap_or_else(|| intern(*source));
                    let tgt_arc = task_set
                        .get(*target)
                        .cloned()
                        .unwrap_or_else(|| intern(*target));

                    adjacency
                        .entry(Arc::clone(&src_arc))
                        .or_default()
                        .push(Arc::clone(&tgt_arc));
                    predecessors
                        .entry(tgt_arc)
                        .or_default()
                        .push(src_arc);
                }
            }
        }

        Self { adjacency, predecessors, task_ids, task_set }
    }

    /// Get dependencies of a task (returns Arc<str> slice)
    #[inline]
    pub fn get_dependencies(&self, task_id: &str) -> &[Arc<str>] {
        static EMPTY: &[Arc<str>] = &[];
        self.predecessors
            .get(task_id)
            .map(|v| v.as_slice())
            .unwrap_or(EMPTY)
    }

    /// Get successors of a task
    #[inline]
    #[allow(dead_code)] // Used for future DAG traversal
    pub fn get_successors(&self, task_id: &str) -> &[Arc<str>] {
        static EMPTY: &[Arc<str>] = &[];
        self.adjacency
            .get(task_id)
            .map(|v| v.as_slice())
            .unwrap_or(EMPTY)
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
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
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
}
