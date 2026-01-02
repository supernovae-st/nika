//! DAG Validation - use: wiring validation (v0.1)
//!
//! Validates:
//! - use: wiring references (task exists, is upstream)
//! - Template refs match use: declarations
//! - Task ID format (snake_case)

use rustc_hash::FxHashSet;

use crate::ast::Workflow;
use crate::binding::{validate_task_id, UseWiring};
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
fn validate_wiring(
    task_id: &str,
    wiring: &UseWiring,
    all_task_ids: &FxHashSet<&str>,
    flow_graph: &FlowGraph,
) -> Result<(), NikaError> {
    for (alias, entry) in wiring {
        // Extract task_id from the path (first segment before '.')
        let from_task = entry.task_id();

        // Validate the source task ID format (snake_case)
        validate_task_id(from_task)?;

        // Validate that the source task exists and is upstream
        validate_from_task(alias, from_task, task_id, all_task_ids, flow_graph)?;
    }

    Ok(())
}

/// Validate that from_task exists and is upstream
fn validate_from_task(
    alias: &str,
    from_task: &str,
    task_id: &str,
    all_task_ids: &FxHashSet<&str>,
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
    use super::*;
    use crate::binding::UseEntry;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // UseEntry.task_id() extraction tests
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
    // Task ID validation tests (snake_case)
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
