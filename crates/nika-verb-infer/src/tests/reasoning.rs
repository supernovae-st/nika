// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The verb carries an explicit reasoning effort to the provider request (R4 B16 · C11), as the
//! bytes the capturing http seam received show: on the route whose catalog lists the level, the
//! body holds `thinking` enabled and the level, never the implicit `low` a short structured call
//! gets; on a route that lists none (a suffixed name the catalog never qualifies) nothing
//! leaves; without one the body keeps its bytes.

use super::*;

fn answer(text: &str) -> String {
    json!({"choices":[{"message":{"content":text},"finish_reason":"stop"}],
        "usage":{"prompt_tokens":20,"completion_tokens":4}})
    .to_string()
}

fn deepseek(seam: &Arc<SeamHttp>, model: &str) -> InferVerb<SeamHttp> {
    let registry = Registry::new(
        Arc::clone(seam),
        ProvidersConfig::new().with_key("deepseek", Secret::new("sk-test")),
    );
    InferVerb::new(Arc::new(registry), model)
}

/// A short structured call (a label's ceiling): the shape the implicit `low` would take.
fn short_structured(effort: Option<ReasoningEffort>) -> InferInput {
    let mut input = InferInput::new("route this line");
    input.max_tokens = Some(4096);
    input.schema = Some(json!({"type":"object","additionalProperties":false,
        "required":["v"],"properties":{"v":{"type":"string"}}}));
    input.reasoning_effort = effort;
    input
}

fn sent_body(seam: &SeamHttp) -> serde_json::Value {
    let sent = seam.captured();
    assert_eq!(sent.len(), 1, "one request");
    serde_json::from_slice(sent[0].body.as_deref().unwrap_or_default()).expect("a json body")
}

#[tokio::test]
async fn an_explicit_effort_rides_the_dispatched_body_on_a_qualified_route() {
    let seam = SeamHttp::with_json(&[&answer(r#"{"v":"work"}"#)]);
    deepseek(&seam, "deepseek/deepseek-v4-pro")
        .run(short_structured(Some(ReasoningEffort::Max)))
        .await
        .expect("answers");
    let body = sent_body(&seam);
    assert_eq!(body["thinking"]["type"], "enabled", "{body}");
    assert_eq!(
        body["reasoning_effort"], "max",
        "never lowered by the cap: {body}"
    );
}

#[tokio::test]
async fn an_explicit_effort_on_a_route_that_lists_no_level_sends_nothing() {
    let seam = SeamHttp::with_json(&[&answer(r#"{"v":"work"}"#)]);
    let refused = deepseek(&seam, "deepseek/deepseek-flash-0731")
        .run(short_structured(Some(ReasoningEffort::Max)))
        .await;
    assert!(refused.is_err(), "{refused:?}");
    assert!(seam.captured().is_empty(), "nothing was sent");
}

#[tokio::test]
async fn without_an_effort_the_body_is_what_it_was() {
    let seam = SeamHttp::with_json(&[&answer(r#"{"v":"work"}"#)]);
    deepseek(&seam, "deepseek/deepseek-v4-pro")
        .run(short_structured(None))
        .await
        .expect("answers");
    let body = sent_body(&seam);
    assert!(body.get("thinking").is_none(), "{body}");
    assert_ne!(body["reasoning_effort"], "max", "{body}");
    assert_eq!(InferInput::new("q").reasoning_effort, None);
}
