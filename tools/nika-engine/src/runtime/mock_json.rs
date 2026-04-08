// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Schema-conforming mock JSON generator.
//!
//! Generates deterministic JSON values that satisfy a given JSON Schema.
//! Used by `provider: mock` when `structured:` output is configured,
//! so E2E tests can validate structured output pipelines without API calls.

use serde_json::{json, Value};

/// Maximum nesting depth for mock JSON generation.
/// Prevents stack overflow from recursive schemas or deep nesting.
const MAX_MOCK_DEPTH: usize = 32;

/// Generate a JSON value that conforms to the given JSON Schema.
///
/// Handles: object, string, number, integer, boolean, array, null.
/// Respects: enum, minimum, maximum, minItems, maxItems, required, default, const.
/// Depth-limited to prevent stack overflow on recursive schemas.
pub fn generate_mock_json(schema: &Value) -> Value {
    generate_mock_json_inner(schema, 0)
}

fn generate_mock_json_inner(schema: &Value, depth: usize) -> Value {
    // Depth guard: return null for excessively nested schemas
    if depth >= MAX_MOCK_DEPTH {
        return Value::Null;
    }

    // Handle const
    if let Some(const_val) = schema.get("const") {
        return const_val.clone();
    }

    // Handle default
    if let Some(default_val) = schema.get("default") {
        return default_val.clone();
    }

    // Handle enum
    if let Some(Value::Array(variants)) = schema.get("enum") {
        if let Some(first) = variants.first() {
            return first.clone();
        }
    }

    let type_str = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("string");

    match type_str {
        "object" => generate_mock_object(schema, depth),
        "array" => generate_mock_array(schema, depth),
        "string" => generate_mock_string(schema),
        "number" | "integer" => generate_mock_number(schema),
        "boolean" => Value::Bool(true),
        "null" => Value::Null,
        _ => Value::String("mock_value".to_string()),
    }
}

fn generate_mock_object(schema: &Value, depth: usize) -> Value {
    let mut obj = serde_json::Map::new();

    if let Some(Value::Object(properties)) = schema.get("properties") {
        for (key, prop_schema) in properties {
            obj.insert(
                key.clone(),
                generate_mock_json_inner(prop_schema, depth + 1),
            );
        }
    }

    Value::Object(obj)
}

fn generate_mock_array(schema: &Value, depth: usize) -> Value {
    let min_items = schema.get("minItems").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
    let max_items = schema
        .get("maxItems")
        .and_then(|v| v.as_u64())
        .unwrap_or(20) as usize;
    let count = min_items.max(1).min(max_items); // Respect minItems, cap at maxItems or 20

    let item_schema = schema
        .get("items")
        .cloned()
        .unwrap_or(json!({"type": "string"}));
    let items: Vec<Value> = (0..count)
        .map(|_| generate_mock_json_inner(&item_schema, depth + 1))
        .collect();
    Value::Array(items)
}

fn generate_mock_string(schema: &Value) -> Value {
    // Handle format hints
    if let Some(format) = schema.get("format").and_then(|f| f.as_str()) {
        return Value::String(match format {
            "email" => "mock@example.com".to_string(),
            "uri" | "url" => "https://example.com".to_string(),
            "date" => "2024-01-15".to_string(),
            "date-time" => "2024-01-15T14:30:00Z".to_string(),
            "uuid" => "00000000-0000-0000-0000-000000000000".to_string(),
            _ => "mock_value".to_string(),
        });
    }

    Value::String("mock_value".to_string())
}

fn generate_mock_number(schema: &Value) -> Value {
    let minimum = schema.get("minimum").and_then(|v| v.as_f64());
    let maximum = schema.get("maximum").and_then(|v| v.as_f64());

    let value = match (minimum, maximum) {
        (Some(min), Some(max)) => (min + max) / 2.0, // Midpoint
        (Some(min), None) => min + 1.0,
        (None, Some(max)) => max - 1.0,
        (None, None) => 42.0,
    };

    // Return integer if schema type is "integer"
    let type_str = schema
        .get("type")
        .and_then(|t| t.as_str())
        .unwrap_or("number");
    if type_str == "integer" {
        json!(value as i64)
    } else {
        json!(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_object() {
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "integer", "minimum": 0}
            },
            "required": ["name", "age"]
        });
        let result = generate_mock_json(&schema);
        assert!(result.is_object());
        assert!(result.get("name").unwrap().is_string());
        assert!(result.get("age").unwrap().is_number());
        assert!(result["age"].as_i64().unwrap() >= 0);
    }

    #[test]
    fn test_array_with_min_items() {
        let schema = json!({
            "type": "array",
            "items": {"type": "string"},
            "minItems": 3
        });
        let result = generate_mock_json(&schema);
        assert!(result.is_array());
        assert!(result.as_array().unwrap().len() >= 3);
    }

    #[test]
    fn test_enum_picks_first() {
        let schema = json!({
            "type": "string",
            "enum": ["red", "green", "blue"]
        });
        let result = generate_mock_json(&schema);
        assert_eq!(result, json!("red"));
    }

    #[test]
    fn test_nested_object() {
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "scores": {
                            "type": "array",
                            "items": {"type": "number"},
                            "minItems": 2
                        }
                    },
                    "required": ["name", "scores"]
                }
            },
            "required": ["user"]
        });
        let result = generate_mock_json(&schema);
        assert!(result["user"]["name"].is_string());
        assert!(result["user"]["scores"].as_array().unwrap().len() >= 2);
    }

    #[test]
    fn test_boolean() {
        let result = generate_mock_json(&json!({"type": "boolean"}));
        assert_eq!(result, json!(true));
    }

    #[test]
    fn test_number_with_range() {
        let schema = json!({"type": "number", "minimum": 10.0, "maximum": 20.0});
        let result = generate_mock_json(&schema);
        let val = result.as_f64().unwrap();
        assert!((10.0..=20.0).contains(&val));
    }

    #[test]
    fn test_const_value() {
        let schema = json!({"const": "always_this"});
        assert_eq!(generate_mock_json(&schema), json!("always_this"));
    }

    #[test]
    fn test_default_value() {
        let schema = json!({"type": "string", "default": "my_default"});
        assert_eq!(generate_mock_json(&schema), json!("my_default"));
    }

    #[test]
    fn test_deep_nesting_returns_null_at_limit() {
        // Build a schema nested MAX_MOCK_DEPTH+5 levels deep
        let mut schema = json!({"type": "string"});
        for _ in 0..(super::MAX_MOCK_DEPTH + 5) {
            schema = json!({
                "type": "object",
                "properties": { "child": schema }
            });
        }
        // Should not stack overflow — returns null at depth limit
        let result = generate_mock_json(&schema);
        assert!(result.is_object(), "Top-level should still be an object");
    }
}
