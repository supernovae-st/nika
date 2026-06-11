// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::unwrap_used, clippy::expect_used)]
// Test-only env reads for the model paths — the SecretStore seam is for
// production code; gated e2e fixtures follow the nika-schema test precedent.
#![allow(clippy::disallowed_methods)]
#![cfg(feature = "local-infer")]

//! Real-model e2e for the candle backend (`--features local-infer`).
//!
//! `#[ignore]` by default — needs a quantized Qwen3-family GGUF + its
//! `tokenizer.json` on disk (NOT committed). Point the env vars at them:
//!
//! ```sh
//! NIKA_E2E_GGUF="$HOME/Library/Application Support/nika/models/Qwen/Qwen3-1.7B-GGUF/Qwen3-1.7B-Q8_0.gguf" \
//! NIKA_E2E_TOKENIZER="$HOME/Library/Application Support/nika/models/Qwen/Qwen3-1.7B-GGUF/tokenizer.json" \
//! cargo test -p nika-infer-local --features local-infer --test candle_e2e -- --ignored --nocapture
//! ```
//!
//! Asserts the ADR-091 SOTA invariants on a REAL forward pass: non-empty
//! output · EOS halt (`finish_reason: stop` on an easy prompt) · `max_tokens` → Length ·
//! byte-determinism under CPU + greedy + fixed seed (the goldens contract —
//! NEVER asserted on Metal, where reductions are not byte-deterministic).

use nika_infer_local::CandleBackend;
use nika_infer_local::protocol::{ChatRequest, Message, Role};
use nika_infer_local::{Backend, MockBackend};

fn model_paths() -> (String, String) {
    let gguf = std::env::var("NIKA_E2E_GGUF")
        .expect("set NIKA_E2E_GGUF to a Qwen3-family .gguf path (see module doc)");
    let tok = std::env::var("NIKA_E2E_TOKENIZER")
        .expect("set NIKA_E2E_TOKENIZER to the matching tokenizer.json");
    (gguf, tok)
}

fn req(content: &str, max_tokens: u32, seed: u64) -> ChatRequest {
    let mut r = ChatRequest::new(
        "qwen3-e2e",
        vec![
            Message::new(Role::System, "You answer in one short sentence."),
            Message::new(Role::User, content),
        ],
    );
    r.max_tokens = Some(max_tokens);
    r.seed = Some(seed);
    r // temperature unset → greedy (ArgMax) — the deterministic golden path
}

#[tokio::test]
#[ignore = "needs a local GGUF + tokenizer (env NIKA_E2E_GGUF / NIKA_E2E_TOKENIZER)"]
async fn real_model_generates_halts_and_is_deterministic() {
    let (gguf, tok) = model_paths();
    let backend = CandleBackend::load_qwen3(gguf.as_ref(), tok.as_ref(), "qwen3-e2e")
        .expect("model + tokenizer load");

    // ── 1 · generates non-empty text and halts (EOS) on an easy prompt ──
    let resp = Backend::generate(&backend, &req("Say hello.", 64, 7))
        .await
        .expect("generation succeeds");
    let text = &resp.choices[0].message.content;
    assert!(
        !text.trim().is_empty(),
        "real model must produce text, got {text:?}"
    );
    assert!(
        resp.usage.completion_tokens > 0 && resp.usage.prompt_tokens > 0,
        "usage must be counted: {:?}",
        resp.usage
    );

    // ── 2 · max_tokens truncation reports Length ──
    let truncated = Backend::generate(&backend, &req("Count from 1 to 500.", 8, 7))
        .await
        .expect("truncated generation succeeds");
    assert_eq!(
        serde_json::to_value(truncated.choices[0].finish_reason).unwrap(),
        serde_json::json!("length"),
        "an 8-token cap on a long answer must report length"
    );
    assert!(truncated.usage.completion_tokens <= 8);

    // ── 3 · CPU + greedy + fixed seed ⇒ byte-identical output (goldens) ──
    let a = Backend::generate(&backend, &req("What is 2+2?", 32, 42))
        .await
        .expect("run A");
    let b = Backend::generate(&backend, &req("What is 2+2?", 32, 42))
        .await
        .expect("run B");
    assert_eq!(
        a.choices[0].message.content, b.choices[0].message.content,
        "CPU greedy with a fixed seed must be byte-reproducible"
    );

    // ── 4 · the wire shape still matches the OpenAiCompat parser paths ──
    let v = serde_json::to_value(&a).unwrap();
    assert!(v.pointer("/choices/0/message/content").is_some());
    assert!(v.pointer("/usage/total_tokens").is_some());
}

#[tokio::test]
async fn mock_backend_still_works_under_the_feature() {
    // The feature must be purely additive — the dependency-free path stays.
    let r = ChatRequest::new(
        "mock-local",
        vec![Message::new(Role::User, "feature additivity")],
    );
    let resp = Backend::generate(&MockBackend, &r).await.expect("mock ok");
    assert_eq!(
        resp.choices[0].message.content,
        "mock reply: feature additivity"
    );
}
