//! Template Resolution - `{{use.alias}}` substitution (v0.1)
//!
//! Single syntax: {{use.alias}} or {{use.alias.field}}
//! True single-pass resolution with Cow<str> for zero-alloc when no templates.
//!
//! Performance optimizations:
//! - Zero-clone traversal (references until final value)
//! - SmallVec for error collection (stack-allocated)
//! - Better capacity estimation for result string

use std::borrow::Cow;
use std::sync::LazyLock;

use regex::Regex;
use rustc_hash::FxHashSet;
use serde_json::Value;
use smallvec::SmallVec;

use crate::error::NikaError;

use super::resolve::ResolvedBindings;

/// Pre-compiled regex for {{use.alias}} or {{use.alias.field}} pattern
static USE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\{\{\s*use\.(\w+(?:\.\w+)*)\s*\}\}").unwrap());

/// Escape for JSON string context
fn escape_for_json(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_control() => {
                result.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => result.push(c),
        }
    }
    result
}

/// Resolve all {{use.alias}} templates using bindings
///
/// Returns Cow::Borrowed when no templates (zero allocation).
/// Returns Cow::Owned with single-pass resolution when templates exist.
///
/// Performance: Zero-clone traversal - uses references until final value_to_string.
///
/// Example: `{{use.forecast}}` → resolved value from bindings
/// Example: `{{use.flight_info.departure}}` → nested access
pub fn resolve<'a>(template: &'a str, bindings: &ResolvedBindings) -> Result<Cow<'a, str>, NikaError> {
    // Early return with borrowed string (zero alloc)
    if !template.contains("{{use.") {
        return Ok(Cow::Borrowed(template));
    }

    // Single-pass: build result by copying segments + inserting replacements
    // Better capacity: template length + some extra for expansions
    let mut result = String::with_capacity(template.len() + 64);
    let mut last_end = 0;
    // SmallVec: stack-allocated for up to 4 errors (common case: 0-1 errors)
    // Note: must be String because alias borrows from cap which is dropped each iteration
    let mut errors: SmallVec<[String; 4]> = SmallVec::new();

    for cap in USE_RE.captures_iter(template) {
        let m = cap.get(0).unwrap();
        let path = &cap[1]; // e.g., "forecast" or "flight_info.departure"

        // Copy segment before this match
        result.push_str(&template[last_end..m.start()]);

        // Split: first segment is alias, rest is nested path
        let mut parts = path.split('.');
        let alias = parts.next().unwrap();

        // Get the resolved value for this alias
        match bindings.get(alias) {
            Some(base_value) => {
                // Zero-clone traversal: use references until we need the final value
                let mut value_ref: &Value = base_value;
                let mut traversed_segments: SmallVec<[&str; 8]> = SmallVec::new();
                traversed_segments.push(alias);

                // Traverse nested path if present (all by reference)
                for segment in parts {
                    let next = if let Ok(idx) = segment.parse::<usize>() {
                        value_ref.get(idx)
                    } else {
                        value_ref.get(segment)
                    };

                    match next {
                        Some(v) => {
                            traversed_segments.push(segment);
                            value_ref = v;
                        }
                        None => {
                            // Determine if it's an invalid traversal or missing field
                            let value_type = match value_ref {
                                Value::Null => "null",
                                Value::Bool(_) => "bool",
                                Value::Number(_) => "number",
                                Value::String(_) => "string",
                                Value::Array(_) => "array",
                                Value::Object(_) => "object",
                            };

                            if matches!(value_ref, Value::Object(_) | Value::Array(_)) {
                                // Field/index doesn't exist - build path for error
                                let traversed_path = traversed_segments.join(".");
                                return Err(NikaError::PathNotFound {
                                    path: format!("{}.{}", traversed_path, segment),
                                });
                            } else {
                                // Trying to traverse a primitive
                                return Err(NikaError::InvalidTraversal {
                                    segment: segment.to_string(),
                                    value_type: value_type.to_string(),
                                    full_path: path.to_string(),
                                });
                            }
                        }
                    }
                }

                // Convert Value to string (strict mode - null is error)
                // This is the ONLY place we convert/allocate for the value
                let replacement = value_to_string(value_ref, path, alias)?;

                // Escape if we're in a JSON context
                let replacement = if is_in_json_context(template, m.start()) {
                    escape_for_json(&replacement)
                } else {
                    replacement
                };

                result.push_str(&replacement);
            }
            None => {
                errors.push(alias.to_string());
            }
        }

        last_end = m.end();
    }

    if !errors.is_empty() {
        return Err(NikaError::Template(format!(
            "Alias(es) not resolved: {}. Did you declare them in 'use:'?",
            errors.join(", ")
        )));
    }

    // Copy remaining segment after last match
    result.push_str(&template[last_end..]);

    Ok(Cow::Owned(result))
}

/// Convert JSON Value to string for template substitution (strict mode)
///
/// Returns error for null values - this prevents silent bugs from missing data.
fn value_to_string(value: &Value, path: &str, alias: &str) -> Result<String, NikaError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Null => Err(NikaError::NullValue {
            path: path.to_string(),
            alias: alias.to_string(),
        }),
        Value::Bool(b) => Ok(b.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        // For objects/arrays, return compact JSON representation
        other => Ok(other.to_string()),
    }
}

/// Check if position is inside a JSON string
fn is_in_json_context(template: &str, pos: usize) -> bool {
    let before = &template[..pos];
    let mut in_string = false;
    let mut escaped = false;

    for ch in before.chars() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => in_string = !in_string,
            _ => {}
        }
    }

    in_string
}

