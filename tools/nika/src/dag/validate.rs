//! DAG Validation - use: wiring validation (v0.1)
//!
//! Validates:
//! - use: wiring references (task exists, is upstream)
//! - Template refs match use: declarations
//! - Task ID format (snake_case)
//!
//! Error codes:
//! - NIKA-055: Invalid task ID format (non-snake_case)
//! - NIKA-080: use.alias references unknown task
//! - NIKA-081: use.alias references non-upstream task
//! - NIKA-082: use.alias creates self-reference

use rustc_hash::FxHashSet;

use crate::ast::Workflow;
use crate::binding::{validate_task_id, WiringSpec};
use crate::error::NikaError;

use super::flow::FlowGraph;

/// Validate a workflow's use: wiring against the flow graph
pub fn validate_use_wiring(workflow: &Workflow, flow_graph: &FlowGraph) -> Result<(), NikaError> {
    // Zero-clone: use &str references instead of owned Strings
    let all_task_ids: FxHashSet<&str> = workflow.tasks.iter().map(|t| t.id.as_str()).collect();

    for task in &workflow.tasks {
        if let Some(ref wiring) = task.use_wiring {
            validate_wiring(&task.id, wiring, &all_task_ids, flow_graph)?;
        }
    }

    Ok(())
}

/// Validate a single use: wiring
///
/// Unified validation for new syntax: `alias: task.path [?? default]`
/// Ensures:
/// 1. Source task ID is valid (snake_case)
/// 2. Source task exists in workflow
/// 3. Source is not self-reference
/// 4. Source task has path to current task
fn validate_wiring(
    task_id: &str,
    wiring: &WiringSpec,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    for (alias, entry) in wiring {
        // Extract task_id from the path (first segment before '.')
        let from_task = entry.task_id();

        // Validate the source task ID format (snake_case) - O(n) check
        validate_task_id(from_task)?;

        // Validate that the source task exists, is not self-referential, and is upstream
        validate_from_task(alias, from_task, task_id, all_task_ids, flow_graph)?;
    }

    Ok(())
}

/// Validate that from_task exists and is upstream
///
/// Checks in order:
/// 1. Task exists in workflow (O(1) hash lookup)
/// 2. Not self-reference (O(1) comparison)
/// 3. Has path from source to current task in DAG (O(V+E) BFS)
fn validate_from_task(
    alias: &str,
    from_task: &str,
    task_id: &str,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    // Check not self-reference first (cheapest O(1) check)
    if from_task == task_id {
        return Err(NikaError::UseCircularDep {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check task exists (O(1) hash lookup)
    if !all_task_ids.contains(from_task) {
        return Err(NikaError::UseUnknownTask {
            alias: alias.to_string(),
            from_task: from_task.to_string(),
            task_id: task_id.to_string(),
        });
    }

    // Check from_task has path to current task (O(V+E) BFS)
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
    use super::*;
    use crate::binding::UseEntry;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: UseEntry.task_id() extraction
    // ─────────────────────────────────────────────────────────────
    // Tests path parsing from "task.field.subfield" format
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn entry_task_id_simple() {
        let entry = UseEntry::new("weather");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_with_path() {
        let entry = UseEntry::new("weather.summary");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_nested_path() {
        let entry = UseEntry::new("weather.data.temp.celsius");
        assert_eq!(entry.task_id(), "weather");
    }

    #[test]
    fn entry_task_id_with_default() {
        let entry = UseEntry::with_default("weather.summary", json!("N/A"));
        assert_eq!(entry.task_id(), "weather");
    }

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS: Task ID validation (snake_case)
    // ─────────────────────────────────────────────────────────────
    // Tests NIKA-055 validation: must be [a-z0-9_]+ format
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn task_id_valid_simple() {
        assert!(validate_task_id("weather").is_ok());
    }

    #[test]
    fn task_id_valid_with_underscore() {
        assert!(validate_task_id("get_weather").is_ok());
        assert!(validate_task_id("fetch_api_data").is_ok());
    }

    #[test]
    fn task_id_valid_with_numbers() {
        assert!(validate_task_id("task123").is_ok());
        assert!(validate_task_id("step2").is_ok());
    }

    #[test]
    fn task_id_invalid_dash() {
        let result = validate_task_id("fetch-api");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-055"));
    }

    #[test]
    fn task_id_invalid_uppercase() {
        let result = validate_task_id("myTask");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-055"));
    }

    #[test]
    fn task_id_invalid_dot() {
        let result = validate_task_id("weather.api");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-055"));
    }
}
