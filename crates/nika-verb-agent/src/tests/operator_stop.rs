// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

use super::*;
use nika_kernel::CancelCtx;
use nika_kernel::tool_executor::{StopOutcome, StoppedCall, ToolCall};
use nika_verb_invoke::{McpPlane, StoppedCallSlot};

#[tokio::test]
async fn a_genuine_stopped_mcp_call_ends_the_loop_without_another_request() {
    let cancel = CancelCtx::new();
    let operator = cancel.clone();
    let parent = StoppedCallSlot::new();
    let provider = Arc::new(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "c-stop-1",
                "mcp:s/slow",
                serde_json::json!({}),
            ))
            .enqueue_response(text_response("must not be requested")),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    assert!(invoke.install_run_stop(cancel.clone()));
    assert!(invoke.install_mcp_plane(McpPlane::stoppable(
        Arc::new(move |call: ToolCall, stop: &CancelCtx| {
            assert!(stop.begin());
            operator.cancel();
            Ok(ToolResult::stopped(&call, StopOutcome::Unknown))
        }),
        Arc::new(|| vec![def("mcp:s/slow")]),
    )));
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        invoke,
        Arc::new(MockToolDefinitionProvider::new()),
        "mock/agent",
    );
    let mut input = AgentInput::new("Read the offered source.");
    input.tools = vec!["mcp:s/slow".to_owned()];
    input.cancel = Some(cancel);
    input.stopped = Some(parent.child());
    let error = verb
        .run(input)
        .await
        .expect_err("operator control ends the loop");
    let VerbAgentError::OperatorStopped {
        call: Some(call), ..
    } = &error
    else {
        panic!("typed operator call: {error:?}");
    };
    assert_eq!(
        **call,
        StoppedCall::new("mcp:s/slow", "c-stop-1", StopOutcome::Unknown)
    );
    assert_eq!(error.spec_code(), "NIKA-CANCEL-001");
    assert!(!error.is_transient());
    assert_eq!(provider.captured_requests().len(), 1);
    let spend = error
        .spend()
        .expect("the already completed model turn stays counted");
    assert_eq!(spend.usage.input_tokens, 10);
    assert_eq!(spend.usage.output_tokens, 5);
    assert_eq!(
        parent.snapshot(),
        vec![StoppedCall::new(
            "mcp:s/slow",
            "c-stop-1",
            StopOutcome::Unknown
        )]
    );
}

#[tokio::test]
async fn an_ordinary_tool_error_mentioning_stop_remains_model_feedback() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "ordinary-1",
                "nika:read",
                serde_json::json!({}),
            ))
            .enqueue_response(text_response("the ordinary failure was explained")),
        MockToolExecutor::new().enqueue_ok(ToolResult::error(
            "ordinary-1",
            "NIKA-CANCEL-001 operator stopped",
        )),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("Explain this read.");
    input.tools = vec!["nika:read".into()];
    input.cancel = Some(CancelCtx::new());
    let output = r
        .verb
        .run(input)
        .await
        .expect("text supplies no operator control");
    assert_eq!(
        output.output,
        AgentValue::Text("the ordinary failure was explained".into())
    );
    assert_eq!(r.provider.captured_requests().len(), 2);
    assert_eq!(r.tools.captured_calls().len(), 1);
}

#[tokio::test]
async fn a_real_stop_before_the_next_turn_sends_no_model_or_tool_request() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("never requested")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let cancel = CancelCtx::new();
    cancel.cancel();
    let mut input = AgentInput::new("A later turn.");
    input.cancel = Some(cancel);
    let error = r
        .verb
        .run(input)
        .await
        .expect_err("stopped before a dispatch");
    assert!(matches!(
        error,
        VerbAgentError::OperatorStopped { call: None, .. }
    ));
    assert_eq!(r.provider.captured_requests().len(), 0);
    assert_eq!(r.tools.captured_calls().len(), 0);
}

