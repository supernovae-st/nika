// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Unit tests for the agent loop — split from `lib.rs` under the
//! 1500-LOC file cap (the inline block crossed it at 1681 · same
//! `mod tests` semantics · `super::*` reaches the crate root).

use super::*;
use nika_error::traits::NikaErrorCode;
use nika_kernel::ai::provider::{InferResponse, ResponseFormat, StopReason, TokenUsage};
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

/// BUG#3 boundary: a tool that ALSO returns a structured value still feeds
/// the model the `content` STRING — the structured plane is invoke-dataflow
/// only, never the LLM's view. (If the agent leaked `structured`, the model
/// would read JSON where it expects text.)
#[tokio::test]
async fn agent_feeds_the_content_string_even_when_the_tool_is_structured() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "call-1",
                "nika:glob",
                serde_json::json!({"pattern": "./**/*.md"}),
            ))
            .enqueue_response(text_response("found two files")),
        // A glob-style structured result: content is the JSON TEXT view, the
        // structured plane carries the real array (the invoke dataflow's signal).
        MockToolExecutor::new().enqueue_ok(
            ToolResult::success("call-1", r#"["a.md","b.md"]"#)
                .with_structured(serde_json::json!(["a.md", "b.md"])),
        ),
        vec![def("nika:glob")],
    );
    let mut input = AgentInput::new("list my docs");
    input.tools = vec!["nika:glob".to_owned()];
    let out = r.verb.run(input).await.expect("completes");
    assert_eq!(out.output, AgentValue::Text("found two files".to_owned()));
    assert_eq!(
        out.tools_cost_usd, None,
        "no tool reported spend — absent, never a fake zero"
    );

    // The model's fed-back ToolResult block carries the CONTENT string —
    // the rendered text, NOT the structured array.
    let reqs = r.provider.captured_requests();
    let turn2 = &reqs[1];
    assert!(
        turn2.messages[2].content.iter().any(|b| matches!(
            b,
            ContentBlock::ToolResult { tool_use_id, content, is_error: false }
                if tool_use_id == "call-1" && content == r#"["a.md","b.md"]"#
        )),
        "the LLM reads the content String · never the structured value"
    );
}

/// The honest-spend channel through the LOOP: a tool whose structured
/// output carries a top-level `cost_usd` (the image builtin on
/// tick-billed providers) accumulates into `tools_cost_usd` — an
/// agent-driven $0.02 render must never surface as $0.00.
#[tokio::test]
async fn agent_accumulates_tool_reported_cost() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "call-1",
                "nika:image_generate",
                serde_json::json!({"prompt": "logo", "output_dir": "./out"}),
            ))
            .enqueue_response(tool_use_response(
                "call-2",
                "nika:image_generate",
                serde_json::json!({"prompt": "logo v2", "output_dir": "./out"}),
            ))
            .enqueue_response(text_response("both rendered")),
        MockToolExecutor::new()
            .enqueue_ok(
                ToolResult::success("call-1", r#"{"images":[]}"#)
                    .with_structured(serde_json::json!({"images": [], "cost_usd": 0.02})),
            )
            .enqueue_ok(
                ToolResult::success("call-2", r#"{"images":[]}"#)
                    .with_structured(serde_json::json!({"images": [], "cost_usd": 0.07})),
            ),
        vec![def("nika:image_generate")],
    );
    let mut input = AgentInput::new("render the logo twice");
    input.tools = vec!["nika:image_generate".to_owned()];
    let out = r.verb.run(input).await.expect("completes");
    let cost = out.tools_cost_usd.expect("spend accumulated");
    assert!((cost - 0.09).abs() < 1e-9, "0.02 + 0.07 → {cost}");
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
        VerbAgentError::MaxTurns { turns: 3, partial_output, .. }
            if partial_output == "calling nika:read"
    ));
}

