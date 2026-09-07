// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The typed-task contract: how a declared `schema:` reaches the seat,
//! and what happens when the value it sends back misses.
//!
//! Measured on three real seats before this suite existed: the schema
//! never rode the FIRST request (the sentinel's `result` parameter had a
//! description and nothing else, so the seat could only guess the keys
//! from the prompt), and a `nika:done` result that missed was fatal on
//! attempt 1 while a prose answer got two repairs. Both are pinned here.
//!
//! Split from `tests.rs` under the 1500-LOC file cap; the rig helpers
//! are shared (`crate::tests`), so a change to the loop's test double
//! still moves both suites at once.

use super::*;
use nika_kernel::ai::provider::{ResponseFormat, ToolDef};
use nika_kernel::runtime::tool_executor::ToolResult;
use nika_kernel_mock::{MockProvider, MockToolExecutor};

use crate::tests::{def, rig, text_response, tool_use_response};

/// The task contract used across this suite: an object with one
/// required integer. A string `score` misses it in a way no extraction
/// trick can rescue — the model has to send a different value.
fn score_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {"score": {"type": "integer"}},
        "required": ["score"],
        "additionalProperties": false
    })
}

/// The `nika:done` definition as it left for the provider on a request.
fn done_def_on(request: &InferRequest) -> &ToolDef {
    request
        .tools
        .iter()
        .find(|t| t.name == DONE_TOOL)
        .expect("the sentinel def rides every request that admits it")
}

// ── (a) the schema binds on the FIRST request ───────────────────────

#[tokio::test]
async fn the_first_request_carries_the_schema_on_the_done_def() {
    // The whole of F1: request #1 must already tell the seat the shape.
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"score": 9}}),
        )),
        MockToolExecutor::new(),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned(), "nika:read".to_owned()];
    input.schema = Some(score_schema());
    r.verb.run(input).await.expect("conforming result");

    let reqs = r.provider.captured_requests();
    let first = &reqs[0];
    let done = done_def_on(first);
    assert_eq!(
        done.parameters["properties"]["result"],
        score_schema(),
        "the declared schema IS the `result` parameter's schema, verbatim"
    );
    assert_eq!(
        done.parameters["required"],
        serde_json::json!(["result"]),
        "a typed task finishes by PASSING the value, not by prose"
    );
    // The other half of the decision: the grammar mechanism stays off the
    // tool-calling turn — the def is what binds during the loop.
    assert!(
        matches!(first.response_format, ResponseFormat::Text),
        "response_format never rides a tool-calling turn"
    );
}

#[tokio::test]
async fn a_native_provider_still_keeps_response_format_off_the_loop() {
    // Even where `supports_response_format()` is true, the loop turns
    // stay unconstrained: a grammar over the message content would fight
    // the tool calls the loop exists to make. The binding is the def.
    let r = rig(
        MockProvider::new("mock")
            .with_response_format_support(true)
            .enqueue_response(tool_use_response("c1", "nika:read", serde_json::json!({})))
            .enqueue_response(tool_use_response(
                "c2",
                DONE_TOOL,
                serde_json::json!({"result": {"score": 3}}),
            )),
        MockToolExecutor::new().enqueue_ok(ToolResult::success("c1", "ok")),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("read then rate");
    input.tools = vec![DONE_TOOL.to_owned(), "nika:read".to_owned()];
    input.schema = Some(score_schema());
    r.verb.run(input).await.expect("conforming result");

    for (i, request) in r.provider.captured_requests().iter().enumerate() {
        assert!(
            matches!(request.response_format, ResponseFormat::Text),
            "turn {} carried a grammar it should not have",
            i + 1
        );
        assert_eq!(
            done_def_on(request).parameters["properties"]["result"],
            score_schema(),
            "every loop turn re-states the contract on the def"
        );
    }
}

#[tokio::test]
async fn an_untyped_task_leaves_the_result_open_and_optional() {
    // No `schema:` ⇒ the sentinel keeps its permissive shape: `result` is
    // any JSON and omitting it (finish on the last words) stays legal.
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({}),
        )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("just talk");
    input.tools = vec![DONE_TOOL.to_owned()];
    r.verb.run(input).await.expect("result-less done finishes");

    let reqs = r.provider.captured_requests();
    let done = done_def_on(&reqs[0]);
    assert!(
        done.parameters.get("required").is_none(),
        "untyped, `result` must stay optional"
    );
    assert!(
        done.parameters["properties"]["result"]
            .get("type")
            .is_none(),
        "untyped, `result` accepts any JSON"
    );
}

