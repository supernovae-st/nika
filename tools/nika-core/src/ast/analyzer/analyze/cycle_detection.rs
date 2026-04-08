// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cycle detection: DAG validation for both raw and analyzed workflows.

use super::*;

/// Detect cyclic dependencies from raw workflow (without AnalyzedWorkflow).
///
/// Builds a lightweight dependency graph from raw task data and runs DFS cycle detection.
/// Used by `validate()`.
pub(super) fn detect_cycles_from_raw(
    tasks: &[Spanned<RawTask>],
    task_table: &TaskTable,
    ctx: &mut AnalyzerContext,
) {
    // Build adjacency list: TaskId → Vec<TaskId>
    let mut adjacency: HashMap<TaskId, Vec<TaskId>> = HashMap::new();
    let mut task_spans: HashMap<TaskId, Span> = HashMap::new();

    for raw_task in tasks.iter() {
        let task_name = &raw_task.value.id.value;
        let Some(task_id) = task_table.get_id(task_name) else {
            continue; // Skip tasks that failed duplicate detection
        };
        task_spans.insert(task_id, raw_task.value.span);
        let deps = adjacency.entry(task_id).or_default();

        // Collect depends_on edges
        if let Some(ref depends_on) = raw_task.value.depends_on {
            for dep_spanned in &depends_on.value {
                if let Some(dep_id) = task_table.get_id(&dep_spanned.value) {
                    deps.push(dep_id);
                }
            }
        }

        // Collect implicit deps from with: bindings
        if let Some(ref with_refs) = raw_task.value.with_refs {
            for (_alias, value_spanned) in with_refs.value.iter() {
                if let Ok(entry) = parse_with_entry(&value_spanned.value) {
                    if let Some(dep_task_name) = entry.task_id() {
                        if let Some(dep_id) = task_table.get_id(dep_task_name) {
                            if !deps.contains(&dep_id) {
                                deps.push(dep_id);
                            }
                        }
                    }
                }
            }
        }
    }

    // DFS cycle detection
    let graph = RawDepGraph {
        adjacency,
        task_table,
        task_spans,
    };
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for &task_id in graph.adjacency.keys() {
        if !visited.contains(&task_id) {
            detect_cycles_raw_dfs(
                task_id,
                &graph,
                &mut visited,
                &mut rec_stack,
                &mut path,
                ctx,
            );
        }
    }
}

/// Read-only dependency graph context for raw cycle detection.
struct RawDepGraph<'a> {
    adjacency: HashMap<TaskId, Vec<TaskId>>,
    task_table: &'a TaskTable,
    task_spans: HashMap<TaskId, Span>,
}

