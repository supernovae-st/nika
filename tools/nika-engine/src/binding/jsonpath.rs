// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSONPath evaluation — RFC 9535 via `serde_json_path`
//!
//! Replaces the minimal custom `util/jsonpath.rs` with RFC 9535 compliance.
//!
//! Supports:
//! - Dot notation: `$.a.b.c`
//! - Array index: `$.a[0].b`
//! - Wildcards: `$[*]`, `$.items[*].name`
//! - Filters: `$.items[?@.price > 100]`
//! - Recursive descent: `$..email`
//! - Slices: `$.items[0:5]`
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::binding::jsonpath;
//!
//! let value = json!({"items": [{"name": "a"}, {"name": "b"}]});
//!
//! // Simple path (fast, no RFC parse overhead)
//! let result = jsonpath::resolve(&value, "items[0].name")?;
//! assert_eq!(result, Some(json!("a")));
//!
//! // Rich JSONPath query (RFC 9535)
//! let result = jsonpath::query(&value, "$.items[*].name")?;
//! assert_eq!(result, json!(["a", "b"]));
//! ```

use serde_json::Value;
use serde_json_path::JsonPath;

use crate::error::NikaError;

/// Try to parse a JSON string Value into a structured Value.
///
/// If `value` is a `Value::String` containing valid JSON (object or array),
/// returns the parsed Value. Otherwise returns `None`.
///
/// This is the single source of truth for the "auto-parse JSON strings"
/// pattern used throughout the binding system (jsonpath, resolve, runner).
pub fn try_parse_json_str(value: &Value) -> Option<Value> {
    if let Value::String(s) = value {
        let trimmed = s.trim();
        if (trimmed.starts_with('{') && trimmed.ends_with('}'))
            || (trimmed.starts_with('[') && trimmed.ends_with(']'))
        {
            serde_json::from_str::<Value>(trimmed).ok()
        } else {
            None
        }
    } else {
        None
    }
}

/// Evaluate a RFC 9535 JSONPath expression against a JSON value.
///
/// Returns:
/// - `Null` when no nodes match
/// - The single value when exactly one node matches
/// - An array of values when multiple nodes match
///
/// Use this for rich expressions with wildcards, filters, slices, or
/// recursive descent. For simple `$.a.b[0]` paths, prefer `resolve()`
/// which avoids the RFC parser overhead.
pub fn query(value: &Value, path: &str) -> Result<Value, NikaError> {
    let jp = JsonPath::parse(path).map_err(|e| NikaError::JsonPathUnsupported {
        path: format!("{}: {}", path, e),
    })?;
    let results = jp.query(value);
    let nodes: Vec<&Value> = results.all();
    match nodes.len() {
        0 => Ok(Value::Null),
        1 => Ok(nodes[0].clone()),
        _ => Ok(Value::Array(nodes.into_iter().cloned().collect())),
    }
}

/// Check if a path uses rich JSONPath features (wildcards, filters, slices, recursive descent).
///
/// Returns `true` for expressions that need `query()` (RFC 9535 parser),
/// `false` for simple dot/index paths that `resolve()` handles directly.
pub fn is_jsonpath(path: &str) -> bool {
    // Check for rich JSONPath operators inside brackets
    if let Some(start) = path.find('[') {
        let bracket_content = &path[start..];
        if bracket_content.contains('*')
            || bracket_content.contains('?')
            || bracket_content.contains(':')
        {
            return true;
        }
    }
    // Recursive descent operator
    path.contains("..")
}

// ═══════════════════════════════════════════════════════════════
// Simple path resolution
// ═══════════════════════════════════════════════════════════════

/// A parsed path segment (field or array index)
#[derive(Debug, Clone, PartialEq)]
pub enum Segment {
    /// Object field access: .field
    Field(String),
    /// Array index access: \[0\]
    Index(usize),
}

