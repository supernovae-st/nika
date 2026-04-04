//! Data processing builtin tools for mechanical operations without LLM.
//!
//! - `nika:json_merge` — Concatenate arrays or deep-merge objects
//! - `nika:set_diff` — Array set difference (A - B)
//! - `nika:zip` — Merge parallel arrays element-wise
//! - `nika:map` — Extract field from each array element
//! - `nika:filter` — Predicate-based array filtering
//! - `nika:group_by` — Group array by field
//! - `nika:chunk` — RAG text chunking
//! - `nika:token_count` — Token counting
//! - `nika:json_query` — JSONPath query on data

use super::BuiltinTool;
use crate::binding::TransformExpr;
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

            // BUG-037: Convert null (no matches) to empty array for tool output.
            // The shared jsonpath::query() returns null for no matches, which is
            // correct for the binding system ($task.result ?? "fallback"). But for
            // the tool, empty array is more useful and composable.
            let output = if results.is_null() {
                Value::Array(vec![])
            } else {
                results
            };

            serde_json::to_string(&output).map_err(|e| NikaError::BuiltinToolError {
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
    /// Optional transform chain to apply to each extracted value (e.g. "| url_path | split('/') | compact | length")
    #[serde(default)]
    transform: Option<String>,
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
                },
                "transform": {
                    "type": "string",
                    "description": "Optional transform chain to apply to each extracted value (e.g. '| url_path | split(\\'/\\') | compact | length')"
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

            let array = params
                .array
                .as_array()
                .ok_or_else(|| NikaError::BuiltinToolError {
                    tool: "nika:map".into(),
                    reason: "Expected array for 'array' parameter".into(),
                })?;

            // Pre-parse transform chain once (fail fast on invalid syntax)
            let transform = params
                .transform
                .as_deref()
                .map(|chain| {
                    TransformExpr::parse(chain).map_err(|e| NikaError::BuiltinToolError {
                        tool: "nika:map".into(),
                        reason: format!("Invalid transform: {e}"),
                    })
                })
                .transpose()?;

            let result: Vec<Value> = array
                .iter()
                .map(|v| {
                    let extracted = extract_field(v, &params.selector);
                    if let Some(ref expr) = transform {
                        expr.apply(&extracted).unwrap_or(Value::Null)
                    } else {
                        extracted
                    }
                })
                .collect();

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:map".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:enrich
// ═══════════════════════════════════════════════════════════════════════════

pub struct EnrichTool;

#[derive(Debug, Deserialize)]
struct EnrichParams {
    /// Array of objects to enrich
    array: Vec<Value>,
    /// Map of field_name → "selector | transform_chain" expressions
    fields: std::collections::BTreeMap<String, String>,
}

/// Parse a field expression like "extracted.title | default('') | lower" into
/// a (dot_path, Option<TransformExpr>) pair.
fn parse_field_expr(expr: &str) -> Result<(&str, Option<TransformExpr>), NikaError> {
    // Split at first ` | ` to separate dot-path from transform chain
    if let Some(pipe_pos) = expr.find(" | ") {
        let selector = expr[..pipe_pos].trim();
        let chain = &expr[pipe_pos..]; // includes the leading " | "
        let transform = TransformExpr::parse(chain).map_err(|e| NikaError::BuiltinToolError {
            tool: "nika:enrich".into(),
            reason: format!("Invalid transform in field expression: {e}"),
        })?;
        Ok((selector, Some(transform)))
    } else {
        Ok((expr.trim(), None))
    }
}

impl BuiltinTool for EnrichTool {
    fn name(&self) -> &'static str {
        "enrich"
    }

    fn description(&self) -> &'static str {
        "Add computed fields to each element in an array using selector + transform expressions"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["array", "fields"],
            "properties": {
                "array": {
                    "type": "array",
                    "description": "Array of objects to enrich"
                },
                "fields": {
                    "type": "object",
                    "description": "Map of field_name → 'selector | transform_chain' expressions",
                    "additionalProperties": { "type": "string" }
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
            let params: EnrichParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:enrich".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // Pre-parse all field expressions (fail fast on invalid syntax)
            let parsed_fields: Vec<(&str, &str, Option<TransformExpr>)> = params
                .fields
                .iter()
                .map(|(name, expr)| {
                    let (selector, transform) = parse_field_expr(expr)?;
                    Ok((name.as_str(), selector, transform))
                })
                .collect::<Result<Vec<_>, NikaError>>()?;

            let result: Vec<Value> = params
                .array
                .into_iter()
                .map(|elem| {
                    // Null elements pass through (PartialSuccess crawl results)
                    if elem.is_null() {
                        return Value::Null;
                    }
                    // Non-object elements pass through
                    let mut obj = match elem {
                        Value::Object(map) => map,
                        other => return other,
                    };

                    for &(name, selector, ref transform) in &parsed_fields {
                        let extracted = extract_field(&Value::Object(obj.clone()), selector);
                        let final_value = if let Some(ref expr) = transform {
                            expr.apply(&extracted).unwrap_or(Value::Null)
                        } else {
                            extracted
                        };
                        obj.insert(name.to_string(), final_value);
                    }

                    Value::Object(obj)
                })
                .collect();

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:enrich".into(),
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
        "gt" => val
            .as_f64()
            .zip(compare.as_f64())
            .is_some_and(|(a, b)| a > b),
        "lt" => val
            .as_f64()
            .zip(compare.as_f64())
            .is_some_and(|(a, b)| a < b),
        "gte" => val
            .as_f64()
            .zip(compare.as_f64())
            .is_some_and(|(a, b)| a >= b),
        "lte" => val
            .as_f64()
            .zip(compare.as_f64())
            .is_some_and(|(a, b)| a <= b),
        "contains" => val
            .as_str()
            .zip(compare.as_str())
            .is_some_and(|(a, b)| a.contains(b)),
        "starts_with" => val
            .as_str()
            .zip(compare.as_str())
            .is_some_and(|(a, b)| a.starts_with(b)),
        "ends_with" => val
            .as_str()
            .zip(compare.as_str())
            .is_some_and(|(a, b)| a.ends_with(b)),
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

            let array = params
                .array
                .as_array()
                .ok_or_else(|| NikaError::BuiltinToolError {
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
// nika:chunk — RAG text chunking
// ═══════════════════════════════════════════════════════════════════════════

pub struct ChunkTool;

#[derive(Debug, Deserialize)]
struct ChunkParams {
    text: String,
    #[serde(default = "default_chunk_size")]
    chunk_size: usize,
    #[serde(default)]
    overlap: usize,
    #[serde(default = "default_chunk_mode")]
    mode: String,
}

fn default_chunk_size() -> usize {
    1000
}
fn default_chunk_mode() -> String {
    "text".into()
}

impl BuiltinTool for ChunkTool {
    fn name(&self) -> &'static str {
        "chunk"
    }

    fn description(&self) -> &'static str {
        "Split text into chunks for RAG pipelines (supports text and markdown modes)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to split into chunks"
                },
                "chunk_size": {
                    "type": "integer",
                    "description": "Maximum characters per chunk (default: 1000)",
                    "default": 1000
                },
                "overlap": {
                    "type": "integer",
                    "description": "Character overlap between chunks (default: 0)",
                    "default": 0
                },
                "mode": {
                    "type": "string",
                    "enum": ["text", "markdown"],
                    "description": "Chunking mode: 'text' (character boundaries) or 'markdown' (respects headings)",
                    "default": "text"
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
            let params: ChunkParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:chunk".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            if params.text.is_empty() {
                return serde_json::to_string(&serde_json::json!({
                    "chunks": [],
                    "count": 0,
                    "mode": params.mode,
                    "chunk_size": params.chunk_size,
                    "overlap": params.overlap,
                }))
                .map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:chunk".into(),
                    reason: format!("Serialization failed: {e}"),
                });
            }

            use text_splitter::{ChunkConfig, MarkdownSplitter, TextSplitter};
            let config = ChunkConfig::new(params.chunk_size)
                .with_overlap(params.overlap)
                .map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:chunk".into(),
                    reason: format!("Invalid chunk config: {e}"),
                })?;

            let chunks: Vec<&str> = match params.mode.as_str() {
                "markdown" => MarkdownSplitter::new(config).chunks(&params.text).collect(),
                _ => TextSplitter::new(config).chunks(&params.text).collect(),
            };

            let count = chunks.len();
            let result = serde_json::json!({
                "chunks": chunks,
                "count": count,
                "mode": params.mode,
                "chunk_size": params.chunk_size,
                "overlap": params.overlap,
            });

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:chunk".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:token_count — heuristic token counting
// ═══════════════════════════════════════════════════════════════════════════

pub struct TokenCountTool;

#[derive(Debug, Deserialize)]
struct TokenCountParams {
    text: String,
    #[serde(default = "default_tokenizer")]
    model: String,
}

fn default_tokenizer() -> String {
    "heuristic".into()
}

impl BuiltinTool for TokenCountTool {
    fn name(&self) -> &'static str {
        "token_count"
    }

    fn description(&self) -> &'static str {
        "Estimate token count for text (heuristic: chars/4)"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["text"],
            "properties": {
                "text": {
                    "type": "string",
                    "description": "Text to count tokens for"
                },
                "model": {
                    "type": "string",
                    "description": "Tokenizer model: 'heuristic' (default, chars/4)",
                    "default": "heuristic"
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
            let params: TokenCountParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:token_count".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // Heuristic: ~4 characters per token (OpenAI's own approximation)
            // Use char count, not byte length, for correct non-ASCII estimation.
            let char_count = params.text.chars().count();
            let token_count = char_count / 4;

            let result = serde_json::json!({
                "tokens": token_count,
                "characters": char_count,
                "model": params.model,
            });

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:token_count".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:jq — Full jq expression evaluation on JSON data
// ═══════════════════════════════════════════════════════════════════════════

pub struct JqTool;

#[derive(Debug, Deserialize)]
struct JqParams {
    /// Input data (any JSON value)
    data: Value,
    /// jq expression to evaluate
    expr: String,
}

impl BuiltinTool for JqTool {
    fn name(&self) -> &'static str {
        "jq"
    }

    fn description(&self) -> &'static str {
        "Evaluate a jq expression on JSON data. Supports group_by, sort_by, map, select, keys, length, and 100+ stdlib functions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["data", "expr"],
            "properties": {
                "data": {
                    "description": "Input JSON data (any type)"
                },
                "expr": {
                    "type": "string",
                    "description": "jq expression to evaluate"
                }
            }
        })
    }

    fn call(
        &self,
        params_json: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + '_>> {
        Box::pin(async move {
            let params: JqParams =
                serde_json::from_str(&params_json).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:jq".into(),
                    reason: format!("Invalid params: {e}"),
                })?;

            let result = nika_core::binding::eval_jq(&params.expr, &params.data).map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "nika:jq".into(),
                    reason: e,
                }
            })?;

            serde_json::to_string(&result).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:jq".into(),
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
// nika:inject — Replace content between markers in a template file
// ═══════════════════════════════════════════════════════════════════════════

