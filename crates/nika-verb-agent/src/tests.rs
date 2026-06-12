// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unit tests for the agent loop — split from `lib.rs` under the
//! 1500-LOC file cap (the inline block crossed it at 1681 · same
//! `mod tests` semantics · `super::*` reaches the crate root).

use super::*;
use nika_kernel::ai::provider::{InferResponse, StopReason, TokenUsage};
use nika_kernel::runtime::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockToolDefinitionProvider, MockToolExecutor};

fn usage(input: u64, output: u64) -> TokenUsage {
    let mut usage = TokenUsage::default();
    usage.input_tokens = input;
    usage.output_tokens = output;
    usage
}

fn text_response(text: &str) -> InferResponse {
    InferResponse::new(
        vec![ContentBlock::Text {
            text: text.to_owned(),
        }],
        usage(10, 5),
        StopReason::EndTurn,
    )
}

fn tool_use_response(id: &str, name: &str, args: serde_json::Value) -> InferResponse {
    InferResponse::new(
        vec![
            ContentBlock::Text {
                text: format!("calling {name}"),
            },
            ContentBlock::ToolUse {
                id: id.to_owned(),
                name: name.to_owned(),
                input: args,
            },
        ],
        usage(10, 5),
        StopReason::ToolUse,
    )
}

fn def(name: &str) -> ToolDef {
    ToolDef::new(name, format!("{name} description"), serde_json::json!({}))
}

struct Rig {
    provider: Arc<MockProvider>,
    tools: Arc<MockToolExecutor>,
    verb: AgentVerb<MockProvider, MockToolExecutor, MockToolDefinitionProvider>,
}

fn rig(provider: MockProvider, tools: MockToolExecutor, universe: Vec<ToolDef>) -> Rig {
    let provider = Arc::new(provider);
    let tools = Arc::new(tools);
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&tools)));
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        invoke,
        Arc::new(MockToolDefinitionProvider::with_defs(universe)),
        "mock/agent",
    );
    Rig {
        provider,
        tools,
        verb,
    }
}

// ── §6 · single-turn no-tools → Completed ──────────────────────────

#[tokio::test]
async fn no_tools_single_turn_completes() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("the answer is 4")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let out = r
        .verb
        .run(AgentInput::new("2+2?"))
        .await
        .expect("completes");
    assert_eq!(out.output, AgentValue::Text("the answer is 4".to_owned()));
    assert_eq!(out.stop_reason, AgentStopReason::Completed);
    assert_eq!(out.turns, 1);
    assert_eq!(out.total_tokens, 15);
    // Pure conversation: the model received NO tools.
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), 1);
    assert!(reqs[0].tools.is_empty());
}

// ── §6 · tool-use → dispatch → feed-back → final ────────────────────

#[tokio::test]
async fn tool_use_dispatches_feeds_back_then_completes() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "call-1",
                "nika:read",
                serde_json::json!({"path": "./notes.md"}),
            ))
            .enqueue_response(text_response("summary: notes say hello")),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("call-1", "hello from notes")),
        vec![def("nika:read"), def("nika:write")],
    );
    let mut input = AgentInput::new("summarize my notes");
    input.tools = vec!["nika:read".to_owned()];
    let out = r.verb.run(input).await.expect("completes");
    assert_eq!(
        out.output,
        AgentValue::Text("summary: notes say hello".to_owned())
    );
    assert_eq!(out.turns, 2);
    assert_eq!(out.total_tokens, 30, "both turns counted");

    // The dispatch reached the REAL invoke verb with the model's args.
    let calls = r.tools.captured_calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].name, "nika:read");
    assert_eq!(calls[0].id.as_str(), "call-1", "engine-supplied id rides");

    // Turn 2 saw the full transcript: tool defs (filtered to the
    // whitelist) + the assistant turn + the fed-back result.
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), 2);
    assert_eq!(reqs[0].tools.len(), 1, "universe filtered to whitelist");
    assert_eq!(reqs[0].tools[0].name, "nika:read");
    let turn2 = &reqs[1];
    assert_eq!(turn2.messages.len(), 3, "user · assistant · tool-results");
    assert!(matches!(turn2.messages[1].role, Role::Assistant));
    assert!(turn2.messages[2].content.iter().any(|b| matches!(
        b,
        ContentBlock::ToolResult { tool_use_id, content, is_error: false }
            if tool_use_id == "call-1" && content == "hello from notes"
    )));
}

