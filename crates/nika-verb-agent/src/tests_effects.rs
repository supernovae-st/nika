// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The loop's effect memory (#1470): an effectful tool call that settled
//! this run — a keyless `nika:fetch` POST, a destructive MCP tool — is
//! never re-issued with the same arguments; the model is told why instead.

use std::sync::Mutex;

use super::*;
use crate::tests::{def, rig, text_response, tool_use_response};
use nika_kernel::runtime::tool_executor::{ToolExecError, ToolResult};
use nika_kernel_mock::{MockProvider, MockToolExecutor};

#[derive(Default)]
struct Recording(Mutex<Vec<AgentEvent>>);

impl AgentObserver for Recording {
    fn on_event(&self, event: &AgentEvent) {
        self.0.lock().unwrap().push(event.clone());
    }
}

fn post(id: &str, body: &str) -> InferResponse {
    tool_use_response(
        id,
        "nika:fetch",
        serde_json::json!({"url": "https://api.example.com/charge", "method": "POST", "body": {"amount": body}}),
    )
}

fn get(id: &str) -> InferResponse {
    tool_use_response(
        id,
        "nika:fetch",
        serde_json::json!({"url": "https://api.example.com/status"}),
    )
}

fn fetch_input() -> AgentInput {
    let mut input = AgentInput::new("charge the card once");
    input.tools = vec!["nika:fetch".to_owned()];
    input
}

fn refused_blocks(reqs: &[InferRequest]) -> Vec<String> {
    reqs.iter()
        .flat_map(|r| r.messages.iter())
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult {
                content,
                is_error: true,
                ..
            } if content.starts_with("[effect-guard]") => Some(content.clone()),
            _ => None,
        })
        .collect()
}

fn refusals(events: &Recording) -> Vec<(u32, String)> {
    events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter_map(|e| match e {
            AgentEvent::EffectReplayRefused { turn, name } => Some((*turn, name.clone())),
            _ => None,
        })
        .collect()
}

fn completed(events: &Recording) -> usize {
    events
        .0
        .lock()
        .unwrap()
        .iter()
        .filter(|e| matches!(e, AgentEvent::ToolCompleted { .. }))
        .count()
}

/// THE issue's shape: the POST fails non-retryably at the lower layer
/// (the runtime would never retry it) and the model re-issues the very
/// same call. The loop refuses the replay in place — one dispatch, the
/// refusal fed back as an error block, the decision on the observer —
/// and the run still concludes.
#[tokio::test]
async fn a_settled_effectful_call_is_not_replayed_within_the_run() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(post("c1", "9.99"))
            .enqueue_response(post("c2", "9.99"))
            .enqueue_response(text_response("charged once")),
        MockToolExecutor::new()
            .enqueue_err(ToolExecError::ExecutionFailed {
                name: "nika:fetch".to_owned(),
                reason: "502 from the gateway".to_owned(),
            })
            .enqueue_ok(ToolResult::success("c2", "must never be reached")),
        vec![def("nika:fetch")],
    );
    let events = Recording::default();
    let out = r
        .verb
        .run_observed(fetch_input(), &events)
        .await
        .expect("the refusal is feedback, never fatal");
    assert_eq!(out.turns, 3);
    assert_eq!(
        r.tools.captured_calls().len(),
        1,
        "the replay never reaches the executor"
    );
    let refused = refused_blocks(&r.provider.captured_requests());
    assert_eq!(refused.len(), 1, "one refusal block fed back: {refused:?}");
    assert!(refused[0].contains("`nika:fetch`") && refused[0].contains("NOT re-issued"));
    assert_eq!(refusals(&events), [(2, "nika:fetch".to_owned())]);
    assert_eq!(completed(&events), 1, "a refused call is not a dispatch");
}

/// A successful effect is remembered too — replaying a charge that went
/// through is exactly the double the law exists to prevent.
#[tokio::test]
async fn a_successful_effect_is_remembered_too() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(post("c1", "9.99"))
            .enqueue_response(post("c2", "9.99"))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("c1", "charged"))
            .enqueue_ok(ToolResult::success("c2", "must never be reached")),
        vec![def("nika:fetch")],
    );
    let events = Recording::default();
    r.verb
        .run_observed(fetch_input(), &events)
        .await
        .expect("completes");
    assert_eq!(r.tools.captured_calls().len(), 1);
    assert_eq!(refusals(&events), [(2, "nika:fetch".to_owned())]);
}

/// Changed arguments are a NEW effect (a second, different charge is the
/// model's decision, not a replay) — dispatched.
#[tokio::test]
async fn a_changed_argument_is_a_new_effect() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(post("c1", "9.99"))
            .enqueue_response(post("c2", "19.99"))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("c1", "charged"))
            .enqueue_ok(ToolResult::success("c2", "charged")),
        vec![def("nika:fetch")],
    );
    let events = Recording::default();
    r.verb
        .run_observed(fetch_input(), &events)
        .await
        .expect("completes");
    assert_eq!(r.tools.captured_calls().len(), 2);
    assert!(refusals(&events).is_empty());
    assert!(refused_blocks(&r.provider.captured_requests()).is_empty());
}

/// Replay-free calls stay free: an identical GET (polling) and an
/// identical read dispatch every time — the memory holds effects only.
#[tokio::test]
async fn replay_free_calls_dispatch_every_time() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(get("c1"))
            .enqueue_response(get("c2"))
            .enqueue_response(tool_use_response(
                "c3",
                "nika:read",
                serde_json::json!({"path": "a"}),
            ))
            .enqueue_response(tool_use_response(
                "c4",
                "nika:read",
                serde_json::json!({"path": "a"}),
            ))
            .enqueue_response(text_response("done")),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("c1", "pending"))
            .enqueue_ok(ToolResult::success("c2", "ready"))
            .enqueue_ok(ToolResult::success("c3", "x"))
            .enqueue_ok(ToolResult::success("c4", "x")),
        vec![def("nika:fetch"), def("nika:read")],
    );
    let mut input = fetch_input();
    input.tools.push("nika:read".to_owned());
    let events = Recording::default();
    r.verb
        .run_observed(input, &events)
        .await
        .expect("completes");
    assert_eq!(r.tools.captured_calls().len(), 4);
    assert!(refusals(&events).is_empty());
}
