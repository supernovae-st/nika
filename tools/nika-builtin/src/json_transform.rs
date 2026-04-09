// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika:json_flatten` / `nika:json_unflatten` — Flatten/unflatten nested JSON.
//!
//! - `json_flatten`: `{"a": {"b": 1}}` → `{"a.b": 1}`
//! - `json_unflatten`: `{"a.b": 1}` → `{"a": {"b": 1}}`

use crate::{BuiltinTool, BuiltinError, __sealed};
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

// ═══════════════════════════════════════════════════════════════════════════
// nika:json_flatten
// ═══════════════════════════════════════════════════════════════════════════

pub struct JsonFlattenTool;

impl __sealed::Sealed for JsonFlattenTool {}

#[derive(Debug, Deserialize)]
struct FlattenParams {
    /// JSON object to flatten
    data: Value,
    /// Separator (default: ".")
    #[serde(default = "default_separator")]
    separator: String,
}

fn default_separator() -> String {
    ".".to_string()
}

impl BuiltinTool for JsonFlattenTool {
    fn name(&self) -> &'static str {
        "json_flatten"
    }

    fn description(&self) -> &'static str {
        "Flatten nested JSON to dot-notation keys: {a:{b:1}} → {\"a.b\":1}"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["data"],
            "properties": {
                "data": {
                    "description": "JSON object to flatten"
                },
                "separator": {
                    "type": "string",
                    "description": "Key separator (default: '.')",
                    "default": "."
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: FlattenParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
                    tool: "nika:json_flatten".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let result = flatten(&params.data, "", &params.separator);

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
                tool: "nika:json_flatten".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

fn flatten(value: &Value, prefix: &str, sep: &str) -> Value {
    let mut result = serde_json::Map::new();

    if let Value::Object(map) = value {
        for (k, v) in map {
            let key = if prefix.is_empty() {
                k.clone()
            } else {
                format!("{prefix}{sep}{k}")
            };

            if let Value::Object(_) = v {
                if let Value::Object(nested) = flatten(v, &key, sep) {
                    result.extend(nested);
                }
            } else {
                result.insert(key, v.clone());
            }
        }
    } else {
        // Non-object: return as-is
        return value.clone();
    }

    Value::Object(result)
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:json_unflatten
// ═══════════════════════════════════════════════════════════════════════════

pub struct JsonUnflattenTool;

impl __sealed::Sealed for JsonUnflattenTool {}

#[derive(Debug, Deserialize)]
struct UnflattenParams {
    /// Flattened JSON object to unflatten
    data: Value,
    /// Separator (default: ".")
    #[serde(default = "default_separator")]
    separator: String,
}

impl BuiltinTool for JsonUnflattenTool {
    fn name(&self) -> &'static str {
        "json_unflatten"
    }

    fn description(&self) -> &'static str {
        "Unflatten dot-notation keys to nested JSON: {\"a.b\":1} → {a:{b:1}}"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["data"],
            "properties": {
                "data": {
                    "description": "Flattened JSON object to unflatten"
                },
                "separator": {
                    "type": "string",
                    "description": "Key separator (default: '.')",
                    "default": "."
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: UnflattenParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
                    tool: "nika:json_unflatten".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let map = match &params.data {
                Value::Object(m) => m,
                Value::Null
                | Value::Bool(_)
                | Value::Number(_)
                | Value::String(_)
                | Value::Array(_) => {
                    return Err(BuiltinError::Other {
                        tool: "nika:json_unflatten".into(),
                        reason: "Expected a JSON object".into(),
                    });
                }
            };

            let mut result = serde_json::Map::new();

            for (key, value) in map {
                let parts: Vec<&str> = key.split(&params.separator).collect();
                set_nested(&mut result, &parts, value.clone());
            }

            serde_json::to_string(&Value::Object(result)).map_err(|e| BuiltinError::Other {
                tool: "nika:json_unflatten".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

fn set_nested(map: &mut serde_json::Map<String, Value>, parts: &[&str], value: Value) {
    if parts.len() == 1 {
        map.insert(parts[0].to_string(), value);
        return;
    }

    let key = parts[0].to_string();
    let entry = map
        .entry(key)
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    // If existing value is not an object (collision: "a": 1 then "a.b": 2),
    // replace the scalar with an object to allow nesting.
    if !entry.is_object() {
        *entry = Value::Object(serde_json::Map::new());
    }

    if let Value::Object(nested) = entry {
        set_nested(nested, &parts[1..], value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── json_flatten ────────────────────────────────────────────

    #[tokio::test]
    async fn flatten_nested_object() {
        let tool = JsonFlattenTool;
        let args = json!({
            "data": {"a": {"b": {"c": 1}}, "d": 2}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["a.b.c"], 1);
        assert_eq!(result["d"], 2);
    }

    #[tokio::test]
    async fn flatten_custom_separator() {
        let tool = JsonFlattenTool;
        let args = json!({
            "data": {"a": {"b": 1}},
            "separator": "/"
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["a/b"], 1);
    }

    #[tokio::test]
    async fn flatten_already_flat() {
        let tool = JsonFlattenTool;
        let args = json!({
            "data": {"x": 1, "y": 2}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result, json!({"x": 1, "y": 2}));
    }

    #[tokio::test]
    async fn flatten_empty_object() {
        let tool = JsonFlattenTool;
        let args = json!({"data": {}});
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result, json!({}));
    }

    // ── json_unflatten ──────────────────────────────────────────

    #[tokio::test]
    async fn unflatten_basic() {
        let tool = JsonUnflattenTool;
        let args = json!({
            "data": {"a.b.c": 1, "a.d": 2, "e": 3}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["a"]["b"]["c"], 1);
        assert_eq!(result["a"]["d"], 2);
        assert_eq!(result["e"], 3);
    }

    #[tokio::test]
    async fn unflatten_custom_separator() {
        let tool = JsonUnflattenTool;
        let args = json!({
            "data": {"a/b": 1},
            "separator": "/"
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["a"]["b"], 1);
    }

    // ── roundtrip ───────────────────────────────────────────────

    #[tokio::test]
    async fn flatten_unflatten_roundtrip() {
        let original =
            json!({"ui": {"button": {"save": "Save", "cancel": "Cancel"}}, "version": 1});

        let flatten_tool = JsonFlattenTool;
        let flat_result = flatten_tool
            .call(json!({"data": original}).to_string())
            .await
            .unwrap();

        let unflatten_tool = JsonUnflattenTool;
        let round_result = unflatten_tool
            .call(json!({"data": serde_json::from_str::<Value>(&flat_result).unwrap()}).to_string())
            .await
            .unwrap();

        let result: Value = serde_json::from_str(&round_result).unwrap();
        assert_eq!(result["ui"]["button"]["save"], "Save");
        assert_eq!(result["ui"]["button"]["cancel"], "Cancel");
        assert_eq!(result["version"], 1);
    }

    #[tokio::test]
    async fn unflatten_not_object_errors() {
        let tool = JsonUnflattenTool;
        let args = json!({"data": [1, 2, 3]});
        let result = tool.call(args.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn unflatten_conflicting_keys_deeper_wins() {
        // "a": 1 then "a.b": 2 — the deeper path replaces the scalar
        let tool = JsonUnflattenTool;
        let args = json!({
            "data": {"a": 1, "a.b": 2}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        // "a" was a scalar 1, but "a.b" forces it to become an object
        assert_eq!(result["a"]["b"], 2);
    }

    #[tokio::test]
    async fn flatten_preserves_arrays() {
        // Arrays inside objects are kept as leaf values, not recursed into
        let tool = JsonFlattenTool;
        let args = json!({
            "data": {"tags": ["rust", "nika"], "meta": {"version": 1}}
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(
            result["tags"],
            json!(["rust", "nika"]),
            "arrays are leaf values"
        );
        assert_eq!(result["meta.version"], 1);
    }
}