/// Extract all alias references from a template (for static validation)
///
/// Returns a Vec of (alias, full_path) tuples.
/// Example: "{{use.weather.temp}}" → vec![("weather", "weather.temp")]
#[allow(dead_code)] // Used in tests and future static validation
pub fn extract_refs(template: &str) -> Vec<(String, String)> {
    USE_RE
        .captures_iter(template)
        .map(|cap| {
            let full_path = cap[1].to_string();
            let alias = full_path.split('.').next().unwrap().to_string();
            (alias, full_path)
        })
        .collect()
}

/// Validate that all template references exist in declared aliases (static validation)
///
/// This is called by `nika validate` before runtime.
/// Returns Ok(()) if valid, Err with first unknown alias if not.
#[allow(dead_code)] // Used in tests and future static validation
pub fn validate_refs(
    template: &str,
    declared_aliases: &FxHashSet<String>,
    task_id: &str,
) -> Result<(), NikaError> {
    for (alias, _full_path) in extract_refs(template) {
        if !declared_aliases.contains(&alias) {
            return Err(NikaError::UnknownAlias {
                alias,
                task_id: task_id.to_string(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::borrow::Cow;

    #[test]
    fn resolve_simple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("forecast", json!("Sunny 25C"));

        let result = resolve("Weather: {{use.forecast}}", &bindings).unwrap();
        assert_eq!(result, "Weather: Sunny 25C");
    }

    #[test]
    fn resolve_number() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("price", json!(89));

        let result = resolve("Price: ${{use.price}}", &bindings).unwrap();
        assert_eq!(result, "Price: $89");
    }

    #[test]
    fn resolve_nested() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("flight_info", json!({"departure": "10:30", "gate": "A12"}));

        let result = resolve("Depart at {{use.flight_info.departure}}", &bindings).unwrap();
        assert_eq!(result, "Depart at 10:30");
    }

    #[test]
    fn resolve_multiple() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("a", json!("first"));
        bindings.set("b", json!("second"));

        let result = resolve("{{use.a}} and {{use.b}}", &bindings).unwrap();
        assert_eq!(result, "first and second");
    }

    #[test]
    fn resolve_object() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"x": 1, "y": 2}));

        let result = resolve("Full: {{use.data}}", &bindings).unwrap();
        // Object is serialized as JSON
        assert!(result.contains("\"x\":1") || result.contains("\"x\": 1"));
    }

    #[test]
    fn resolve_alias_not_found() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("known", json!("value"));

        let result = resolve("{{use.unknown}}", &bindings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("unknown"));
    }

    #[test]
    fn resolve_path_not_found() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"a": 1}));

        let result = resolve("{{use.data.nonexistent}}", &bindings);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_no_templates() {
        let bindings = ResolvedBindings::new();
        let result = resolve("No templates here", &bindings).unwrap();
        assert_eq!(result, "No templates here");
        // Verify zero-alloc: should be Cow::Borrowed
        assert!(matches!(result, Cow::Borrowed(_)));
    }

    #[test]
    fn resolve_with_templates_is_owned() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("x", json!("value"));
        let result = resolve("Has {{use.x}} template", &bindings).unwrap();
        assert_eq!(result, "Has value template");
        // With templates: should be Cow::Owned
        assert!(matches!(result, Cow::Owned(_)));
    }

    #[test]
    fn resolve_array_index() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("items", json!(["first", "second", "third"]));

        let result = resolve("Item: {{use.items.0}}", &bindings).unwrap();
        assert_eq!(result, "Item: first");
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: Strict mode tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn resolve_null_is_error() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!(null));

        let result = resolve("Value: {{use.data}}", &bindings);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-072"));
        assert!(err.to_string().contains("Null value"));
    }

    #[test]
    fn resolve_nested_null_is_error() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!({"value": null}));

        let result = resolve("Value: {{use.data.value}}", &bindings);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-072"));
    }

    #[test]
    fn resolve_invalid_traversal_on_string() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("data", json!("just a string"));

        let result = resolve("{{use.data.field}}", &bindings);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-073"));
        assert!(err.to_string().contains("string"));
    }

    #[test]
    fn resolve_invalid_traversal_on_number() {
        let mut bindings = ResolvedBindings::new();
        bindings.set("price", json!(42));

        let result = resolve("{{use.price.currency}}", &bindings);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-073"));
        assert!(err.to_string().contains("number"));
    }

    // ─────────────────────────────────────────────────────────────
    // v0.1: Static validation tests
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn extract_refs_simple() {
        let refs = extract_refs("Hello {{use.weather}}!");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("weather".to_string(), "weather".to_string()));
    }

    #[test]
    fn extract_refs_nested() {
        let refs = extract_refs("{{use.data.field.sub}}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0], ("data".to_string(), "data.field.sub".to_string()));
    }

    #[test]
    fn extract_refs_multiple() {
        let refs = extract_refs("{{use.a}} and {{use.b.c}}");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].0, "a");
        assert_eq!(refs[1].0, "b");
    }

    #[test]
    fn extract_refs_none() {
        let refs = extract_refs("No templates here");
        assert!(refs.is_empty());
    }

    #[test]
    fn validate_refs_success() {
        let declared: FxHashSet<String> =
            ["weather", "price"].iter().map(|s| s.to_string()).collect();
        let result = validate_refs("{{use.weather}} costs {{use.price}}", &declared, "task1");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_refs_unknown_alias() {
        let declared: FxHashSet<String> = ["weather"].iter().map(|s| s.to_string()).collect();
        let result = validate_refs("{{use.weather}} and {{use.unknown}}", &declared, "task1");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("NIKA-071"));
        assert!(err.to_string().contains("unknown"));
    }
}
