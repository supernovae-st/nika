//! JQ tools: jq (via jaq-core), json_query (deprecated)

use super::super::BuiltinTool;
use crate::error::NikaError;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

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
}