#[tokio::test]
async fn absent_max_turns_blames_the_contract_that_declares_the_default() {
    // F-P22 (NEP-0017) · the positive acceptance: nothing wrote
    // `max_turns:`, so the DEFAULT (spec 02-verbs.md §agent · default 10)
    // is what the loop exhausts — the stop is imputed « by the contract »
    // and the receipt names the declarer, never the caller. (Changing
    // observations keep the stall guard silent so the TURN budget is the
    // stop under test.)
    let poll = tool_use_response("c", "nika:read", serde_json::json!({}));
    let mut provider = MockProvider::new("mock");
    let mut tools = MockToolExecutor::new();
    for i in 0..DEFAULT_MAX_TURNS {
        provider = provider.enqueue_response(poll.clone());
        tools = tools.enqueue_ok(ToolResult::success("c", format!("progress {i}%")));
    }
    let r = rig(provider, tools, vec![def("nika:read")]);
    let mut input = AgentInput::new("never finishes");
    input.tools = vec!["nika:read".to_owned()];
    // NO `max_turns:` — the contract's declared default trips.
    let err = r
        .verb
        .run(input)
        .await
        .expect_err("the default budget trips");
    assert!(
        matches!(
            &err,
            VerbAgentError::MaxTurns {
                turns: DEFAULT_MAX_TURNS,
                blame: BlamePolarity::ByTheContract,
                blame_source,
                ..
            } if blame_source.contains("spec 02-verbs.md")
        ),
        "{err}"
    );
    let rendered = err.to_string();
    // The leading sentence stays byte-stable — the blame is an APPEND.
    assert_eq!(
        rendered,
        "agent hit max_turns (10) without completing · blame: by-the-contract \
         (spec 02-verbs.md §agent · `max_turns` default 10)"
    );
}

#[tokio::test]
async fn written_max_turns_blames_the_caller_not_the_contract() {
    // F-P22 · the negative acceptance: the SAME stop under a task-written
    // `max_turns:` is imputed « by the caller » (F-A5) — the third
    // polarity exists but is NOT spoken here.
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            "nika:read",
            serde_json::json!({}),
        )),
        MockToolExecutor::new(),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("never finishes");
    input.tools = vec!["nika:read".to_owned()];
    input.max_turns = Some(1);
    let err = r
        .verb
        .run(input)
        .await
        .expect_err("the caller's budget trips");
    assert!(
        matches!(
            &err,
            VerbAgentError::MaxTurns {
                turns: 1,
                blame: BlamePolarity::ByTheCaller,
                ..
            }
        ),
        "{err}"
    );
    let rendered = err.to_string();
    assert!(rendered.contains("blame: by-the-caller"), "{rendered}");
    assert!(
        !rendered.contains("by-the-contract"),
        "the caller's own bound never names the contract: {rendered}"
    );
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
            VerbAgentError::MaxTokens { total_tokens: 15, partial_output, .. }
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
        VerbAgentError::WhitelistViolation { tool, .. } if tool == "nika:write"
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

