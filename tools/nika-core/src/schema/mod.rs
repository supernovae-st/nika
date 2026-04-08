// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! JSON Schema derivation from example values.
//!
//! Pure data transformation utilities that convert JSON examples into
//! JSON Schema documents for structured output validation.
//!
//! - `json_to_schema()` — derives schema from a single example value
//! - `json_to_schema_strict()` — same but adds `additionalProperties: false`
//!
//! # Array Union
//!
//! When an array contains multiple objects, all items are merged:
//! - Properties: union of all keys across all objects
//! - Required: only keys present as non-null in ALL objects
//! - Type conflicts: `anyOf` with unique schemas
//!
//! # Strict Mode
//!
//! Adds `additionalProperties: false` recursively to all object schemas.
//! Mirrors OpenAI Structured Outputs strict mode — the LLM cannot inject
//! extra keys like `reasoning` or `_debug`.

use serde_json::{json, Map, Value};

/// Derive a JSON Schema from a JSON example value.
///
/// Recursively inspects the example and produces a schema that would validate
/// any JSON with the same structure (same keys, same types, same nesting).
///
/// - Objects → `{ "type": "object", "properties": {...}, "required": [...] }`
/// - Arrays  → merged union of all items (see module docs)
/// - Strings → `{ "type": "string" }`
/// - Numbers → `{ "type": "number" }` (integers get `"integer"`)
/// - Bools   → `{ "type": "boolean" }`
/// - Null    → `{ "type": "null" }`
pub fn json_to_schema(value: &Value) -> Value {
    schema_inner(value, false)
}

/// Derive a strict JSON Schema with `additionalProperties: false` on all objects.
///
/// Same derivation as `json_to_schema()`, but every object schema includes
/// `"additionalProperties": false` — preventing the LLM from adding extra keys.
pub fn json_to_schema_strict(value: &Value) -> Value {
    schema_inner(value, true)
}

/// Core recursive schema derivation.
fn schema_inner(value: &Value, strict: bool) -> Value {
    match value {
        Value::Object(map) => object_schema(map, strict),
        Value::Array(items) => array_schema(items, strict),
        Value::String(_) => json!({ "type": "string" }),
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                json!({ "type": "integer" })
            } else {
                json!({ "type": "number" })
            }
        }
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Null => json!({ "type": "null" }),
    }
}

/// Derive schema for a single JSON object.
fn object_schema(map: &Map<String, Value>, strict: bool) -> Value {
    let mut properties = Map::new();
    let mut required: Vec<Value> = Vec::new();
    for (key, val) in map {
        // Null values are excluded from required — they likely represent optional fields.
        if !val.is_null() {
            required.push(Value::String(key.clone()));
        }
        properties.insert(key.clone(), schema_inner(val, strict));
    }
    let mut schema = json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": required
    });
    if strict {
        schema["additionalProperties"] = json!(false);
    }
    schema
}

/// Derive schema for a JSON array with union semantics.
///
/// - Empty array → `{ "type": "array" }`
/// - All objects (2+) → merge properties across all items
/// - Otherwise → derive each, deduplicate, `anyOf` if multiple unique schemas
fn array_schema(items: &[Value], strict: bool) -> Value {
    if items.is_empty() {
        return json!({ "type": "array" });
    }

    // All objects with 2+ items → merge into unified item schema
    if items.len() > 1 && items.iter().all(|v| v.is_object()) {
        return json!({ "type": "array", "items": merge_object_examples(items, strict) });
    }

    // Single item, primitives, or mixed types → deduplicate schemas
    let schemas: Vec<Value> = items.iter().map(|v| schema_inner(v, strict)).collect();
    let unique = deduplicate_schemas(schemas);
    if unique.len() == 1 {
        json!({ "type": "array", "items": unique.into_iter().next().unwrap() })
    } else {
        json!({ "type": "array", "items": { "anyOf": unique } })
    }
}

