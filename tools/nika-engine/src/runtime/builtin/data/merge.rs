// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Merge tools: json_merge, set_diff, zip

use super::super::BuiltinTool;
use crate::error::NikaError;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

// nika:json_merge
// ═══════════════════════════════════════════════════════════════════════════

pub struct JsonMergeTool;

#[derive(Debug, Deserialize)]
struct JsonMergeParams {
    /// Arrays to concatenate or objects to merge
    items: Vec<Value>,
    /// "concat" (arrays) or "deep_merge" (objects). Default: auto-detect.
    #[serde(default)]
    mode: Option<String>,
}

impl BuiltinTool for JsonMergeTool {
    fn name(&self) -> &'static str {
        "json_merge"
    }

    fn description(&self) -> &'static str {
        "Concatenate arrays or deep-merge objects. Auto-detects mode from input types."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["items"],
            "properties": {
                "items": {
                    "type": "array",
                    "description": "Arrays to concatenate or objects to deep-merge",
                    "minItems": 1
                },
                "mode": {
                    "type": "string",
                    "enum": ["concat", "deep_merge"],
                    "description": "Merge mode (auto-detected if omitted)"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: JsonMergeParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:json_merge".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            if params.items.is_empty() {
                return Ok("[]".to_string());
            }

            // Auto-detect mode: if first item is array → concat, object → deep_merge
            let mode = params.mode.as_deref().unwrap_or_else(|| {
                if params.items[0].is_array() {
                    "concat"
                } else {
                    "deep_merge"
                }
            });

            let result = match mode {
                "concat" => {
                    let mut merged = Vec::new();
                    for item in &params.items {
                        match item {
                            Value::Array(arr) => merged.extend(arr.iter().cloned()),
                            other => merged.push(other.clone()),
                        }
                    }
                    Value::Array(merged)
                }
                "deep_merge" => {
                    let mut base = serde_json::Map::new();
                    for item in &params.items {
                        if let Value::Object(obj) = item {
                            deep_merge_objects(&mut base, obj);
                        } else {
                            return Err(NikaError::BuiltinToolError {
                                tool: "nika:json_merge".into(),
                                reason: "deep_merge mode requires all items to be objects".into(),
                            });
                        }
                    }
                    Value::Object(base)
                }
                _ => {
                    return Err(NikaError::BuiltinToolError {
                        tool: "nika:json_merge".into(),
                        reason: format!("Unknown mode: {mode}. Use 'concat' or 'deep_merge'."),
                    });
                }
            };

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:json_merge".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

use nika_core::binding::deep_merge as deep_merge_objects_core;

fn deep_merge_objects(
    base: &mut serde_json::Map<String, Value>,
    overlay: &serde_json::Map<String, Value>,
) {
    deep_merge_objects_core(base, overlay);
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:set_diff
// ═══════════════════════════════════════════════════════════════════════════

pub struct SetDiffTool;

#[derive(Debug, Deserialize)]
struct SetDiffParams {
    /// First array (A)
    a: Vec<Value>,
    /// Second array (B) — items to remove from A
    b: Vec<Value>,
}

impl BuiltinTool for SetDiffTool {
    fn name(&self) -> &'static str {
        "set_diff"
    }

    fn description(&self) -> &'static str {
        "Compute array set difference: A - B (items in A not in B)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["a", "b"],
            "properties": {
                "a": {
                    "type": "array",
                    "description": "First array (source)"
                },
                "b": {
                    "type": "array",
                    "description": "Second array (items to remove from A)"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: SetDiffParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:set_diff".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // Use string representation for comparison (handles mixed types)
            let b_strings: std::collections::HashSet<String> =
                params.b.iter().map(|v| v.to_string()).collect();

            let diff: Vec<Value> = params
                .a
                .into_iter()
                .filter(|v| !b_strings.contains(&v.to_string()))
                .collect();

            serde_json::to_string(&diff).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:set_diff".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:zip
// ═══════════════════════════════════════════════════════════════════════════

pub struct ZipTool;

#[derive(Debug, Deserialize)]
struct ZipParams {
    /// Base array of objects
    base: Vec<Value>,
    /// Overlay array (merged element-wise into base)
    overlay: Vec<Value>,
}

impl BuiltinTool for ZipTool {
    fn name(&self) -> &'static str {
        "zip"
    }

    fn description(&self) -> &'static str {
        "Merge two arrays element-wise. Objects are shallow-merged, other types become [a, b] pairs."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["base", "overlay"],
            "properties": {
                "base": {
                    "type": "array",
                    "description": "Base array"
                },
                "overlay": {
                    "type": "array",
                    "description": "Overlay array (merged into base element-wise)"
                }
            },
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: ZipParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:zip".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let len = params.base.len().max(params.overlay.len());
            let mut result = Vec::with_capacity(len);

            for i in 0..len {
                let base_val = params.base.get(i).cloned().unwrap_or(Value::Null);
                let overlay_val = params.overlay.get(i).cloned().unwrap_or(Value::Null);

                match (base_val, overlay_val) {
                    (Value::Object(mut base_obj), Value::Object(overlay_obj)) => {
                        // Shallow merge: overlay fields overwrite base fields
                        for (k, v) in overlay_obj {
                            base_obj.insert(k, v);
                        }
                        result.push(Value::Object(base_obj));
                    }
                    (a, b) => {
                        result.push(serde_json::json!([a, b]));
                    }
                }
            }

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:zip".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    // ── json_merge ───────────────────────────────────────────────
    #[tokio::test]
    async fn json_merge_concat_arrays() {
        let tool = JsonMergeTool;
        let result = tool
            .call(r#"{"items": [["a", "b"], ["c", "d"]]}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["a", "b", "c", "d"]));
    }

    #[tokio::test]
    async fn json_merge_deep_merge_objects() {
        let tool = JsonMergeTool;
        let result = tool
            .call(
                r#"{"items": [{"a": 1, "nested": {"x": 1}}, {"b": 2, "nested": {"y": 2}}]}"#.into(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["a"], 1);
        assert_eq!(parsed["b"], 2);
        assert_eq!(parsed["nested"]["x"], 1);
        assert_eq!(parsed["nested"]["y"], 2);
    }

    #[tokio::test]
    async fn json_merge_empty() {
        let tool = JsonMergeTool;
        let result = tool.call(r#"{"items": []}"#.into()).await.unwrap();
        assert_eq!(result, "[]");
    }

    // ── set_diff ────────────────────────────────────────────────
    #[tokio::test]
    async fn set_diff_strings() {
        let tool = SetDiffTool;
        let result = tool
            .call(r#"{"a": ["x", "y", "z"], "b": ["y"]}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["x", "z"]));
    }

    #[tokio::test]
    async fn set_diff_empty_b() {
        let tool = SetDiffTool;
        let result = tool
            .call(r#"{"a": [1, 2, 3], "b": []}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!([1, 2, 3]));
    }

    #[tokio::test]
    async fn set_diff_all_removed() {
        let tool = SetDiffTool;
        let result = tool
            .call(r#"{"a": [1, 2], "b": [1, 2, 3]}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!([]));
    }

    // ── zip ─────────────────────────────────────────────────────
    #[tokio::test]
    async fn zip_objects() {
        let tool = ZipTool;
        let result = tool
            .call(r#"{"base": [{"url": "/a"}, {"url": "/b"}], "overlay": [{"title": "A"}, {"title": "B"}]}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed[0]["url"], "/a");
        assert_eq!(parsed[0]["title"], "A");
        assert_eq!(parsed[1]["url"], "/b");
        assert_eq!(parsed[1]["title"], "B");
    }

    #[tokio::test]
    async fn zip_mismatched_lengths() {
        let tool = ZipTool;
        let result = tool
            .call(r#"{"base": [{"a": 1}], "overlay": [{"b": 2}, {"c": 3}]}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
        assert_eq!(parsed[0]["a"], 1);
        assert_eq!(parsed[0]["b"], 2);
        // Second element: base is null, overlay is {c: 3}
        assert_eq!(parsed[1], json!([null, {"c": 3}]));
    }
}