// ── (c) a conforming result is accepted first try ───────────────────

#[tokio::test]
async fn a_conforming_done_result_costs_exactly_one_request() {
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"score": 7}}),
        )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(score_schema());
    let out = r.verb.run(input).await.expect("conforming result");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"score": 7}))
    );
    assert_eq!(out.stop_reason, AgentStopReason::ExplicitCompletion);
    assert_eq!(
        r.provider.captured_requests().len(),
        1,
        "a conforming result is never re-asked"
    );
}

// ── (b) a non-conforming result REPAIRS, then is fatal WITH the count ─

#[tokio::test]
async fn a_non_conforming_done_result_is_repaired_not_fatal() {
    // The heart of F2: the miss goes back as THAT call's tool result and
    // the model finishes again. One repair, then a clean value.
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(tool_use_response(
                "c1",
                DONE_TOOL,
                serde_json::json!({"result": {"score": "nine"}}),
            ))
            .enqueue_response(tool_use_response(
                "c2",
                DONE_TOOL,
                serde_json::json!({"result": {"score": 9}}),
            )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(score_schema());
    let out = r.verb.run(input).await.expect("the repair conforms");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"score": 9}))
    );

    // The feedback rode the wire as a TOOL RESULT answering that call —
    // the agentic convention, not a bare user message.
    let reqs = r.provider.captured_requests();
    assert_eq!(reqs.len(), 2, "one repair round-trip, no more");
    let repair_turn = &reqs[1];
    let fed_back = repair_turn
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .find_map(|b| match b {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } if tool_use_id == "c1" => Some((content.clone(), *is_error)),
            _ => None,
        })
        .expect("the miss answers the `nika:done` call it came from");
    assert!(fed_back.1, "a miss is an error observation");
    assert!(
        fed_back
            .0
            .contains("did not satisfy the required output schema"),
        "the feedback names the contract: {}",
        fed_back.0
    );
    assert!(
        fed_back.0.contains("integer"),
        "the feedback carries the validator's own words: {}",
        fed_back.0
    );
    // And the def still carries the schema on the repair turn.
    assert_eq!(
        done_def_on(repair_turn).parameters["properties"]["result"],
        score_schema()
    );
}

#[tokio::test]
async fn done_repairs_exhaust_the_budget_then_the_verdict_names_the_count() {
    // Three misses under the default budget of 2: repair, repair, verdict.
    let bad = || {
        tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"score": "nine"}}),
        )
    };
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(bad())
            .enqueue_response(bad())
            .enqueue_response(bad()),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(score_schema());
    let err = r.verb.run(input).await.expect_err("budget exhausted");
    let VerbAgentError::SchemaValidation { detail, .. } = &err else {
        panic!("expected the schema verdict, got {err:?}");
    };
    assert!(
        detail.contains("after 2 repair attempts"),
        "the verdict must say how many repairs were tried: {detail}"
    );
    assert_eq!(
        nika_error::traits::NikaErrorCode::nika_code(&err).num,
        464,
        "still NIKA-464 · wire NIKA-INFER-002"
    );
    assert_eq!(
        r.provider.captured_requests().len(),
        3,
        "the first call + exactly DEFAULT_SCHEMA_RETRY_BUDGET repairs"
    );
}

#[tokio::test]
async fn a_zero_budget_run_keeps_the_done_miss_single_shot() {
    // `with_schema_retry_budget(0)` is the documented single-shot knob —
    // it must still bind (no repair) and must NOT invent an attempt count.
    let provider = Arc::new(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"score": "nine"}}),
        )),
    );
    let tools = Arc::new(MockToolExecutor::new());
    let verb = AgentVerb::new(
        Arc::clone(&provider),
        Arc::new(InvokeVerb::new(Arc::clone(&tools))),
        Arc::new(nika_kernel_mock::MockToolDefinitionProvider::with_defs(
            Vec::new(),
        )),
        "mock/agent",
    )
    .with_schema_retry_budget(0);
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(score_schema());
    let err = verb.run(input).await.expect_err("no repair budget");
    let VerbAgentError::SchemaValidation { detail, .. } = &err else {
        panic!("expected the schema verdict, got {err:?}");
    };
    assert!(
        !detail.contains("repair attempt"),
        "zero spent ⇒ no attempt phrase: {detail}"
    );
    assert_eq!(provider.captured_requests().len(), 1, "single-shot");
}