/// Merge multiple object examples into a unified schema.
///
/// - Properties: union of all keys across all objects
/// - Required: only keys present as non-null in ALL objects
/// - Conflicting types for the same key: `anyOf` with unique schemas
fn merge_object_examples(items: &[Value], strict: bool) -> Value {
    use std::collections::BTreeMap;

    let total = items.len();
    let mut all_properties: BTreeMap<String, Vec<&Value>> = BTreeMap::new();

    for item in items {
        if let Value::Object(map) = item {
            for (key, val) in map {
                all_properties.entry(key.clone()).or_default().push(val);
            }
        }
    }

    let mut properties = Map::new();
    let mut required: Vec<Value> = Vec::new();

    for (key, values) in &all_properties {
        // Derive schema for each occurrence, deduplicate
        let schemas: Vec<Value> = values.iter().map(|v| schema_inner(v, strict)).collect();
        let unique = deduplicate_schemas(schemas);
        let prop_schema = if unique.len() == 1 {
            unique.into_iter().next().unwrap()
        } else {
            json!({ "anyOf": unique })
        };
        properties.insert(key.clone(), prop_schema);

        // Required: must appear in ALL items and never be null
        if values.len() == total && values.iter().all(|v| !v.is_null()) {
            required.push(Value::String(key.clone()));
        }
    }

    let mut schema = json!({
        "type": "object",
        "properties": Value::Object(properties),
        "required": required
    });
    if strict {
        schema["additionalProperties"] = json!(false);
    }
    schema
}

