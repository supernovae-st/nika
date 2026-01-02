//! DAG Validation - use: wiring validation (v0.1)
//!
//! Validates:
//! - use: wiring references (task exists, is upstream)
//! - Template refs match use: declarations
//! - JSONPath syntax (minimal subset)

use rustc_hash::FxHashSet;

use crate::ast::Workflow;
use crate::error::NikaError;
use crate::util::jsonpath;
use crate::binding::{UseEntry, UseWiring};

use super::flow::FlowGraph;

/// Validate a workflow's use: wiring against the flow graph
pub fn validate_use_wiring(workflow: &Workflow, flow_graph: &FlowGraph) -> Result<(), NikaError> {
    let all_task_ids: FxHashSet<String> = workflow.tasks.iter().map(|t| t.id.clone()).collect();

    for task in &workflow.tasks {
        if let Some(ref wiring) = task.use_wiring {
            validate_wiring(&task.id, wiring, &all_task_ids, flow_graph)?;
        }
    }

    Ok(())
}

/// Validate a single use: wiring
fn validate_wiring(
    task_id: &str,
    wiring: &UseWiring,
    all_task_ids: &FxHashSet<String>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    for (alias, entry) in wiring {
        match entry {
            // Form 1: alias: task.path - extract task_id from path
            UseEntry::Path(path) => {
                let from_task = path.split('.').next().unwrap_or(path);
                validate_from_task(alias, from_task, task_id, all_task_ids, flow_graph)?;
            }

            // Form 2: task.path: [fields] - the key is the path
            UseEntry::Batch(_) => {
                let from_task = alias.split('.').next().unwrap_or(alias);
                validate_from_task(from_task, from_task, task_id, all_task_ids, flow_graph)?;
            }

            // Form 3: alias: { from, path, default }
            UseEntry::Advanced(adv) => {
                validate_from_task(alias, &adv.from, task_id, all_task_ids, flow_graph)?;

                // Validate JSONPath syntax if present
                if let Some(ref path) = adv.path {
                    jsonpath::validate(path)?;
                }
            }
        }
    }

    Ok(())
}

/// Validate that from_task exists and is upstream
fn validate_from_task(
    alias: &str,
    from_task: &str,
    task_id: &str,
    all_task_ids: &FxHashSet<String>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    // Check task exists
    if !all_task_ids.contains(from_task) {
        return Err(NikaError::UseUnknownTask {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check not self-reference
    if from_task == task_id {
        return Err(NikaError::UseCircularDep {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check from_task is upstream (has path TO current task)
    if !flow_graph.has_path(from_task, task_id) {
        return Err(NikaError::UseNotUpstream {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::util::jsonpath;

    // ─────────────────────────────────────────────────────────────
    // JSONPath validation tests (delegated to jsonpath::validate)
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn jsonpath_simple_dot() {
        assert!(jsonpath::validate("$.a.b.c").is_ok());
        assert!(jsonpath::validate("a.b.c").is_ok());
        assert!(jsonpath::validate("field").is_ok());
    }

    #[test]
    fn jsonpath_with_array_index() {
        assert!(jsonpath::validate("$.items[0]").is_ok());
        assert!(jsonpath::validate("$.data[0].name").is_ok());
        assert!(jsonpath::validate("items[123].value").is_ok());
    }

    #[test]
    fn jsonpath_invalid_filter() {
        assert!(jsonpath::validate("$.a[?(@.x==1)]").is_err());
    }

    #[test]
    fn jsonpath_invalid_wildcard() {
        assert!(jsonpath::validate("$.a[*]").is_err());
    }

    #[test]
    fn jsonpath_invalid_slice() {
        assert!(jsonpath::validate("$.a[0:5]").is_err());
    }

    #[test]
    fn jsonpath_empty_segment() {
        assert!(jsonpath::validate("$.a..b").is_err());
        assert!(jsonpath::validate("$..a").is_err());
    }

    #[test]
    fn jsonpath_field_names_are_permissive() {
        // jsonpath parser accepts any non-empty string as field name
        // (matches JSON which allows any string as object key)
        assert!(jsonpath::validate("$.123invalid").is_ok());
        assert!(jsonpath::validate("$.a-b").is_ok());
        assert!(jsonpath::validate("$.some_field").is_ok());
    }
}
