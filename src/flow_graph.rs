//! Flow graph built from workflow flows (Arc<str> optimized)
//!
//! Uses Arc<str> for zero-cost cloning of task IDs.

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::workflow::Workflow;

/// Graph of task dependencies built from flows
///
/// Uses Arc<str> internally for zero-cost cloning.
pub struct FlowGraph {
    /// task_id -> list of successor task_ids
    adjacency: HashMap<Arc<str>, Vec<Arc<str>>>,
    /// task_id -> list of predecessor task_ids (dependencies)
    predecessors: HashMap<Arc<str>, Vec<Arc<str>>>,
    /// All task IDs (for iteration)
    task_ids: Vec<Arc<str>>,
    /// Quick lookup for task existence (used in from_workflow for Arc reuse)
    #[allow(dead_code)] // Used in from_workflow for Arc<str> reuse
    task_set: HashSet<Arc<str>>,
}

impl FlowGraph {
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let capacity = workflow.tasks.len();
        let mut adjacency: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::with_capacity(capacity);
        let mut predecessors: HashMap<Arc<str>, Vec<Arc<str>>> = HashMap::with_capacity(capacity);
        let mut task_ids: Vec<Arc<str>> = Vec::with_capacity(capacity);
        let mut task_set: HashSet<Arc<str>> = HashSet::with_capacity(capacity);

        // Create Arc<str> once per task, reuse everywhere
        for task in &workflow.tasks {
            let id: Arc<str> = Arc::from(task.id.as_str());
            task_ids.push(Arc::clone(&id));
            task_set.insert(Arc::clone(&id));
            adjacency.insert(Arc::clone(&id), Vec::new());
            predecessors.insert(id, Vec::new());
        }

        // Build from flows (lookup Arc from set)
        for flow in &workflow.flows {
            let sources = flow.source.as_vec();
            let targets = flow.target.as_vec();

            for source in &sources {
                for target in &targets {
                    // Find existing Arc<str> or create new (shouldn't happen if task exists)
                    let src_arc = task_set
                        .get(*source)
                        .cloned()
                        .unwrap_or_else(|| Arc::from(*source));
                    let tgt_arc = task_set
                        .get(*target)
                        .cloned()
                        .unwrap_or_else(|| Arc::from(*target));

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

        let mut visited: HashSet<&str> = HashSet::new();
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