// ── §6 · nika:done with / without result ───────────────────────────

#[tokio::test]
async fn done_sentinel_with_result_is_explicit_structured() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "call-done",
            DONE_TOOL,
            serde_json::json!({"result": {"answer": 4}}),
        )),
        MockToolExecutor::new(),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("finish with a value");
    input.tools = vec!["nika:*".to_owned()];
    let out = r.verb.run(input).await.expect("explicit completion");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"answer": 4}))
    );
    assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
    // The sentinel is loop-owned: NOTHING was dispatched.
    assert!(r.tools.captured_calls().is_empty());
    // And its def was synthesized into the model's tool list.
    let reqs = r.provider.captured_requests();
    assert!(reqs[0].tools.iter().any(|d| d.name == DONE_TOOL));
}

#[tokio::test]
async fn done_sentinel_without_result_finishes_with_last_text() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("c1", "nika:read", serde_json::json!({})))
            .enqueue_response(InferResponse::new(
                vec![
                    ContentBlock::Text {
                        text: "done reading — all good".to_owned(),
                    },
                    ContentBlock::ToolUse {
                        id: "c2".to_owned(),
                        name: DONE_TOOL.to_owned(),
                        input: serde_json::json!({}),
                    },
                ],
                usage(10, 5),
                StopReason::ToolUse,
            )),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "file body")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read then finish");
    input.tools = vec!["nika:read".to_owned(), DONE_TOOL.to_owned()];
    let out = r.verb.run(input).await.expect("explicit completion");
    assert_eq!(
        out.output,
        AgentValue::Text("done reading — all good".to_owned())
    );
    assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
    assert_eq!(out.turns, 2);
}

// ── §6 · budgets are failures with partial_output ───────────────────

#[tokio::test]
async fn max_turns_fails_with_partial_output() {
    let looping = tool_use_response("c", "nika:read", serde_json::json!({}));
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(looping.clone())
            .enqueue_response(looping.clone())
            .enqueue_response(looping),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("c", "1"))
            .enqueue_ok(ToolResult::success("c", "2"))
            .enqueue_ok(ToolResult::success("c", "3")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("never finishes");
    input.tools = vec!["nika:read".to_owned()];
    input.max_turns = Some(3);
    let err = r.verb.run(input).await.expect_err("budget failure");
    assert!(matches!(
        &err,
        VerbAgentError::MaxTurns { turns: 3, partial_output }
            if partial_output == "calling nika:read"
    ));
}

#[tokio::test]
async fn max_tokens_total_fails_at_the_exact_boundary_before_dispatch() {
    // Turn 1 spends exactly the budget (15) AND wants to continue (tool
    // call) → exhausted (`>=`) → MaxTokens BEFORE dispatching, with the
    // turn's text as partial_output. The `>` mutation survivor dies here.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("c", "nika:read", serde_json::json!({})))
            .enqueue_response(text_response("should never run")),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "data")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("expensive");
    input.tools = vec!["nika:read".to_owned()];
    input.max_tokens_total = Some(15); // turn 1 spends exactly 15
    let err = r.verb.run(input).await.expect_err("token budget");
    assert!(
        matches!(
            &err,
            VerbAgentError::MaxTokens { total_tokens: 15, partial_output }
                if partial_output == "calling nika:read"
        ),
        "{err}"
    );
    // The budget gate fired BEFORE the tool ran (no wasted side effect).
    assert!(r.tools.captured_calls().is_empty());
    // And the second turn was never requested.
    assert_eq!(r.provider.captured_requests().len(), 1);
}

#[tokio::test]
async fn natural_completion_over_budget_is_success_not_failure() {
    // A FINISHED answer is a success even if its turn crossed the budget
    // — budgets stop the loop from CONTINUING, they don't fail a
    // concluded run (spec §2: terminal-1 precedes the budget gate).
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("the final answer")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("one expensive answer");
    input.max_tokens_total = Some(1); // the single turn spends 15 > 1
    let out = r
        .verb
        .run(input)
        .await
        .expect("a concluded answer succeeds");
    assert_eq!(out.output, AgentValue::Text("the final answer".to_owned()));
    assert_eq!(out.stop_reason, AgentStopReason::Completed);
}

// ── multiple tool calls in ONE turn ─────────────────────────────────

