// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! nika:cost — Introspection tool that reports workflow token usage and cost.
//!
//! Iterates `ProviderResponded` events from the `EventLog` and returns:
//! - Total input/output/cache tokens
//! - Total estimated cost in USD
//! - Per-task breakdown
//!
//! # Returns
//!
//! ```json
//! {
//!   "total_input_tokens": 10000,
//!   "total_output_tokens": 5000,
//!   "total_cache_read_tokens": 200,
//!   "total_cost_usd": 0.045,
//!   "calls": 3,
//!   "per_task": {
//!     "research": { "input_tokens": 8000, "output_tokens": 4000, "cost_usd": 0.039 }
//!   }
//! }
//! ```

use super::BuiltinTool;
use crate::error::NikaError;
use nika_event::EventLog;
use rustc_hash::FxHashMap;
use serde::Serialize;
use std::future::Future;
use std::pin::Pin;

/// nika:cost builtin tool — reports token usage and cost from the current workflow.
///
/// EventLog is Clone (internally Arc-backed), so cloning shares the same event data.
pub struct CostTool {
    event_log: EventLog,
}

impl CostTool {
    pub fn new(event_log: EventLog) -> Self {
        Self { event_log }
    }
}

#[derive(Debug, Serialize)]
struct CostResponse {
    total_input_tokens: u64,
    total_output_tokens: u64,
    total_cache_read_tokens: u64,
    total_cost_usd: f64,
    calls: usize,
    per_task: FxHashMap<String, TaskCost>,
}

#[derive(Debug, Default, Serialize)]
struct TaskCost {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cost_usd: f64,
    calls: usize,
}

impl BuiltinTool for CostTool {
    fn name(&self) -> &'static str {
        "cost"
    }

    fn description(&self) -> &'static str {
        "Report token usage and estimated cost for the current workflow run"
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
            let mut response = CostResponse {
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_read_tokens: 0,
                total_cost_usd: 0.0,
                calls: 0,
                per_task: FxHashMap::default(),
            };

            self.event_log.with_events(|events| {
                for event in events {
                    if let nika_event::EventKind::ProviderResponded {
                        task_id,
                        input_tokens,
                        output_tokens,
                        cache_read_tokens,
                        cost_usd,
                        ..
                    } = &event.kind
                    {
                        response.total_input_tokens =
                            response.total_input_tokens.saturating_add(*input_tokens);
                        response.total_output_tokens =
                            response.total_output_tokens.saturating_add(*output_tokens);
                        response.total_cache_read_tokens = response
                            .total_cache_read_tokens
                            .saturating_add(*cache_read_tokens);
                        response.total_cost_usd += cost_usd;
                        response.calls += 1;

                        let entry = response.per_task.entry(task_id.to_string()).or_default();
                        entry.input_tokens = entry.input_tokens.saturating_add(*input_tokens);
                        entry.output_tokens = entry.output_tokens.saturating_add(*output_tokens);
                        entry.cache_read_tokens =
                            entry.cache_read_tokens.saturating_add(*cache_read_tokens);
                        entry.cost_usd += cost_usd;
                        entry.calls += 1;
                    }
                }
            });

            serde_json::to_string(&response).map_err(|e| NikaError::BuiltinToolError {
                tool: "nika:cost".into(),
                reason: format!("Failed to serialize cost response: {e}"),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nika_event::{EventKind, EventLog};
    use std::sync::Arc;

    fn make_provider_responded(task_id: &str, input: u64, output: u64, cost: f64) -> EventKind {
        EventKind::ProviderResponded {
            task_id: Arc::from(task_id),
            request_id: None,
            input_tokens: input,
            output_tokens: output,
            cache_read_tokens: 0,
            ttft_ms: None,
            finish_reason: nika_event::types::FinishReason::Stop,
            cost_usd: cost,
        }
    }

    #[tokio::test]
    async fn test_cost_tool_empty_log() {
        let log = EventLog::new();
        let tool = CostTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total_input_tokens"], 0);
        assert_eq!(v["total_output_tokens"], 0);
        assert_eq!(v["total_cost_usd"], 0.0);
        assert_eq!(v["calls"], 0);
    }

    #[tokio::test]
    async fn test_cost_tool_single_provider_call() {
        let log = EventLog::new();
        log.emit(make_provider_responded("task1", 1000, 500, 0.003));
        let tool = CostTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total_input_tokens"], 1000);
        assert_eq!(v["total_output_tokens"], 500);
        assert_eq!(v["calls"], 1);
        assert!((v["total_cost_usd"].as_f64().unwrap() - 0.003).abs() < 1e-10);
    }

    #[tokio::test]
    async fn test_cost_tool_multiple_tasks() {
        let log = EventLog::new();
        log.emit(make_provider_responded("research", 8000, 4000, 0.039));
        log.emit(make_provider_responded("summarize", 2000, 1000, 0.006));
        let tool = CostTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total_input_tokens"], 10000);
        assert_eq!(v["total_output_tokens"], 5000);
        assert_eq!(v["calls"], 2);
        assert!((v["total_cost_usd"].as_f64().unwrap() - 0.045).abs() < 1e-10);
        // Per-task breakdown
        assert_eq!(v["per_task"]["research"]["input_tokens"], 8000);
        assert_eq!(v["per_task"]["summarize"]["input_tokens"], 2000);
    }

    #[tokio::test]
    async fn test_cost_tool_with_cached_tokens() {
        let log = EventLog::new();
        log.emit(EventKind::ProviderResponded {
            task_id: Arc::from("cached_task"),
            request_id: None,
            input_tokens: 1000,
            output_tokens: 200,
            cache_read_tokens: 500,
            ttft_ms: Some(50),
            finish_reason: nika_event::types::FinishReason::Stop,
            cost_usd: 0.001,
        });
        let tool = CostTool::new(log);
        let result = tool.call("{}".into()).await.unwrap();
        let v: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(v["total_cache_read_tokens"], 500);
        assert_eq!(v["per_task"]["cached_task"]["cache_read_tokens"], 500);
    }

    #[tokio::test]
    async fn test_cost_tool_name_and_schema() {
        let log = EventLog::new();
        let tool = CostTool::new(log);
        assert_eq!(tool.name(), "cost");
        assert_eq!(
            tool.description(),
            "Report token usage and estimated cost for the current workflow run"
        );
        let schema = tool.parameters_schema();
        assert_eq!(schema["type"], "object");
    }
}
