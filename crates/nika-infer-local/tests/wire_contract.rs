// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! e2e wire-contract test — the self-consistency check from the crate spec.
//!
//! The sidecar's whole value is that the engine's EXISTING `OpenAiCompat`
//! client (`nika-providers`) can consume it with zero new dispatch code. That
//! client's `parse_response` is `pub(crate)`, so this test pins the contract
//! at the WIRE level instead: it runs a request end-to-end through the
//! [`Backend`] seam, serializes the [`ChatResponse`], and asserts the exact
//! JSON-pointer paths + literal values `openai_compat::parse_response` reads
//! (verified against its source · `wire/openai_compat.rs`):
//!
//! - `/choices/0/message/content`  → non-empty string
//! - `/choices/0/finish_reason`    → `"stop"` (`EndTurn`) · `"length"` (`MaxTokens`)
//! - `/usage/prompt_tokens` + `/usage/completion_tokens` → u64
//! - `/id`                         → string (request id)
//!
//! If either side drifts (our serializer OR their parser's paths), this is
//! the test that goes red.

use nika_infer_local::protocol::{ChatRequest, Message, Role};
use nika_infer_local::{Backend, MockBackend};
use serde_json::Value;

fn request(content: &str, max_tokens: Option<u32>) -> ChatRequest {
    // ChatRequest is #[non_exhaustive]: external consumers (this tests/ dir
    // compiles as one) construct via new() then set knobs — the exact shape
    // the upcoming HTTP server layer will use.
    let mut req = ChatRequest::new(
        "mock-local",
        vec![
            Message::new(Role::System, "be terse"),
            Message::new(Role::User, content),
        ],
    );
    req.max_tokens = max_tokens;
    req
}

/// Run the backend, return the response as wire JSON (what the HTTP layer
/// will send and the engine's `OpenAiCompat` client will parse).
async fn roundtrip(req: &ChatRequest) -> Value {
    let resp = MockBackend.generate(req).await.expect("generate ok");
    serde_json::to_value(&resp).expect("serializes")
}

#[tokio::test]
async fn wire_shape_matches_the_openai_compat_parser_paths() {
    let v = roundtrip(&request("hello wire", None)).await;

    // The exact pointer paths nika-providers' parse_response reads.
    let content = v
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .expect("/choices/0/message/content must be a string");
    assert!(!content.is_empty(), "parser drops empty content");

    let finish = v
        .pointer("/choices/0/finish_reason")
        .and_then(Value::as_str)
        .expect("/choices/0/finish_reason must be a string");
    assert_eq!(finish, "stop", "EndTurn maps from the literal \"stop\"");

    assert!(
        v.pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .is_some(),
        "/usage/prompt_tokens must be u64"
    );
    assert!(
        v.pointer("/usage/completion_tokens")
            .and_then(Value::as_u64)
            .is_some(),
        "/usage/completion_tokens must be u64"
    );
    assert!(
        v.pointer("/id").and_then(Value::as_str).is_some(),
        "/id must be a string (request_id)"
    );
}

#[tokio::test]
async fn truncated_generation_reports_the_length_literal() {
    // max_tokens forces truncation → the parser must see the exact literal
    // "length" (its map_finish: "length" → StopReason::MaxTokens).
    let v = roundtrip(&request("a b c d e f g", Some(2))).await;
    assert_eq!(
        v.pointer("/choices/0/finish_reason")
            .and_then(Value::as_str),
        Some("length"),
        "MaxTokens maps from the literal \"length\""
    );
}

#[tokio::test]
async fn roles_serialize_to_the_openai_lowercase_literals() {
    // The assistant message role must be the lowercase literal the
    // OpenAI-compat ecosystem expects.
    let v = roundtrip(&request("role check", None)).await;
    assert_eq!(
        v.pointer("/choices/0/message/role").and_then(Value::as_str),
        Some("assistant"),
    );
}

#[tokio::test]
async fn usage_total_is_the_sum_of_parts() {
    let v = roundtrip(&request("two words", None)).await;
    let p = v
        .pointer("/usage/prompt_tokens")
        .and_then(Value::as_u64)
        .unwrap();
    let c = v
        .pointer("/usage/completion_tokens")
        .and_then(Value::as_u64)
        .unwrap();
    let t = v
        .pointer("/usage/total_tokens")
        .and_then(Value::as_u64)
        .unwrap();
    assert_eq!(t, p + c, "total_tokens must equal prompt + completion");
}
