//! Workflow validation (v0.1)
//!
//! Validates:
//! - use: block references (task exists, is upstream)
//! - Template refs match use: declarations
//! - JSONPath syntax (minimal subset)

use std::collections::HashSet;

use crate::dag::DagAnalyzer;
use crate::error::NikaError;
use crate::template;
use crate::use_block::{UseBlock, UseEntry};
use crate::workflow::Workflow;

/// Validate a workflow's use: blocks against the DAG
pub fn validate_use_blocks(workflow: &Workflow, dag: &DagAnalyzer) -> Result<(), NikaError> {
    let all_task_ids: HashSet<String> = workflow.tasks.iter().map(|t| t.id.clone()).collect();

    for task in &workflow.tasks {
        if let Some(ref use_block) = task.use_block {
            validate_use_block(&task.id, use_block, &all_task_ids, dag)?;
        }
    }

    Ok(())
}

/// Validate a single use: block
fn validate_use_block(
    task_id: &str,
    use_block: &UseBlock,
    all_task_ids: &HashSet<String>,
    dag: &DagAnalyzer,
) -> Result<(), NikaError> {
    for (alias, entry) in use_block {
        match entry {
            // Form 1: alias: task.path - extract task_id from path
            UseEntry::Path(path) => {
                let from_task = path.split('.').next().unwrap_or(path);
                validate_from_task(alias, from_task, task_id, all_task_ids, dag)?;
            }

            // Form 2: task.path: [fields] - the key is the path
            UseEntry::Batch(_) => {
                let from_task = alias.split('.').next().unwrap_or(alias);
                validate_from_task(from_task, from_task, task_id, all_task_ids, dag)?;
            }

            // Form 3: alias: { from, path, default }
            UseEntry::Advanced(adv) => {
                validate_from_task(alias, &adv.from, task_id, all_task_ids, dag)?;

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
    dag: &DagAnalyzer,
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
    if !dag.has_path(from_task, task_id) {
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
    let path = if path.starts_with("$.") {
        &path[2..] // Skip "$."
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

/// Validate template references against declared use: aliases
pub fn validate_template_refs(
    template: &str,
    use_block: Option<&UseBlock>,
    task_id: &str,
) -> Result<(), NikaError> {
    let declared: HashSet<String> = use_block
        .map(|b| extract_declared_aliases(b))
        .unwrap_or_default();

    template::validate_refs(template, &declared, task_id)
}

/// Extract all declared aliases from a use: block
fn extract_declared_aliases(use_block: &UseBlock) -> HashSet<String> {
    let mut aliases = HashSet::new();

    for (key, entry) in use_block {
        match entry {
            // Form 1: alias is the key
            UseEntry::Path(_) => {
                aliases.insert(key.clone());
            }
            // Form 2: fields become aliases
            UseEntry::Batch(fields) => {
                for field in fields {
                    aliases.insert(field.clone());
                }
            }
            // Form 3: alias is the key
            UseEntry::Advanced(_) => {
                aliases.insert(key.clone());
            }
        }
    }

    aliases
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

    // ─────────────────────────────────────────────────────────────
    // Declared aliases extraction tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn extract_aliases_path_form() {
        let mut block = UseBlock::new();
        block.insert("weather".to_string(), UseEntry::Path("forecast.summary".to_string()));

        let aliases = extract_declared_aliases(&block);
        assert!(aliases.contains("weather"));
        assert_eq!(aliases.len(), 1);
    }

    #[test]
    fn extract_aliases_batch_form() {
        let mut block = UseBlock::new();
        block.insert(
            "flight".to_string(),
            UseEntry::Batch(vec!["price".to_string(), "airline".to_string()]),
        );

        let aliases = extract_declared_aliases(&block);
        assert!(aliases.contains("price"));
        assert!(aliases.contains("airline"));
        assert_eq!(aliases.len(), 2);
    }

    #[test]
    fn extract_aliases_advanced_form() {
        use crate::use_block::UseAdvanced;

        let mut block = UseBlock::new();
        block.insert(
            "summary".to_string(),
            UseEntry::Advanced(UseAdvanced {
                from: "weather".to_string(),
                path: Some("data.summary".to_string()),
                default: None,
            }),
        );

        let aliases = extract_declared_aliases(&block);
        assert!(aliases.contains("summary"));
        assert_eq!(aliases.len(), 1);
    }
}
