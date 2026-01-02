//! Workflow validation (v0.1)
//!
//! Validates:
//! - use: wiring references (task exists, is upstream)
//! - Template refs match use: declarations
//! - JSONPath syntax (minimal subset)

use std::collections::HashSet;

use crate::error::NikaError;
use crate::flow_graph::FlowGraph;
use crate::use_wiring::{UseEntry, UseWiring};
use crate::workflow::Workflow;

/// Validate a workflow's use: wiring against the flow graph
pub fn validate_use_wiring(workflow: &Workflow, flow_graph: &FlowGraph) -> Result<(), NikaError> {
    let all_task_ids: HashSet<String> = workflow.tasks.iter().map(|t| t.id.clone()).collect();

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
    all_task_ids: &HashSet<String>,
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
                    validate_jsonpath(path)?;
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
    all_task_ids: &HashSet<String>,
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

/// Validate JSONPath syntax (v0.1 minimal subset)
///
/// Supported:
/// - $.a.b.c (dot notation)
/// - $.a[0].b (array index)
///
/// Not supported:
/// - Filters: $.a[?(@.x==1)]
/// - Wildcards: $.a[*]
/// - Slices: $.a[0:5]
/// - Unions: $.a[0,1,2]
pub fn validate_jsonpath(path: &str) -> Result<(), NikaError> {
    // Must start with $ or be simple dot path
    let path = if let Some(stripped) = path.strip_prefix("$.") {
        stripped
    } else if path.starts_with('$') {
        return Err(NikaError::JsonPathUnsupported {
            path: path.to_string(),
        });
    } else {
        // Simple dot path without $. prefix is OK
        path
    };

    // Check each segment
    for segment in path.split('.') {
        if segment.is_empty() {
            return Err(NikaError::JsonPathUnsupported {
                path: path.to_string(),
            });
        }

        // Check for array index: field[0]
        if segment.contains('[') {
            if !is_valid_array_segment(segment) {
                return Err(NikaError::JsonPathUnsupported {
                    path: path.to_string(),
                });
            }
        } else if !is_valid_identifier(segment) {
            return Err(NikaError::JsonPathUnsupported {
                path: path.to_string(),
            });
        }
    }

    Ok(())
}

/// Check if segment is valid identifier: [a-zA-Z_][a-zA-Z0-9_]*
fn is_valid_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Check if segment is valid array access: field[0] or field[123]
fn is_valid_array_segment(s: &str) -> bool {
    if let Some(bracket_pos) = s.find('[') {
        // Must end with ]
        if !s.ends_with(']') {
            return false;
        }

        // Field part must be valid identifier
        let field = &s[..bracket_pos];
        if !field.is_empty() && !is_valid_identifier(field) {
            return false;
        }

        // Index must be a number
        let index = &s[bracket_pos + 1..s.len() - 1];
        if index.is_empty() || !index.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }

        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────────────────────────────────────────
    // JSONPath validation tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn jsonpath_simple_dot() {
        assert!(validate_jsonpath("$.a.b.c").is_ok());
        assert!(validate_jsonpath("a.b.c").is_ok());
        assert!(validate_jsonpath("field").is_ok());
    }

    #[test]
    fn jsonpath_with_array_index() {
        assert!(validate_jsonpath("$.items[0]").is_ok());
        assert!(validate_jsonpath("$.data[0].name").is_ok());
        assert!(validate_jsonpath("items[123].value").is_ok());
    }

    #[test]
    fn jsonpath_invalid_filter() {
        assert!(validate_jsonpath("$.a[?(@.x==1)]").is_err());
    }

    #[test]
    fn jsonpath_invalid_wildcard() {
        assert!(validate_jsonpath("$.a[*]").is_err());
    }

    #[test]
    fn jsonpath_invalid_slice() {
        assert!(validate_jsonpath("$.a[0:5]").is_err());
    }

    #[test]
    fn jsonpath_empty_segment() {
        assert!(validate_jsonpath("$.a..b").is_err());
        assert!(validate_jsonpath("$..a").is_err());
    }

    #[test]
    fn jsonpath_invalid_identifier() {
        assert!(validate_jsonpath("$.123invalid").is_err());
        assert!(validate_jsonpath("$.a-b").is_err());
    }
}
