// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Contract test for `GenAiAttrs` (Q13 — `OTel` `GenAI` semconv bridge).

#![allow(clippy::expect_used, clippy::unwrap_used)]

use nika_kernel::genai::{GenAiAttrs, GenAiOperation, GenAiSystem};
use nika_kernel::{InferRequest, InferResponse, StopReason, TokenUsage};

#[test]
fn infer_request_carries_genai_attrs_default() {
    let req = InferRequest::new("anthropic/claude-sonnet-4-7", Vec::new());
    // Default attrs is empty/unknown; producers populate.
    assert_eq!(req.gen_ai.system, GenAiSystem::Unknown);
    assert_eq!(req.gen_ai.operation, GenAiOperation::Chat);
}

#[test]
fn infer_response_carries_genai_attrs_default() {
    let resp = InferResponse::new(Vec::new(), TokenUsage::new(0, 0), StopReason::EndTurn);
    assert_eq!(resp.gen_ai.system, GenAiSystem::Unknown);
    assert!(resp.gen_ai.response_id.is_none());
    assert!(resp.gen_ai.response_model.is_none());
}

#[test]
fn genai_attrs_is_non_exhaustive_constructor_only() {
    let attrs = GenAiAttrs::new();
    assert_eq!(attrs.system, GenAiSystem::Unknown);
}

#[test]
fn genai_system_serde_snake_case() {
    let s = GenAiSystem::Anthropic;
    let json = serde_json::to_string(&s).expect("serialize");
    assert_eq!(json, "\"anthropic\"");

    let back: GenAiSystem = serde_json::from_str("\"open_ai\"").expect("deserialize");
    assert_eq!(back, GenAiSystem::OpenAi);
}

#[test]
fn genai_attrs_full_roundtrip() {
    let mut attrs = GenAiAttrs::new();
    attrs.system = GenAiSystem::Mistral;
    attrs.operation = GenAiOperation::Embedding;
    attrs.response_id = Some("resp_abc123".into());
    attrs.response_model = Some("mistral-large-2".into());
    let json = serde_json::to_string(&attrs).expect("serialize");
    let back: GenAiAttrs = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.system, GenAiSystem::Mistral);
    assert_eq!(back.response_id.as_deref(), Some("resp_abc123"));
}