#[tokio::test]
async fn done_among_siblings_under_schema_reask_strips_orphan_tool_calls() {
    // The SCHEMA variant of the above: [nika:read, nika:done] in ONE turn,
    // under a `schema:` task. done wins (FinalText); the prose "final words"
    // does not conform, so the loop re-asks WITH the schema. That re-ask is a
    // tools-OFF turn — it must NOT re-send the UNANSWERED read+done tool_calls
    // (a strict provider 400s: "tool_call_ids did not have response messages"
    // · the openai parallel-tool-call bug). The orphan `ToolUse` blocks must
    // be stripped from the final assistant turn before the re-ask.
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
        MockProvider::new("mock")
            .enqueue_response(mixed)
            // the schema re-ask returns a conforming object.
            .enqueue_response(text_response(r#"{"ok": true}"#)),
        MockToolExecutor::new(),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("finish under schema");
    input.tools = vec!["nika:read".to_owned(), DONE_TOOL.to_owned()];
    input.schema = Some(serde_json::json!({
        "type": "object",
        "required": ["ok"],
        "properties": { "ok": { "type": "boolean" } }
    }));
    let out = r.verb.run(input).await.expect("the schema re-ask conforms");
    assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
    // The re-ask request (the LAST one) carries NO orphan tool_call — the
    // unanswered read+done `ToolUse` blocks were stripped before the re-ask.
    let reqs = r.provider.captured_requests();
    assert!(reqs.len() >= 2, "a schema re-ask round-trip happened");
    let reask = reqs.last().expect("the re-ask request");
    assert!(
        !reask.messages.iter().any(|m| m
            .content
            .iter()
            .any(|b| matches!(b, ContentBlock::ToolUse { .. }))),
        "the schema re-ask must carry NO orphan tool_call (tools-off turn)"
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
    // Cost Intelligence follow-up: the turns that DID run are real
    // money — the error carries their spend (usage + model) so the
    // dispatch layer can price it and the budget gate can see it.
    let spend = err
        .spend()
        .expect("a billed turn preceded the failure — spend rides the error");
    assert!(
        spend.usage.input_tokens + spend.usage.output_tokens > 0,
        "turn 1's absorbed usage survives the turn-2 failure"
    );
    assert_eq!(spend.model_resolved.as_deref(), Some("mock/agent"));
}

// ── Cost Intelligence · billed-then-failed spend rides the error ────

#[tokio::test]
async fn failed_loop_carries_tool_spend_for_the_ledger() {
    // Turn 1 dispatches a PRICED tool ($0.06 via the honest-spend
    // channel) · turn 2's provider call dies — the loop's tool spend
    // must not vanish with the failure (it already happened).
    use nika_kernel::ai::provider::ProviderError;
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("c", "nika:read", serde_json::json!({})))
            .enqueue_error(ProviderError::Api {
                status: 500,
                message: "upstream 500".to_owned(),
            }),
        MockToolExecutor::new().enqueue_ok(
            ToolResult::success("c", "rendered")
                .with_structured(serde_json::json!({"ok": true, "cost_usd": 0.06})),
        ),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("burns a paid tool then dies");
    input.tools = vec!["nika:read".to_owned()];
    let err = r.verb.run(input).await.expect_err("provider error");
    let spend = err.spend().expect("spend rides");
    assert_eq!(
        spend.tools_cost_usd,
        Some(0.06),
        "the tool's real spend survives the loop failure"
    );
}

#[tokio::test]
async fn pre_loop_failures_carry_no_spend() {
    // InvalidParam fires before any provider call — nothing was billed,
    // nothing rides (a zero-signal spend must read as None).
    let r = rig(
        MockProvider::new("mock"),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let err = r
        .verb
        .run(AgentInput::new(""))
        .await
        .expect_err("empty prompt refused");
    assert!(matches!(err, VerbAgentError::InvalidParam { .. }), "{err}");
    assert!(
        err.spend().is_none(),
        "no billed call → no spend decoration"
    );
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
        VerbAgentError::WhitelistViolation { tool, .. } if tool == "nika:write"
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

// ── §6 · the security boundary is never fed back (NIKA-468) ─────────

#[tokio::test]
async fn security_boundary_refusal_fails_the_loop_unfed_back() {
    // A whitelisted tool whose call the boundary refuses: the refusal's
    // own spec code rides the error-metadata seam (BUG-D) — the loop
    // FAILS HARD (NIKA-468) and the refusal never materializes as a
    // model-visible feedback block (the whitelist stop's invariant,
    // discovered one seam later, at effect time).
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "c1",
                "nika:read",
                serde_json::json!({"path": "/etc/passwd"}),
            ))
            .enqueue_response(text_response("never reached")),
        MockToolExecutor::new().enqueue_ok(
            ToolResult::error("c1", "outside the declared fs boundary").with_error_meta(
                nika_kernel::runtime::tool_executor::ToolErrorMeta::new(
                    Some("NIKA-SEC-004".to_owned()),
                    false,
                ),
            ),
        ),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("probe the boundary");
    input.tools = vec!["nika:read".to_owned()];
    let err = r.verb.run(input).await.expect_err("security stop");
    assert!(matches!(
        &err,
        VerbAgentError::SecurityBoundary { tool, code, .. }
            if tool == "nika:read" && code == "NIKA-SEC-004"
    ));
    assert_eq!(
        r.provider.captured_requests().len(),
        1,
        "the loop failed before any feedback could be composed — the \
         model never learns which boundary refused (no probing oracle)"
    );
}

#[tokio::test]
async fn a_coded_non_security_tool_error_stays_feedback() {
    // The carve-out is EXACTLY the two security codes: a coded
    // non-security failure (the fetch builtin's own code) keeps the
    // agentic convention — fed back, the model recovers.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "c1",
                "nika:fetch",
                serde_json::json!({"url": "https://example.com"}),
            ))
            .enqueue_response(text_response("recovered")),
        MockToolExecutor::new().enqueue_ok(
            ToolResult::error("c1", "fetch blew up").with_error_meta(
                nika_kernel::runtime::tool_executor::ToolErrorMeta::new(
                    Some("NIKA-BUILTIN-FETCH-001".to_owned()),
                    false,
                ),
            ),
        ),
        vec![def("nika:fetch")],
    );
    let mut input = AgentInput::new("resilient");
    input.tools = vec!["nika:fetch".to_owned()];
    let out = r
        .verb
        .run(input)
        .await
        .expect("a coded non-security error stays feedback");
    assert_eq!(out.output, AgentValue::Text("recovered".to_owned()));
    assert_eq!(r.provider.captured_requests().len(), 2);
}

