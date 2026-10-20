// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use super::*;
use crate::test_support::{FakeHttp, resolved_with};
use nika_kernel::ai::provider::{Message, ToolDef};

fn tool() -> ToolDef {
    ToolDef::new(
        "nika:read",
        "Read a file",
        json!({"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}),
    )
}

/// A conversation one tool round deep: the system words, the request, the assistant's reasoning
/// state and call, and the tool's output.
fn conversation() -> InferRequest {
    let state = json!({"id": "rs_1", "type": "reasoning", "content": [],
        "encrypted_content": "gAAA-sealed", "summary": []});
    let messages = vec![
        Message::text(Role::System, "Use the tool."),
        Message::text(Role::User, "What is the first word of notes.txt?"),
        Message::new(
            Role::Assistant,
            vec![
                ContentBlock::Reasoning {
                    wire: WIRE.to_owned(),
                    state,
                },
                ContentBlock::Reasoning {
                    wire: "anthropic".to_owned(),
                    state: json!({"type": "thinking"}),
                },
                ContentBlock::ToolUse {
                    id: "call_1".to_owned(),
                    name: "nika:read".to_owned(),
                    input: json!({"path": "notes.txt"}),
                },
            ],
        ),
        Message::new(
            Role::User,
            vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".to_owned(),
                content: "Bonjour tout le monde".to_owned(),
                is_error: false,
            }],
        ),
    ];
    let mut req = InferRequest::new("gpt-6-astra", messages);
    req.tools = vec![tool()];
    req.max_tokens = Some(2_000);
    req
}

/// The conversation becomes Responses input items: the system words are the instructions, the
/// tools are internally tagged and non-strict, this door's reasoning state rides back verbatim
/// in place (another wire's never), the call rides without its item id, its output by call id.
#[test]
fn a_tool_round_becomes_input_items_with_its_reasoning_state() {
    let req = conversation();
    let names = ToolNameMap::from_tools(&req.tools);
    let body = request_body("gpt-6-astra", &req, &names).expect("body");
    assert_eq!(body["instructions"], "Use the tool.");
    assert_eq!(body["store"], false);
    assert_eq!(body["include"], json!(["reasoning.encrypted_content"]));
    assert_eq!(body["max_output_tokens"], 2_000);
    assert!(body.get("max_completion_tokens").is_none() && body.get("messages").is_none());
    let wire = names.to_wire("nika:read");
    assert_eq!(
        body["tools"],
        json!([{"type": "function", "name": wire, "description": "Read a file",
            "parameters": tool().parameters, "strict": false}])
    );
    assert_eq!(body["tool_choice"], "auto");
    assert_eq!(
        body["input"],
        json!([
            {"role": "user", "content": [{"type": "input_text",
                "text": "What is the first word of notes.txt?"}]},
            {"id": "rs_1", "type": "reasoning", "content": [],
                "encrypted_content": "gAAA-sealed", "summary": []},
            {"type": "function_call", "call_id": "call_1", "name": wire,
                "arguments": "{\"path\":\"notes.txt\"}"},
            {"type": "function_call_output", "call_id": "call_1",
                "output": "Bonjour tout le monde"}
        ])
    );
}

/// A stop sequence has no Responses spelling: refused before any byte, never dropped.
#[test]
fn a_stop_sequence_is_refused_never_dropped() {
    let mut req = conversation();
    req.stop_sequences = vec!["END".to_owned()];
    let names = ToolNameMap::from_tools(&req.tools);
    let refused = request_body("gpt-6-astra", &req, &names).expect_err("no stop sequences");
    assert!(refused.to_string().contains("stop sequences"), "{refused}");
}

/// A live gpt-6-sol reply of 2026-10-16 that called the tool, then one that reasoned and
/// answered: the reasoning state is kept for the next request, the call comes back under its
/// canonical name, the stop says which, and the usage law reads the meters complete.
#[test]
fn a_reply_parses_into_calls_text_state_and_complete_usage() {
    let req = conversation();
    let names = ToolNameMap::from_tools(&req.tools);
    let fake = FakeHttp::with_json(200, "{}");
    let rp = resolved_with(&fake, "openai/gpt-6-sol", "sk-test");
    let wire = names.to_wire("nika:read");
    let called = json!({
        "id": "resp_1", "model": "gpt-6-sol", "status": "completed", "service_tier": "default",
        "incomplete_details": null,
        "output": [{"id": "fc_1", "type": "function_call", "status": "completed",
            "arguments": "{\"path\":\"notes.txt\"}", "call_id": "call_9", "name": wire}],
        "usage": {"input_tokens": 69, "input_tokens_details":
            {"cache_write_tokens": 0, "cached_tokens": 0}, "output_tokens": 20,
            "output_tokens_details": {"reasoning_tokens": 0}, "total_tokens": 89},
        "tool_usage": {"image_gen": {"input_tokens": 0, "output_tokens": 0, "total_tokens": 0},
            "web_search": {"num_requests": 0}},
    });
    let resp = parse_response(&rp, called.to_string().as_bytes(), &names).expect("parsed");
    assert_eq!(resp.stop_reason, StopReason::ToolUse);
    assert!(
        matches!(&resp.content[..], [ContentBlock::ToolUse { id, name, input }]
        if id == "call_9" && name == "nika:read" && input["path"] == "notes.txt")
    );
    assert_eq!(
        (resp.usage.input_tokens, resp.usage.output_tokens),
        (69, 20)
    );
    assert_eq!(
        resp.usage_completeness,
        nika_kernel::ai::provider::UsageCompleteness::Complete
    );
    assert_eq!(resp.gen_ai.response_model.as_deref(), Some("gpt-6-sol"));
    let answered = json!({
        "id": "resp_2", "model": "gpt-6-sol", "status": "completed", "service_tier": "default",
        "output": [
            {"id": "rs_2", "type": "reasoning", "content": [], "encrypted_content": "gAAA",
                "summary": [{"type": "summary_text", "text": "Counting."}]},
            {"id": "msg_2", "type": "message", "status": "completed", "role": "assistant",
                "content": [{"type": "output_text", "annotations": [], "text": "62"}]}],
        "usage": {"input_tokens": 63, "output_tokens": 42,
            "output_tokens_details": {"reasoning_tokens": 35}, "total_tokens": 105},
    });
    let resp = parse_response(&rp, answered.to_string().as_bytes(), &names).expect("parsed");
    assert_eq!(resp.stop_reason, StopReason::EndTurn);
    assert!(matches!(&resp.content[..], [
        ContentBlock::Thinking { text },
        ContentBlock::Reasoning { wire, state },
        ContentBlock::Text { text: answer },
    ] if text == "Counting." && wire == WIRE && state["id"] == "rs_2" && answer == "62"));
    assert_eq!(resp.usage.reasoning_tokens, Some(35));
    let mut cut = answered.clone();
    cut["incomplete_details"] = json!({"reason": "max_output_tokens"});
    let resp = parse_response(&rp, cut.to_string().as_bytes(), &names).expect("parsed");
    assert_eq!(resp.stop_reason, StopReason::MaxTokens);
}