pub struct InjectTool;

#[derive(Debug, Deserialize)]
struct InjectParams {
    /// Path to the template file
    template: String,
    /// Path to write the output file
    output: String,
    /// Marker that starts the replaceable section (line containing this string)
    start_marker: String,
    /// Marker that ends the replaceable section (line containing this string)
    end_marker: String,
    /// Content to inject between the markers
    content: String,
}

impl BuiltinTool for InjectTool {
    fn name(&self) -> &'static str {
        "inject"
    }

    fn description(&self) -> &'static str {
        "Read a template file, replace content between start/end markers with injected content, write to output file."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "required": ["template", "output", "start_marker", "end_marker", "content"],
            "properties": {
                "template": {"type": "string", "description": "Path to template file"},
                "output": {"type": "string", "description": "Path to output file"},
                "start_marker": {"type": "string", "description": "Line containing this string starts the replaceable section"},
                "end_marker": {"type": "string", "description": "Line containing this string ends the replaceable section"},
                "content": {"type": "string", "description": "Content to inject between markers"}
            }
        })
    }

    fn call(
        &self,
        params_json: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + '_>> {
        Box::pin(async move {
            let params: InjectParams =
                serde_json::from_str(&params_json).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:inject".into(),
                    reason: format!("Invalid params: {e}"),
                })?;

            let template = std::fs::read_to_string(&params.template).map_err(|e| {
                NikaError::BuiltinToolError {
                    tool: "nika:inject".into(),
                    reason: format!("Cannot read template '{}': {e}", params.template),
                }
            })?;

            let mut output = String::new();
            let mut skipping = false;

            for line in template.lines() {
                if line.contains(&params.start_marker) {
                    output.push_str(line);
                    output.push('\n');
                    output.push_str(&params.content);
                    output.push('\n');
                    skipping = true;
                    continue;
                }
                if skipping && line.contains(&params.end_marker) {
                    skipping = false;
                }
                if !skipping {
                    output.push_str(line);
                    output.push('\n');
                }
            }

            if let Some(parent) = std::path::Path::new(&params.output).parent() {
                std::fs::create_dir_all(parent).ok();
            }
            std::fs::write(&params.output, &output).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:inject".into(),
                reason: format!("Cannot write '{}': {e}", params.output),
            })?;

            Ok(serde_json::json!({
                "path": params.output,
                "size": output.len(),
                "lines": output.lines().count()
            })
            .to_string())
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

    #[tokio::test]
    async fn json_query_no_match_returns_empty_array() {
        let tool = JsonQueryTool;
        let data = json!({"items": [{"name": "a"}]});
        let result = tool
            .call(serde_json::json!({"data": data, "query": "$..nonexistent"}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!([]));
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
        let result = tool
            .call(r#"{"array": [], "selector": "x"}"#.into())
            .await
            .unwrap();
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

    #[tokio::test]
    async fn filter_missing_field_excluded() {
        // Elements without the filter field should be excluded (null != 200)
        let tool = FilterTool;
        let result = tool
            .call(
                r#"{"array": [{"status": 200}, {"other": "x"}, {"status": 200}], "field": "status", "op": "eq", "value": 200}"#
                    .into(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn filter_starts_with_op() {
        let tool = FilterTool;
        let result = tool
            .call(
                r#"{"array": [{"path": "/api/v1"}, {"path": "/blog"}, {"path": "/api/v2"}], "field": "path", "op": "starts_with", "value": "/api"}"#
                    .into(),
            )
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.as_array().unwrap().len(), 2);
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
        let result = tool
            .call(r#"{"array": [], "key": "x"}"#.into())
            .await
            .unwrap();
        assert_eq!(result, "{}");
    }

    // ── chunk ───────────────────────────────────────────────────
    #[tokio::test]
    async fn chunk_splits_text() {
        let tool = ChunkTool;
        let long_text = "a".repeat(3000);
        let result = tool
            .call(serde_json::json!({"text": long_text, "chunk_size": 1000}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["count"].as_u64().unwrap() >= 3);
        assert_eq!(parsed["mode"], "text");
    }

    #[tokio::test]
    async fn chunk_small_text_single_chunk() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "Hello world", "chunk_size": 1000}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 1);
    }

    #[tokio::test]
    async fn chunk_empty_text() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "", "chunk_size": 1000}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["count"], 0);
        assert_eq!(parsed["chunks"], json!([]));
    }

    #[tokio::test]
    async fn chunk_markdown_mode() {
        let tool = ChunkTool;
        let md = "# Title\n\nParagraph one.\n\n# Another\n\nParagraph two.";
        let result = tool
            .call(serde_json::json!({"text": md, "mode": "markdown", "chunk_size": 30}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert!(parsed["count"].as_u64().unwrap() >= 2);
        assert_eq!(parsed["mode"], "markdown");
    }

    // ── token_count ─────────────────────────────────────────────
    #[tokio::test]
    async fn token_count_heuristic() {
        let tool = TokenCountTool;
        // 100 chars → ~25 tokens
        let text = "a".repeat(100);
        let result = tool
            .call(serde_json::json!({"text": text}).to_string())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["tokens"], 25);
        assert_eq!(parsed["characters"], 100);
        assert_eq!(parsed["model"], "heuristic");
    }

    #[tokio::test]
    async fn token_count_empty() {
        let tool = TokenCountTool;
        let result = tool.call(r#"{"text": ""}"#.into()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed["tokens"], 0);
        assert_eq!(parsed["characters"], 0);
    }

    #[tokio::test]
    async fn token_count_non_ascii() {
        let tool = TokenCountTool;
        // "héllo" = 5 chars, 6 bytes in UTF-8 — must use char count not byte count
        let result = tool
            .call(r#"{"text": "héllo café résumé"}"#.into())
            .await
            .unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        // 17 chars (h-é-l-l-o- -c-a-f-é- -r-é-s-u-m-é), not 20 bytes
        assert_eq!(parsed["characters"], 17);
        assert_eq!(parsed["tokens"], 4); // 17/4 = 4
    }

    #[tokio::test]
    async fn chunk_overlap_exceeds_size_errors() {
        let tool = ChunkTool;
        let result = tool
            .call(r#"{"text": "hello world", "chunk_size": 10, "overlap": 20}"#.into())
            .await;
        assert!(result.is_err());
    }

    // ── map + transform ─────────────────────────────────────────
    #[tokio::test]
    async fn map_with_transform_depth() {
        let tool = MapTool;
        let data = json!({
            "array": [
                {"url": "https://example.com"},
                {"url": "https://example.com/en/page"},
                {"url": "https://example.com/fr/docs/api/ref"}
            ],
            "selector": "url",
            "transform": "| url_path | split('/') | compact | length"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!([0, 2, 4]));
    }

    #[tokio::test]
    async fn map_with_transform_first_segment() {
        let tool = MapTool;
        let data = json!({
            "array": [
                {"url": "https://example.com/en/page"},
                {"url": "https://example.com/fr-ca/docs"},
                {"url": "https://example.com"}
            ],
            "selector": "url",
            "transform": "| url_path | split('/') | compact | first | default('default')"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["en", "fr-ca", "default"]));
    }

    #[tokio::test]
    async fn map_without_transform_unchanged() {
        // Existing behavior must remain intact when no transform is specified
        let tool = MapTool;
        let data = json!({
            "array": [{"url": "/a"}, {"url": "/b"}],
            "selector": "url"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(["/a", "/b"]));
    }

    // ── enrich ──────────────────────────────────────────────────
    #[tokio::test]
    async fn enrich_locale_and_depth() {
        let tool = EnrichTool;
        let data = json!({
            "array": [
                {"url": "https://example.com/en/about", "status": 200},
                {"url": "https://example.com/fr/docs/api", "status": 200}
            ],
            "fields": {
                "locale": "url | url_path | split('/') | compact | first | default('default')",
                "depth": "url | url_path | split('/') | compact | length"
            }
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr[0]["locale"], "en");
        assert_eq!(arr[0]["depth"], 2);
        assert_eq!(arr[0]["status"], 200); // original fields preserved
        assert_eq!(arr[1]["locale"], "fr");
        assert_eq!(arr[1]["depth"], 3);
    }

    #[tokio::test]
    async fn enrich_soft_404() {
        let tool = EnrichTool;
        let data = json!({
            "array": [
                {"url": "/a", "extracted": {"title": "Page Not Found - Site"}},
                {"url": "/b", "extracted": {"title": "About Us"}}
            ],
            "fields": {
                "is_soft_404": "extracted.title | default('') | lower | contains('not found')"
            }
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr[0]["is_soft_404"], true);
        assert_eq!(arr[1]["is_soft_404"], false);
    }

    #[tokio::test]
    async fn enrich_null_elements() {
        let tool = EnrichTool;
        let data = json!({
            "array": [null, {"url": "https://example.com/en"}],
            "fields": {
                "locale": "url | url_path | split('/') | compact | first | default('default')"
            }
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert!(arr[0].is_null()); // null passed through
        assert_eq!(arr[1]["locale"], "en");
    }

    #[tokio::test]
    async fn enrich_missing_fields() {
        let tool = EnrichTool;
        let data = json!({
            "array": [
                {"url": "/a"},
                {"url": "/b", "extracted": {"title": "Hello"}}
            ],
            "fields": {
                "title": "extracted.title | default('')"
            }
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr[0]["title"], ""); // missing → default('')
        assert_eq!(arr[1]["title"], "Hello");
    }

    // ═══════════════════════════════════════════════════════════════════════
    // nika:jq tests
    // ═══════════════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn jq_basic_group_by() {
        let tool = JqTool;
        let data = json!({
            "data": [
                {"locale": "en", "section": "blog"},
                {"locale": "en", "section": "docs"},
                {"locale": "en", "section": "blog"},
                {"locale": "fr", "section": "docs"}
            ],
            "expr": "[group_by(.locale)[] | {locale: .[0].locale, count: length}]"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // group_by sorts by key: en first, fr second
        assert_eq!(arr[0]["locale"], "en");
        assert_eq!(arr[0]["count"], 3);
        assert_eq!(arr[1]["locale"], "fr");
        assert_eq!(arr[1]["count"], 1);
    }

    #[tokio::test]
    async fn jq_nested_tree_for_treemap() {
        let tool = JqTool;
        let data = json!({
            "data": [
                {"locale": "en", "section": "blog"},
                {"locale": "en", "section": "blog"},
                {"locale": "en", "section": "docs"},
                {"locale": "fr", "section": "docs"},
                {"locale": "fr", "section": "blog"}
            ],
            "expr": "[group_by(.locale)[] | {name: .[0].locale, children: [group_by(.section)[] | {name: .[0].section, value: length}] | sort_by(-.value)}] | sort_by(-(.children | map(.value) | add))"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // en has 3 total, fr has 2 — en first (desc sort)
        assert_eq!(arr[0]["name"], "en");
        assert_eq!(arr[0]["children"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn jq_sort_by_and_select() {
        let tool = JqTool;
        let data = json!({
            "data": [
                {"name": "a", "score": 3},
                {"name": "b", "score": 1},
                {"name": "c", "score": 5}
            ],
            "expr": "[.[] | select(.score > 2)] | sort_by(.score) | reverse"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["name"], "c"); // score 5 first
        assert_eq!(arr[1]["name"], "a"); // score 3 second
    }

    #[tokio::test]
    async fn jq_error_on_bad_expression() {
        let tool = JqTool;
        let data = json!({
            "data": [1, 2, 3],
            "expr": "invalid[[[syntax"
        });
        let result = tool.call(data.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn jq_keys_and_length() {
        let tool = JqTool;
        let data = json!({
            "data": {"a": 1, "b": 2, "c": 3},
            "expr": "keys | length"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed, json!(3));
    }

    #[tokio::test]
    async fn jq_to_entries() {
        let tool = JqTool;
        let data = json!({
            "data": {"x": 10, "y": 20},
            "expr": "[to_entries[] | {k: .key, v: .value}]"
        });
        let result = tool.call(data.to_string()).await.unwrap();
        let parsed: Value = serde_json::from_str(&result).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
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