#[tokio::test]
async fn ssrf_floor_refusal_fails_the_loop_unfed_back() {
    // NIKA-SEC-005 (the SSRF floor) is the second boundary code — same
    // hard stop, same silence toward the model.
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c1",
            "nika:fetch",
            serde_json::json!({"url": "http://169.254.169.254/latest/meta-data"}),
        )),
        MockToolExecutor::new().enqueue_ok(
            ToolResult::error("c1", "blocked by the SSRF floor").with_error_meta(
                nika_kernel::runtime::tool_executor::ToolErrorMeta::new(
                    Some("NIKA-SEC-005".to_owned()),
                    false,
                ),
            ),
        ),
        vec![def("nika:fetch")],
    );
    let mut input = AgentInput::new("reach the metadata service");
    input.tools = vec!["nika:fetch".to_owned()];
    let err = r.verb.run(input).await.expect_err("security stop");
    assert!(matches!(
        &err,
        VerbAgentError::SecurityBoundary { code, .. } if code == "NIKA-SEC-005"
    ));
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
    // BUG#11: a prose final answer triggers the schema re-ask. With the
    // default budget the loop re-asks twice; a model that NEVER produces
    // JSON exhausts the budget and the post-hoc validation is the verdict
    // (NIKA-464 · validation intact). 1 initial + DEFAULT_SCHEMA_RETRY_BUDGET
    // re-asks = 3 prose responses queued.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(text_response("not json at all"))
            .enqueue_response(text_response("still no json"))
            .enqueue_response(text_response("nope, prose forever")),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.schema = Some(serde_json::json!({"type": "object"}));
    let err = r.verb.run(input).await.expect_err("schema gate");
    assert!(
        matches!(err, VerbAgentError::SchemaValidation { .. }),
        "{err}"
    );
    // The model was re-asked WITH the schema constraint each round.
    assert_eq!(
        r.provider.captured_requests().len(),
        1 + DEFAULT_SCHEMA_RETRY_BUDGET as usize,
        "initial answer + the schema re-asks"
    );
}

#[tokio::test]
async fn schema_reask_rescues_a_prose_final_answer() {
    // The BUG#11 fix proper: the model's free-text final answer ("Paris")
    // does NOT conform; the engine re-asks WITH the schema and the model
    // then replies with the object — no NIKA-464, no author hand-instruction.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(text_response("Paris"))
            .enqueue_response(text_response(r#"{"answer": "Paris"}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("capital of France?");
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    }));
    let out = r.verb.run(input).await.expect("the re-ask rescues it");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"answer": "Paris"}))
    );
    assert_eq!(out.stop_reason, AgentStopReason::Completed);
    // Exactly one re-ask happened (the prose answer + the structured re-ask).
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), 2, "free answer + one schema re-ask");
    // The re-ask carried the assistant's prose turn + a schema instruction,
    // and offered NO tools (tools-vs-structured-output sidestep).
    let reask = &reqs[1];
    assert!(reask.tools.is_empty(), "the re-ask is a tools-OFF turn");
    assert!(
        reask.messages.iter().any(|m| matches!(m.role, Role::User)
            && m.content.iter().any(|b| matches!(
                b,
                ContentBlock::Text { text } if text.contains("JSON Schema")
            ))),
        "the re-ask instructs the schema (instruction fallback · non-native mock)"
    );
}