#[tokio::test]
async fn multiple_tools_in_one_turn_all_dispatch_in_order() {
    let two_calls = InferResponse::new(
        vec![
            ContentBlock::ToolUse {
                id: "a".to_owned(),
                name: "nika:read".to_owned(),
                input: serde_json::json!({"path": "a.txt"}),
            },
            ContentBlock::ToolUse {
                id: "b".to_owned(),
                name: "nika:glob".to_owned(),
                input: serde_json::json!({"pattern": "*.md"}),
            },
        ],
        usage(10, 5),
        StopReason::ToolUse,
    );
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(two_calls)
            .enqueue_response(text_response("both done")),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("a", "A body"))
            .enqueue_ok(ToolResult::success("b", "b.md")),
        vec![def("nika:read"), def("nika:glob")],
    );
    let mut input = AgentInput::new("read and glob");
    input.tools = vec!["nika:read".to_owned(), "nika:glob".to_owned()];
    let out = r.verb.run(input).await.expect("completes");
    assert_eq!(out.output, AgentValue::Text("both done".to_owned()));
    let calls = r.tools.captured_calls();
    assert_eq!(calls.len(), 2, "both tools dispatched");
    assert_eq!(calls[0].name, "nika:read");
    assert_eq!(calls[1].name, "nika:glob");
    // Both results fed back in ONE user message.
    let turn2 = &r.provider.captured_requests()[1];
    let fed = &turn2.messages[2];
    assert_eq!(fed.content.len(), 2, "both tool results in one turn");
}

#[tokio::test]
async fn whitelist_violation_on_second_tool_dispatches_neither() {
    // The whole batch is validated BEFORE any dispatch: the allowed
    // first tool must NOT run when a later sibling is denied.
    let two = InferResponse::new(
        vec![
            ContentBlock::ToolUse {
                id: "a".to_owned(),
                name: "nika:read".to_owned(),
                input: serde_json::json!({}),
            },
            ContentBlock::ToolUse {
                id: "b".to_owned(),
                name: "nika:write".to_owned(),
                input: serde_json::json!({"path": "/etc/x"}),
            },
        ],
        usage(10, 5),
        StopReason::ToolUse,
    );
    let r = rig(
        MockProvider::new("mock").enqueue_response(two),
        MockToolExecutor::new()
            .enqueue_ok(ToolResult::success("a", "leaked?"))
            .enqueue_ok(ToolResult::success("b", "never")),
        vec![def("nika:read"), def("nika:write")],
    );
    let mut input = AgentInput::new("partial escape");
    input.tools = vec!["nika:read".to_owned()]; // write NOT whitelisted
    let err = r.verb.run(input).await.expect_err("security stop");
    assert!(matches!(
        &err,
        VerbAgentError::WhitelistViolation { tool } if tool == "nika:write"
    ));
    assert!(
        r.tools.captured_calls().is_empty(),
        "the allowed sibling NEVER ran — batch validated before dispatch"
    );
}

#[tokio::test]
async fn done_among_siblings_completes_without_dispatching_them() {
    // [nika:read, nika:done] in ONE turn: done wins, the sibling does
    // NOT run (no side effect on a terminating turn · no dropped result).
    let mixed = InferResponse::new(
        vec![
            ContentBlock::Text {
                text: "final words".to_owned(),
            },
            ContentBlock::ToolUse {
                id: "r".to_owned(),
                name: "nika:read".to_owned(),
                input: serde_json::json!({}),
            },
            ContentBlock::ToolUse {
                id: "d".to_owned(),
                name: DONE_TOOL.to_owned(),
                input: serde_json::json!({}),
            },
        ],
        usage(10, 5),
        StopReason::ToolUse,
    );
    let r = rig(
        MockProvider::new("mock").enqueue_response(mixed),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("r", "should not run")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read then done together");
    input.tools = vec!["nika:read".to_owned(), DONE_TOOL.to_owned()];
    let out = r.verb.run(input).await.expect("explicit completion");
    assert_eq!(out.output, AgentValue::Text("final words".to_owned()));
    assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
    assert!(
        r.tools.captured_calls().is_empty(),
        "the sibling tool never ran — done preempts the batch"
    );
}

// ── mid-loop inference failure → NIKA-463 ───────────────────────────

#[tokio::test]
async fn inference_error_mid_loop_maps_to_463() {
    use nika_kernel::ai::provider::ProviderError;
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("c", "nika:read", serde_json::json!({})))
            .enqueue_error(ProviderError::Api {
                status: 500,
                message: "upstream 500".to_owned(),
            }),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c", "data")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("fails on turn 2");
    input.tools = vec!["nika:read".to_owned()];
    let err = r.verb.run(input).await.expect_err("provider error");
    assert!(matches!(err, VerbAgentError::Inference { .. }), "{err}");
    assert_eq!(nika_error::traits::NikaErrorCode::nika_code(&err).num, 463);
}

