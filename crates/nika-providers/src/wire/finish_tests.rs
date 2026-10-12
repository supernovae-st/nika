// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! A finish is classified from the provider's own finish field, never from the text: a limit
//! finish that carries a tool call is cut (the call is visible; the verb refuses to dispatch it),
//! and an interrupted generation is never a normal end.

use crate::test_support::{FakeHttp, resolved_with};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, Message, ProviderInferDyn as _, Role, StopReason,
};
use serde_json::json;

fn request(model: &str) -> InferRequest {
    let mut request = InferRequest::new(model, vec![Message::text(Role::User, "write the notes")]);
    request.max_tokens = Some(64);
    request
}

async fn finish_of(provider: &str, body: &serde_json::Value) -> (StopReason, bool, Option<String>) {
    let http = FakeHttp::with_json(200, &body.to_string());
    let response = (resolved_with(&http, provider, "test-key").infer(request("m")))
        .await
        .expect("a well-formed answer");
    let called = (response.content.iter()).any(|b| matches!(b, ContentBlock::ToolUse { .. }));
    (response.stop_reason, called, response.finish_reason_raw)
}

/// Every wire: an answer the provider stopped at its output limit while writing a tool call is
/// cut, never a tool-use turn, even though the call's arguments parse.
#[tokio::test]
async fn a_limit_finish_carrying_a_tool_call_is_cut_on_every_wire() {
    let anthropic = json!({"id": "a", "model": "claude-haiku-4-5", "stop_reason": "max_tokens",
        "content": [{"type": "tool_use", "id": "t1", "name": "write",
            "input": {"path": "notes.md"}}],
        "usage": {"input_tokens": 9, "output_tokens": 64}});
    let compat = json!({"id": "c", "model": "gpt-4o-mini", "choices": [{"finish_reason": "length",
        "message": {"content": null, "tool_calls": [{"id": "c1", "type": "function",
            "function": {"name": "write", "arguments": "{\"path\": \"notes.md\"}"}}]}}],
        "usage": {"prompt_tokens": 9, "completion_tokens": 64}});
    let gemini = json!({"candidates": [{"finishReason": "MAX_TOKENS", "content": {"role": "model",
        "parts": [{"functionCall": {"name": "write", "args": {"path": "notes.md"}}}]}}],
        "usageMetadata": {"promptTokenCount": 9, "candidatesTokenCount": 64,
            "totalTokenCount": 73}});
    let ollama = json!({"model": "qwen3.5:4b", "done": true, "done_reason": "length",
        "message": {"role": "assistant", "content": "", "tool_calls": [{"function":
            {"name": "write", "arguments": {"path": "notes.md"}}}]},
        "prompt_eval_count": 9, "eval_count": 64});
    let wires = [
        ("anthropic", anthropic),
        ("openai", compat),
        ("gemini/flash-25", gemini),
        ("ollama/qwen3.5:4b", ollama),
    ];
    for (provider, body) in wires {
        let (stop, called, _) = finish_of(provider, &body).await;
        assert_eq!(stop, StopReason::MaxTokens, "{provider}");
        assert!(called, "{provider}: the cut call stays visible to the verb");
        assert!(stop.is_incomplete(), "{provider}");
    }
}

/// The documented finishes outside the plain output cap: a context-window cut is cut; an
/// interrupted generation is `Interrupted`. The provider's own word is kept.
#[tokio::test]
async fn documented_cut_and_interrupted_finishes_are_never_a_normal_end() {
    let anthropic = |stop: &str| {
        json!({"id": "a", "model": "claude-haiku-4-5", "stop_reason": stop,
            "content": [{"type": "text", "text": "{\"done\": true}"}],
            "usage": {"input_tokens": 9, "output_tokens": 64}})
    };
    let compat = |finish: &str| {
        json!({"id": "c", "model": "m", "choices": [{"finish_reason": finish,
            "message": {"content": "{\"done\": true}"}}],
            "usage": {"prompt_tokens": 9, "completion_tokens": 64}})
    };
    let cases = [
        (
            "anthropic",
            anthropic("model_context_window_exceeded"),
            StopReason::MaxTokens,
        ),
        (
            "anthropic",
            anthropic("pause_turn"),
            StopReason::Interrupted,
        ),
        ("mistral", compat("model_length"), StopReason::MaxTokens),
        ("mistral", compat("error"), StopReason::Interrupted),
        ("openrouter", compat("error"), StopReason::Interrupted),
        (
            "deepseek",
            compat("insufficient_system_resource"),
            StopReason::Interrupted,
        ),
        ("deepseek", compat("aborted"), StopReason::Interrupted),
    ];
    for (provider, body, expected) in cases {
        let word = (body.get("stop_reason"))
            .or_else(|| body["choices"][0].get("finish_reason"))
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned);
        let (stop, _, raw) = finish_of(provider, &body).await;
        assert_eq!(stop, expected, "{provider} {word:?}");
        assert!(stop.is_incomplete(), "{provider} {word:?}");
        assert_eq!(raw, word, "the provider's own word is kept");
    }
    let (stop, _, _) = finish_of("deepseek", &compat("stop")).await;
    assert!(!stop.is_incomplete(), "a complete answer stays complete");
}