#[tokio::test]
async fn schema_reask_wires_response_format_natively_when_supported() {
    // On a provider that supports response_format (openai-compat · gemini ·
    // mock-with-support), the re-ask wires the schema NATIVELY onto the
    // request rather than relying on the prose instruction (infer parity).
    let provider = MockProvider::new("mock")
        .with_response_format_support(true)
        .enqueue_response(text_response("Paris"))
        .enqueue_response(text_response(r#"{"answer": "Paris"}"#));
    let r = rig(provider, MockToolExecutor::new(), Vec::new());
    let mut input = AgentInput::new("capital of France?");
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    });
    input.schema = Some(schema.clone());
    let out = r.verb.run(input).await.expect("native re-ask rescues it");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"answer": "Paris"}))
    );
    let reask = &r.provider.captured_requests()[1];
    assert!(
        matches!(&reask.response_format, ResponseFormat::JsonSchema(s) if *s == schema),
        "native providers carry the schema on response_format, not just prose"
    );
}

#[tokio::test]
async fn schema_reask_runs_after_the_tool_loop_completes() {
    // The schema enforcement is on the FINAL answer ONLY — the tool loop
    // runs first (unconstrained · tools work), THEN the free-text answer is
    // re-asked into shape. Proof the two phases compose (BUG#11 + the loop).
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response("c1", "nika:read", serde_json::json!({})))
            .enqueue_response(text_response("the file says hello")) // prose final
            .enqueue_response(text_response(r#"{"summary": "hello"}"#)), // re-ask
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "hello")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read then summarize as JSON");
    input.tools = vec!["nika:read".to_owned()];
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"summary": {"type": "string"}},
        "required": ["summary"]
    }));
    let out = r.verb.run(input).await.expect("loop + re-ask");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"summary": "hello"}))
    );
    // The tool actually ran (loop intact), then the answer was re-asked.
    assert_eq!(
        r.tools.captured_calls().len(),
        1,
        "the tool loop still runs"
    );
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), 3, "tool turn + prose answer + schema re-ask");
    assert!(
        !reqs[0].tools.is_empty(),
        "the tool-calling turn carried tools (schema NOT on it)"
    );
    assert!(
        matches!(reqs[0].response_format, ResponseFormat::Text),
        "the tool turn never carries the schema (the conflict sidestep)"
    );
}

#[tokio::test]
async fn schema_already_conforming_final_answer_costs_no_reask() {
    // A well-behaved model that already replies with the object is validated
    // in place — NO extra round-trip (the re-ask is reserved for prose).
    let r = rig(
        MockProvider::new("mock").enqueue_response(text_response(r#"{"answer": "Rome"}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("capital of Italy?");
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    }));
    let out = r.verb.run(input).await.expect("conforms first try");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"answer": "Rome"}))
    );
    assert_eq!(
        r.provider.captured_requests().len(),
        1,
        "no re-ask when the answer already conforms"
    );
}

#[tokio::test]
async fn schema_reask_via_done_without_result_uses_last_text() {
    // A result-LESS `nika:done` finishes on the last free text — that text
    // gets the SAME schema re-ask (the BUG#11 path is the free-text path,
    // however the loop reached it).
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(InferResponse::new(
                vec![
                    ContentBlock::Text {
                        text: "London".to_owned(),
                    },
                    ContentBlock::ToolUse {
                        id: "d".to_owned(),
                        name: DONE_TOOL.to_owned(),
                        input: serde_json::json!({}),
                    },
                ],
                usage(10, 5),
                StopReason::ToolUse,
            ))
            .enqueue_response(text_response(r#"{"answer": "London"}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("capital of the UK?");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(serde_json::json!({
        "type": "object",
        "properties": {"answer": {"type": "string"}},
        "required": ["answer"]
    }));
    let out = r
        .verb
        .run(input)
        .await
        .expect("done-without-result re-asks");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"answer": "London"}))
    );
    assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
}

