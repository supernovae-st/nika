// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Cross-provider parity matrix — the house rule made executable:
//! **the same assertions run against every canonical profile** (a failure
//! on one provider is an engine bug, not a provider quirk).
//!
//! Per wired profile (14 of 14 — gemini wired at s8.6):
//! 1. `infer` returns text content + populated `GenAiAttrs`
//! 2. `infer_stream` yields ≥1 `Delta` and exactly one terminal `Done`
//! 3. provider 401 maps to `ProviderError::AuthFailed` (http wires)
//! 4. `ProviderMeta` answers coherently with the wire family

use std::sync::Arc;

use nika_kernel::ai::provider::{
    InferEvent, InferRequest, Message, ProviderError, ProviderInferDyn as _, ProviderMeta as _,
    ProviderStreamDyn as _, Role,
};
use nika_kernel::genai::GenAiSystem;
use nika_kernel::secret::Secret;

use crate::profile::{WireFormat, seed};
use crate::registry::{ProviderRegistry, ProvidersConfig, ResolvedProvider};
use crate::test_support::{FakeHttp, collect};

const ANTHROPIC_OK: &str = r#"{"id":"msg_p","model":"claude-test",
    "content":[{"type":"text","text":"parity"}],"stop_reason":"end_turn",
    "usage":{"input_tokens":2,"output_tokens":1}}"#;

const OPENAI_OK: &str = r#"{"id":"cc_p","model":"compat-test",
    "choices":[{"message":{"content":"parity"},"finish_reason":"stop"}],
    "usage":{"prompt_tokens":2,"completion_tokens":1}}"#;

const AUTH_ERR: &str = r#"{"error":{"message":"invalid key"}}"#;

const GEMINI_OK: &str = r#"{"responseId":"g_p","modelVersion":"gemini-test",
    "candidates":[{"content":{"parts":[{"text":"parity"}]},"finishReason":"STOP"}],
    "usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":1}}"#;

const GEMINI_SSE: &str = "data: {\"responseId\":\"g_s\",\"candidates\":[{\"content\":{\"parts\":[{\"text\":\"parity\"}]},\"finishReason\":\"STOP\"}],\"usageMetadata\":{\"promptTokenCount\":2,\"candidatesTokenCount\":1}}\n\n";

const ANTHROPIC_SSE: &str = concat!(
    "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_s\",\"usage\":{\"input_tokens\":2}}}\n\n",
    "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"parity\"}}\n\n",
    "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":1}}\n\n",
    "data: {\"type\":\"message_stop\"}\n\n",
);

const OPENAI_SSE: &str = concat!(
    "data: {\"id\":\"cc_s\",\"choices\":[{\"delta\":{\"content\":\"parity\"},\"finish_reason\":null}]}\n\n",
    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
    "data: {\"choices\":[],\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1}}\n\n",
    "data: [DONE]\n\n",
);

fn request() -> InferRequest {
    InferRequest::new("ignored", vec![Message::text(Role::User, "parity check")])
}

fn ok_fixture(wire: WireFormat) -> &'static str {
    match wire {
        WireFormat::Anthropic => ANTHROPIC_OK,
        WireFormat::Gemini => GEMINI_OK,
        _ => OPENAI_OK,
    }
}

fn sse_fixture(wire: WireFormat) -> &'static str {
    match wire {
        WireFormat::Anthropic => ANTHROPIC_SSE,
        WireFormat::Gemini => GEMINI_SSE,
        _ => OPENAI_SSE,
    }
}

/// Resolve `id/test-model` against the given fake (key injected when the
/// profile requires one).
fn resolve_on(fake: &Arc<FakeHttp>, id: &str, requires_key: bool) -> ResolvedProvider<FakeHttp> {
    let mut config = ProvidersConfig::new();
    if requires_key {
        config = config.with_key(id, Secret::new("parity-test-key"));
    }
    ProviderRegistry::new(Arc::clone(fake), config)
        .resolve(&format!("{id}/test-model"))
        .expect("parity profile resolves")
}

fn wired_http_profiles() -> Vec<(&'static str, WireFormat, bool)> {
    seed()
        .into_iter()
        .filter(|p| !matches!(p.wire, WireFormat::Mock))
        .map(|p| (p.id, p.wire, p.requires_key))
        .collect()
}

#[tokio::test]
async fn every_wired_profile_infers_with_attributed_gen_ai() {
    for (id, wire, requires_key) in wired_http_profiles() {
        let fake = FakeHttp::with_json(200, ok_fixture(wire));
        let rp = resolve_on(&fake, id, requires_key);
        let resp = rp
            .infer(request())
            .await
            .unwrap_or_else(|e| panic!("[{id}] infer must succeed on the parity fixture: {e}"));

        let text_ok = resp.content.iter().any(
            |b| matches!(b, nika_kernel::ai::provider::ContentBlock::Text { text } if text == "parity"),
        );
        assert!(text_ok, "[{id}] text content mapped");
        assert_eq!(resp.usage.input_tokens, 2, "[{id}] usage mapped");
        assert_ne!(
            resp.gen_ai.system,
            GenAiSystem::Unknown,
            "[{id}] gen_ai.system attributed (Gate-2 parity)"
        );
        assert!(
            resp.gen_ai.response_model.is_some(),
            "[{id}] gen_ai.response_model populated"
        );
        assert!(resp.request_id.is_some(), "[{id}] request id kept");
    }
}

