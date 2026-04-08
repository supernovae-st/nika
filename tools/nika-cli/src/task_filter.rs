// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Task filtering helpers for `--task` and `--from` CLI flags.
//!
//! These operate on the analyzed workflow AST to prune the task graph
//! before execution, keeping only the target task + its transitive
//! dependencies (--task) or forward successors (--from).

use nika_engine::ast::analyzed::AnalyzedWorkflow;
use nika_engine::error::NikaError;

/// Keep only `target_id` and its transitive dependencies (backward BFS).
pub fn filter_tasks_for_target(
    workflow: &mut AnalyzedWorkflow,
    target_id: &str,
) -> Result<(), NikaError> {
    // Verify target exists
    if !workflow.tasks.iter().any(|t| t.name == target_id) {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Task '{}' not found. Available: {}",
                target_id,
                workflow
                    .tasks
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // BFS to collect transitive dependencies
    let mut required: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    required.insert(target_id.to_string());
    queue.push_back(target_id.to_string());

    while let Some(task_name) = queue.pop_front() {
        if let Some(task) = workflow.tasks.iter().find(|t| t.name == task_name) {
            // Explicit depends_on
            for dep_id in &task.depends_on {
                if let Some(dep_name) = workflow.task_table.get_name(*dep_id) {
                    if required.insert(dep_name.to_string()) {
                        queue.push_back(dep_name.to_string());
                    }
                }
            }
            // Implicit deps from with: bindings
            for dep_id in &task.implicit_deps {
                if let Some(dep_name) = workflow.task_table.get_name(*dep_id) {
                    if required.insert(dep_name.to_string()) {
                        queue.push_back(dep_name.to_string());
                    }
                }
            }
        }
    }

    workflow
        .tasks
        .retain(|t| required.contains(t.name.as_str()));
    Ok(())
}

/// Keep `from_id` and all its transitive successors (forward BFS).
pub fn filter_tasks_from(
    workflow: &mut AnalyzedWorkflow,
    from_id: &str,
) -> Result<(), NikaError> {
    if !workflow.tasks.iter().any(|t| t.name == from_id) {
        return Err(NikaError::ValidationError {
            reason: format!(
                "Task '{}' not found. Available: {}",
                from_id,
                workflow
                    .tasks
                    .iter()
                    .map(|t| t.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }

    // Forward BFS: find from_id + all transitive successors
    let mut keep: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    keep.insert(from_id.to_string());
    queue.push_back(from_id.to_string());

    while let Some(current) = queue.pop_front() {
        for task in &workflow.tasks {
            // Check if this task depends on `current` (making it a successor)
            let is_successor =
                task.depends_on
                    .iter()
                    .chain(task.implicit_deps.iter())
                    .any(|dep_id| {
                        workflow
                            .task_table
                            .get_name(*dep_id)
                            .is_some_and(|name| name == current)
                    });
            if is_successor && keep.insert(task.name.clone()) {
                queue.push_back(task.name.clone());
            }
        }
    }

    workflow.tasks.retain(|t| keep.contains(t.name.as_str()));
    Ok(())
}