/// The usage law: any meter outside the text reply's shape leaves the meters incomplete.
#[test]
fn every_other_axis_leaves_the_responses_usage_incomplete() {
    let complete = json!({"service_tier": "default", "usage": {"input_tokens": 100,
        "input_tokens_details": {"cached_tokens": 40, "cache_write_tokens": 50},
        "output_tokens": 20, "output_tokens_details": {"reasoning_tokens": 20},
        "total_tokens": 120}});
    assert!(complete_usage(&complete));
    for (name, field, value) in [
        ("flex tier", "/service_tier", json!("flex")),
        (
            "a web search",
            "/tool_usage",
            json!({"web_search": {"num_requests": 1}}),
        ),
        ("total", "/usage/total_tokens", json!(121)),
        (
            "read and write beyond the input",
            "/usage/input_tokens_details/cached_tokens",
            json!(51),
        ),
        (
            "reasoning beyond the output",
            "/usage/output_tokens_details/reasoning_tokens",
            json!(21),
        ),
        (
            "unknown detail",
            "/usage/input_tokens_details/audio_tokens",
            json!(0),
        ),
        ("unknown key", "/usage/new_axis", json!(0)),
        ("empty prompt", "/usage/input_tokens", json!(0)),
    ] {
        let mut reply = complete.clone();
        let (parent, key) = field.rsplit_once('/').expect("pointer");
        let target = if parent.is_empty() {
            &mut reply
        } else {
            reply.pointer_mut(parent).expect("parent")
        };
        target[key] = value;
        assert!(!complete_usage(&reply), "{name}");
    }
}

/// Only function tools to `OpenAI`'s own endpoint ride the door: a text request keeps Chat
/// Completions, and so does another provider or a configured gateway.
#[test]
fn only_tools_to_openais_own_endpoint_ride_the_door() {
    let fake = FakeHttp::with_json(200, "{}");
    let openai = resolved_with(&fake, "openai/gpt-6-astra", "sk-test");
    let mut req = conversation();
    assert!(wanted(&openai, &req));
    req.tools.clear();
    assert!(!wanted(&openai, &req), "no tool, no door");
    let deepseek = resolved_with(&fake, "deepseek/deepseek-flash", "sk-test");
    assert!(!wanted(&deepseek, &conversation()), "another provider");
    let mut gateway = resolved_with(&fake, "openai/gpt-6-astra", "sk-test");
    gateway.base_url = "https://gateway.example/v1/chat/completions".to_owned();
    assert!(!wanted(&gateway, &conversation()), "a configured gateway");
    assert_eq!(
        endpoint("https://api.openai.com/v1/chat/completions").as_deref(),
        Some("https://api.openai.com/v1/responses")
    );
}

/// The whole door: a tool request to `openai/gpt-6-astra` is sent to `/v1/responses` with its
/// bearer key and Responses body, and its reply comes back as the call it made.
#[tokio::test]
async fn a_tool_request_to_openai_rides_the_responses_endpoint() {
    let fake = FakeHttp::with_json(
        200,
        r#"{"id":"resp_1","model":"gpt-6-astra","status":"completed","service_tier":"default",
            "output":[{"id":"fc_1","type":"function_call","status":"completed",
                "arguments":"{\"path\":\"notes.txt\"}","call_id":"call_1","name":"nika_read"}],
            "usage":{"input_tokens":69,"output_tokens":20,"total_tokens":89}}"#,
    );
    let rp = resolved_with(&fake, "openai/gpt-6-astra", "sk-test");
    let mut req = InferRequest::new(
        "gpt-6-astra",
        vec![Message::text(Role::User, "Read notes.txt")],
    );
    req.tools = vec![tool()];
    req.max_tokens = Some(500);
    let resp = super::super::infer(&rp, req).await.expect("answered");
    assert_eq!(resp.stop_reason, StopReason::ToolUse);
    let sent = fake.captured();
    assert_eq!(sent[0].url, "https://api.openai.com/v1/responses");
    assert_eq!(
        sent[0].headers.get("authorization").map(String::as_str),
        Some("Bearer sk-test")
    );
    let body: Value = serde_json::from_slice(sent[0].body.as_ref().expect("body")).expect("json");
    assert_eq!(body["max_output_tokens"], 500);
    assert!(body.get("messages").is_none() && body["input"].is_array());
}
