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
            let mut for_each_expansion: usize = 0;
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
                        EventKind::ForEachStarted { item_count, .. } => {
                            // Each for_each replaces 1 YAML definition with N iterations
                            for_each_expansion += item_count.saturating_sub(1);
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
            // otherwise fall back to observed tasks count.
            // Adjust for for_each expansion: each for_each replaces 1 YAML
            // task definition with N runtime iterations.
            let task_count = total_task_count.unwrap_or(observed_tasks.len()) + for_each_expansion;

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

    #[tokio::test]
    async fn test_dag_info_for_each_expansion() {
        let log = EventLog::new();

        // 3 YAML tasks: generate_items, process (for_each), check_dag
        log.emit(EventKind::WorkflowStarted {
            task_count: 3,
            generation_id: "test".into(),
            workflow_hash: "abc".into(),
            nika_version: "0.61.0".into(),
        });

        // generate_items completes
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("generate_items"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("generate_items"),
            verb: Arc::from("exec"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::TaskCompleted {
            task_id: Arc::from("generate_items"),
            output: Arc::new(serde_json::json!(["a", "b", "c", "d", "e"])),
            duration_ms: 10,
        });

        // for_each "process" expands to 5 iterations
        log.emit(EventKind::ForEachStarted {
            task_id: Arc::from("process"),
            item_count: 5,
            concurrency: 2,
            fail_fast: true,
        });

        // 3 iterations completed, 1 failed, 1 still pending
        for i in 0..3 {
            log.emit(EventKind::TaskStarted {
                task_id: Arc::from(format!("process[{i}]")),
                verb: Arc::from("infer"),
                inputs: Arc::new(serde_json::json!({})),
            });
            log.emit(EventKind::TaskCompleted {
                task_id: Arc::from(format!("process[{i}]")),
                output: Arc::new(serde_json::json!("ok")),
                duration_ms: 100,
            });
        }
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("process[3]"),
            verb: Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::TaskFailed {
            task_id: Arc::from("process[3]"),
            error: "rate limit".into(),
            duration_ms: 50,
            error_code: None,
        });

        let tool = DagInfoTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();

        // 3 YAML tasks + (5-1) for_each expansion = 7 total
        assert_eq!(v["task_count"], 7);
        // generate_items + process[0..2] = 4 completed
        assert_eq!(v["completed"], 4);
        // process[3] failed
        assert_eq!(v["failed"], 1);
        // process[4] + check_dag still pending = 2
        assert_eq!(v["pending"], 2);
    }

    #[tokio::test]
    async fn test_dag_info_multiple_for_each() {
        let log = EventLog::new();

        // 4 YAML tasks: setup, fetch_all (for_each 3), process_all (for_each 2), report
        log.emit(EventKind::WorkflowStarted {
            task_count: 4,
            generation_id: "test".into(),
            workflow_hash: "abc".into(),
            nika_version: "0.61.0".into(),
        });

        // Two for_each expansions
        log.emit(EventKind::ForEachStarted {
            task_id: Arc::from("fetch_all"),
            item_count: 3,
            concurrency: 3,
            fail_fast: false,
        });
        log.emit(EventKind::ForEachStarted {
            task_id: Arc::from("process_all"),
            item_count: 2,
            concurrency: 1,
            fail_fast: true,
        });

        // setup completed
        log.emit(EventKind::TaskCompleted {
            task_id: Arc::from("setup"),
            output: Arc::new(serde_json::json!("ok")),
            duration_ms: 5,
        });

        // All 3 fetch iterations completed
        for i in 0..3 {
            log.emit(EventKind::TaskCompleted {
                task_id: Arc::from(format!("fetch_all[{i}]")),
                output: Arc::new(serde_json::json!("ok")),
                duration_ms: 100,
            });
        }

        let tool = DagInfoTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();

        // 4 YAML + (3-1) + (2-1) = 7 total
        assert_eq!(v["task_count"], 7);
        // setup + fetch_all[0..2] = 4 completed
        assert_eq!(v["completed"], 4);
        assert_eq!(v["failed"], 0);
        // process_all[0], process_all[1], report = 3 pending
        assert_eq!(v["pending"], 3);
    }
}
