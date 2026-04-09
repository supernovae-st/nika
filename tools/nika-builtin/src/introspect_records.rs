// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

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

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use nika_kernel::builtin::{BuiltinError, BuiltinTool, __sealed};
use nika_kernel::scope::RecordQuery;
use serde::Deserialize;

/// nika:records builtin tool — queries compressed records from the current workflow.
pub struct RecordsTool {
    records: Arc<dyn RecordQuery>,
}

impl __sealed::Sealed for RecordsTool {}

impl RecordsTool {
    pub fn new(records: Arc<dyn RecordQuery>) -> Self {
        Self { records }
    }
}

#[derive(Debug, Deserialize)]
struct RecordsParams {
    #[serde(default)]
    task_id: Option<String>,
    #[serde(default)]
    min_confidence: Option<f64>,
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
    ) -> Pin<Box<dyn Future<Output = Result<String, BuiltinError>> + Send + 'a>> {
        Box::pin(async move {
            let params: RecordsParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:records".into(),
                    reason: format!("Invalid JSON parameters: {e}"),
                })?;

            // RecordView is already Serialize — collect and filter
            let results: Vec<nika_kernel::scope::RecordView> = if let Some(ref tid) = params.task_id
            {
                self.records
                    .get_record_view(tid)
                    .filter(|r| params.min_confidence.is_none_or(|min| r.confidence >= min))
                    .into_iter()
                    .collect()
            } else {
                self.records
                    .iter_record_views()
                    .into_iter()
                    .filter(|r| params.min_confidence.is_none_or(|min| r.confidence >= min))
                    .collect()
            };

            serde_json::to_string(&results).map_err(|e| BuiltinError::Other {
                tool: "nika:records".into(),
                reason: format!("Failed to serialize records: {e}"),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_kernel::scope::RecordView;
    use nika_kernel_mock::MockRecordStore;

    fn make_view(task_id: &str, confidence: f64) -> RecordView {
        RecordView {
            task_id: task_id.into(),
            summary: format!("Summary of {task_id}"),
            key_findings: vec!["finding1".into()],
            confidence,
            compression_ratio: 0.05,
        }
    }

    #[tokio::test]
    async fn test_records_tool_empty() {
        let records = Arc::new(MockRecordStore::new());
        let tool = RecordsTool::new(records);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_records_tool_returns_all() {
        let records = Arc::new(MockRecordStore::new());
        records.seed(make_view("a", 0.9));
        records.seed(make_view("b", 0.7));
        let tool = RecordsTool::new(records);
        let result = tool.call("{}".into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 2);
    }

    #[tokio::test]
    async fn test_records_tool_filter_by_task_id() {
        let records = Arc::new(MockRecordStore::new());
        records.seed(make_view("a", 0.9));
        records.seed(make_view("b", 0.7));
        let tool = RecordsTool::new(records);
        let result = tool.call(r#"{"task_id":"a"}"#.into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["task_id"], "a");
    }

    #[tokio::test]
    async fn test_records_tool_filter_by_confidence() {
        let records = Arc::new(MockRecordStore::new());
        records.seed(make_view("high", 0.9));
        records.seed(make_view("low", 0.3));
        let tool = RecordsTool::new(records);
        let result = tool.call(r#"{"min_confidence":0.5}"#.into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["task_id"], "high");
    }

    #[tokio::test]
    async fn test_records_tool_invalid_params() {
        let records = Arc::new(MockRecordStore::new());
        let tool = RecordsTool::new(records);
        let result = tool.call("not json".into()).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("NIKA-212"));
    }

    #[tokio::test]
    async fn test_records_tool_combined_filters() {
        let records = Arc::new(MockRecordStore::new());
        records.seed(make_view("a", 0.9));
        records.seed(make_view("b", 0.3));
        let tool = RecordsTool::new(records);
        // Filter by task_id AND min_confidence — "a" exists with 0.9 > 0.5
        let result = tool
            .call(r#"{"task_id":"a","min_confidence":0.5}"#.into())
            .await
            .unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["task_id"], "a");

        // Filter by task_id AND min_confidence — "b" exists but 0.3 < 0.5
        let result = tool
            .call(r#"{"task_id":"b","min_confidence":0.5}"#.into())
            .await
            .unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 0);
    }
}