// ── §6 · whitelist violation = immediate, zero dispatch ─────────────

#[tokio::test]
async fn whitelist_violation_fails_immediately_with_zero_dispatch() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c1",
            "nika:write",
            serde_json::json!({"path": "/etc/passwd"}),
        )),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "never")),
        vec![def("nika:read"), def("nika:write")],
    );
    let mut input = AgentInput::new("try to escape");
    input.tools = vec!["nika:read".to_owned()];
    let err = r.verb.run(input).await.expect_err("security stop");
    assert!(matches!(
        &err,
        VerbAgentError::WhitelistViolation { tool } if tool == "nika:write"
    ));
    assert!(
        r.tools.captured_calls().is_empty(),
        "the denied tool was NEVER dispatched"
    );
}

// ── §6 · tool errors are fed back, never fatal ──────────────────────

#[tokio::test]
async fn tool_error_feeds_back_and_the_loop_continues() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("c1", "nika:read", serde_json::json!({})))
            .enqueue_response(text_response("recovered without the file")),
        MockToolExecutor::new(), // empty queue → dispatch error
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("resilient");
    input.tools = vec!["nika:read".to_owned()];
    let out = r.verb.run(input).await.expect("loop survives tool failure");
    assert_eq!(
        out.output,
        AgentValue::Text("recovered without the file".to_owned())
    );
    // The fed-back result is an ERROR result carrying the NIKA code.
    let reqs = r.provider.captured_requests();
    let fed_back = &reqs[1].messages[2];
    assert!(fed_back.content.iter().any(|b| matches!(
        b,
        ContentBlock::ToolResult { is_error: true, content, .. }
            if content.starts_with("NIKA-")
    )));
}

// ── §6 · final-message schema validation ────────────────────────────

#[tokio::test]
async fn schema_validates_the_final_text_into_structured() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response(r#"{"score": 9}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"score": {"type": "integer"}},
        "required": ["score"]
    }));
    let out = r.verb.run(input).await.expect("valid");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"score": 9}))
    );
}

#[tokio::test]
async fn schema_rejects_a_nonconforming_final_message() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("not json at all")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.schema = Some(serde_json::json!({"type": "object"}));
    let err = r.verb.run(input).await.expect_err("schema gate");
    assert!(matches!(err, VerbAgentError::SchemaValidation { .. }));
}

#[tokio::test]
async fn schema_extracts_json_from_a_fenced_final_message() {
    // The infer.schema: parity — real models wrap JSON in ``` fences /
    // prose; the agent must extract the balanced span, not reject it.
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response(
            "Here is the result:\n```json\n{\"score\": 9}\n```\nHope that helps!",
        )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"score": {"type": "integer"}},
        "required": ["score"]
    }));
    let out = r.verb.run(input).await.expect("fenced JSON extracts");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"score": 9}))
    );
}

#[tokio::test]
async fn schema_validates_the_done_result_value() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"score": "nine"}}),
        )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"score": {"type": "integer"}},
        "required": ["score"]
    }));
    let err = r
        .verb
        .run(input)
        .await
        .expect_err("done result misses schema");
    assert!(matches!(err, VerbAgentError::SchemaValidation { .. }));
}

// ── §6 · seam failures + parameter validation ───────────────────────

#[tokio::test]
async fn tool_defs_failure_maps_to_466() {
    let provider = Arc::new(MockProvider::new("mock"));
    let tools = Arc::new(MockToolExecutor::new());
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(InvokeVerb::new(Arc::clone(&tools))),
        Arc::new(MockToolDefinitionProvider::unavailable("mcp down")),
        "mock/agent",
    );
    let mut input = AgentInput::new("needs tools");
    input.tools = vec!["nika:*".to_owned()];
    let err = verb.run(input).await.expect_err("seam failure");
    assert!(matches!(err, VerbAgentError::ToolDefs { .. }));
    assert_eq!(nika_error::traits::NikaErrorCode::nika_code(&err).num, 466);
    // Pure conversation never touches the seam:
    let ok = verb.run(AgentInput::new("no tools")).await;
    assert!(
        ok.is_err(),
        "provider queue empty — but NOT a ToolDefs error"
    );
    assert!(matches!(ok.unwrap_err(), VerbAgentError::Inference { .. }));
}

