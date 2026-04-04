//! for_each binding resolution — extracted from runner.rs
//!
//! Resolves for_each items expressions from all binding formats into a `Vec<Value>`.
//! Handles 4 formats:
//! 1. Pipe transform: `$task | transform ?? default`
//! 2. Dollar prefix: `$alias` or `$alias.nested.path` or `$inputs.xxx`
//! 3. Template inputs: `{{inputs.param_path}}`
//! 4. Template with: `{{with.alias.path}}`

use serde_json::Value;

use crate::binding::ResolvedBindings;
use crate::error::NikaError;
use crate::store::RunContext;

/// Result of resolving for_each items from a binding expression.
#[allow(dead_code)] // Wired into runner.rs in next commit
pub(crate) enum ForEachResolution {
    /// Successfully resolved to a list of items.
    Items(Vec<Value>),
    /// Expression is not a binding reference (may be a literal array).
    NotABinding,
    /// Binding resolution failed — caller should emit TaskFailed with this message.
    Failed(String),
}

/// Resolve a for_each items expression from any binding format into items.
///
/// TODO: Wire into runner.rs to replace inline resolution block (next commit).
///
/// This is the unified entry point that replaces 4 separate branches in runner.rs.
/// It handles `$task | transform`, `$alias.path`, `{{inputs.x}}`, `{{with.x}}`.
///
/// Returns `ForEachResolution::NotABinding` for literal arrays or unrecognized formats
/// (caller should try `for_each.parse_items()` as fallback).
#[allow(dead_code)] // Wired into runner.rs in next commit
pub(crate) fn resolve_for_each_binding(
    items_str: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> ForEachResolution {
    if items_str.starts_with('$') && (items_str.contains('|') || items_str.contains("??")) {
        // Format 1: Pipe transform — $task | transform ?? default
        resolve_pipe_transform(items_str, bindings, datastore)
    } else if let Some(alias) = items_str.strip_prefix('$') {
        // Format 2: Dollar prefix — $alias or $alias.nested.path or $inputs.xxx
        resolve_dollar_binding(alias, items_str, bindings, datastore)
    } else if items_str.contains("{{inputs.") {
        // Format 3: Template inputs — {{inputs.param_path}}
        resolve_template_inputs(items_str, datastore)
    } else if items_str.contains("{{with.") {
        // Format 4: Template with — {{with.alias.path}}
        resolve_template_with(items_str, bindings, datastore)
    } else {
        ForEachResolution::NotABinding
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Format 1: $task | transform ?? default
// ═══════════════════════════════════════════════════════════════════════════

fn resolve_pipe_transform(
    items_str: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> ForEachResolution {
    let entry = match crate::binding::parse_with_entry(items_str) {
        Ok(e) => e,
        Err(e) => {
            return ForEachResolution::Failed(format!(
                "for_each items '{}' parse error: {}",
                items_str, e
            ));
        }
    };

    // Resolve the raw value from the binding source
    let raw_value = match resolve_binding_source(
        &entry.source.source,
        &entry.source.segments,
        items_str,
        bindings,
        datastore,
        entry.default.as_ref(),
    ) {
        Ok(v) => v,
        Err(msg) => return ForEachResolution::Failed(msg),
    };

    // Apply transforms
    let transformed = if let Some(ref expr) = entry.transform {
        match expr.apply(&raw_value) {
            Ok(v) => v,
            Err(e) => {
                return ForEachResolution::Failed(format!(
                    "for_each binding '{}' transform failed: {}",
                    items_str, e
                ));
            }
        }
    } else {
        raw_value
    };

    // Apply default if transformed value is null
    let final_value = if transformed.is_null() {
        entry.default.unwrap_or(transformed)
    } else {
        transformed
    };

    to_items_or_fail(&final_value, items_str)
}

// ═══════════════════════════════════════════════════════════════════════════
// Format 2: $alias or $alias.nested.path or $inputs.xxx
// ═══════════════════════════════════════════════════════════════════════════

fn resolve_dollar_binding(
    alias: &str,
    items_str: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> ForEachResolution {
    // Check for $inputs.xxx format first (workflow inputs)
    if alias.starts_with("inputs.") {
        return match datastore.resolve_input_path(alias) {
            Some(value) => to_items_or_fail(&value, items_str),
            None => ForEachResolution::Failed(format!(
                "for_each input '{}' not found in workflow inputs",
                alias
            )),
        };
    }

    // $alias or $alias.nested.path
    let mut segments = alias.split('.');
    let base_alias = match segments.next() {
        Some(a) if !a.is_empty() => a,
        _ => {
            return ForEachResolution::Failed(
                "for_each: empty alias after '$' prefix".to_string(),
            );
        }
    };

    // Try with: bindings first, then fall back to datastore
    let base_value = match bindings
        .get_resolved(base_alias, datastore)
        .or_else(|_| {
            datastore
                .get_output(base_alias)
                .map(|arc| arc.as_ref().clone())
                .ok_or_else(|| NikaError::BindingNotFound {
                    alias: base_alias.to_string(),
                })
        }) {
        Ok(v) => v,
        Err(e) => {
            return ForEachResolution::Failed(format!(
                "for_each binding '{}' not found: {}",
                base_alias, e
            ));
        }
    };

    // Auto-parse JSON strings before traversal
    let working = crate::binding::jsonpath::try_parse_json_str(&base_value)
        .unwrap_or_else(|| base_value.clone());

    // Traverse nested path segments
    let remaining: Vec<&str> = segments.collect();
    match traverse_path(&working, &remaining) {
        Ok(value) => to_items_or_fail(value, items_str),
        Err(segment) => ForEachResolution::Failed(format!(
            "for_each binding '${}': nested path segment '{}' not found",
            alias, segment
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Format 3: {{inputs.param_path}}
// ═══════════════════════════════════════════════════════════════════════════

fn resolve_template_inputs(items_str: &str, datastore: &RunContext) -> ForEachResolution {
    let Some(start) = items_str.find("{{inputs.") else {
        return ForEachResolution::NotABinding;
    };
    let after = &items_str[start + 9..];
    let Some(end) = after.find("}}") else {
        return ForEachResolution::NotABinding;
    };
    let param_path = &after[..end];
    let full_path = format!("inputs.{}", param_path);

    match datastore.resolve_input_path(&full_path) {
        Some(value) => to_items_or_fail(&value, items_str),
        None => ForEachResolution::Failed(format!(
            "for_each input '{}' not found in workflow inputs",
            full_path
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Format 4: {{with.alias.path}}
// ═══════════════════════════════════════════════════════════════════════════

fn resolve_template_with(
    items_str: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
) -> ForEachResolution {
    let Some(start) = items_str.find("{{with.") else {
        return ForEachResolution::NotABinding;
    };
    let after = &items_str[start + 7..];
    let Some(end) = after.find("}}") else {
        return ForEachResolution::NotABinding;
    };
    let path = &after[..end];
    let mut parts = path.split('.');
    let Some(alias) = parts.next() else {
        return ForEachResolution::NotABinding;
    };

    let base_value = match bindings.get_resolved(alias, datastore) {
        Ok(v) => v,
        Err(e) => {
            return ForEachResolution::Failed(format!(
                "for_each binding '{}' not found: {}",
                alias, e
            ));
        }
    };

    // Auto-parse JSON strings
    let working = crate::binding::jsonpath::try_parse_json_str(&base_value)
        .unwrap_or_else(|| base_value.clone());

    let remaining: Vec<&str> = parts.collect();
    match traverse_path(&working, &remaining) {
        Ok(value) => to_items_or_fail(value, items_str),
        Err(_segment) => ForEachResolution::Failed(format!(
            "for_each items: path traversal failed for '{{{{with.{}}}}}'",
            path
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Shared helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Resolve a binding source (Task, Input, Env) with path traversal and default fallback.
fn resolve_binding_source(
    source: &crate::binding::BindingSource,
    path_segments: &[crate::binding::PathSegment],
    items_str: &str,
    bindings: &ResolvedBindings,
    datastore: &RunContext,
    default: Option<&Value>,
) -> Result<Value, String> {
    let base_value = match source {
        crate::binding::BindingSource::Task(task_id) => {
            match bindings
                .get_resolved(task_id, datastore)
                .or_else(|_| {
                    datastore
                        .get_output(task_id)
                        .map(|arc| arc.as_ref().clone())
                        .ok_or_else(|| NikaError::BindingNotFound {
                            alias: task_id.to_string(),
                        })
                }) {
                Ok(output) => {
                    let working = crate::binding::jsonpath::try_parse_json_str(&output)
                        .unwrap_or_else(|| output.clone());

                    // Navigate path segments
                    let mut value_ref = &working;
                    for seg in path_segments {
                        let next = match seg {
                            crate::binding::PathSegment::Field(f) => value_ref.get(f.as_ref()),
                            crate::binding::PathSegment::Index(i) => value_ref.get(*i),
                        };
                        match next {
                            Some(v) => value_ref = v,
                            None => {
                                return Err(format!(
                                    "for_each binding '{}': path segment '{:?}' not found",
                                    items_str, seg
                                ));
                            }
                        }
                    }
                    value_ref.clone()
                }
                Err(_) => match default {
                    Some(d) => d.clone(),
                    None => {
                        return Err(format!(
                            "for_each binding '{}': '{}' not found in with: bindings or task outputs",
                            items_str, task_id
                        ));
                    }
                },
            }
        }
        crate::binding::BindingSource::Input(sub_path) => {
            let full_path = format!("inputs.{}", sub_path);
            match datastore.resolve_input_path(&full_path) {
                Some(v) => v,
                None => match default {
                    Some(d) => d.clone(),
                    None => {
                        return Err(format!(
                            "for_each binding '{}': input '{}' not found",
                            items_str, full_path
                        ));
                    }
                },
            }
        }
        crate::binding::BindingSource::Env(var) => match std::env::var(var.as_ref()) {
            Ok(v) => Value::String(v),
            Err(_) => match default {
                Some(d) => d.clone(),
                None => {
                    return Err(format!(
                        "for_each binding '{}': env var '{}' not set",
                        items_str, var
                    ));
                }
            },
        },
        other => {
            return Err(format!(
                "for_each binding '{}': unsupported source type '{:?}'",
                items_str, other
            ));
        }
    };

    Ok(base_value)
}

/// Traverse nested path segments on a JSON value.
///
/// Returns a reference to the final value, or the failing segment name.
fn traverse_path<'a>(value: &'a Value, segments: &[&str]) -> Result<&'a Value, String> {
    let mut current = value;
    for &segment in segments {
        let next = if let Ok(idx) = segment.parse::<usize>() {
            current.get(idx)
        } else {
            current.get(segment)
        };
        match next {
            Some(v) => current = v,
            None => return Err(segment.to_string()),
        }
    }
    Ok(current)
}

/// Convert a resolved value to an array of items, or fail with a message.
fn to_items_or_fail(value: &Value, items_str: &str) -> ForEachResolution {
    match super::runner::value_to_array(value) {
        Some(items) => ForEachResolution::Items(items),
        None => ForEachResolution::Failed(format!(
            "for_each binding '{}' resolved to non-array value after transforms",
            items_str
        )),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn traverse_path_empty_segments() {
        let val = json!({"a": 1});
        assert_eq!(traverse_path(&val, &[]).unwrap(), &json!({"a": 1}));
    }

    #[test]
    fn traverse_path_single_field() {
        let val = json!({"items": [1, 2, 3]});
        assert_eq!(traverse_path(&val, &["items"]).unwrap(), &json!([1, 2, 3]));
    }

    #[test]
    fn traverse_path_nested_fields() {
        let val = json!({"data": {"results": {"items": [4, 5]}}});
        assert_eq!(
            traverse_path(&val, &["data", "results", "items"]).unwrap(),
            &json!([4, 5])
        );
    }

    #[test]
    fn traverse_path_array_index() {
        let val = json!({"arr": [10, 20, 30]});
        let arr = traverse_path(&val, &["arr"]).unwrap();
        assert_eq!(traverse_path(arr, &["1"]).unwrap(), &json!(20));
    }

    #[test]
    fn traverse_path_missing_segment() {
        let val = json!({"a": 1});
        let err = traverse_path(&val, &["missing"]).unwrap_err();
        assert_eq!(err, "missing");
    }

    #[test]
    fn traverse_path_deeply_nested_missing() {
        let val = json!({"a": {"b": 1}});
        let err = traverse_path(&val, &["a", "c"]).unwrap_err();
        assert_eq!(err, "c");
    }

    #[test]
    fn not_a_binding_for_literal_array() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();
        match resolve_for_each_binding("[1, 2, 3]", &bindings, &datastore) {
            ForEachResolution::NotABinding => {} // expected
            ForEachResolution::Items(_) => panic!("Expected NotABinding, got Items"),
            ForEachResolution::Failed(msg) => panic!("Expected NotABinding, got Failed: {msg}"),
        }
    }

    #[test]
    fn not_a_binding_for_plain_string() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();
        match resolve_for_each_binding("just a string", &bindings, &datastore) {
            ForEachResolution::NotABinding => {}
            _ => panic!("Expected NotABinding"),
        }
    }

    #[test]
    fn dollar_inputs_resolves_from_datastore() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();
        let mut inputs = rustc_hash::FxHashMap::default();
        inputs.insert("locales".to_string(), json!(["en", "fr", "de"]));
        datastore.set_inputs(inputs);

        match resolve_for_each_binding("$inputs.locales", &bindings, &datastore) {
            ForEachResolution::Items(items) => {
                assert_eq!(items, vec![json!("en"), json!("fr"), json!("de")]);
            }
            ForEachResolution::Failed(msg) => panic!("Failed: {}", msg),
            ForEachResolution::NotABinding => panic!("Expected Items"),
        }
    }

    #[test]
    fn dollar_inputs_missing_fails() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();

        match resolve_for_each_binding("$inputs.nonexistent", &bindings, &datastore) {
            ForEachResolution::Failed(msg) => {
                assert!(msg.contains("not found"), "Error: {msg}");
            }
            _ => panic!("Expected Failed"),
        }
    }

    #[test]
    fn template_inputs_resolves() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();
        let mut inputs = rustc_hash::FxHashMap::default();
        inputs.insert("items".to_string(), json!([1, 2, 3]));
        datastore.set_inputs(inputs);

        match resolve_for_each_binding("{{inputs.items}}", &bindings, &datastore) {
            ForEachResolution::Items(items) => {
                assert_eq!(items, vec![json!(1), json!(2), json!(3)]);
            }
            ForEachResolution::Failed(msg) => panic!("Failed: {}", msg),
            ForEachResolution::NotABinding => panic!("Expected Items"),
        }
    }

    #[test]
    fn dollar_alias_with_path_resolves() {
        let datastore = RunContext::new();
        // Simulate a task output as JSON string
        datastore.insert(
            std::sync::Arc::from("prev_task"),
            crate::store::TaskResult::success(
                json!({"tags": ["a", "b", "c"]}).to_string(),
                std::time::Duration::ZERO,
            ),
        );

        let bindings = ResolvedBindings::new();
        match resolve_for_each_binding("$prev_task.tags", &bindings, &datastore) {
            ForEachResolution::Items(items) => {
                assert_eq!(items, vec![json!("a"), json!("b"), json!("c")]);
            }
            ForEachResolution::Failed(msg) => panic!("Failed: {}", msg),
            ForEachResolution::NotABinding => panic!("Expected Items"),
        }
    }

    #[test]
    fn empty_alias_after_dollar_fails() {
        let bindings = ResolvedBindings::new();
        let datastore = RunContext::new();
        match resolve_for_each_binding("$", &bindings, &datastore) {
            ForEachResolution::Failed(msg) => {
                assert!(msg.contains("empty alias"), "Error: {msg}");
            }
            _ => panic!("Expected Failed"),
        }
    }
}
