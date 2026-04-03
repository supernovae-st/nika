//! `nika:aggregate` — Array statistics (sum, avg, min, max, count).
//!
//! Replaces `infer: temperature: 0.0` workarounds for trivial math.

use super::BuiltinTool;
use crate::error::NikaError;
use serde::Deserialize;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;

pub struct AggregateTool;

#[derive(Debug, Deserialize)]
struct AggregateParams {
    /// Array of numbers (or objects — use `field` to extract)
    array: Vec<Value>,
    /// Operations to compute
    ops: Vec<String>,
    /// Optional: extract this field from each object before aggregating
    #[serde(default)]
    field: Option<String>,
}

impl BuiltinTool for AggregateTool {
    fn name(&self) -> &'static str {
        "aggregate"
    }

    fn description(&self) -> &'static str {
        "Compute statistics (sum, avg, min, max, count) over an array of numbers"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "required": ["array", "ops"],
            "properties": {
                "array": {
                    "type": "array",
                    "description": "Array of numbers or objects"
                },
                "ops": {
                    "type": "array",
                    "items": { "type": "string", "enum": ["sum", "avg", "min", "max", "count"] },
                    "description": "Aggregation operations to compute"
                },
                "field": {
                    "type": "string",
                    "description": "Extract this field from objects before aggregating"
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
            let params: AggregateParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinToolError {
                    tool: "nika:aggregate".into(),
                    reason: format!("Invalid parameters: {e}"),
                })?;

            // Extract numeric values
            let numbers: Vec<f64> = params
                .array
                .iter()
                .filter_map(|v| {
                    let target = if let Some(ref field) = params.field {
                        v.get(field)?
                    } else {
                        v
                    };
                    target.as_f64()
                })
                .collect();

            let mut result = serde_json::Map::new();

            for op in &params.ops {
                let val = match op.as_str() {
                    "count" => Value::Number(numbers.len().into()),
                    "sum" => {
                        let sum: f64 = numbers.iter().sum();
                        to_json_number(sum)
                    }
                    "avg" => {
                        if numbers.is_empty() {
                            Value::Null
                        } else {
                            let sum: f64 = numbers.iter().sum();
                            to_json_number(sum / numbers.len() as f64)
                        }
                    }
                    "min" => numbers
                        .iter()
                        .copied()
                        .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .map(to_json_number)
                        .unwrap_or(Value::Null),
                    "max" => numbers
                        .iter()
                        .copied()
                        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                        .map(to_json_number)
                        .unwrap_or(Value::Null),
                    other => {
                        return Err(NikaError::BuiltinToolError {
                            tool: "nika:aggregate".into(),
                            reason: format!(
                                "Unknown operation: {other}. Use sum, avg, min, max, count."
                            ),
                        });
                    }
                };
                result.insert(op.clone(), val);
            }

            serde_json::to_string(&Value::Object(result)).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:aggregate".into(),
                reason: format!("Serialization failed: {e}"),
            })
        })
    }
}

fn to_json_number(f: f64) -> Value {
    // Return integer if possible
    if f.fract() == 0.0 && f.abs() < i64::MAX as f64 {
        Value::Number((f as i64).into())
    } else {
        serde_json::Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn aggregate_basic() {
        let tool = AggregateTool;
        let args = json!({
            "array": [10, 20, 30, 40, 50],
            "ops": ["sum", "avg", "min", "max", "count"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["sum"], 150);
        assert_eq!(result["avg"], 30);
        assert_eq!(result["min"], 10);
        assert_eq!(result["max"], 50);
        assert_eq!(result["count"], 5);
    }

    #[tokio::test]
    async fn aggregate_floats() {
        let tool = AggregateTool;
        let args = json!({
            "array": [1.5, 2.5, 3.0],
            "ops": ["sum", "avg"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["sum"], 7.0);
        let avg = result["avg"].as_f64().unwrap();
        assert!((avg - 2.333_333_333).abs() < 0.001);
    }

    #[tokio::test]
    async fn aggregate_with_field() {
        let tool = AggregateTool;
        let args = json!({
            "array": [{"name": "Alice", "score": 90}, {"name": "Bob", "score": 80}],
            "ops": ["avg", "sum"],
            "field": "score"
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["sum"], 170);
        assert_eq!(result["avg"], 85);
    }

    #[tokio::test]
    async fn aggregate_empty_array() {
        let tool = AggregateTool;
        let args = json!({
            "array": [],
            "ops": ["sum", "avg", "min", "max", "count"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["count"], 0);
        assert_eq!(result["sum"], 0);
        assert_eq!(result["avg"], Value::Null);
        assert_eq!(result["min"], Value::Null);
        assert_eq!(result["max"], Value::Null);
    }

    #[tokio::test]
    async fn aggregate_unknown_op_errors() {
        let tool = AggregateTool;
        let args = json!({
            "array": [1, 2],
            "ops": ["median"]
        });
        let result = tool.call(args.to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn aggregate_single_element() {
        let tool = AggregateTool;
        let args = json!({
            "array": [42],
            "ops": ["sum", "avg", "min", "max", "count"]
        });
        let result: Value =
            serde_json::from_str(&tool.call(args.to_string()).await.unwrap()).unwrap();
        assert_eq!(result["sum"], 42);
        assert_eq!(result["avg"], 42);
        assert_eq!(result["min"], 42);
        assert_eq!(result["max"], 42);
        assert_eq!(result["count"], 1);
    }
}
