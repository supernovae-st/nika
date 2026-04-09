// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Transform tools: map, filter, enrich

use crate::{BuiltinTool, BuiltinError, __sealed};
use nika_core::binding::transform::TransformExpr;
use nika_core::binding::transform::navigate_dot_path;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub struct MapTool;

impl __sealed::Sealed for MapTool {}

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
pub(super) fn extract_field(value: &Value, path: &str) -> Value {
    navigate_dot_path(value, path)
        .cloned()
        .unwrap_or(Value::Null)
}

/// Extract a field from a Map without wrapping in Value::Object first.
/// Avoids cloning the entire map for a simple field lookup.
fn extract_field_from_map(obj: &serde_json::Map<String, Value>, path: &str) -> Value {
    let mut segments = path.split('.');
    let first = match segments.next() {
        Some(s) => s,
        None => return Value::Null,
    };
    let mut current = match obj.get(first) {
        Some(v) => v,
        None => return Value::Null,
    };
    for segment in segments {
        current = match current.get(segment) {
            Some(v) => v,
            None => return Value::Null,
        };
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
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: MapParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
                    tool: "nika:map".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let array = params
                .array
                .as_array()
                .ok_or_else(|| BuiltinError::Other {
                    tool: "nika:map".into(),
                    reason: "Expected array for 'array' parameter".into(),
                })?;

            // Pre-parse transform chain once (fail fast on invalid syntax)
            let transform = params
                .transform
                .as_deref()
                .map(|chain| {
                    TransformExpr::parse(chain).map_err(|e| BuiltinError::Other {
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

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
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

impl __sealed::Sealed for EnrichTool {}

#[derive(Debug, Deserialize)]
struct EnrichParams {
    /// Array of objects to enrich
    array: Vec<Value>,
    /// Map of field_name → "selector | transform_chain" expressions
    fields: std::collections::BTreeMap<String, String>,
}

/// Parse a field expression like "extracted.title | default('') | lower" into
/// a (dot_path, Option<TransformExpr>) pair.
fn parse_field_expr(expr: &str) -> Result<(&str, Option<TransformExpr>), BuiltinError> {
    // Split at first ` | ` to separate dot-path from transform chain
    if let Some(pipe_pos) = expr.find(" | ") {
        let selector = expr[..pipe_pos].trim();
        let chain = &expr[pipe_pos..]; // includes the leading " | "
        let transform = TransformExpr::parse(chain).map_err(|e| BuiltinError::Other {
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
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: EnrichParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
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
                .collect::<Result<Vec<_>, BuiltinError>>()?;

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
                        other @ (Value::Null
                        | Value::Bool(_)
                        | Value::Number(_)
                        | Value::String(_)
                        | Value::Array(_)) => return other,
                    };

                    for &(name, selector, ref transform) in &parsed_fields {
                        let extracted = extract_field_from_map(&obj, selector);
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

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
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

impl __sealed::Sealed for FilterTool {}

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
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: FilterParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::Other {
                    tool: "nika:filter".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            let array = params
                .array
                .as_array()
                .ok_or_else(|| BuiltinError::Other {
                    tool: "nika:filter".into(),
                    reason: "Expected array for 'array' parameter".into(),
                })?;

            let result: Vec<Value> = array
                .iter()
                .filter(|v| matches_predicate(v, &params.field, &params.op, &params.value))
                .cloned()
                .collect();

            serde_json::to_string(&result).map_err(|e| BuiltinError::Other {
                tool: "nika:filter".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// nika:group_by
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

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
}