#[tokio::test]
async fn every_wired_profile_streams_deltas_then_exactly_one_done() {
    for (id, wire, requires_key) in wired_http_profiles() {
        let fake = FakeHttp::with_stream(200, sse_fixture(wire), 9);
        let rp = resolve_on(&fake, id, requires_key);
        let events = collect(
            rp.infer_stream(request())
                .await
                .unwrap_or_else(|e| panic!("[{id}] stream must open: {e}")),
        )
        .await;

        let deltas = events
            .iter()
            .filter(|e| matches!(e, Ok(InferEvent::Delta { .. })))
            .count();
        let dones = events
            .iter()
            .filter(|e| matches!(e, Ok(InferEvent::Done { .. })))
            .count();
        assert!(deltas >= 1, "[{id}] at least one delta");
        assert_eq!(dones, 1, "[{id}] exactly one Done");
        assert!(
            matches!(events.last(), Some(Ok(InferEvent::Done { .. }))),
            "[{id}] Done is terminal"
        );
        let usage_ok = events
            .iter()
            .any(|e| matches!(e, Ok(InferEvent::Usage(u)) if u.input_tokens == 2));
        assert!(usage_ok, "[{id}] Usage event emitted on the stream");
    }
}

#[tokio::test]
async fn every_wired_profile_stream_maps_401_to_auth_failed() {
    for (id, _wire, requires_key) in wired_http_profiles() {
        let fake = FakeHttp::with_stream(401, AUTH_ERR, 64);
        let rp = resolve_on(&fake, id, requires_key);
        let err = rp
            .infer_stream(request())
            .await
            .err()
            .unwrap_or_else(|| panic!("[{id}] stream 401 must error"));
        assert!(
            matches!(err, ProviderError::AuthFailed { .. }),
            "[{id}] stream 401 → AuthFailed (same table as infer), got {err:?}"
        );
    }
}

#[tokio::test]
async fn every_wired_profile_maps_401_to_auth_failed() {
    for (id, wire, requires_key) in wired_http_profiles() {
        let _ = wire;
        let fake = FakeHttp::with_json(401, AUTH_ERR);
        let rp = resolve_on(&fake, id, requires_key);
        let err = rp.infer(request()).await.expect_err("401 must be an error");
        assert!(
            matches!(err, ProviderError::AuthFailed { .. }),
            "[{id}] 401 → AuthFailed, got {err:?}"
        );
    }
}

#[tokio::test]
async fn mock_passes_the_same_matrix_without_network() {
    let reg = ProviderRegistry::without_http(ProvidersConfig::new());
    let rp = reg.resolve("mock/test-model").expect("mock resolves");

    let resp = rp.infer(request()).await.expect("mock infers");
    assert!(!resp.content.is_empty(), "[mock] content present");
    assert!(resp.request_id.is_some(), "[mock] request id");
    assert!(
        resp.gen_ai.response_model.is_some(),
        "[mock] response_model populated"
    );
    // The mock is the ONE profile whose gen_ai.system stays Unknown by
    // design (no upstream system to attribute) — locked explicitly so the
    // divergence from the 13 http profiles is intentional, not an omission.
    assert_eq!(resp.gen_ai.system, GenAiSystem::Unknown, "[mock] by design");

    let events = collect(rp.infer_stream(request()).await.expect("mock streams")).await;
    let dones = events
        .iter()
        .filter(|e| matches!(e, Ok(InferEvent::Done { .. })))
        .count();
    assert_eq!(dones, 1, "[mock] exactly one Done");
    assert!(matches!(events.last(), Some(Ok(InferEvent::Done { .. }))));
}

#[test]
fn meta_is_coherent_across_the_fourteen() {
    let reg = ProviderRegistry::new(
        Arc::new(crate::registry::NoHttp),
        seed()
            .iter()
            .filter(|p| p.requires_key)
            .fold(ProvidersConfig::new(), |c, p| {
                c.with_key(p.id, Secret::new("k"))
            }),
    );
    for p in seed() {
        let rp = reg
            .resolve(&format!("{}/m", p.id))
            .unwrap_or_else(|e| panic!("[{}] resolves: {e}", p.id));
        assert_eq!(rp.name(), p.id, "[{}] ProviderMeta::name", p.id);
        let supports = rp.supports_response_format();
        match p.wire {
            WireFormat::OpenAiCompat | WireFormat::Mock | WireFormat::Gemini => {
                assert!(supports, "[{}] response_format supported", p.id);
            }
            _ => assert!(!supports, "[{}] honest capability answer", p.id),
        }
    }
}
