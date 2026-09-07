// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! the OpenAI-compatible usage parse, judged at BOTH doors —
//! split from `openai_compat.rs` at the 1500-line file cap.

use nika_kernel::ai::provider::{InferEvent, InferRequest, Message, Role};

use super::openai_compat::{infer, infer_stream};
use crate::test_support::{FakeHttp, collect, resolved_with};

fn req(messages: Vec<Message>) -> InferRequest {
    InferRequest::new("test-model", messages)
}

/// the two doors read the SAME usage. The measured payload
/// (prompt 5015 of which 4992 cached · one completion token) parses
/// identically through the non-stream response and through the
/// stream translator — before this, the stream built a bare
/// `TokenUsage::new(prompt, completion)` and a streamed cached
/// prompt would have priced at the full input rate.
#[tokio::test]
async fn the_two_doors_parse_one_usage_object_the_same_way() {
    const USAGE: &str = concat!(
        r#"{"prompt_tokens":5015,"completion_tokens":1,"total_tokens":5016,"#,
        r#""prompt_tokens_details":{"cached_tokens":4992},"#,
        r#""completion_tokens_details":{"reasoning_tokens":0}}"#,
    );
    let body = format!(
        r#"{{"id":"chatcmpl-p","model":"gpt-4o-mini-2024-07-18",
           "choices":[{{"message":{{"content":"y"}},"finish_reason":"stop"}}],
           "usage":{USAGE}}}"#
    );
    let fake = FakeHttp::with_json(200, &body);
    let rp = resolved_with(&fake, "openai", "sk-test");
    let direct = infer(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("answers");

    let sse = format!("data: {{\"choices\":[],\"usage\":{USAGE}}}\n\ndata: [DONE]\n\n");
    let fake = FakeHttp::with_stream(200, &sse, 8);
    let rp = resolved_with(&fake, "openai", "sk-test");
    let stream = infer_stream(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("opens");
    let streamed = collect(stream)
        .await
        .into_iter()
        .find_map(|e| match e {
            Ok(InferEvent::Usage(u)) => Some(u),
            _ => None,
        })
        .expect("the stream carries a usage frame");

    assert_eq!(direct.usage, streamed, "one parse, two doors");
    assert_eq!(streamed.input_tokens, 5015);
    assert_eq!(streamed.cache_read_tokens, Some(4992));
    assert_eq!(streamed.output_tokens, 1);
    // The responder names itself on the non-stream door.
    assert_eq!(
        direct.gen_ai.response_model.as_deref(),
        Some("gpt-4o-mini-2024-07-18")
    );
}

/// `DeepSeek` reports its prompt cache at the TOP of the usage object;
/// zero parse existed for it, so a cached `DeepSeek` prompt priced at
/// the full input rate (an over-report — safe direction, not honest).
#[tokio::test]
async fn a_deepseek_shaped_usage_lands_in_the_cached_split() {
    let body = r#"{"id":"ds-1","model":"deepseek-chat",
        "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":1000,"completion_tokens":7,
                 "prompt_cache_hit_tokens":896,"prompt_cache_miss_tokens":104}}"#;
    let fake = FakeHttp::with_json(200, body);
    let rp = resolved_with(&fake, "deepseek", "sk-test");
    let resp = infer(&rp, req(vec![Message::text(Role::User, "x")]))
        .await
        .expect("answers");
    assert_eq!(resp.usage.input_tokens, 1000);
    assert_eq!(
        resp.usage.cache_read_tokens,
        Some(896),
        "the hit portion prices at the cache-read rate, not the input rate"
    );
}