/// Deduplicate schemas by JSON equality.
fn deduplicate_schemas(schemas: Vec<Value>) -> Vec<Value> {
    let mut seen = Vec::new();
    for schema in schemas {
        if !seen.contains(&schema) {
            seen.push(schema);
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // BASIC TYPE DERIVATION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn flat_object() {
        let example = json!({ "title": "hello", "count": 42 });
        let schema = json_to_schema(&example);
        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["title"]["type"], "string");
        assert_eq!(schema["properties"]["count"]["type"], "integer");
    }

    #[test]
    fn nested_object() {
        let example = json!({
            "head": { "title": "x", "rating": { "value": 4.7 } },
            "sections": [{ "type": "hero", "fields": { "title": "y" } }]
        });
        let schema = json_to_schema(&example);
        assert_eq!(schema["properties"]["head"]["type"], "object");
        assert_eq!(
            schema["properties"]["head"]["properties"]["rating"]["properties"]["value"]["type"],
            "number"
        );
        assert_eq!(schema["properties"]["sections"]["type"], "array");
        assert_eq!(
            schema["properties"]["sections"]["items"]["properties"]["type"]["type"],
            "string"
        );
    }

    #[test]
    fn empty_array() {
        let schema = json_to_schema(&json!([]));
        assert_eq!(schema["type"], "array");
        assert!(schema.get("items").is_none());
    }

    #[test]
    fn null_excluded_from_required() {
        let example = json!({
            "name": "alice",
            "optional": null,
            "score": 42
        });
        let schema = json_to_schema(&example);
        let required = schema["required"].as_array().unwrap();
        let required_keys: Vec<&str> = required.iter().map(|v| v.as_str().unwrap()).collect();
        assert!(required_keys.contains(&"name"));
        assert!(required_keys.contains(&"score"));
        assert!(!required_keys.contains(&"optional"));
    }

    #[test]
    fn primitives() {
        assert_eq!(json_to_schema(&json!("hello"))["type"], "string");
        assert_eq!(json_to_schema(&json!(true))["type"], "boolean");
        assert_eq!(json_to_schema(&json!(null))["type"], "null");
        assert_eq!(json_to_schema(&json!(1.5))["type"], "number");
        assert_eq!(json_to_schema(&json!(42))["type"], "integer");
    }

    // ═══════════════════════════════════════════════════════════════
    // ARRAY UNION
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn array_union_merges_object_properties() {
        let example = json!([
            { "id": 1, "name": "Alice" },
            { "id": 2, "name": "Bob", "email": "bob@x.com" }
        ]);
        let schema = json_to_schema(&example);
        assert_eq!(schema["type"], "array");
        let items = &schema["items"];
        assert_eq!(items["type"], "object");
        // Union: id + name + email
        assert!(items["properties"]["id"].is_object());
        assert!(items["properties"]["name"].is_object());
        assert!(items["properties"]["email"].is_object());
        // Required: only id and name (email missing in first object)
        let required: Vec<&str> = items["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"id"));
        assert!(required.contains(&"name"));
        assert!(!required.contains(&"email"));
    }

    #[test]
    fn array_single_object_unchanged() {
        let example = json!([{ "id": 1, "name": "Alice" }]);
        let schema = json_to_schema(&example);
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "object");
        assert_eq!(schema["items"]["properties"]["id"]["type"], "integer");
    }

    #[test]
    fn array_mixed_types_uses_any_of() {
        let example = json!([1, "hello", true]);
        let schema = json_to_schema(&example);
        assert_eq!(schema["type"], "array");
        let any_of = schema["items"]["anyOf"].as_array().unwrap();
        assert_eq!(any_of.len(), 3);
    }

    #[test]
    fn array_same_primitives_deduplicates() {
        let example = json!([1, 2, 3]);
        let schema = json_to_schema(&example);
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "integer");
        assert!(schema["items"].get("anyOf").is_none());
    }

    #[test]
    fn array_union_type_conflict_uses_any_of() {
        let example = json!([
            { "value": 42 },
            { "value": "hello" }
        ]);
        let schema = json_to_schema(&example);
        let items = &schema["items"];
        let value_schema = &items["properties"]["value"];
        let any_of = value_schema["anyOf"].as_array().unwrap();
        assert_eq!(any_of.len(), 2);
    }

    #[test]
    fn array_union_optional_null_field() {
        let example = json!([
            { "name": "Alice", "bio": null },
            { "name": "Bob", "bio": "developer" }
        ]);
        let schema = json_to_schema(&example);
        let items = &schema["items"];
        let required: Vec<&str> = items["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert!(required.contains(&"name"));
        // bio is null in first item → not required
        assert!(!required.contains(&"bio"));
    }

    #[test]
    fn array_union_deeply_nested() {
        // Two objects with "user" having different shapes → anyOf for the user property
        let example = json!([
            { "user": { "name": "Alice" } },
            { "user": { "name": "Bob", "role": "admin" } }
        ]);
        let schema = json_to_schema(&example);
        let user_schema = &schema["items"]["properties"]["user"];
        // Each "user" value is derived independently then deduplicated.
        // Different shapes → anyOf with two object schemas.
        let any_of = user_schema["anyOf"].as_array().unwrap();
        assert_eq!(any_of.len(), 2);
        // Both sub-schemas should be objects
        assert!(any_of.iter().all(|s| s["type"] == "object"));
    }

    // ═══════════════════════════════════════════════════════════════
    // STRICT MODE
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn strict_adds_additional_properties_false() {
        let example = json!({ "name": "Alice", "age": 30 });
        let schema = json_to_schema_strict(&example);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["type"], "object");
    }

    #[test]
    fn strict_recursive_on_nested_objects() {
        let example = json!({
            "user": { "name": "Alice" },
            "settings": { "theme": "dark" }
        });
        let schema = json_to_schema_strict(&example);
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(schema["properties"]["user"]["additionalProperties"], false);
        assert_eq!(
            schema["properties"]["settings"]["additionalProperties"],
            false
        );
    }

    #[test]
    fn strict_on_array_items() {
        let example = json!([{ "id": 1 }]);
        let schema = json_to_schema_strict(&example);
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["additionalProperties"], false);
    }

    #[test]
    fn non_strict_no_additional_properties() {
        let example = json!({ "name": "Alice" });
        let schema = json_to_schema(&example);
        assert!(schema.get("additionalProperties").is_none());
    }

    #[test]
    fn strict_on_merged_array_union() {
        let example = json!([
            { "id": 1, "name": "Alice" },
            { "id": 2, "email": "bob@x.com" }
        ]);
        let schema = json_to_schema_strict(&example);
        assert_eq!(schema["items"]["additionalProperties"], false);
    }
}
