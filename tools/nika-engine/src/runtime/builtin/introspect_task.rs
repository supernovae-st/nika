// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:task_status — Introspection tool returning status of a specific task.
//!
//! Searches `EventLog` for lifecycle events matching a given `task_id` and
//! looks up token/cost data from `ProviderResponded` events.
//!
//! # Parameters
//!
//! ```json
//! { "task_id": "research" }
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "task_id": "research",
//!   "status": "completed",
//!   "duration_ms": 1234,
//!   "tokens": 500,
//!   "cost_usd": 0.01,
//!   "has_record": false
//! }
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use crate::store::RunContext;
use nika_event::{EventKind, EventLog};
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// nika:task_status builtin tool — reports status of a specific task.
pub struct TaskStatusTool {
    event_log: EventLog,
    datastore: Arc<RunContext>,
}

impl TaskStatusTool {
    pub fn new(event_log: EventLog, datastore: Arc<RunContext>) -> Self {
        Self {
            event_log,
            datastore,
        }
    }
}

#[derive(Debug, Deserialize)]
struct TaskStatusParams {
    task_id: String,
}

#[derive(Debug, Serialize)]
struct TaskStatusResponse {
    task_id: String,
    status: String,
    duration_ms: u64,
    tokens: u64,
    cost_usd: f64,
    has_record: bool,
}

impl BuiltinTool for TaskStatusTool {
    fn name(&self) -> &'static str {
        "task_status"
    }

    fn description(&self) -> &'static str {
        "Return the execution status and metrics of a specific task"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": {
                    "type": "string",
                    "description": "The task ID to look up"
                }
            },
            "required": ["task_id"],
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let params: TaskStatusParams =
                serde_json::from_str(&args).map_err(|e| NikaError::BuiltinInvalidParams {
                    tool: "nika:task_status".into(),
                    reason: format!("Invalid JSON parameters: {e}"),
                })?;

            let tid = &params.task_id;
            let mut status = "unknown".to_string();
            let mut duration_ms: u64 = 0;
            let mut tokens: u64 = 0;
            let mut cost_usd: f64 = 0.0;
            let mut found = false;

            self.event_log.with_events(|events| {
                for event in events {
                    match &event.kind {
                        EventKind::TaskScheduled { task_id, .. } if task_id.as_ref() == tid => {
                            status = "pending".to_string();
                            found = true;
                        }
                        EventKind::TaskStarted { task_id, .. } if task_id.as_ref() == tid => {
                            status = "running".to_string();
                            found = true;
                        }
                        EventKind::TaskCompleted {
                            task_id,
                            duration_ms: dur,
                            ..
                        } if task_id.as_ref() == tid => {
                            status = "completed".to_string();
                            duration_ms = *dur;
                            found = true;
                        }
                        EventKind::TaskFailed {
                            task_id,
                            duration_ms: dur,
                            ..
                        } if task_id.as_ref() == tid => {
                            status = "failed".to_string();
                            duration_ms = *dur;
                            found = true;
                        }
                        EventKind::ProviderResponded {
                            task_id,
                            input_tokens,
                            output_tokens,
                            cost_usd: cost,
                            ..
                        } if task_id.as_ref() == tid => {
                            tokens = tokens
                                .saturating_add(*input_tokens)
                                .saturating_add(*output_tokens);
                            cost_usd += cost;
                        }
                        _ => {}
                    }
                }
            });

            if !found {
                return Err(NikaError::BuiltinToolError {
                    tool: "nika:task_status".into(),
                    reason: format!("Task '{}' not found in event log", tid),
                });
            }

            let has_record = self.datastore.has_record(tid);

            let response = TaskStatusResponse {
                task_id: tid.clone(),
                status,
                duration_ms,
                tokens,
                cost_usd,
                has_record,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:task_status".into(),
                reason: format!("Failed to serialize task status: {e}"),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::{EventKind, EventLog};
    use std::sync::Arc;

    #[tokio::test]
    async fn test_task_status_completed() {
        let log = EventLog::new();
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));

        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("research"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("research"),
            verb: Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::ProviderResponded {
            task_id: Arc::from("research"),
            request_id: None,
            input_tokens: 400,
            output_tokens: 100,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: nika_event::types::FinishReason::Stop,
            cost_usd: 0.01,
        });
        log.emit(EventKind::TaskCompleted {
            task_id: Arc::from("research"),
            output: Arc::new(serde_json::json!("done")),
            duration_ms: 1234,
        });

        let tool = TaskStatusTool::new(log, ctx);
        let result = tool.call(r#"{"task_id":"research"}"#.into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["task_id"], "research");
        assert_eq!(v["status"], "completed");
        assert_eq!(v["duration_ms"], 1234);
        assert_eq!(v["tokens"], 500);
        assert!((v["cost_usd"].as_f64().unwrap() - 0.01).abs() < 1e-10);
        assert_eq!(v["has_record"], false);
    }

    #[tokio::test]
    async fn test_task_status_missing_returns_error() {
        let log = EventLog::new();
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));

        let tool = TaskStatusTool::new(log, ctx);
        let result = tool.call(r#"{"task_id":"nonexistent"}"#.into()).await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }
}
