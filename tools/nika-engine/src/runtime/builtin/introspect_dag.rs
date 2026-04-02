//! nika:dag_info — Introspection tool returning DAG structure.
//!
//! Iterates `EventLog` events to count task lifecycle events.
//!
//! # Parameters
//!
//! ```json
//! {}
//! ```
//!
//! # Returns
//!
//! ```json
//! {
//!   "task_count": 5,
//!   "completed": 3,
//!   "failed": 1,
//!   "pending": 1
//! }
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use nika_event::{EventKind, EventLog};
use rustc_hash::FxHashSet;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;

/// nika:dag_info builtin tool — reports DAG structure from the current workflow.
pub struct DagInfoTool {
    event_log: EventLog,
}

impl DagInfoTool {
    pub fn new(event_log: EventLog) -> Self {
        Self { event_log }
    }
}

#[derive(Debug, Serialize)]
struct DagInfoResponse {
    task_count: usize,
    completed: usize,
    failed: usize,
    pending: usize,
}

impl BuiltinTool for DagInfoTool {
    fn name(&self) -> &'static str {
        "dag_info"
    }

    fn description(&self) -> &'static str {
        "Return DAG structure info: task counts by status"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    fn call<'a>(
        &'a self,
        _args: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, NikaError>> + Send + 'a>> {
        Box::pin(async move {
            let mut total_task_count: Option<usize> = None;
            let mut scheduled: FxHashSet<String> = FxHashSet::default();
            let mut started: FxHashSet<String> = FxHashSet::default();
            let mut completed: FxHashSet<String> = FxHashSet::default();
            let mut failed: FxHashSet<String> = FxHashSet::default();

            self.event_log.with_events(|events| {
                for event in events {
                    match &event.kind {
                        EventKind::WorkflowStarted { task_count, .. } => {
                            total_task_count = Some(*task_count);
                        }
                        EventKind::TaskScheduled { task_id, .. } => {
                            scheduled.insert(task_id.to_string());
                        }
                        EventKind::TaskStarted { task_id, .. } => {
                            started.insert(task_id.to_string());
                        }
                        EventKind::TaskCompleted { task_id, .. } => {
                            completed.insert(task_id.to_string());
                        }
                        EventKind::TaskFailed { task_id, .. } => {
                            failed.insert(task_id.to_string());
                        }
                        _ => {}
                    }
                }
            });

            // All known tasks = union of event-observed tasks
            let observed_tasks: FxHashSet<String> = scheduled
                .iter()
                .chain(started.iter())
                .chain(completed.iter())
                .chain(failed.iter())
                .cloned()
                .collect();

            // Use total from WorkflowStarted (all DAG tasks) if available,
            // otherwise fall back to observed tasks count
            let task_count = total_task_count.unwrap_or(observed_tasks.len());

            // Pending = total tasks minus completed and failed
            let pending = task_count.saturating_sub(completed.len() + failed.len());

            let response = DagInfoResponse {
                task_count,
                completed: completed.len(),
                failed: failed.len(),
                pending,
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:dag_info".into(),
                reason: format!("Failed to serialize DAG info: {e}"),
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
    async fn test_dag_info_empty_log() {
        let log = EventLog::new();
        let tool = DagInfoTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["task_count"], 0);
        assert_eq!(v["completed"], 0);
        assert_eq!(v["failed"], 0);
        assert_eq!(v["pending"], 0);
    }

    #[tokio::test]
    async fn test_dag_info_with_events() {
        let log = EventLog::new();

        // Emit workflow start with total task count
        log.emit(EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "test".into(),
            workflow_hash: "abc".into(),
            nika_version: "0.61.0".into(),
        });

        // Schedule 3 tasks
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("research"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("summarize"),
            dependencies: vec![Arc::from("research")],
        });
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("publish"),
            dependencies: vec![Arc::from("summarize")],
        });

        // Start and complete research
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("research"),
            verb: Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::TaskCompleted {
            task_id: Arc::from("research"),
            output: Arc::new(serde_json::json!("done")),
            duration_ms: 500,
        });

        // Start and fail summarize
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("summarize"),
            verb: Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::TaskFailed {
            task_id: Arc::from("summarize"),
            error: "timeout".into(),
            duration_ms: 1000,
            error_code: None,
        });

        let tool = DagInfoTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["task_count"], 3);
        assert_eq!(v["completed"], 1);
        assert_eq!(v["failed"], 1);
        assert_eq!(v["pending"], 1); // publish never started
    }
}