#[tokio::test]
async fn the_repair_feedback_drops_the_siblings_that_never_ran() {
    // Terminal 2: `nika:done` wins over its batch-mates and they are NOT
    // dispatched. Re-sending them would leave an unanswered tool call on
    // the transcript — a 400 on a strict wire. Only the sentinel's own
    // call may ride back into the repair turn.
    let turn = InferResponse::new(
        vec![
            ContentBlock::ToolUse {
                id: "sibling".to_owned(),
                name: "nika:read".to_owned(),
                input: serde_json::json!({}),
            },
            ContentBlock::ToolUse {
                id: "done-1".to_owned(),
                name: DONE_TOOL.to_owned(),
                input: serde_json::json!({"result": {"score": "nine"}}),
            },
        ],
        crate::tests::usage(10, 5),
        nika_kernel::ai::provider::StopReason::ToolUse,
    );
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(turn)
            .enqueue_response(tool_use_response(
                "done-2",
                DONE_TOOL,
                serde_json::json!({"result": {"score": 1}}),
            )),
        MockToolExecutor::new(),
        vec![def("nika:read")],
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned(), "nika:read".to_owned()];
    input.schema = Some(score_schema());
    r.verb.run(input).await.expect("the repair conforms");

    let reqs = r.provider.captured_requests();
    let ids: Vec<&str> = reqs[1]
        .messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        ids,
        vec!["done-1"],
        "only the sentinel's own call survives into the repair turn"
    );
    assert_eq!(
        r.tools.captured_calls().len(),
        0,
        "a batch-mate of a winning `done` never runs"
    );
}

// ── the ONE budget · both repair paths draw on it ───────────────────

#[tokio::test]
async fn the_done_repair_and_the_text_reask_share_one_budget() {
    // A run that spends one repair on a `nika:done` miss has ONE left for
    // the free-text re-ask — never a second full allowance.
    let r = rig(
        MockProvider::new("mock")
            // 1 · a done result that misses → repair #1
            .enqueue_response(tool_use_response(
                "c1",
                DONE_TOOL,
                serde_json::json!({"result": {"score": "nine"}}),
            ))
            // 2 · the model gives up on the tool and answers in prose →
            //     the free-text path, which now has 1 repair left
            .enqueue_response(text_response("the score is nine"))
            // 3 · the one remaining re-ask · still prose → verdict
            .enqueue_response(text_response("still nine, sorry"))
            // 4 · would be a SECOND allowance · must never be requested
            .enqueue_response(text_response(r#"{"score": 9}"#)),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(score_schema());
    let err = r.verb.run(input).await.expect_err("one budget, not two");
    let VerbAgentError::SchemaValidation { detail, .. } = &err else {
        panic!("expected the schema verdict, got {err:?}");
    };
    assert!(
        detail.contains("after 2 repair attempts"),
        "one done repair + one text re-ask = 2: {detail}"
    );
    assert_eq!(
        r.provider.captured_requests().len(),
        3,
        "the 4th canned response must stay unconsumed"
    );
}

// ── (d) the def's `$defs` ride at the wrapper root, where a `$ref` lands ─

/// A contract whose `result` reaches into its own `$defs` — the shape a
/// schema-authoring tool emits by default.
fn rows_schema() -> serde_json::Value {
    serde_json::json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "rows": {"type": "array", "items": {"$ref": "#/$defs/row"}}
        },
        "required": ["rows"],
        "$defs": {
            "row": {
                "type": "object",
                "properties": {"id": {"type": "integer"}},
                "required": ["id"]
            }
        }
    })
}