/// DFS helper for raw cycle detection.
fn detect_cycles_raw_dfs(
    task_id: TaskId,
    graph: &RawDepGraph<'_>,
    visited: &mut HashSet<TaskId>,
    rec_stack: &mut HashSet<TaskId>,
    path: &mut Vec<TaskId>,
    ctx: &mut AnalyzerContext,
) {
    visited.insert(task_id);
    rec_stack.insert(task_id);
    path.push(task_id);

    if let Some(deps) = graph.adjacency.get(&task_id) {
        for dep_id in deps {
            if !visited.contains(dep_id) {
                detect_cycles_raw_dfs(*dep_id, graph, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(dep_id) {
                // Found cycle
                let cycle_start = path.iter().position(|&id| id == *dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| graph.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = graph.task_table.get_name(*dep_id) {
                    cycle_with_close.push(name);
                }
                let span = graph
                    .task_spans
                    .get(&task_id)
                    .copied()
                    .unwrap_or(Span::dummy());
                ctx.add_error(AnalyzeError::cyclic_dependency(span, &cycle_with_close));
            }
        }
    }

    path.pop();
    rec_stack.remove(&task_id);
}

/// Detect cyclic dependencies using DFS.
///
/// Checks both `depends_on` (explicit ordering) and `implicit_deps` (from with: bindings).
pub(super) fn detect_cycles(workflow: &AnalyzedWorkflow, ctx: &mut AnalyzerContext) {
    let mut visited = HashSet::new();
    let mut rec_stack = HashSet::new();
    let mut path = Vec::new();

    for task in &workflow.tasks {
        if !visited.contains(&task.id) {
            detect_cycles_dfs(
                task.id,
                workflow,
                &mut visited,
                &mut rec_stack,
                &mut path,
                ctx,
            );
        }
    }
}

fn detect_cycles_dfs(
    task_id: TaskId,
    workflow: &AnalyzedWorkflow,
    visited: &mut HashSet<TaskId>,
    rec_stack: &mut HashSet<TaskId>,
    path: &mut Vec<TaskId>,
    ctx: &mut AnalyzerContext,
) {
    visited.insert(task_id);
    rec_stack.insert(task_id);
    path.push(task_id);

    if let Some(task) = workflow.get_task(task_id) {
        // Check explicit depends_on dependencies
        for dep_id in &task.depends_on {
            if !visited.contains(dep_id) {
                detect_cycles_dfs(*dep_id, workflow, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(dep_id) {
                // Found cycle
                let cycle_start = path.iter().position(|&id| id == *dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| workflow.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = workflow.task_table.get_name(*dep_id) {
                    cycle_with_close.push(name);
                }
                ctx.add_error(AnalyzeError::cyclic_dependency(
                    task.span,
                    &cycle_with_close,
                ));
            }
        }

        // Check implicit dependencies (from with: bindings)
        for dep_id in &task.implicit_deps {
            if !visited.contains(dep_id) {
                detect_cycles_dfs(*dep_id, workflow, visited, rec_stack, path, ctx);
            } else if rec_stack.contains(dep_id) {
                // Found cycle via with: binding
                let cycle_start = path.iter().position(|&id| id == *dep_id).unwrap();
                let cycle_path: Vec<&str> = path[cycle_start..]
                    .iter()
                    .filter_map(|id| workflow.task_table.get_name(*id))
                    .collect();
                let mut cycle_with_close = cycle_path.clone();
                if let Some(name) = workflow.task_table.get_name(*dep_id) {
                    cycle_with_close.push(name);
                }
                ctx.add_error(AnalyzeError::cyclic_dependency(
                    task.span,
                    &cycle_with_close,
                ));
            }
        }
    }

    path.pop();
    rec_stack.remove(&task_id);
}

/// Detect artifact path collisions between tasks (static paths only).
///
/// Paths containing `{{` are templates resolved at runtime and cannot be checked statically.
/// For_each tasks are also skipped since each iteration may produce unique paths.
pub(super) fn detect_artifact_collisions(workflow: &AnalyzedWorkflow, ctx: &mut AnalyzerContext) {
    use crate::ast::artifact::{ArtifactMode, ArtifactSpec};
    // (path → (task_name, is_safe_mode)) where safe = append or unique
    let mut seen: HashMap<String, (String, bool)> = HashMap::new();

    for task in &workflow.tasks {
        // Skip for_each tasks — their artifact paths are per-iteration
        if task.for_each.is_some() {
            continue;
        }

        let outputs: Vec<&crate::ast::artifact::ArtifactOutput> = match task.artifact.as_ref() {
            Some(ArtifactSpec::Single(out)) => vec![out],
            Some(ArtifactSpec::Multiple(outs)) => outs.iter().collect(),
            _ => continue,
        };

        for out in outputs {
            let path = out.path.as_str();
            // Skip template paths — can't check statically
            if path.contains("{{") {
                continue;
            }
            let is_safe_mode =
                matches!(out.mode, Some(ArtifactMode::Append | ArtifactMode::Unique));
            if let Some((prev_task, prev_safe)) = seen.get(path) {
                // Warn only when at least one side uses overwrite/fail (destructive)
                if !is_safe_mode || !prev_safe {
                    ctx.warnings.push(AnalyzeError {
                        kind: AnalyzeErrorKind::InvalidValue,
                        span: task.span,
                        message: format!(
                            "Artifact path '{}' in task '{}' collides with task '{}' — \
                             the second write will overwrite the first",
                            path, task.name, prev_task
                        ),
                        suggestion: Some(
                            "Use mode: append, mode: unique, or mode: fail to handle duplicates"
                                .to_string(),
                        ),
                        note: None,
                    });
                }
            } else {
                seen.insert(path.to_string(), (task.name.clone(), is_safe_mode));
            }
        }
    }
}
