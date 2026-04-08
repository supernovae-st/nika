//! nika:records — Introspection tool that queries compressed Records.
//!
//! # Parameters
//!
//! ```json
//! {
//!   "task_id": "research",       // optional filter by task
//!   "min_confidence": 0.7        // optional minimum confidence
//! }
//! ```
//!
//! # Returns
//!
//! ```json
//! [
//!   {
//!     "task_id": "research",
//!     "summary": "AI is transforming...",
//!     "confidence": 0.85,
//!     "compression_ratio": 0.03
//!   }
//! ]
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use crate::store::RunContext;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// nika:records builtin tool — queries compressed records from the current workflow.
pub struct RecordsTool {
    datastore: Arc<RunContext>,
}

impl RecordsTool {
    pub fn new(datastore: Arc<RunContext>) -> Self {
        Self { datastore }
    }
}

#[derive(Debug, Deserialize)]
struct RecordsParams {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    min_confidence: Option<f64>,
}

#[derive(Debug, Serialize)]
struct RecordSummary {
    task_id: String,
    summary: String,
    confidence: f64,
    compression_ratio: f64,
    key_findings: Vec<String>,
}

impl BuiltinTool for RecordsTool {
    fn name(&self) -> &'static str {
        "records"
    }

    fn description(&self) -> &'static str {
        "Query compressed task records from the current workflow run"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "Filter by specific task ID"
                },
                "min_confidence": {
                    "type": "number",
                    "description": "Minimum confidence threshold (0.0-1.0)"
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
            let params: RecordsParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:records".into(),
                    reason: format!("Invalid JSON parameters: {e}"),
                })?;

            // Fast-path: single task_id lookup avoids cloning all records
            let results: Vec<RecordSummary> = if let Some(ref tid) = params.task_id {
                self.datastore
                    .get_record(tid)
                    .filter(|r| params.min_confidence.is_none_or(|min| r.confidence >= min))
                    .into_iter()
                    .map(|record| RecordSummary {
                        task_id: tid.clone(),
                        summary: record.summary.clone(),
                        confidence: record.confidence,
                        compression_ratio: record.compression_ratio(),
                        key_findings: record.key_findings.clone(),
                    })
                    .collect()
            } else {
                self.datastore
                    .iter_records()
                    .into_iter()
                    .filter(|(_, record)| {
                        params
                            .min_confidence
                            .is_none_or(|min| record.confidence >= min)
                    })
                    .map(|(tid, record)| RecordSummary {
                        task_id: tid.to_string(),
                        summary: record.summary.clone(),
                        confidence: record.confidence,
                        compression_ratio: record.compression_ratio(),
                        key_findings: record.key_findings.clone(),
                    })
                    .collect()
            };

            serde_json::to_string(&results).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:records".into(),
                reason: format!("Failed to serialize records: {e}"),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::record::Record;
    use crate::store::RunContext;

    fn make_record(task_id: &str, confidence: f64) -> Record {
        Record {
            task_id: Arc::from(task_id),
            summary: format!("Summary of {task_id}"),
            key_findings: vec!["finding1".to_string()],
            raw_output: None,
            confidence,
            tokens_original: 1000,
            tokens_compressed: 100,
            compression_model: "mock".to_string(),
            compression_cost_usd: 0.0,
            compression_duration: std::time::Duration::ZERO,
        }
    }

    #[tokio::test]
    async fn test_records_tool_empty() {
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));
        let tool = RecordsTool::new(ctx);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_records_tool_returns_all() {
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));
        ctx.set_record("a".into(), make_record("a", 0.9));
        ctx.set_record("b".into(), make_record("b", 0.7));
        let tool = RecordsTool::new(ctx);
        let result = tool.call("{}".into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 2);
    }

    #[tokio::test]
    async fn test_records_tool_filter_by_task_id() {
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));
        ctx.set_record("a".into(), make_record("a", 0.9));
        ctx.set_record("b".into(), make_record("b", 0.7));
        let tool = RecordsTool::new(ctx);
        let result = tool.call(r#"{"task_id":"a"}"#.into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["task_id"], "a");
    }

    #[tokio::test]
    async fn test_records_tool_filter_by_confidence() {
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));
        ctx.set_record("high".into(), make_record("high", 0.9));
        ctx.set_record("low".into(), make_record("low", 0.3));
        let tool = RecordsTool::new(ctx);
        let result = tool.call(r#"{"min_confidence":0.5}"#.into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["task_id"], "high");
    }
}