#[tokio::test]
async fn the_done_def_hoists_defs_to_the_wrapper_root() {
    // On the wire the document root is the SENTINEL'S parameter schema,
    // not the declared schema: `#/$defs/row` resolves against the wrapper,
    // so the definitions must ride at its root and NOT under
    // `properties.result` (nested verbatim, a seat that validates its tool
    // input followed the pointer to nothing).
    let r = rig(
        MockProvider::new("mock").enqueue_response(tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"rows": [{"id": 1}]}}),
        )),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("list the rows");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(rows_schema());
    let out = r
        .verb
        .run(input)
        .await
        .expect("local validation resolves the ref at the declared root");
    assert_eq!(
        out.output,
        AgentValue::Structured(serde_json::json!({"rows": [{"id": 1}]}))
    );

    let reqs = r.provider.captured_requests();
    let parameters = &done_def_on(&reqs[0]).parameters;
    assert_eq!(
        parameters["$defs"]["row"]["required"],
        serde_json::json!(["id"]),
        "the definitions ride at the wrapper root, where the pointer lands"
    );
    let result = &parameters["properties"]["result"];
    assert!(
        result.get("$defs").is_none(),
        "no second copy under `result`"
    );
    assert!(
        result.get("$schema").is_none(),
        "`$schema` has no place inside a tool input"
    );
    assert_eq!(
        result["properties"]["rows"]["items"]["$ref"],
        serde_json::json!("#/$defs/row"),
        "the pointer itself is left as the author wrote it"
    );
    assert_eq!(result["required"], serde_json::json!(["rows"]));
}

#[test]
fn the_done_def_hoists_definitions_too_and_drops_the_schema_keyword() {
    // The draft-7 spelling hoists the same way; `$schema` is DROPPED from
    // the nested schema, not moved — a tool input declares no dialect.
    let declared = serde_json::json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "type": "object",
        "properties": {"row": {"$ref": "#/definitions/row"}},
        "definitions": {"row": {"type": "object"}}
    });
    let whitelist = Whitelist::new(&[DONE_TOOL.to_owned()]);
    let defs = crate::intrinsic::synthesized_defs(&whitelist, Some(&declared));
    let parameters = &defs[0].parameters;
    assert_eq!(
        parameters["definitions"]["row"],
        serde_json::json!({"type": "object"})
    );
    let result = &parameters["properties"]["result"];
    assert!(result.get("definitions").is_none());
    assert!(result.get("$schema").is_none());
    assert!(parameters.get("$schema").is_none(), "dropped, not hoisted");
    assert_eq!(
        result["properties"]["row"]["$ref"],
        serde_json::json!("#/definitions/row")
    );
}

#[test]
fn a_schema_without_defs_renders_the_pinned_wrapper_byte_for_byte() {
    // No regression on the pinned shape: nothing to hoist ⇒ the wrapper is
    // exactly the one every seat has read since the schema first rode
    // the def.
    let whitelist = Whitelist::new(&[DONE_TOOL.to_owned()]);
    let defs = crate::intrinsic::synthesized_defs(&whitelist, Some(&score_schema()));
    let pinned = serde_json::json!({
        "type": "object",
        "properties": {"result": score_schema()},
        "required": ["result"]
    });
    assert_eq!(defs[0].parameters.to_string(), pinned.to_string());
}

// ── (e) a repair is a request: the token budget gates it ────────────

#[tokio::test]
async fn a_repair_turn_is_gated_by_the_token_budget() {
    // Two violating dones queued; the first answer already spends the
    // whole budget (15 tokens · `>=` exhausted). Measured before the gate:
    // the loop asked for the repair anyway and the run ended on NIKA-464
    // after TWO requests, one past the author's budget. A repair is a
    // request like any other: not asked, the budget verdict instead.
    let bad = || {
        tool_use_response(
            "c",
            DONE_TOOL,
            serde_json::json!({"result": {"score": "nine"}}),
        )
    };
    let r = rig(
        MockProvider::new("mock")
            .enqueue_response(bad())
            .enqueue_response(bad()),
        MockToolExecutor::new(),
        Vec::new(),
    );
    let mut input = AgentInput::new("rate it");
    input.tools = vec![DONE_TOOL.to_owned()];
    input.schema = Some(score_schema());
    input.max_tokens_total = Some(15); // the first answer spends exactly 15
    let err = r.verb.run(input).await.expect_err("the budget terminal");
    assert!(
        matches!(
            &err,
            VerbAgentError::MaxTokens {
                total_tokens: 15,
                ..
            }
        ),
        "a met budget ends the run on the budget verdict, not on a repair: {err:?}"
    );
    assert_eq!(
        nika_error::traits::NikaErrorCode::nika_code(&err).num,
        461,
        "NIKA-461 · wire NIKA-AGENT-002"
    );
    assert_eq!(
        r.provider.captured_requests().len(),
        1,
        "the repair was never requested"
    );
}
