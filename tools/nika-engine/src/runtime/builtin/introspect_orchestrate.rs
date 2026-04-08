//! nika:orchestrate — Introspection tool returning aggregate workflow stats.
//!
//! Aggregates from `EventLog` and `RunContext` to provide a high-level
//! overview of the current workflow execution.
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
//!   "total_tasks": 5,
//!   "completed": 3,
//!   "total_tokens": 15000,
//!   "total_cost_usd": 0.045,
//!   "records_count": 2
//! }
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use crate::store::RunContext;
use nika_event::{EventKind, EventLog};
use rustc_hash::FxHashSet;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// nika:orchestrate builtin tool — aggregate workflow stats.
pub struct OrchestrateTool {
    event_log: EventLog,
    datastore: Arc<RunContext>,
}

impl OrchestrateTool {
    pub fn new(event_log: EventLog, datastore: Arc<RunContext>) -> Self {
        Self {
            event_log,
            datastore,
        }
    }
}

#[derive(Debug, Serialize)]
struct OrchestrateResponse {
    total_tasks: usize,
    completed: usize,
    total_tokens: u64,
    total_cost_usd: f64,
    records_count: usize,
    /// Current orchestration round (from events, 0 if no rounds yet)
    #[serde(skip_serializing_if = "Option::is_none")]
    round: Option<u32>,
    /// Workflow goal (if orchestrator mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    goal: Option<String>,
    /// Confidence target (if orchestrator mode)
    #[serde(skip_serializing_if = "Option::is_none")]
    confidence_target: Option<f64>,
    /// Cost limit in USD (if configured)
    #[serde(skip_serializing_if = "Option::is_none")]
    cost_limit_usd: Option<f64>,
}

impl BuiltinTool for OrchestrateTool {
    fn name(&self) -> &'static str {
        "orchestrate"
    }

    fn description(&self) -> &'static str {
        "Return aggregate workflow stats: tasks, tokens, cost, records"
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
            let mut all_tasks: FxHashSet<String> = FxHashSet::default();
            let mut completed: FxHashSet<String> = FxHashSet::default();
            let mut total_tokens: u64 = 0;
            let mut total_cost_usd: f64 = 0.0;
            let mut latest_round: Option<u32> = None;
            let mut goal: Option<String> = None;

            self.event_log.with_events(|events| {
                for event in events {
                    match &event.kind {
                        EventKind::TaskScheduled { task_id, .. }
                        | EventKind::TaskStarted { task_id, .. }
                        | EventKind::TaskFailed { task_id, .. } => {
                            all_tasks.insert(task_id.to_string());
                        }
                        EventKind::TaskCompleted { task_id, .. } => {
                            all_tasks.insert(task_id.to_string());
                            completed.insert(task_id.to_string());
                        }
                        EventKind::ProviderResponded {
                            input_tokens,
                            output_tokens,
                            cost_usd,
                            ..
                        } => {
                            total_tokens = total_tokens
                                .saturating_add(input_tokens.saturating_add(*output_tokens));
                            total_cost_usd += cost_usd;
                        }
                        EventKind::OrchestratorStarted { goal: g, .. } => {
                            goal = Some(g.clone());
                        }
                        EventKind::OrchestratorRound { round, .. } => {
                            latest_round = Some(*round);
                        }
                        _ => {}
                    }
                }
            });

            let records_count = self.datastore.iter_records().len();

            let response = OrchestrateResponse {
                total_tasks: all_tasks.len(),
                completed: completed.len(),
                total_tokens,
                total_cost_usd,
                records_count,
                round: latest_round,
                goal,
                confidence_target: None, // Set by caller if orchestrator mode
                cost_limit_usd: None,    // Set by caller if orchestrator mode
            };

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:orchestrate".into(),
                reason: format!("Failed to serialize orchestrate response: {e}"),
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
    async fn test_orchestrate_basic_stats() {
        let log = EventLog::new();
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));

        // 2 tasks, 1 completed
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("a"),
            dependencies: vec![],
        });
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("b"),
            dependencies: vec![Arc::from("a")],
        });
        log.emit(EventKind::TaskStarted {
            task_id: Arc::from("a"),
            verb: Arc::from("infer"),
            inputs: Arc::new(serde_json::json!({})),
        });
        log.emit(EventKind::ProviderResponded {
            task_id: Arc::from("a"),
            request_id: None,
            input_tokens: 1000,
            output_tokens: 500,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: nika_event::types::FinishReason::Stop,
            cost_usd: 0.003,
        });
        log.emit(EventKind::TaskCompleted {
            task_id: Arc::from("a"),
            output: Arc::new(serde_json::json!("done")),
            duration_ms: 200,
        });

        let tool = OrchestrateTool::new(log, ctx);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total_tasks"], 2);
        assert_eq!(v["completed"], 1);
        assert_eq!(v["total_tokens"], 1500);
        assert!((v["total_cost_usd"].as_f64().unwrap() - 0.003).abs() < 1e-10);
        assert_eq!(v["records_count"], 0);
        // No orchestrator events → fields absent
        assert!(v.get("round").is_none());
        assert!(v.get("goal").is_none());
    }

    #[tokio::test]
    async fn test_orchestrate_with_round_events() {
        let log = EventLog::new();
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));

        log.emit(EventKind::OrchestratorStarted {
            goal: "Research AI".to_string(),
            max_rounds: 10,
            agent: None,
        });
        log.emit(EventKind::OrchestratorRound {
            round: 1,
            records_count: 3,
            cost_usd: 0.05,
        });
        log.emit(EventKind::OrchestratorRound {
            round: 2,
            records_count: 5,
            cost_usd: 0.10,
        });

        let tool = OrchestrateTool::new(log, ctx);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["round"], 2);
        assert_eq!(v["goal"], "Research AI");
    }

    #[tokio::test]
    async fn test_orchestrate_no_orchestrator_events() {
        let log = EventLog::new();
        let ctx = Arc::new(RunContext::new(nika_core::trust::InvocationSource::Test));

        // Only task events, no orchestrator events
        log.emit(EventKind::TaskScheduled {
            task_id: Arc::from("x"),
            dependencies: vec![],
        });

        let tool = OrchestrateTool::new(log, ctx);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert!(v.get("round").is_none());
        assert!(v.get("goal").is_none());
    }
}
