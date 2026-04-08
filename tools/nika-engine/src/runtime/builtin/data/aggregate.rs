// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Aggregate tools: group_by, tree_data

use super::super::BuiltinTool;
use super::transform::extract_field;
use crate::error::NikaError;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub struct GroupByTool;

#[derive(Debug, Deserialize)]
struct GroupByParams {
    /// Array of objects to group
    array: Value,
    /// Field to group by (dot-path)
    key: String,
}

impl BuiltinTool for GroupByTool {
    fn name(&self) -> &'static str {
        "group_by"
    }

    fn description(&self) -> &'static str {
        "Group array elements by a field value into {key: [items]} object"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["array", "key"],
            "properties": {
                "array": {
                    "description": "Array of objects to group"
                },
                "key": {
                    "type": "string",
                    "description": "Dot-path field to group by"
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
            let params: GroupByParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:group_by".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let array = params
                .array
                .as_array()
                .ok_or_else(|| NikaError::BuiltinToolError {
                    tool: "nika:group_by".into(),
                    reason: "Expected array for 'array' parameter".into(),
                })?;

            let mut groups: indexmap::IndexMap<String, Vec<Value>> = indexmap::IndexMap::new();
            for item in array {
                let key_val = extract_field(item, &params.key);
                let key_str = match &key_val {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => b.to_string(),
                    Value::Null => "null".to_string(),
                    _ => key_val.to_string(),
                };
                groups.entry(key_str).or_default().push(item.clone());
            }

            let result: serde_json::Map<String, Value> = groups
                .into_iter()
                .map(|(k, v)| (k, Value::Array(v)))
                .collect();

            serde_json::to_string(&Value::Object(result)).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:group_by".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:tree_data — Nested group_by for treemap/dashboard hierarchy
// ═══════════════════════════════════════════════════════════════════════════

pub struct TreeDataTool;

#[derive(Debug, Deserialize)]
struct TreeDataParams {
    /// Input array of objects
    array: Vec<Value>,
    /// Primary grouping field
    group_by: String,
    /// Secondary grouping field (optional — produces 2-level tree)
    #[serde(default)]
    sub_group_by: Option<String>,
    /// Prefix for sub-group names (e.g. "/" for URL sections)
    #[serde(default)]
    name_prefix: Option<String>,
    /// Sort order for children: "asc" or "desc" (by value). Default: desc.
    #[serde(default)]
    sort: Option<String>,
}

impl BuiltinTool for TreeDataTool {
    fn name(&self) -> &'static str {
        "tree_data"
    }

    fn description(&self) -> &'static str {
        "Group array into hierarchical tree data (for treemap/dashboard). Groups by primary field, optionally sub-groups by secondary field, counts items per group."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["array", "group_by"],
            "properties": {
                "array": {
                    "type": "array",
                    "description": "Input array of objects to group"
                },
                "group_by": {
                    "type": "string",
                    "description": "Primary grouping field name"
                },
                "sub_group_by": {
                    "type": "string",
                    "description": "Secondary grouping field (produces 2-level tree)"
                },
                "name_prefix": {
                    "type": "string",
                    "description": "Prefix for sub-group names (e.g. '/')"
                },
                "sort": {
                    "type": "string",
                    "enum": ["asc", "desc"],
                    "description": "Sort children by value (default: desc)"
                }
            }
        })
    }

    fn call(
        &self,
        params_json: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + '_>> {
        Box::pin(async move {
            let params: TreeDataParams =
                serde_json::from_str(&params_json).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:tree_data".into(),
                    reason: format!("Invalid params: {e}"),
                })?;

            let prefix = params.name_prefix.unwrap_or_default();
            let sort_desc = params.sort.as_deref() != Some("asc");

            // Primary grouping
            let mut groups: indexmap::IndexMap<String, Vec<&Value>> = indexmap::IndexMap::new();
            for item in &params.array {
                let key = item
                    .get(&params.group_by)
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                groups.entry(key).or_default().push(item);
            }

            let mut result: Vec<Value> = Vec::new();

            for (group_name, items) in &groups {
                if let Some(ref sub_field) = params.sub_group_by {
                    // 2-level: sub-group within each primary group
                    let mut sub_groups: indexmap::IndexMap<String, usize> =
                        indexmap::IndexMap::new();
                    for item in items {
                        let sub_key = item
                            .get(sub_field)
                            .and_then(|v| v.as_str())
                            .unwrap_or("other")
                            .to_string();
                        *sub_groups.entry(sub_key).or_insert(0) += 1;
                    }

                    let mut children: Vec<Value> = sub_groups
                        .into_iter()
                        .map(|(name, count)| {
                            serde_json::json!({
                                "name": format!("{prefix}{name}"),
                                "value": count
                            })
                        })
                        .collect();

                    if sort_desc {
                        children.sort_by(|a, b| {
                            b["value"]
                                .as_u64()
                                .unwrap_or(0)
                                .cmp(&a["value"].as_u64().unwrap_or(0))
                        });
                    } else {
                        children.sort_by(|a, b| {
                            a["value"]
                                .as_u64()
                                .unwrap_or(0)
                                .cmp(&b["value"].as_u64().unwrap_or(0))
                        });
                    }

                    result.push(serde_json::json!({
                        "name": group_name,
                        "children": children
                    }));
                } else {
                    // 1-level: just count per group
                    result.push(serde_json::json!({
                        "name": format!("{prefix}{group_name}"),
                        "value": items.len()
                    }));
                }
            }

            // Sort groups by total value
            let total_of = |v: &Value| -> u64 {
                v["children"]
                    .as_array()
                    .map(|arr| arr.iter().map(|c| c["value"].as_u64().unwrap_or(0)).sum())
                    .unwrap_or_else(|| v["value"].as_u64().unwrap_or(0))
            };
            if sort_desc {
                result.sort_by_key(|v| std::cmp::Reverse(total_of(v)));
            } else {
                result.sort_by_key(|v| total_of(v));
            }

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:tree_data".into(),
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

    // ── group_by ────────────────────────────────────────────────
    #[tokio::test]
    async fn group_by_string_field() {
        let tool = GroupByTool;
        let result = tool
            .call(r#"{"array": [{"type": "a", "v": 1}, {"type": "b", "v": 2}, {"type": "a", "v": 3}], "key": "type"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["a"].as_array().unwrap().len(), 2);
        assert_eq!(parsed["b"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn group_by_missing_key_goes_to_null() {
        let tool = GroupByTool;
        let result = tool
            .call(r#"{"array": [{"type": "a"}, {"other": "b"}], "key": "type"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["a"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["null"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn group_by_empty_array() {
        let tool = GroupByTool;
        let result = tool
            .call(r#"{"array": [], "key": "x"}"#.into())
            .await
            .unwrap();
        assert_eq!(result, "{}");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // nika:tree_data tests
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn tree_data_basic_grouping() {
        let tool = TreeDataTool;
        let data = json!({
            "array": [
                {"locale": "en", "section": "blog"},
                {"locale": "en", "section": "docs"},
                {"locale": "en", "section": "blog"},
                {"locale": "fr", "section": "docs"}
            ],
            "group_by": "locale",
            "sub_group_by": "section",
            "name_prefix": "/",
            "sort": "desc"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // en has 3 items, fr has 1 — en first (desc)
        assert_eq!(arr[0]["name"], "en");
        let children = arr[0]["children"].as_array().unwrap();
        assert_eq!(children.len(), 2);
        // blog=2 > docs=1 (desc sort)
        assert_eq!(children[0]["name"], "/blog");
        assert_eq!(children[0]["value"], 2);
        assert_eq!(children[1]["name"], "/docs");
        assert_eq!(children[1]["value"], 1);
    }

    #[tokio::test]
    async fn tree_data_single_level() {
        let tool = TreeDataTool;
        let data = json!({
            "array": [
                {"type": "page"},
                {"type": "page"},
                {"type": "asset"},
                {"type": "page"}
            ],
            "group_by": "type"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // page=3 > asset=1 (desc by default)
        assert_eq!(arr[0]["name"], "page");
        assert_eq!(arr[0]["value"], 3);
        assert_eq!(arr[1]["name"], "asset");
        assert_eq!(arr[1]["value"], 1);
    }

    #[tokio::test]
    async fn tree_data_sort_asc() {
        let tool = TreeDataTool;
        let data = json!({
            "array": [
                {"g": "a"}, {"g": "a"}, {"g": "a"},
                {"g": "b"}
            ],
            "group_by": "g",
            "sort": "asc"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        // b=1 first (asc)
        assert_eq!(arr[0]["name"], "b");
        assert_eq!(arr[0]["value"], 1);
    }

    #[tokio::test]
    async fn tree_data_empty_array() {
        let tool = TreeDataTool;
        let data = json!({
            "array": [],
            "group_by": "locale"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!([]));
    }
}
