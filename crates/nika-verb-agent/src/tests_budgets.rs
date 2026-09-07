// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Cumulative budgets on typed final answers. Scripts fix the spend and
//! capture the requests independently of the loop's counters: an invalid
//! answer still needs work, while an already valid conclusion must win.

use std::sync::Mutex;
use std::time::Duration;

use super::*;
use crate::tests::{def, rig, text_response, tool_use_response, usage};
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::ProviderError;
use nika_kernel::runtime::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockToolExecutor};

#[derive(Default)]
struct Recording(Mutex<Vec<AgentEvent>>);

impl AgentObserver for Recording {
    fn on_event(&self, event: &AgentEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

impl Recording {
    fn checkpoints(&self) -> Vec<(u32, u64)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                AgentEvent::BudgetCheckpoint {
                    turn, total_tokens, ..
                } => Some((*turn, *total_tokens)),
                _ => None,
            })
            .collect()
    }

    fn finished(&self) -> Vec<(u32, u64)> {
        self.0
            .lock()
            .unwrap()
            .iter()
            .filter_map(|event| match event {
                AgentEvent::Finished {
                    turns,
                    total_tokens,
                } => Some((*turns, *total_tokens)),
                _ => None,
            })
            .collect()
    }
}

fn typed_input(budget: u64) -> AgentInput {
    let mut input = AgentInput::new("return the integer score");
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"score": {"type": "integer"}},
        "required": ["score"],
        "additionalProperties": false
    }));
    input.max_tokens_total = Some(budget);
    input.tools = vec![DONE_TOOL.to_owned()];
    input.timeout = Some(Duration::from_secs(43));
    input
}

fn metered(mut response: InferResponse, input: u64, output: u64) -> InferResponse {
    response.usage = usage(input, output);
    response
}

fn assert_token_stop(err: &VerbAgentError, tokens: u64, input: u64, output: u64) {
    assert!(
        matches!(err, VerbAgentError::MaxTokens { total_tokens, .. } if *total_tokens == tokens),
        "expected token stop at {tokens}, got {err:?}"
    );
    assert_eq!(err.nika_code().num, 461);
    assert_eq!(err.spec_code(), "NIKA-AGENT-002");
    let spend = err.spend().expect("all observed response usage survives");
    assert_eq!(spend.usage.input_tokens, input);
    assert_eq!(spend.usage.output_tokens, output);
    assert_eq!(spend.model_resolved.as_deref(), Some("mock/agent"));
}