/// Parse a simple JSONPath string into segments.
///
/// Supports:
/// - `$.a.b.c` (dot notation with `$` prefix)
/// - `a.b.c` (dot notation without prefix)
/// - `items[0].name` (array index in bracket notation)
/// - `items.0.name` (array index as numeric segment)
///
/// Does NOT support wildcards, filters, slices, or recursive descent.
/// Use `query()` for those.
pub fn parse(path: &str) -> Result<Vec<Segment>, NikaError> {
    // Remove $. prefix if present
    let path = if let Some(stripped) = path.strip_prefix("$.") {
        stripped
    } else if path == "$" {
        return Ok(vec![]);
    } else {
        path
    };

    if path.is_empty() {
        return Ok(vec![]);
    }

    let mut segments = Vec::new();

    for part in path.split('.') {
        if part.is_empty() {
            return Err(NikaError::JsonPathUnsupported {
                path: path.to_string(),
            });
        }

        // Check for array index: field[0] or just [0]
        if let Some(bracket_pos) = part.find('[') {
            let field = &part[..bracket_pos];
            if !field.is_empty() {
                segments.push(Segment::Field(field.to_string()));
            }

            if !part.ends_with(']') {
                return Err(NikaError::JsonPathUnsupported {
                    path: path.to_string(),
                });
            }

            let index_str = &part[bracket_pos + 1..part.len() - 1];
            let index: usize = index_str
                .parse()
                .map_err(|_| NikaError::JsonPathUnsupported {
                    path: path.to_string(),
                })?;

            segments.push(Segment::Index(index));
        } else if let Ok(index) = part.parse::<usize>() {
            segments.push(Segment::Index(index));
        } else {
            segments.push(Segment::Field(part.to_string()));
        }
    }

    Ok(segments)
}

/// Apply parsed segments to a JSON value.
///
/// Uses references internally, clones once at the end.
pub fn apply(value: &Value, segments: &[Segment]) -> Option<Value> {
    // Auto-parse JSON strings before traversal so that exec: output
    // like '{"name":"Nika"}' can be accessed via $task.name
    let parsed;
    let mut current = if let Some(v) = try_parse_json_str(value) {
        parsed = v;
        &parsed
    } else {
        value
    };

    for segment in segments {
        current = match segment {
            Segment::Field(name) => current.get(name)?,
            Segment::Index(idx) => current.get(*idx)?,
        };
    }

    Some(current.clone())
}

/// Parse and apply a simple path in one step.
///
/// For simple dot/index paths like `data.items[0].name`.
/// Returns `Ok(None)` if the path doesn't match.
///
/// For rich JSONPath expressions (wildcards, filters), use `query()`.
pub fn resolve(value: &Value, path: &str) -> Result<Option<Value>, NikaError> {
    let segments = parse(path)?;
    Ok(apply(value, &segments))
}

