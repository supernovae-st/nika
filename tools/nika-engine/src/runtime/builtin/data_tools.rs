//! Data processing builtin tools for mechanical operations without LLM.
//!
//! - `nika:json_merge` — Concatenate arrays or deep-merge objects
//! - `nika:set_diff` — Array set difference (A - B)
//! - `nika:zip` — Merge parallel arrays element-wise
//! - `nika:json_query` — JSONPath query on data

use super::BuiltinTool;
use crate::error::NikaError;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

// ═══════════════════════════════════════════════════════════════════════════
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

fn deep_merge_objects(
    base: &mut serde_json::Map<String, Value>,
    overlay: &serde_json::Map<String, Value>,
) {
    for (key, value) in overlay {
        match (base.get_mut(key), value) {
            (Some(Value::Object(base_obj)), Value::Object(overlay_obj)) => {
                deep_merge_objects(base_obj, overlay_obj);
            }
            _ => {
                base.insert(key.clone(), value.clone());
            }
        }
    }
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
// nika:json_query
// ═══════════════════════════════════════════════════════════════════════════

pub struct JsonQueryTool;

#[derive(Debug, Deserialize)]
struct JsonQueryParams {
    /// Data to query
    data: Value,
    /// JSONPath expression (e.g., "$..url", "$.items[0].name")
    query: String,
}

impl BuiltinTool for JsonQueryTool {
    fn name(&self) -> &'static str {
        "json_query"
    }

    fn description(&self) -> &'static str {
        "Query JSON data using JSONPath expressions (e.g., '$..url', '$.items[0].name')"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["data", "query"],
            "properties": {
                "data": {
                    "description": "JSON data to query"
                },
                "query": {
                    "type": "string",
                    "description": "JSONPath expression (e.g., '$..url', '$.items[*].name')"
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
            let params: JsonQueryParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:json_query".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // Use nika-engine's JSONPath implementation
            let results =
                crate::binding::jsonpath::query(&params.data, &params.query).map_err(|e| {
                    NikaError::BuiltinToolError {
                        tool: "nika:json_query".into(),
                        reason: format!("JSONPath query failed: {e}"),
                    }
                })?;

            serde_json::to_string(&results).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:json_query".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:map
// ═══════════════════════════════════════════════════════════════════════════

pub struct MapTool;

#[derive(Debug, Deserialize)]
struct MapParams {
    /// Array of objects to extract from
    array: Value,
    /// Dot-path field name to extract (e.g. "loc", "address.city")
    selector: String,
}

/// Extract a field from a value by navigating a dot-separated path.
fn extract_field(value: &Value, path: &str) -> Value {
    let mut current = value;
    for segment in path.split('.') {
        match current.get(segment) {
            Some(v) => current = v,
            None => return Value::Null,
        }
    }
    current.clone()
}

impl BuiltinTool for MapTool {
    fn name(&self) -> &'static str {
        "map"
    }

    fn description(&self) -> &'static str {
        "Extract a field from each element in an array (dot-path selector)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["array", "selector"],
            "properties": {
                "array": {
                    "description": "Array of objects to extract from"
                },
                "selector": {
                    "type": "string",
                    "description": "Dot-path field to extract (e.g. 'loc', 'address.city')"
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
            let params: MapParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:map".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let array = params.array.as_array().ok_or_else(|| NikaError::BuiltinToolError {
                tool: "nika:map".into(),
                reason: "Expected array for 'array' parameter".into(),
            })?;

            let result: Vec<Value> = array.iter().map(|v| extract_field(v, &params.selector)).collect();

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:map".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:filter
// ═══════════════════════════════════════════════════════════════════════════

pub struct FilterTool;

#[derive(Debug, Deserialize)]
struct FilterParams {
    /// Array to filter
    array: Value,
    /// Field to compare (dot-path)
    field: String,
    /// Comparison operator: eq, ne, gt, lt, gte, lte, contains, starts_with, ends_with
    op: String,
    /// Value to compare against
    value: Value,
}

fn matches_predicate(element: &Value, field: &str, op: &str, compare: &Value) -> bool {
    let val = extract_field(element, field);
    match op {
        "eq" => &val == compare,
        "ne" => &val != compare,
        "gt" => val.as_f64().zip(compare.as_f64()).is_some_and(|(a, b)| a > b),
        "lt" => val.as_f64().zip(compare.as_f64()).is_some_and(|(a, b)| a < b),
        "gte" => val.as_f64().zip(compare.as_f64()).is_some_and(|(a, b)| a >= b),
        "lte" => val.as_f64().zip(compare.as_f64()).is_some_and(|(a, b)| a <= b),
        "contains" => val.as_str().zip(compare.as_str()).is_some_and(|(a, b)| a.contains(b)),
        "starts_with" => val.as_str().zip(compare.as_str()).is_some_and(|(a, b)| a.starts_with(b)),
        "ends_with" => val.as_str().zip(compare.as_str()).is_some_and(|(a, b)| a.ends_with(b)),
        _ => false,
    }
}

impl BuiltinTool for FilterTool {
    fn name(&self) -> &'static str {
        "filter"
    }

    fn description(&self) -> &'static str {
        "Filter an array by field predicate (eq, ne, gt, lt, contains, starts_with, ends_with)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["array", "field", "op", "value"],
            "properties": {
                "array": {
                    "description": "Array to filter"
                },
                "field": {
                    "type": "string",
                    "description": "Dot-path field to compare"
                },
                "op": {
                    "type": "string",
                    "enum": ["eq", "ne", "gt", "lt", "gte", "lte", "contains", "starts_with", "ends_with"],
                    "description": "Comparison operator"
                },
                "value": {
                    "description": "Value to compare against"
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
            let params: FilterParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:filter".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let array = params.array.as_array().ok_or_else(|| NikaError::BuiltinToolError {
                tool: "nika:filter".into(),
                reason: "Expected array for 'array' parameter".into(),
            })?;

            let result: Vec<Value> = array
                .iter()
                .filter(|v| matches_predicate(v, &params.field, &params.op, &params.value))
                .cloned()
                .collect();

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:filter".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:group_by
// ═══════════════════════════════════════════════════════════════════════════

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

            let array = params.array.as_array().ok_or_else(|| NikaError::BuiltinToolError {
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
// Tests
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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

    // ── json_query ──────────────────────────────────────────────
    #[tokio::test]
    async fn json_query_recursive_descent() {
        let tool = JsonQueryTool;
        let data = json!({
            "pages": [
                {"links": [{"url": "/a"}, {"url": "/b"}]},
                {"links": [{"url": "/c"}]}
            ]
        });
        let result = tool
            .call(serde_json::json!({"data": data, "query": "$..url"}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["/a", "/b", "/c"]));
    }

    #[tokio::test]
    async fn json_query_array_index() {
        let tool = JsonQueryTool;
        let data = json!({"items": ["a", "b", "c"]});
        let result = tool
            .call(serde_json::json!({"data": data, "query": "$.items[1]"}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        // Single match returns the value directly (not wrapped in array)
        assert_eq!(parsed, json!("b"));
    }

    // ── map ─────────────────────────────────────────────────────
    #[tokio::test]
    async fn map_extract_field() {
        let tool = MapTool;
        let result = tool
            .call(r#"{"array": [{"url": "/a", "title": "A"}, {"url": "/b", "title": "B"}], "selector": "url"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["/a", "/b"]));
    }

    #[tokio::test]
    async fn map_nested_field() {
        let tool = MapTool;
        let result = tool
            .call(r#"{"array": [{"addr": {"city": "Paris"}}, {"addr": {"city": "Lyon"}}], "selector": "addr.city"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["Paris", "Lyon"]));
    }

    #[tokio::test]
    async fn map_missing_field_returns_null() {
        let tool = MapTool;
        let result = tool
            .call(r#"{"array": [{"a": 1}, {"b": 2}], "selector": "a"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!([1, null]));
    }

    #[tokio::test]
    async fn map_empty_array() {
        let tool = MapTool;
        let result = tool.call(r#"{"array": [], "selector": "x"}"#.into()).await.unwrap();
        assert_eq!(result, "[]");
    }

    // ── filter ──────────────────────────────────────────────────
    #[tokio::test]
    async fn filter_eq_numeric() {
        let tool = FilterTool;
        let result = tool
            .call(r#"{"array": [{"status": 200}, {"status": 404}, {"status": 200}], "field": "status", "op": "eq", "value": 200}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn filter_contains_string() {
        let tool = FilterTool;
        let result = tool
            .call(r#"{"array": [{"url": "/api/users"}, {"url": "/blog/post"}, {"url": "/api/items"}], "field": "url", "op": "contains", "value": "/api/"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn filter_gt() {
        let tool = FilterTool;
        let result = tool
            .call(r#"{"array": [{"score": 10}, {"score": 50}, {"score": 90}], "field": "score", "op": "gt", "value": 40}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn filter_empty_result() {
        let tool = FilterTool;
        let result = tool
            .call(r#"{"array": [{"x": 1}], "field": "x", "op": "eq", "value": 999}"#.into())
            .await
            .unwrap();
        assert_eq!(result, "[]");
    }

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
        let result = tool.call(r#"{"array": [], "key": "x"}"#.into()).await.unwrap();
        assert_eq!(result, "{}");
    }
}