async fn exhausted_final_text_never_reasks(first: InferResponse) {
    // The first answer spends 11+6=17. Exact, exceeded and zero
    // ceilings all forbid an additional request for an invalid answer.
    for budget in [17, 16, 0] {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(metered(first.clone(), 11, 6))
                .enqueue_response(text_response(r#"{"score":7}"#)),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let events = Recording::default();
        let result = r.verb.run_observed(typed_input(budget), &events).await;
        assert_eq!(
            r.provider.captured_requests().len(),
            1,
            "budget {budget}: an invalid typed answer cannot buy a repair"
        );
        assert_token_stop(&result.expect_err("no typed conclusion"), 17, 11, 6);
        assert!(r.tools.captured_calls().is_empty());
        assert_eq!(events.checkpoints(), [(1, 17)]);
        assert!(events.finished().is_empty());
    }
}

#[tokio::test]
async fn exhausted_natural_text_never_reasks() {
    exhausted_final_text_never_reasks(text_response("score is seven")).await;
}

#[tokio::test]
async fn exhausted_resultless_done_never_reasks() {
    exhausted_final_text_never_reasks(tool_use_response("done", DONE_TOOL, serde_json::json!({})))
        .await;
}

#[tokio::test]
async fn exhausted_string_done_never_reasks() {
    exhausted_final_text_never_reasks(tool_use_response(
        "done",
        DONE_TOOL,
        serde_json::json!({"result": "score is seven"}),
    ))
    .await;
}

#[tokio::test]
async fn exhausted_null_done_never_reasks() {
    exhausted_final_text_never_reasks(tool_use_response(
        "done",
        DONE_TOOL,
        serde_json::json!({"result": null}),
    ))
    .await;
}

async fn assert_done_partial_text(result: serde_json::Value, mixed: bool) {
    let expected = "collected eight rows; the ninth needs validation";
    let mut done = tool_use_response("done", DONE_TOOL, serde_json::json!({"result": result}));
    done.content[0] = ContentBlock::Text {
        text: expected.to_owned(),
    };
    let mut provider = MockProvider::new("mock");
    if mixed {
        provider = provider.enqueue_response(metered(
            tool_use_response(
                "bad",
                DONE_TOOL,
                serde_json::json!({"result":{"score":false}}),
            ),
            11,
            6,
        ));
    }
    let (input_tokens, output_tokens, total, calls) = if mixed {
        (13, 10, 40, 2)
    } else {
        (11, 6, 17, 1)
    };
    let r = rig(
        provider
            .enqueue_response(metered(done, input_tokens, output_tokens))
            .enqueue_response(text_response(r#"{"score":7}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let events = Recording::default();
    let err = r
        .verb
        .run_observed(typed_input(total), &events)
        .await
        .expect_err("the budget forbids another request");
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), calls);
    assert!(
        reqs.iter()
            .all(|q| q.timeout == Some(Duration::from_secs(43)))
    );
    let (spent_input, spent_output) = if mixed { (24, 16) } else { (11, 6) };
    assert_token_stop(&err, total, spent_input, spent_output);
    let checkpoints = if mixed {
        vec![(1, 17), (2, 40)]
    } else {
        vec![(1, 17)]
    };
    assert_eq!(events.checkpoints(), checkpoints);
    assert!(events.finished().is_empty());
    assert!(r.tools.captured_calls().is_empty());
    let partial_output = match err {
        VerbAgentError::MaxTokens { partial_output, .. } => Some(partial_output),
        _ => None,
    };
    assert_eq!(
        partial_output.as_deref(),
        Some(expected),
        "preserve prose, not the done candidate"
    );
}

#[tokio::test]
async fn null_done_budget_error_preserves_assistant_text() {
    assert_done_partial_text(serde_json::Value::Null, false).await;
}

#[tokio::test]
async fn string_done_budget_error_preserves_assistant_text() {
    assert_done_partial_text(serde_json::json!("unusable result string"), false).await;
}

#[tokio::test]
async fn mixed_done_budget_error_preserves_latest_assistant_text() {
    for result in [
        serde_json::Value::Null,
        serde_json::json!("unusable result string"),
    ] {
        assert_done_partial_text(result, true).await;
    }
}

#[tokio::test]
async fn successive_reasks_replace_the_budget_error_text() {
    for latest in ["the newest partial answer", ""] {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(metered(
                    tool_use_response("done", DONE_TOOL, serde_json::json!({"result": null})),
                    11,
                    6,
                ))
                .enqueue_response(metered(text_response("first reask prose"), 13, 10))
                .enqueue_response(metered(text_response(latest), 19, 12))
                .enqueue_response(text_response(r#"{"score":7}"#)),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let events = Recording::default();
        let err = r
            .verb
            .with_schema_retry_budget(3)
            .run_observed(typed_input(71), &events)
            .await
            .expect_err("no fourth call");
        assert_token_stop(&err, 71, 43, 28);
        let reqs = r.provider.captured_requests();
        assert_eq!(reqs.len(), 3);
        assert!(reqs[1..].iter().all(|q| q.tools.is_empty()));
        assert!(
            reqs.iter()
                .all(|q| q.timeout == Some(Duration::from_secs(43)))
        );
        assert_eq!(events.checkpoints(), [(1, 17), (1, 40), (1, 71)]);
        assert!(events.finished().is_empty());
        assert!(r.tools.captured_calls().is_empty());
        assert!(
            matches!(err, VerbAgentError::MaxTokens { partial_output, .. } if partial_output == latest)
        );
    }
}

#[tokio::test]
async fn every_text_repair_checks_the_accumulated_usage() {
    // 17 initially + 23 on the first repair = 40. The queued valid
    // answer is a trap: it must never be requested at the boundary.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(metered(text_response("initial prose"), 11, 6))
            .enqueue_response(metered(text_response("still prose"), 13, 10))
            .enqueue_response(text_response(r#"{"score":7}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let events = Recording::default();
    let result = r.verb.run_observed(typed_input(40), &events).await;
    assert_eq!(r.provider.captured_requests().len(), 2);
    let err = result.expect_err("no third request at the cumulative ceiling");
    assert_token_stop(&err, 40, 24, 16);
    assert!(
        matches!(err, VerbAgentError::MaxTokens { partial_output, .. } if partial_output == "still prose")
    );
    assert_eq!(events.checkpoints(), [(1, 17), (1, 40)]);
    assert!(events.finished().is_empty());
    assert!(r.tools.captured_calls().is_empty());
}

#[tokio::test]
async fn done_then_text_repairs_share_the_cumulative_token_ceiling() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(metered(
                tool_use_response(
                    "bad",
                    DONE_TOOL,
                    serde_json::json!({"result":{"score":"bad"}}),
                ),
                11,
                6,
            ))
            .enqueue_response(metered(text_response("unfinished prose"), 13, 10))
            .enqueue_response(text_response(r#"{"score":7}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let events = Recording::default();
    let result = r.verb.run_observed(typed_input(40), &events).await;
    assert_eq!(r.provider.captured_requests().len(), 2);
    assert_token_stop(
        &result.expect_err("no text repair after done spent the budget"),
        40,
        24,
        16,
    );
    assert_eq!(events.checkpoints(), [(1, 17), (2, 40)]);
    assert!(events.finished().is_empty());
    assert!(r.tools.captured_calls().is_empty());
}

#[tokio::test]
async fn a_valid_typed_conclusion_wins_at_or_above_both_budgets() {
    let expected = serde_json::json!({"score": 7});
    for first in [
        text_response(r#"{"score":7}"#),
        tool_use_response("done", DONE_TOOL, serde_json::json!({"result": expected})),
        tool_use_response(
            "done",
            DONE_TOOL,
            serde_json::json!({"result": r#"{"score":7}"#}),
        ),
    ] {
        for budget in [17, 16, 0] {
            let r = rig(
                MockProvider::new("mock").enqueue_response(metered(first.clone(), 11, 6)),
                MockToolExecutor::new(),
                Vec::new(),
            );
            let events = Recording::default();
            let mut input = typed_input(budget);
            input.max_turns = Some(1);
            let out = r
                .verb
                .run_observed(input, &events)
                .await
                .expect("valid conclusion wins");
            assert_eq!(out.output, AgentValue::Structured(expected.clone()));
            assert_eq!((out.turns, out.total_tokens), (1, 17));
            assert_eq!((out.usage.input_tokens, out.usage.output_tokens), (11, 6));
            assert_eq!(r.provider.captured_requests().len(), 1);
            assert_eq!(events.finished(), [(1, 17)]);
        }
    }
}

#[tokio::test]
async fn a_text_repair_below_budget_can_complete_over_budget() {
    let r = rig(
        MockProvider::new("mock")
            .with_response_format_support(true)
            .enqueue_response(metered(text_response("prose"), 11, 6))
            .enqueue_response(metered(text_response(r#"{"score":7}"#), 13, 10)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let events = Recording::default();
    let out = r
        .verb
        .run_observed(typed_input(18), &events)
        .await
        .expect("one token left allows repair");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"score":7}))
    );
    assert_eq!(out.total_tokens, 40);
    assert_eq!((out.usage.input_tokens, out.usage.output_tokens), (24, 16));
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), 2);
    assert!(reqs[1].tools.is_empty());
    assert!(matches!(
        reqs[1].response_format,
        nika_kernel::ai::provider::ResponseFormat::JsonSchema(_)
    ));
    assert!(
        reqs.iter()
            .all(|r| r.timeout == Some(Duration::from_secs(43)))
    );
    assert_eq!(events.checkpoints(), [(1, 17), (1, 40)]);
    // Existing telemetry counts tool-loop turns, not tools-off repairs.
    assert_eq!(out.turns, 1);
    assert_eq!(events.finished(), [(1, 40)]);
}

#[tokio::test]
async fn repair_allowance_exhaustion_remains_a_schema_error() {
    // No repair remains: validate as-is, regardless of token budget.
    // The token gate governs additional requests, not error rewriting.
    for allowance in [0, 1, 2] {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(metered(text_response("prose"), 11, 6))
                .enqueue_response(metered(text_response("prose"), 13, 10))
                .enqueue_response(metered(text_response("prose"), 19, 12)),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let verb = r.verb.with_schema_retry_budget(allowance);
        let budget = [17, 40, 71][usize::from(allowance)];
        let err = verb
            .run(typed_input(budget))
            .await
            .expect_err("schema never conformed");
        assert!(
            matches!(&err, VerbAgentError::SchemaValidation { .. }),
            "{err:?}"
        );
        assert_eq!(
            r.provider.captured_requests().len(),
            usize::from(allowance) + 1
        );
        let spend = err.spend().expect("usage retained");
        assert_eq!(spend.usage.input_tokens + spend.usage.output_tokens, budget);
    }
}

#[tokio::test]
async fn a_repair_provider_failure_retains_prior_usage_without_finished() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(metered(text_response("prose"), 11, 6))
            .enqueue_error(ProviderError::Api {
                status: 503,
                message: "scripted outage".to_owned(),
            }),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let events = Recording::default();
    let err = r
        .verb
        .run_observed(typed_input(18), &events)
        .await
        .expect_err("provider failed");
    assert!(matches!(err, VerbAgentError::Inference { .. }));
    let spend = err.spend().expect("first response usage retained");
    assert_eq!(
        (spend.usage.input_tokens, spend.usage.output_tokens),
        (11, 6)
    );
    assert_eq!(r.provider.captured_requests().len(), 2);
    assert_eq!(events.checkpoints(), [(1, 17)]);
    assert!(events.finished().is_empty());
}

#[tokio::test]
async fn the_last_ordinary_turn_never_dispatches_an_unconsumed_tool() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(metered(
            tool_use_response("read", "nika:read", serde_json::json!({})),
            11,
            6,
        )),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("read", "unused")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read");
    input.tools = vec!["nika:read".to_owned()];
    input.max_turns = Some(1);
    let events = Recording::default();
    let err = r
        .verb
        .run_observed(input, &events)
        .await
        .expect_err("turn budget");
    assert!(matches!(err, VerbAgentError::MaxTurns { turns: 1, .. }));
    assert_eq!(err.spec_code(), "NIKA-AGENT-001");
    assert_eq!(r.provider.captured_requests().len(), 1);
    assert!(r.tools.captured_calls().is_empty());
    assert_eq!(events.checkpoints(), [(1, 17)]);
    assert!(events.finished().is_empty());
}
