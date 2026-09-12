// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Explicit completion cannot be replaced by a text-only plan (#1519).
use super::*;
use crate::tests::{rig, text_response, tool_use_response};
use nika_kernel_mock::{MockProvider, MockToolExecutor};

#[tokio::test]
async fn plan_then_done_preserves_the_transcript_and_counts_both_turns() {
    for tools in [vec!["nika:done"], vec!["nika:*"]] {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(text_response("I will inspect the input."))
                .enqueue_response(tool_use_response(
                    "finished",
                    DONE_TOOL,
                    serde_json::json!({"result": 7}),
                )),
            MockToolExecutor::new(),
            Vec::new(),
        );
        let mut input = AgentInput::new("finish explicitly");
        input.tools = tools.into_iter().map(str::to_owned).collect();
        input.max_turns = Some(2);
        let out = r
            .verb
            .run(input)
            .await
            .expect("explicit done succeeds on last turn");
        assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
        assert_eq!(out.output, AgentValue::Structured(serde_json::json!(7)));
        assert_eq!(out.turns, 2);
        assert_eq!(out.total_tokens, 30);
        assert!(r.tools.captured_calls().is_empty());
        let requests = r.provider.captured_requests();
        assert_eq!(requests.len(), 2);
        let messages = &requests[1].messages;
        assert_eq!(messages[messages.len() - 2].role, Role::Assistant);
        assert_eq!(messages[messages.len() - 1].role, Role::User);
        assert!(
            matches!(&messages[messages.len()-2].content[0], ContentBlock::Text { text } if text == "I will inspect the input.")
        );
        assert!(
            matches!(&messages[messages.len()-1].content[0], ContentBlock::Text { text } if text.contains("nika:done"))
        );
        assert!(requests[1].tools.iter().any(|tool| tool.name == DONE_TOOL));
    }
}

#[tokio::test]
async fn repeated_plans_exhaust_turns_without_an_extra_request() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(text_response("first plan"))
            .enqueue_response(text_response("second plan")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("finish explicitly");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.max_turns = Some(2);
    let err = r
        .verb
        .run(input)
        .await
        .expect_err("plans are not completion");
    assert!(
        matches!(err, VerbAgentError::MaxTurns { turns: 2, partial_output, .. } if partial_output == "second plan")
    );
    assert_eq!(r.provider.captured_requests().len(), 2);
    assert!(r.tools.captured_calls().is_empty());
}

#[tokio::test]
async fn text_continuation_respects_the_token_boundary() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("plan at the limit")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("finish explicitly");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.max_tokens_total = Some(15);
    let err = r
        .verb
        .run(input)
        .await
        .expect_err("no token allowance for a continuation");
    assert!(
        matches!(err, VerbAgentError::MaxTokens { total_tokens: 15, partial_output, .. } if partial_output == "plan at the limit")
    );
    assert_eq!(r.provider.captured_requests().len(), 1);
}

#[tokio::test]
async fn excluded_done_keeps_natural_completion() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("finished")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("answer in text");
    input.tools = vec!["nika:*".to_owned(), "!nika:done".to_owned()];
    input.max_turns = Some(1);
    input.max_tokens_total = Some(15);
    let out = r
        .verb
        .run(input)
        .await
        .expect("completed text can finish at the budget boundary");
    assert_eq!(out.stop_reason, AgentStopReason::Completed);
    assert_eq!(out.output, AgentValue::Text("finished".to_owned()));
    assert_eq!(r.provider.captured_requests().len(), 1);
}

#[tokio::test]
async fn a_plan_does_not_spend_the_schema_repair_allowance() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(text_response("I will produce the object."))
            .enqueue_response(tool_use_response(
                "bad",
                DONE_TOOL,
                serde_json::json!({"result": {"score": "bad"}}),
            ))
            .enqueue_response(tool_use_response(
                "good",
                DONE_TOOL,
                serde_json::json!({"result": {"score": 7}}),
            )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("return an integer");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(
        serde_json::json!({"type": "object", "properties": {"score": {"type": "integer"}}, "required": ["score"]}),
    );
    let out = r
        .verb
        .with_schema_retry_budget(1)
        .run(input)
        .await
        .expect("one schema repair remains after the plan");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"score": 7}))
    );
    assert_eq!(out.turns, 3);
    assert_eq!(r.provider.captured_requests().len(), 3);
}