#[tokio::test]
async fn control_char_tool_name_is_a_redacted_violation_zero_dispatch() {
    // A model name with a newline (log-injection class) is a violation
    // by construction · the error carries a REDACTED form, never raw.
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c1",
            "nika:read\n<FAKE LOG LINE> level=error",
            serde_json::json!({}),
        )),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "never")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("inject");
    input.tools = vec!["nika:*".to_owned()];
    let err = r.verb.run(input).await.expect_err("violation");
    let VerbAgentError::WhitelistViolation { tool } = &err else {
        panic!("expected WhitelistViolation, got {err}");
    };
    assert!(
        !tool.contains('\n'),
        "the logged name is sanitized: {tool:?}"
    );
    assert!(r.tools.captured_calls().is_empty(), "nothing dispatched");
}

#[tokio::test]
async fn max_turns_above_the_ceiling_is_a_param_error() {
    let r = rig(
        MockProvider::new("mock"),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("ok");
    input.max_turns = Some(MAX_TURNS_CEILING + 1);
    let err = r.verb.run(input).await.expect_err("over ceiling");
    assert!(
        matches!(&err, VerbAgentError::InvalidParam { param, .. } if *param == "max_turns"),
        "{err}"
    );
}

#[test]
fn is_clean_tool_name_draws_the_control_char_boundary_exactly() {
    // The boundary is 0x20: control chars (< 0x20) are dirty, SPACE
    // (0x20) is clean — an interior space is unusual but not a control
    // char (the `< 0x20` vs `<= 0x20` distinction).
    assert!(is_clean_tool_name("nika:read"));
    assert!(
        is_clean_tool_name("a b"),
        "interior space is not a control char"
    );
    assert!(!is_clean_tool_name("a\tb"), "tab (0x09) is a control char");
    assert!(!is_clean_tool_name("a\nb"), "newline is a control char");
    assert!(
        !is_clean_tool_name(" a"),
        "leading whitespace rejected (trim)"
    );
    assert!(
        !is_clean_tool_name("a "),
        "trailing whitespace rejected (trim)"
    );
    assert!(!is_clean_tool_name(""), "empty rejected");
    assert!(
        !is_clean_tool_name("a\u{7f}b"),
        "DEL (0x7f) is a control char"
    );
}

#[test]
fn agent_output_new_constructs_the_non_exhaustive_struct() {
    let out = AgentOutput::new(
        AgentValue::Text("x".to_owned()),
        AgentStopReason::Completed,
        3,
        42,
    );
    assert_eq!(out.turns, 3);
    assert_eq!(out.total_tokens, 42);
}

#[tokio::test]
async fn invalid_params_are_465() {
    let r = rig(
        MockProvider::new("mock"),
        MockToolExecutor::new(),
        Vec::new(),
    );
    for (input, param) in [
        (AgentInput::new("   "), "prompt"),
        (
            {
                let mut i = AgentInput::new("ok");
                i.temperature = Some(3.0);
                i
            },
            "temperature",
        ),
        (
            {
                let mut i = AgentInput::new("ok");
                i.max_turns = Some(0);
                i
            },
            "max_turns",
        ),
    ] {
        let err = r.verb.run(input).await.expect_err(param);
        assert!(
            matches!(&err, VerbAgentError::InvalidParam { param: p, .. } if *p == param),
            "{err}"
        );
    }
}

// ── model resolution + system prompt plumbing ───────────────────────

#[tokio::test]
async fn model_override_and_system_prompt_ride_the_request() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response("ok")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("hello");
    input.model = Some("mock/special".to_owned());
    input.system = Some("be terse".to_owned());
    input.temperature = Some(0.2);
    let _ = r.verb.run(input).await.expect("completes");
    let req = &r.provider.captured_requests()[0];
    assert_eq!(req.model, "mock/special");
    assert_eq!(req.temperature, Some(0.2));
    assert!(matches!(req.messages[0].role, Role::System));
    assert!(matches!(req.messages[1].role, Role::User));
}
