//! Flow graph built from workflow flows:

use std::collections::{HashMap, HashSet, VecDeque};
use crate::workflow::Workflow;

/// Graph of task dependencies built from flows:
pub struct FlowGraph {
    /// task_id -> list of successor task_ids
    adjacency: HashMap<String, Vec<String>>,
    /// task_id -> list of predecessor task_ids (dependencies)
    predecessors: HashMap<String, Vec<String>>,
    /// All task IDs
    task_ids: HashSet<String>,
}

impl FlowGraph {
    pub fn from_workflow(workflow: &Workflow) -> Self {
        let capacity = workflow.tasks.len();
        let mut adjacency: HashMap<String, Vec<String>> = HashMap::with_capacity(capacity);
        let mut predecessors: HashMap<String, Vec<String>> = HashMap::with_capacity(capacity);
        let mut task_ids: HashSet<String> = HashSet::with_capacity(capacity);

        // Initialize all tasks (single clone per task)
        for task in &workflow.tasks {
            let id = task.id.clone();
            task_ids.insert(id.clone());
            adjacency.insert(id.clone(), Vec::new());
            predecessors.insert(id, Vec::new());
        }

        // Build from flows
        for flow in &workflow.flows {
            let sources = flow.source.as_vec();
            let targets = flow.target.as_vec();

            for source in &sources {
                for target in &targets {
                    adjacency
                        .entry(source.to_string())
                        .or_default()
                        .push(target.to_string());
                    predecessors
                        .entry(target.to_string())
                        .or_default()
                        .push(source.to_string());
                }
            }
        }

        Self { adjacency, predecessors, task_ids }
    }

    /// Empty vec for when task has no dependencies
    const EMPTY: &'static [String] = &[];

    /// Get dependencies of a task (returns slice, no allocation)
    #[inline]
    pub fn get_dependencies(&self, task_id: &str) -> &[String] {
        self.predecessors
            .get(task_id)
            .map(|v| v.as_slice())
            .unwrap_or(Self::EMPTY)
    }

    /// Get successors of a task (returns slice, no allocation)
    #[inline]
    pub fn get_successors(&self, task_id: &str) -> &[String] {
        self.adjacency
            .get(task_id)
            .map(|v| v.as_slice())
            .unwrap_or(Self::EMPTY)
    }

    /// Find tasks with no successors (final tasks)
    pub fn get_final_tasks(&self) -> Vec<String> {
        self.task_ids
            .iter()
            .filter(|id| {
                self.adjacency
                    .get(*id)
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
            })
            .cloned()
            .collect()
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
                    if neighbor == to {
                        return true;
                    }
                    if !visited.contains(neighbor.as_str()) {
                        visited.insert(neighbor.as_str());
                        queue.push_back(neighbor.as_str());
                    }
                }
            }
        }

        false
    }
}