#[tokio::test]
async fn a_stopped_batch_keeps_a_completed_tools_reported_cost() {
    let cancel = CancelCtx::new();
    let operator = cancel.clone();
    let provider = Arc::new(
        MockProvider::new("mock").enqueue_response(InferResponse::new(
            vec![
                ContentBlock::ToolUse {
                    id: "paid-1".into(),
                    name: "nika:read".into(),
                    input: serde_json::json!({}),
                },
                ContentBlock::ToolUse {
                    id: "stop-2".into(),
                    name: "mcp:s/slow".into(),
                    input: serde_json::json!({}),
                },
            ],
            usage(10, 5),
            StopReason::ToolUse,
        )),
    );
    let tools = Arc::new(
        MockToolExecutor::new().enqueue_ok(
            ToolResult::success("paid-1", "completed")
                .with_structured(serde_json::json!({"cost_usd":2.5})),
        ),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    assert!(invoke.install_run_stop(cancel.clone()));
    assert!(invoke.install_mcp_plane(McpPlane::stoppable(
        Arc::new(move |call, stop| {
            assert!(stop.begin());
            operator.cancel();
            Ok(ToolResult::stopped(&call, StopOutcome::Unknown))
        }),
        Arc::new(|| vec![def("mcp:s/slow")])
    )));
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        invoke,
        Arc::new(MockToolDefinitionProvider::with_defs(vec![def(
            "nika:read",
        )])),
        "mock/agent",
    );
    let mut input = AgentInput::new("Use both tools.");
    input.tools = vec!["nika:read".into(), "mcp:s/slow".into()];
    input.cancel = Some(cancel);
    input.stopped = Some(StoppedCallSlot::new());
    let error = verb.run(input).await.expect_err("stopped second call");
    assert!(
        matches!(&error, VerbAgentError::OperatorStopped { call: Some(call), .. } if call.call_id == "stop-2")
    );
    let spend = error.spend().expect("completed costs survive");
    assert_eq!(spend.tools_cost_usd, Some(2.5));
    assert_eq!(spend.usage.input_tokens, 10);
    assert_eq!(spend.usage.output_tokens, 5);
    assert_eq!(tools.captured_calls().len(), 1);
    assert_eq!(provider.captured_requests().len(), 1);
}

#[tokio::test]
async fn a_complete_mcp_batch_uses_the_normal_cost_fold_without_double_counting_receipts() {
    let parent = StoppedCallSlot::new();
    let provider = Arc::new(
        MockProvider::new("mock")
            .enqueue_response(InferResponse::new(
                vec![
                    ContentBlock::ToolUse {
                        id: "paid-0".into(),
                        name: "mcp:s/paid".into(),
                        input: serde_json::json!({}),
                    },
                    ContentBlock::ToolUse {
                        id: "paid-1".into(),
                        name: "mcp:s/paid".into(),
                        input: serde_json::json!({}),
                    },
                ],
                usage(10, 5),
                StopReason::ToolUse,
            ))
            .enqueue_response(text_response("completed both")),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    assert!(invoke.install_mcp_plane(McpPlane::new(
        Arc::new(|call| {
            Ok(ToolResult::success(call.id.to_string(), "completed")
                .with_structured(serde_json::json!({"cost_usd":1.25})))
        }),
        Arc::new(|| vec![def("mcp:s/paid")])
    )));
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        invoke,
        Arc::new(MockToolDefinitionProvider::new()),
        "mock/agent",
    );
    let mut input = AgentInput::new("Use both paid tools.");
    input.tools = vec!["mcp:s/paid".into()];
    input.stopped = Some(parent.child());
    let output = verb.run(input).await.expect("a complete batch");
    assert_eq!(output.tools_cost_usd, Some(2.5));
    assert_eq!(parent.reported_cost_usd(), Some(2.5));
    assert_eq!(parent.snapshot(), Vec::<StoppedCall>::new());
    assert_eq!(provider.captured_requests().len(), 2);
}

#[tokio::test]
async fn repeated_textual_tool_ids_on_distinct_physical_turns_each_keep_their_cost() {
    let parent = StoppedCallSlot::new();
    let provider = Arc::new(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "reused",
                "mcp:s/paid",
                serde_json::json!({"turn":1}),
            ))
            .enqueue_response(tool_use_response(
                "reused",
                "mcp:s/paid",
                serde_json::json!({"turn":2}),
            ))
            .enqueue_response(text_response("completed both turns")),
    );
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    assert!(invoke.install_mcp_plane(McpPlane::new(
        Arc::new(|call| {
            Ok(ToolResult::success(call.id.to_string(), "completed")
                .with_structured(serde_json::json!({"cost_usd":2.5})))
        }),
        Arc::new(|| vec![def("mcp:s/paid")])
    )));
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        invoke,
        Arc::new(MockToolDefinitionProvider::new()),
        "mock/agent",
    );
    let mut input = AgentInput::new("Read the two offered source versions.");
    input.tools = vec!["mcp:s/paid".into()];
    input.stopped = Some(parent.child());
    let output = verb
        .run(input)
        .await
        .expect("two physically distinct turns");
    assert_eq!(output.tools_cost_usd, Some(5.0));
    assert_eq!(parent.reported_cost_usd(), Some(5.0));
    assert_eq!(parent.snapshot(), Vec::<StoppedCall>::new());
    assert_eq!(provider.captured_requests().len(), 3);
}
