// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:threads — Introspection tool returning task thread statuses.
//!
//! Builds a status map from `TaskStarted`, `TaskCompleted`, `TaskFailed`,
//! and `TaskScheduled` events. Optionally filters by status.
//!
//! # Parameters
//!
//! ```json
//! { "status": "running" }
//! ```
//!
//! # Returns
//!
//! ```json
//! [
//!   { "task_id": "research", "status": "completed", "verb": "infer" },
//!   { "task_id": "summarize", "status": "running", "verb": "infer" }
//! ]
//! ```

use crate::{BuiltinError, BuiltinTool, __sealed};
use nika_event::{EventKind, EventLog};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::future::Future;
use std::pin::Pin;

/// nika:threads builtin tool — lists all tasks with their execution status.
pub struct ThreadsTool {
    event_log: EventLog,
}

impl __sealed::Sealed for ThreadsTool {}

impl ThreadsTool {
    pub fn new(event_log: EventLog) -> Self {
        Self { event_log }
    }
}

#[derive(Debug, Deserialize)]
struct ThreadsParams {
    #[serde(default)]
    status: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ThreadEntry {
    task_id: String,
    status: String,
    verb: String,
}

impl BuiltinTool for ThreadsTool {
    fn name(&self) -> &'static str {
        "threads"
    }

    fn description(&self) -> &'static str {
        "List all tasks with their execution status, optionally filtered"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "description": "Filter by status: pending, running, completed, failed",
                    "enum": ["pending", "running", "completed", "failed"]
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
            let params: ThreadsParams =
                serde_json::from_str(&args).map_err(|e| BuiltinError::InvalidArgs {
                    tool: "nika:threads".into(),
                    reason: format!("Invalid JSON parameters: {e}"),
                })?;

            // Track status and verb per task
            let mut status_map: FxHashMap<String, (String, String)> = FxHashMap::default();

            self.event_log.with_events(|events| {
                for event in events {
                    #[allow(clippy::wildcard_enum_match_arm)]
                    match &event.kind {
                        EventKind::TaskScheduled { task_id, .. } => {
                            status_map
                                .entry(task_id.to_string())
                                .or_insert_with(|| ("pending".to_string(), String::new()));
                        }
                        EventKind::TaskStarted { task_id, verb, .. } => {
                            let entry = status_map
                                .entry(task_id.to_string())
                                .or_insert_with(|| (String::new(), String::new()));
                            entry.0 = "running".to_string();
                            entry.1 = verb.to_string();
                        }
                        EventKind::TaskCompleted { task_id, .. } => {
                            if let Some(entry) = status_map.get_mut(task_id.as_ref()) {
                                entry.0 = "completed".to_string();
                            } else {
                                status_map.insert(
                                    task_id.to_string(),
                                    ("completed".to_string(), String::new()),
                                );
                            }
                        }
                        EventKind::TaskFailed { task_id, .. } => {
                            if let Some(entry) = status_map.get_mut(task_id.as_ref()) {
                                entry.0 = "failed".to_string();
                            } else {
                                status_map.insert(
                                    task_id.to_string(),
                                    ("failed".to_string(), String::new()),
                                );
                            }
                        }
                        _ => {}
                    }
                }
            });

            let mut entries: Vec<ThreadEntry> = status_map
                .into_iter()
                .filter(|(_, (st, _))| params.status.as_ref().is_none_or(|filter| st == filter))
                .map(|(tid, (st, verb))| ThreadEntry {
                    task_id: tid,
                    status: st,
                    verb,
                })
                .collect();

            // Sort by task_id for deterministic output
            entries.sort_by(|a, b| a.task_id.cmp(&b.task_id));

            serde_json::to_string(&entries).map_err(|e| BuiltinError::Other {
                tool: "nika:threads".into(),
                reason: format!("Failed to serialize threads: {e}"),
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
    async fn test_threads_empty_log() {
        let log = EventLog::new();
        let tool = ThreadsTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 0);
    }

    #[tokio::test]
    async fn test_threads_mixed_statuses() {
        let log = EventLog::new();

        // Task A: completed
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("a"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("a"),
            verb: Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::TaskCompleted {
            task_id: Arc::from("a"),
            output: Arc::new(serde_json::json!("ok")),
            duration_ms: 100,
        });

        // Task B: running
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("b"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("b"),
            verb: Arc::from("fetch"),
            inputs: Arc::new(serde_json::json!({})),
        });

        // Task C: failed
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("c"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("c"),
            verb: Arc::from("exec"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::TaskFailed {
            task_id: Arc::from("c"),
            error: "exit code 1".into(),
            duration_ms: 50,
            error_code: None,
        });

        // Task D: pending (only scheduled)
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("d"),
            dependencies: vec![Arc::from("a")],
        });

        // No filter — all 4
        let tool = ThreadsTool::new(log.clone());
        let result = tool.call("{}".into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 4);

        // Filter by running
        let result = tool.call(r#"{"status":"running"}"#.into()).await.unwrap();
        let v: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0]["task_id"], "b");
        assert_eq!(v[0]["status"], "running");
        assert_eq!(v[0]["verb"], "fetch");
    }
}
