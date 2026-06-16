// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]

//! Integration tests for the v1 sidecar HTTP server (ADR-093).
//!
//! A real TCP round-trip on an ephemeral port over [`MockBackend`] — the
//! client side is a hand-rolled HTTP/1.1 request (no HTTP-client dev-dep),
//! so the test exercises exactly the bytes a wire client sends.

#![cfg(feature = "server")]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

use nika_infer_local::protocol::{ChatRequest, ChatResponse, FinishReason, Message, Role};
use nika_infer_local::{MockBackend, serve};

/// Bind the mock-backed server on an ephemeral loopback port.
fn start() -> nika_infer_local::ServerHandle {
    let addr: SocketAddr = "127.0.0.1:0".parse().expect("loopback literal");
    serve(Arc::new(MockBackend), addr).expect("server binds")
}

/// One blocking HTTP/1.1 exchange: returns (status, body).
fn exchange(addr: SocketAddr, method: &str, path: &str, body: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).expect("connect");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).expect("send");
    let mut raw = String::new();
    stream.read_to_string(&mut raw).expect("read");

    let status: u16 = raw
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .expect("status line");
    let payload = raw
        .split_once("\r\n\r\n")
        .map(|(_, b)| b.to_owned())
        .unwrap_or_default();
    (status, payload)
}

fn chat_json(content: &str) -> String {
    let request = ChatRequest::new("mock-local", vec![Message::new(Role::User, content)]);
    serde_json::to_string(&request).expect("serialize request")
}

#[test]
fn health_returns_ok() {
    let server = start();
    let (status, body) = exchange(server.addr(), "GET", "/health", "");
    assert_eq!(status, 200);
    assert_eq!(body, r#"{"status":"ok"}"#);
}

#[test]
fn completions_round_trip_through_the_wire() {
    let server = start();
    let (status, body) = exchange(
        server.addr(),
        "POST",
        "/v1/chat/completions",
        &chat_json("ping"),
    );
    assert_eq!(status, 200, "body: {body}");

    // The body must parse as our own wire type (the same JSON shape the
    // nika-providers OpenAiCompat parser reads — pinned by wire_contract.rs).
    let reply: ChatResponse = serde_json::from_str(&body).expect("wire-shaped response");
    assert_eq!(reply.choices[0].message.content, "mock reply: ping");
    assert_eq!(reply.choices[0].finish_reason, FinishReason::Stop);
    assert_eq!(reply.model, MockBackend::MODEL);
    assert!(reply.usage.total_tokens > 0);
}

/// Gate-9 canary — the full sidecar chain end-to-end over HTTP produces output
/// that is wire-valid at the EXACT untyped JSON pointer paths the engine's
/// `nika-providers` OpenAiCompat parser walks (pinned identically in
/// `wire_contract.rs`, but there against the trait — here proven on the real
/// served bytes). This is the integration the crate exists for: a wire client
/// POSTs, the sidecar answers, the bytes parse where the consumer looks.
/// Asserting the untyped pointers (not our own `ChatResponse` deserialization)
/// is the point — a serde rename would pass the typed round-trip yet silently
/// break the external parser. The workflow-level `.nika.yaml` canary lands once
/// a verb routes `model: local/<x>` to this sidecar.
#[test]
fn served_bytes_are_wire_valid_at_the_openai_compat_paths() {
    use serde_json::Value;

    let server = start();
    let (status, body) = exchange(
        server.addr(),
        "POST",
        "/v1/chat/completions",
        &chat_json("canary"),
    );
    assert_eq!(status, 200, "body: {body}");

    let v: Value = serde_json::from_str(&body).expect("served body is JSON");
    assert_eq!(
        v.pointer("/choices/0/message/content")
            .and_then(Value::as_str),
        Some("mock reply: canary"),
        "/choices/0/message/content"
    );
    assert_eq!(
        v.pointer("/choices/0/message/role").and_then(Value::as_str),
        Some("assistant"),
        "/choices/0/message/role must be the lowercase literal"
    );
    assert_eq!(
        v.pointer("/choices/0/finish_reason")
            .and_then(Value::as_str),
        Some("stop"),
        "/choices/0/finish_reason must be the literal \"stop\""
    );
    assert!(
        v.pointer("/usage/prompt_tokens")
            .and_then(Value::as_u64)
            .is_some()
            && v.pointer("/usage/completion_tokens")
                .and_then(Value::as_u64)
                .is_some(),
        "/usage/{{prompt,completion}}_tokens must be u64"
    );
    assert!(
        v.pointer("/id").and_then(Value::as_str).is_some(),
        "/id (request id) must be a string"
    );
}

#[test]
fn malformed_json_is_a_400_invalid_request() {
    let server = start();
    let (status, body) = exchange(server.addr(), "POST", "/v1/chat/completions", "{not json");
    assert_eq!(status, 400);
    assert!(body.contains("invalid_request_error"), "body: {body}");
}

#[test]
fn backend_invalid_request_maps_to_400() {
    // No user turn → MockBackend returns InvalidRequest → 400 at the boundary.
    let server = start();
    let request = ChatRequest::new("mock-local", vec![Message::new(Role::System, "be terse")]);
    let json = serde_json::to_string(&request).expect("serialize");
    let (status, body) = exchange(server.addr(), "POST", "/v1/chat/completions", &json);
    assert_eq!(status, 400);
    assert!(body.contains("no user message"), "body: {body}");
}

#[test]
fn stream_requests_are_refused_not_ignored() {
    let server = start();
    let mut request = ChatRequest::new("mock-local", vec![Message::new(Role::User, "hi")]);
    request.stream = true;
    let json = serde_json::to_string(&request).expect("serialize");
    let (status, body) = exchange(server.addr(), "POST", "/v1/chat/completions", &json);
    assert_eq!(status, 400);
    assert!(body.contains("streaming"), "body: {body}");
}

#[test]
fn unknown_route_is_404_and_wrong_method_is_405() {
    let server = start();
    let (status, _) = exchange(server.addr(), "GET", "/v2/nope", "");
    assert_eq!(status, 404);
    let (status, _) = exchange(server.addr(), "GET", "/v1/chat/completions", "");
    assert_eq!(status, 405);
}

#[test]
fn query_strings_do_not_break_routing() {
    let server = start();
    let (status, _) = exchange(server.addr(), "GET", "/health?probe=1", "");
    assert_eq!(status, 200);
}

#[test]
fn shutdown_releases_the_port() {
    let server = start();
    let addr = server.addr();
    server.shutdown();
    // The listener is gone: a fresh connect must fail (or be refused fast).
    assert!(
        TcpStream::connect(addr).is_err(),
        "port still accepting after shutdown"
    );
}