#[tokio::test]
async fn schema_zero_retry_budget_is_single_shot() {
    // With the budget at 0, the final answer is validated as-is and never
    // re-asked — a prose answer is the NIKA-464 verdict immediately.
    let provider =
        Arc::new(MockProvider::new("mock").enqueue_response(text_response("just prose")));
    let tools = Arc::new(MockToolExecutor::new());
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(InvokeVerb::new(Arc::clone(&tools))),
        Arc::new(MockToolDefinitionProvider::with_defs(Vec::new())),
        "mock/agent",
    )
    .with_schema_retry_budget(0);
    let mut input = AgentInput::new("rate it");
    input.schema = Some(serde_json::json!({"type": "object", "required": ["x"]}));
    let err = verb.run(input).await.expect_err("single shot");
    assert!(
        matches!(err, VerbAgentError::SchemaValidation { .. }),
        "{err}"
    );
    assert_eq!(
        provider.captured_requests().len(),
        1,
        "budget 0 = no re-ask"
    );
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
async fn schema_object_plus_done_string_reasks_instead_of_spend_then_die() {
    // C07 / issue 1301: agent schema wants an object, `nika:done` result
    // is a string (the mock / a model that serialized JSON as a string).
    // Repair = re-ask with the schema; do not emit INFER-002 after the
    // paid turns with no second chance.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "c",
                DONE_TOOL,
                serde_json::json!({"result": "a prose final"}),
            ))
            .enqueue_response(text_response(r#"{"findings":["one"],"sources":["two"]}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("research it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["findings", "sources"],
        "properties": {
            "findings": {"type": "array", "items": {"type": "string"}},
            "sources": {"type": "array", "items": {"type": "string"}}
        }
    }));
    let out = r
        .verb
        .run(input)
        .await
        .expect("string result vs object schema re-asks");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"findings":["one"],"sources":["two"]}))
    );
    assert_eq!(
        r.provider.captured_requests().len(),
        2,
        "one tool turn + one schema re-ask, never spend-then-die"
    );
}

#[tokio::test]
async fn uncompilable_schema_fails_before_any_infer() {
    let r = rig(
        MockProvider::new("mock"),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("research it");
    input.schema = Some(serde_json::json!({"type": "string", "pattern": "["}));
    let err = r.verb.run(input).await.expect_err("bad schema");
    assert!(
        matches!(err, VerbAgentError::SchemaValidation { .. }),
        "{err}"
    );
    assert_eq!(
        r.provider.captured_requests().len(),
        0,
        "C07: fail before paid infer"
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
    let VerbAgentError::WhitelistViolation { tool, .. } = &err else {
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

// ── §R3-F1 · the usage-absence gate (the 2026-07-29 audit, run 3) ────

/// A billed backend that omits the usage block on a PRICED model: the
/// loop fails CLOSED (`NIKA-AGENT-005`), never continues invisibly past
/// the budget it just blinded. The refusal rides the turn it was
/// discovered on (it cannot be known before the response arrives).
#[tokio::test]
async fn an_omitting_backend_on_a_priced_model_fails_closed() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(
            tool_use_response("call-1", "nika:read", serde_json::json!({"path": "./x.md"}))
                .with_usage_reported(false),
        ),
        MockToolExecutor::new(),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("go");
    input.model = Some("anthropic/claude-sonnet-5".to_owned());
    input.tools = vec!["nika:read".to_owned()];
    let err = r.verb.run(input).await.expect_err("the gate refuses");
    assert!(
        matches!(err, VerbAgentError::UsageUnmetered { .. }),
        "{err:?}"
    );
    assert_eq!(err.spec_code(), "NIKA-AGENT-005");
    assert!(!err.is_transient(), "a wire-contract verdict never retries");
    assert_eq!(
        r.provider.captured_requests().len(),
        1,
        "the call ran once — the absence is discovered on its response"
    );
}

/// The carve-out: an unreported zero on an UNPRICED model (the mock's
/// honest zero) is the documented unmetered case — the loop proceeds.
#[tokio::test]
async fn an_unreported_zero_on_an_unpriced_model_proceeds() {
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(text_response("honest zero").with_usage_reported(false)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let out = r
        .verb
        .run(AgentInput::new("hi"))
        .await
        .expect("the carve-out proceeds");
    assert_eq!(out.output, AgentValue::Text("honest zero".to_owned()));
}
