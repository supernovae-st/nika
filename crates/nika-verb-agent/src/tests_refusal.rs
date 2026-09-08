// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Refusal wins over valid answers, tool dispatch and exhausted budgets.

use super::*;
use crate::tests::{def, rig, text_response, tool_use_response};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::StopReason;
use nika_kernel::runtime::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockToolExecutor};
use std::sync::Mutex;

#[derive(Default)]
struct Recording(Mutex<Vec<AgentEvent>>);

impl AgentObserver for Recording {
    fn on_event(&self, event: &AgentEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

fn typed_input(budget: u64) -> AgentInput {
    let mut input = AgentInput::new("return an integer score");
    input.schema = Some(serde_json::json!({
        "type": "object", "properties": {"score": {"type": "integer"}},
        "required": ["score"], "additionalProperties": false
    }));
    input.tools = vec!["nika:read".into(), DONE_TOOL.into()];
    input.max_tokens_total = Some(budget);
    input
}

#[tokio::test]
async fn refusal_precedes_output_tools_and_exhausted_budgets() {
    for raw in [false, true] {
        for reask in [false, true] {
            for tight in [false, true] {
                for mut response in [
                    text_response("invalid JSON"),
                    text_response(r#"{"score":7}"#),
                    tool_use_response("c", "nika:read", serde_json::json!({})),
                    tool_use_response("c", DONE_TOOL, serde_json::json!({"result":{"score":7}})),
                ] {
                    response.stop_reason = if raw {
                        StopReason::Unknown("refusal".into())
                    } else {
                        StopReason::ContentFilter
                    };
                    response.finish_reason_raw = raw.then(|| "refusal".into());
                    response.usage.cache_read_tokens = Some(3);
                    response.usage.reasoning_tokens = Some(2);
                    let mut provider = MockProvider::new("mock");
                    if reask {
                        provider = provider.enqueue_response(text_response("first invalid"));
                    }
                    let r = rig(
                        provider
                            .enqueue_response(response)
                            .enqueue_response(text_response(r#"{"score":7}"#)),
                        MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "data")),
                        vec![def("nika:read")],
                    );
                    let calls = 1 + u64::from(reask);
                    let budget = if tight { calls * 15 } else { 6000 };
                    let verb =
                        r.verb
                            .with_schema_retry_budget(if tight { u8::from(reask) } else { 2 });
                    let events = Recording::default();
                    let err = verb
                        .run_observed(typed_input(budget), &events)
                        .await
                        .expect_err("explicit refusal wins over classification");
                    assert!(matches!(err, VerbAgentError::Inference { .. }), "{err:?}");
                    assert_eq!(err.spec_code(), "NIKA-INFER-001");
                    assert!(!err.is_transient());
                    let spend = err.spend().expect("the refusal was billed");
                    assert_eq!(spend.usage.input_tokens, 10 * calls);
                    assert_eq!(spend.usage.output_tokens, 5 * calls);
                    assert_eq!(spend.usage.cache_read_tokens, Some(3));
                    assert_eq!(spend.usage.reasoning_tokens, Some(2));
                    assert_eq!(spend.model_resolved.as_deref(), Some("mock/agent"));
                    assert_eq!(r.provider.captured_requests().len() as u64, calls);
                    assert!(r.tools.captured_calls().is_empty());
                    let events = events.0.lock().unwrap();
                    let checkpoints: Vec<u64> = events
                        .iter()
                        .filter_map(|event| match event {
                            AgentEvent::BudgetCheckpoint { total_tokens, .. } => {
                                Some(*total_tokens)
                            }
                            _ => None,
                        })
                        .collect();
                    assert_eq!(checkpoints, (1..=calls).map(|n| n * 15).collect::<Vec<_>>());
                    assert!(
                        !events
                            .iter()
                            .any(|e| matches!(e, AgentEvent::Finished { .. }))
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn ordinary_refusal_like_text_is_repairable_without_a_signal() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(text_response("I cannot answer: synthetic refusal text"))
            .enqueue_response(text_response(r#"{"score":7}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let out = r
        .verb
        .run(typed_input(6000))
        .await
        .expect("ordinary repair");
    assert_eq!(r.provider.captured_requests().len(), 2);
    assert_eq!(out.usage.input_tokens, 20);
    assert_eq!(out.usage.output_tokens, 10);
}
