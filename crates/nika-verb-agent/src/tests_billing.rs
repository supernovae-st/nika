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

/// The refusal the loop fed back to the model on its last request.
fn fed_back_error(r: &crate::tests::Rig) -> String {
    let requests = r.provider.captured_requests();
    let last = requests.last().expect("a request after the tool turn");
    last.messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|block| match block {
            ContentBlock::ToolResult {
                content,
                is_error: true,
                ..
            } => Some(content.clone()),
            _ => None,
        })
        .expect("the refusal is fed back as an error result")
}

fn draw(provider: &str, model: Option<&str>) -> InferResponse {
    let mut args = serde_json::json!({
        "provider": provider, "size": "1024x1024", "quality": "low",
        "prompt": "a star", "output_dir": "./out"
    });
    if let (Some(model), Some(map)) = (model, args.as_object_mut()) {
        map.insert("model".into(), model.into());
    }
    tool_use_response("c", "nika:image_generate", args)
}

/// C-D3 · under a cost cap, a paid media tool call the catalog cannot price
/// is refused before its request and fed back (the model can pick another
/// provider): the executor never sees it and nothing is billed. A priced
/// call whose floor exceeds what is left is refused the same way.
#[tokio::test]
async fn a_capped_loop_refuses_paid_media_the_cap_cannot_bound() {
    for (provider, model, remaining, why) in [
        ("openai", Some("gpt-image-1.5"), 5.0, "no catalog price"),
        (
            "gemini",
            Some("gemini-2.0-flash-preview-image-generation"),
            5.0,
            "no catalog price",
        ),
        ("xai", None, 0.01, "costs at least $0.020000"),
        // C-D3 · the dated table's floor: gpt-image-2 low at 1024x1024.
        ("openai", None, 0.001, "costs at least $0.006000"),
    ] {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(draw(provider, model))
                .enqueue_response(text_response("done")),
            MockToolExecutor::new(),
            vec![def("nika:image_generate")],
        );
        let mut input = AgentInput::new("draw");
        input.tools = vec!["nika:image_generate".into()];
        input.cost_remaining_usd = Some(remaining);
        let out = r.verb.run(input).await.expect("the loop goes on");
        assert!(
            r.tools.captured_calls().is_empty(),
            "{provider}: no request"
        );
        assert!(out.tools_unpriced.is_empty(), "{provider}: nothing billed");
        let fed = fed_back_error(&r);
        assert!(
            fed.starts_with("NIKA-1704")
                && fed.contains(&format!("`{provider}`"))
                && fed.contains(why),
            "{provider}: {fed}"
        );
    }
}

/// A cap leaves unmetered media and a priced call that fits untouched, and
/// no cap judges nothing: the call reaches the executor.
#[tokio::test]
async fn a_loop_dispatches_media_the_cap_bounds_or_no_cap_at_all() {
    for (provider, remaining) in [
        ("mock", Some(0.0)),
        ("xai", Some(1.0)),
        ("openai", Some(1.0)),
        ("openai", None),
    ] {
        let r = rig(
            MockProvider::new("mock")
                .enqueue_response(draw(provider, None))
                .enqueue_response(text_response("done")),
            MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "{}")),
            vec![def("nika:image_generate")],
        );
        let mut input = AgentInput::new("draw");
        input.tools = vec!["nika:image_generate".into()];
        input.cost_remaining_usd = remaining;
        r.verb.run(input).await.expect("completes");
        assert_eq!(r.tools.captured_calls().len(), 1, "{provider}: dispatched");
    }
}

/// C-D3 · a returned paid media call that reported no spend is named as
/// unpriced on the output, one entry per call, and on the failure spend of
/// a loop that dies afterwards: the run never settles `priced` over it. A
/// call that reported its `cost_usd` (xAI) is metered, never unpriced.
#[tokio::test]
async fn returned_unpriced_media_rides_the_output_and_the_failure_spend() {
    use nika_types::cost::UnpricedReason;
    let returned = |id: &str| {
        ToolResult::success(id, "{}").with_structured(serde_json::json!({"provider": "openai"}))
    };
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(draw("openai", Some("gpt-image-1.5")))
            .enqueue_response(tool_use_response(
                "x",
                "nika:image_generate",
                serde_json::json!({"provider": "xai", "prompt": "a star", "output_dir": "./out"}),
            ))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new()
            .enqueue_ok(returned("c"))
            .enqueue_ok(
                ToolResult::success("x", "{}")
                    .with_structured(serde_json::json!({"cost_usd": 0.02})),
            ),
        vec![def("nika:image_generate")],
    );
    let mut input = AgentInput::new("draw twice");
    input.tools = vec!["nika:image_generate".into()];
    let out = r.verb.run(input).await.expect("completes");
    assert_eq!(out.tools_unpriced, [UnpricedReason::MissingCatalogPrice]);
    assert_eq!(out.tools_cost_usd, Some(0.02));

    let error = ProviderError::Connection {
        reason: "interrupted".into(),
    };
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(draw("openai", Some("gpt-image-1.5")))
            .enqueue_error(error),
        MockToolExecutor::new().enqueue_ok(returned("c")),
        vec![def("nika:image_generate")],
    );
    let mut input = AgentInput::new("draw then fail");
    input.tools = vec!["nika:image_generate".into()];
    let failed = r.verb.run(input).await.expect_err("the second turn fails");
    let spend = failed.spend().expect("observed spend");
    assert_eq!(spend.tools_unpriced, [UnpricedReason::MissingCatalogPrice]);
}

/// C-D3 · a capped loop counts its own media spend: once a priced image
/// has reported most of what the cap left, the next one whose floor exceeds
/// the rest is refused before its request and fed back.
#[tokio::test]
async fn a_capped_loop_charges_its_own_media_spend_against_the_cap() {
    let billed = |id: &str| {
        ToolResult::success(id, "{}").with_structured(serde_json::json!({
            "provider": "openai", "model": "gpt-image-2", "cost_usd": 0.008,
            "cost_basis": "catalog_rate_x_usage"
        }))
    };
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(draw("openai", None))
            .enqueue_response(draw("openai", None))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new()
            .enqueue_ok(billed("c"))
            .enqueue_ok(billed("c")),
        vec![def("nika:image_generate")],
    );
    let mut input = AgentInput::new("draw twice");
    input.tools = vec!["nika:image_generate".into()];
    input.cost_remaining_usd = Some(0.01);
    input.stopped = Some(nika_verb_invoke::StoppedCallSlot::new());
    let out = r.verb.run(input).await.expect("the loop goes on");
    assert_eq!(
        r.tools.captured_calls().len(),
        1,
        "the second image is refused"
    );
    assert_eq!(
        out.tools_cost_usd,
        Some(0.008),
        "the first image is charged"
    );
    let fed = fed_back_error(&r);
    assert!(
        fed.starts_with("NIKA-1704")
            && fed.contains("costs at least $0.006000")
            && fed.contains("$0.002000 left"),
        "{fed}"
    );
}
