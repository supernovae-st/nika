// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
use super::*;
use crate::tests::{def, rig, text_response, tool_use_response};
use nika_kernel::runtime::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockToolExecutor};
use nika_types::cost::{Cost, InferenceCall, InferenceRoute};

fn observed(mut response: InferResponse, endpoint: &str, cost: Option<Cost>) -> InferResponse {
    let mut call = InferenceCall::new();
    call.route = Some(InferenceRoute::new(
        "fixture".into(),
        "selected".into(),
        endpoint.into(),
    ));
    call.response_model = Some("selected".into());
    call.usage = Some(response.usage.clone());
    call.usage_complete = true;
    call.pricing = Some(r#"{"kind":"test_fixture"}"#.into());
    call.estimated_usd = cost;
    response.inference_calls = vec![call];
    response
}

#[tokio::test]
async fn s80_successful_agent_carries_observed_route() {
    let response = observed(
        text_response("done"),
        "https://one.example/v1/chat/completions",
        Some(Cost::new(20)),
    );
    let expected = response.inference_calls.clone();
    let r = rig(
        MockProvider::new("mock").enqueue_response(response),
        MockToolExecutor::new(),
        vec![],
    );
    let out = r.verb.run(AgentInput::new("finish")).await.expect("agent");
    assert_eq!(out.inference_calls, expected);
}

#[tokio::test]
async fn s80_agent_route_switch_is_not_collapsed_to_requested_model() {
    let one = observed(
        tool_use_response("c", "nika:read", serde_json::json!({})),
        "https://one.example/v1/chat/completions",
        Some(Cost::new(20)),
    );
    let two = observed(
        text_response("done"),
        "https://two.example/v1/chat/completions",
        None,
    );
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(one)
            .enqueue_response(two),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "data")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read then finish");
    input.tools = vec!["nika:read".into()];
    let out = r.verb.run(input).await.expect("agent");
    assert_eq!(out.inference_calls.len(), 2);
    assert_ne!(out.inference_calls[0].route, out.inference_calls[1].route);
    assert_eq!(out.inference_calls[0].known_estimate(), Some(Cost::new(20)));
    assert_eq!(out.inference_calls[1].known_estimate(), None);
}

#[tokio::test]
async fn s80_failed_agent_preserves_completed_call_failed_call_and_known_tools() {
    let one = observed(
        tool_use_response("c", "nika:read", serde_json::json!({})),
        "https://one.example/v1/chat/completions",
        Some(Cost::new(20)),
    );
    let mut failed = InferenceCall::new();
    failed.requested_endpoint = Some("https://two.example/v1/chat/completions".into());
    let error = ProviderError::Connection {
        reason: "interrupted".into(),
    }
    .with_inference_calls(vec![failed.clone()]);
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(one)
            .enqueue_error(error),
        MockToolExecutor::new().enqueue_ok(
            ToolResult::success("c", "data").with_structured(serde_json::json!({"cost_usd":0.25})),
        ),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read then finish");
    input.tools = vec!["nika:read".into()];
    let error = r.verb.run(input).await.expect_err("second call fails");
    let spend = error.spend().expect("observed spend");
    assert_eq!(spend.inference_calls.len(), 2);
    assert_eq!(spend.inference_calls[1], failed);
    assert_eq!(spend.tools_cost_usd, Some(0.25));
}