/// Validate simple JSONPath syntax without evaluating.
pub fn validate(path: &str) -> Result<(), NikaError> {
    parse(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ═══════════════════════════════════════════════════════════════
    // Simple path resolution
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn parse_simple_path() {
        let segments = parse("$.a.b.c").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Field("a".to_string()),
                Segment::Field("b".to_string()),
                Segment::Field("c".to_string()),
            ]
        );
    }

    #[test]
    fn parse_without_dollar() {
        let segments = parse("a.b").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Field("a".to_string()),
                Segment::Field("b".to_string()),
            ]
        );
    }

    #[test]
    fn parse_with_array_index() {
        let segments = parse("$.items[0].name").unwrap();
        assert_eq!(
            segments,
            vec![
                Segment::Field("items".to_string()),
                Segment::Index(0),
                Segment::Field("name".to_string()),
            ]
        );
    }

    #[test]
    fn parse_just_root() {
        let segments = parse("$").unwrap();
        assert!(segments.is_empty());
    }

    #[test]
    fn apply_simple() {
        let value = json!({"a": {"b": "value"}});
        let segments = parse("$.a.b").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, Some(json!("value")));
    }

    #[test]
    fn apply_array_index() {
        let value = json!({"items": ["first", "second", "third"]});
        let segments = parse("$.items[1]").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, Some(json!("second")));
    }

    #[test]
    fn apply_nested_array() {
        let value = json!({
            "users": [
                {"name": "Alice"},
                {"name": "Bob"}
            ]
        });
        let segments = parse("$.users[0].name").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, Some(json!("Alice")));
    }

    #[test]
    fn apply_missing_field() {
        let value = json!({"a": 1});
        let segments = parse("$.b").unwrap();
        let result = apply(&value, &segments);
        assert_eq!(result, None);
    }

    #[test]
    fn resolve_shorthand() {
        let value = json!({"price": {"currency": "EUR", "amount": 100}});
        let result = resolve(&value, "$.price.currency").unwrap();
        assert_eq!(result, Some(json!("EUR")));
    }

    #[test]
    fn parse_numeric_index_as_dot() {
        let segments = parse("items.0").unwrap();
        assert_eq!(
            segments,
            vec![Segment::Field("items".to_string()), Segment::Index(0)]
        );
    }

    #[test]
    fn apply_numeric_index_as_dot() {
        let value = json!({"items": ["first", "second"]});
        let result = resolve(&value, "items.1").unwrap();
        assert_eq!(result, Some(json!("second")));
    }

    // ═══════════════════════════════════════════════════════════════
    // Rich JSONPath via serde_json_path (RFC 9535)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn query_root() {
        let value = json!({"name": "test"});
        let result = query(&value, "$").unwrap();
        assert_eq!(result, json!({"name": "test"}));
    }

    #[test]
    fn query_single_field() {
        let value = json!({"name": "test", "age": 30});
        let result = query(&value, "$.name").unwrap();
        assert_eq!(result, json!("test"));
    }

    #[test]
    fn query_nested_field() {
        let value = json!({"data": {"items": [1, 2, 3]}});
        let result = query(&value, "$.data.items").unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn query_wildcard() {
        let value = json!({
            "items": [
                {"name": "Alice"},
                {"name": "Bob"},
                {"name": "Charlie"}
            ]
        });
        let result = query(&value, "$.items[*].name").unwrap();
        assert_eq!(result, json!(["Alice", "Bob", "Charlie"]));
    }

    #[test]
    fn query_array_index() {
        let value = json!({"items": ["a", "b", "c"]});
        let result = query(&value, "$.items[1]").unwrap();
        assert_eq!(result, json!("b"));
    }

    #[test]
    fn query_recursive_descent() {
        let value = json!({
            "a": {"email": "a@test.com"},
            "b": {"nested": {"email": "b@test.com"}}
        });
        let result = query(&value, "$..email").unwrap();
        // Recursive descent returns all matching nodes
        assert!(result.is_array());
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert!(arr.contains(&json!("a@test.com")));
        assert!(arr.contains(&json!("b@test.com")));
    }

    #[test]
    fn query_filter() {
        let value = json!({
            "items": [
                {"name": "cheap", "price": 5},
                {"name": "expensive", "price": 150},
                {"name": "mid", "price": 50}
            ]
        });
        let result = query(&value, "$.items[?@.price > 100]").unwrap();
        assert_eq!(result, json!({"name": "expensive", "price": 150}));
    }

    #[test]
    fn query_filter_multiple_results() {
        let value = json!({
            "items": [
                {"name": "a", "price": 5},
                {"name": "b", "price": 150},
                {"name": "c", "price": 200}
            ]
        });
        let result = query(&value, "$.items[?@.price > 100]").unwrap();
        assert_eq!(
            result,
            json!([
                {"name": "b", "price": 150},
                {"name": "c", "price": 200}
            ])
        );
    }

    #[test]
    fn query_no_match() {
        let value = json!({"a": 1});
        let result = query(&value, "$.missing").unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn query_slice() {
        let value = json!({"items": [0, 1, 2, 3, 4, 5]});
        let result = query(&value, "$.items[1:4]").unwrap();
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn query_invalid_syntax() {
        let result = query(&json!({}), "$.items[[invalid");
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // is_jsonpath detection
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn is_jsonpath_simple_paths() {
        assert!(!is_jsonpath("$.a.b.c"));
        assert!(!is_jsonpath("items[0].name"));
        assert!(!is_jsonpath("a.b"));
    }

    #[test]
    fn is_jsonpath_rich_expressions() {
        assert!(is_jsonpath("$.items[*].name"));
        assert!(is_jsonpath("$.items[?@.price > 100]"));
        assert!(is_jsonpath("$..email"));
        assert!(is_jsonpath("$.items[0:5]"));
        assert!(is_jsonpath("$.items[:3]"));
    }

    // ═══════════════════════════════════════════════════════════════
    // Validate
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn validate_valid_paths() {
        assert!(validate("$.a.b").is_ok());
        assert!(validate("items[0].name").is_ok());
        assert!(validate("$").is_ok());
    }

    #[test]
    fn validate_empty_segment() {
        assert!(validate("a..b").is_err());
    }
}
