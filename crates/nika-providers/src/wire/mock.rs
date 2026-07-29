// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `mock` provider — deterministic · zero network · zero key.
//!
//! This is the `hello.yaml` first-run surface (install → `nika run` in 60
//! seconds with NO API key): a stable echo of the last user message, fixed
//! usage arithmetic, and a 3-delta synthetic stream. Determinism is the
//! contract — engine tests and conformance fixtures rely on byte-stable
//! output for identical input.
//!
//! A request carrying `response_format: JsonSchema` does NOT echo — it
//! returns a SYNTHESIZED schema-conformant instance (`mock_schema` · F3),
//! so every structured workflow dry-runs offline instead of dying
//! NIKA-INFER-002 after burning its retry budget.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use nika_kernel::ai::provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, ProviderError,
    ResponseFormat, Role, StopReason, TokenUsage,
};

use super::mock_schema;
use crate::registry::ResolvedProvider;

/// Deterministic single-shot response — the echo, or (schema attached) a
/// synthesized conformant instance as PURE JSON (no prefix noise: the
/// schema instruction demands "ONLY a JSON value", and the mock complies
/// exactly like a well-behaved model). The conformance-mock contract the
/// runtime/agent fixtures' own notes pin: tools granted ⇒ the mock
/// invokes the FIRST granted tool, every turn, with args synthesized
/// against the tool's own schema — an agent loop drives offline to its
/// `done` (002) or its exhaustion (001). No tools offered (or tool use
/// disabled) ⇒ the plain echo, unchanged.
pub(crate) fn infer<H>(rp: &ResolvedProvider<H>, request: &InferRequest) -> InferResponse {
    let called = match &request.tool_choice {
        nika_kernel::ai::provider::ToolChoice::None => None,
        nika_kernel::ai::provider::ToolChoice::Specific(name) => {
            request.tools.iter().find(|t| &t.name == name)
        }
        _ => request.tools.first(),
    };
    if let Some(tool) = called {
        let input = mock_schema::synthesize(&tool.parameters);
        let echo = last_user_text(request);
        let text = format!(
            "mock({model}) · invoke {tool} · the first granted tool, schema-shaped",
            model = rp.wire_model(),
            tool = tool.name
        );
        let usage = usage_for(&echo, &text);
        let mut resp = InferResponse::new(
            vec![ContentBlock::ToolUse {
                id: format!("mock-call-{}", request.messages.len()),
                name: tool.name.clone(),
                input,
            }],
            usage,
            StopReason::ToolUse,
        );
        resp.request_id = Some("mock-request".to_owned());
        resp.finish_reason_raw = Some("tool_use".to_owned());
        resp.gen_ai.response_id = resp.request_id.clone();
        resp.gen_ai.response_model = Some(rp.wire_model().to_owned());
        return resp;
    }
    let echo = last_user_text(request);
    let text = match &request.response_format {
        ResponseFormat::JsonSchema(schema) => mock_schema::synthesize(schema).to_string(),
        _ => format!("mock({model}) · {echo}", model = rp.wire_model()),
    };
    let usage = usage_for(&echo, &text);

    let mut resp = InferResponse::new(
        vec![ContentBlock::Text { text }],
        usage,
        StopReason::EndTurn,
    );
    resp.request_id = Some("mock-request".to_owned());
    resp.finish_reason_raw = Some("end_turn".to_owned());
    resp.gen_ai.response_id = resp.request_id.clone();
    resp.gen_ai.response_model = Some(rp.wire_model().to_owned());
    resp
}

/// Deterministic 3-delta stream: the response text split in three, then
/// `Usage`, then `Done`.
pub(crate) fn infer_stream<H>(
    rp: &ResolvedProvider<H>,
    request: &InferRequest,
) -> InferEventStream {
    let full = infer(rp, request);
    let mut events = VecDeque::new();
    if let Some(ContentBlock::ToolUse { id, name, input }) = full.content.first() {
        // The tool-call turn streams as the wire protocol speaks it:
        // start · one input delta · usage · done(tool_use).
        events.push_back(Ok(InferEvent::ToolUseStart {
            id: id.clone(),
            name: name.clone(),
        }));
        events.push_back(Ok(InferEvent::ToolUseDelta {
            id: id.clone(),
            partial_json: input.to_string(),
        }));
    } else {
        let text = match full.content.first() {
            Some(ContentBlock::Text { text }) => text.clone(),
            _ => String::new(),
        };
        for piece in split_in_three(&text) {
            if !piece.is_empty() {
                events.push_back(Ok(InferEvent::Delta { text: piece }));
            }
        }
    }
    events.push_back(Ok(InferEvent::Usage(full.usage.clone())));
    events.push_back(Ok(InferEvent::Done {
        stop_reason: full.stop_reason,
        request_id: full.request_id.clone(),
        finish_reason_raw: full.finish_reason_raw.clone(),
    }));
    Box::pin(ReadyStream(events))
}

fn last_user_text(request: &InferRequest) -> String {
    request
        .messages
        .iter()
        .rev()
        .find(|m| !matches!(m.role, Role::Assistant | Role::System))
        .map(|m| {
            m.content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Whitespace-token arithmetic — stable, explainable usage numbers.
fn usage_for(input: &str, output: &str) -> TokenUsage {
    TokenUsage::new(
        input.split_whitespace().count() as u64,
        output.split_whitespace().count() as u64,
    )
}

fn split_in_three(text: &str) -> [String; 3] {
    let chars: Vec<char> = text.chars().collect();
    let third = chars.len().div_ceil(3);
    let mut parts: [String; 3] = Default::default();
    for (i, chunk) in chars.chunks(third.max(1)).take(3).enumerate() {
        parts[i] = chunk.iter().collect();
    }
    parts
}

/// Immediately-ready stream over a queue (the mock never waits).
struct ReadyStream(VecDeque<Result<InferEvent, ProviderError>>);

impl Stream for ReadyStream {
    type Item = Result<InferEvent, ProviderError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(self.0.pop_front())
    }
}

#[cfg(test)]
mod tests {
    use nika_kernel::ai::provider::Message;

    use super::*;
    use crate::registry::{ProviderRegistry, ProvidersConfig};
    use crate::test_support::collect;

    fn resolved() -> ResolvedProvider {
        ProviderRegistry::without_http(ProvidersConfig::new())
            .resolve("mock/echo")
            .expect("mock resolves")
    }

    fn req(text: &str) -> InferRequest {
        InferRequest::new("echo", vec![Message::text(Role::User, text)])
    }

    #[test]
    fn schema_request_synthesizes_pure_json() {
        // F3: a JsonSchema request returns ONLY a conformant JSON value —
        // no `mock(model) ·` prefix (the schema instruction says "ONLY a
        // JSON value" and the mock behaves like a well-behaved model).
        let rp = resolved();
        let mut request = req("extract the fields");
        request.response_format =
            nika_kernel::ai::provider::ResponseFormat::JsonSchema(serde_json::json!({
                "type": "object",
                "required": ["headline", "score"],
                "properties": {
                    "headline": { "type": "string" },
                    "score": { "type": "integer", "minimum": 0 }
                }
            }));
        let a = infer(&rp, &request);
        let b = infer(&rp, &request);
        let (ContentBlock::Text { text: ta }, ContentBlock::Text { text: tb }) =
            (&a.content[0], &b.content[0])
        else {
            panic!("expected text blocks");
        };
        assert_eq!(ta, tb, "byte-stable under a schema too");
        let parsed: serde_json::Value = serde_json::from_str(ta).expect("pure JSON · no prefix");
        assert_eq!(parsed["headline"], "mock");
        assert_eq!(parsed["score"], 0);
        // the plain-echo contract is untouched
        let plain = infer(&rp, &req("hello"));
        let ContentBlock::Text { text } = &plain.content[0] else {
            panic!("expected text");
        };
        assert!(text.starts_with("mock(echo) · "), "{text}");
    }

    #[test]
    fn tools_offered_invokes_the_first_granted_tool_schema_shaped() {
        // The conformance-mock contract (the runtime/agent fixtures' own
        // note): tools granted ⇒ the mock invokes the FIRST granted tool
        // every turn, with args synthesized against the tool's own
        // schema — agent/001 exhausts, agent/002 reaches its done.
        let rp = resolved();
        let mut request = req("loop forever");
        request.tools = vec![
            nika_kernel::ai::provider::ToolDef::new(
                "nika:wait",
                "wait",
                serde_json::json!({ "type": "object", "required": ["ms"], "properties": { "ms": { "type": "integer" } } }),
            ),
            nika_kernel::ai::provider::ToolDef::new(
                "nika:done",
                "done",
                serde_json::json!({ "type": "object" }),
            ),
        ];
        let a = infer(&rp, &request);
        let ContentBlock::ToolUse { id, name, input } = &a.content[0] else {
            panic!("expected a tool call, got {:?}", a.content[0]);
        };
        assert_eq!(name, "nika:wait", "the FIRST granted tool, always");
        assert!(input["ms"].is_number(), "schema-shaped args: {input}");
        assert!(matches!(a.stop_reason, StopReason::ToolUse));
        // Determinism is the contract — same request, same call id.
        let b = infer(&rp, &request);
        let ContentBlock::ToolUse { id: id_b, .. } = &b.content[0] else {
            panic!("expected a tool call");
        };
        assert_eq!(id, id_b, "byte-stable under tools too");

        // Tool use explicitly disabled ⇒ the plain echo, unchanged.
        let mut off = req("loop forever");
        off.tool_choice = nika_kernel::ai::provider::ToolChoice::None;
        off.tools = request.tools.clone();
        let plain = infer(&rp, &off);
        let ContentBlock::Text { text } = &plain.content[0] else {
            panic!("ToolChoice::None must keep the echo");
        };
        assert!(text.starts_with("mock(echo) · "), "{text}");

        // A specific choice names its tool, first or not.
        let mut specific = req("finish");
        specific.tool_choice =
            nika_kernel::ai::provider::ToolChoice::Specific("nika:done".to_owned());
        specific.tools = request.tools;
        let done = infer(&rp, &specific);
        let ContentBlock::ToolUse { name, .. } = &done.content[0] else {
            panic!("expected a tool call");
        };
        assert_eq!(name, "nika:done");
    }

    #[tokio::test]
    async fn stream_mirrors_the_tool_call() {
        let rp = resolved();
        let mut request = req("loop forever");
        request.tools = vec![nika_kernel::ai::provider::ToolDef::new(
            "nika:wait",
            "wait",
            serde_json::json!({ "type": "object" }),
        )];
        let events = collect(infer_stream(&rp, &request)).await;
        let kinds: Vec<String> = events
            .iter()
            .map(|e| match e {
                Ok(InferEvent::ToolUseStart { name, .. }) => format!("start:{name}"),
                Ok(InferEvent::ToolUseDelta { partial_json, .. }) => {
                    format!("delta:{partial_json}")
                }
                Ok(InferEvent::Usage(_)) => "usage".to_owned(),
                Ok(InferEvent::Done { stop_reason, .. }) => format!("done:{stop_reason:?}"),
                other => format!("other:{other:?}"),
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "start:nika:wait".to_owned(),
                "delta:{}".to_owned(),
                "usage".to_owned(),
                "done:ToolUse".to_owned()
            ],
            "{kinds:?}"
        );
    }

    #[test]
    fn deterministic_echo_and_usage() {
        let rp = resolved();
        let a = infer(&rp, &req("bonjour le monde"));
        let b = infer(&rp, &req("bonjour le monde"));
        match (&a.content[0], &b.content[0]) {
            (ContentBlock::Text { text: ta }, ContentBlock::Text { text: tb }) => {
                assert_eq!(ta, tb, "byte-stable");
                assert!(ta.contains("bonjour le monde"));
                assert!(ta.contains("mock(echo)"));
            }
            other => panic!("expected text blocks, got {other:?}"),
        }
        assert_eq!(a.usage.input_tokens, 3);
        assert!(matches!(a.stop_reason, StopReason::EndTurn));
        assert_eq!(a.request_id.as_deref(), Some("mock-request"));
    }

    #[tokio::test]
    async fn stream_is_deltas_usage_done_and_reassembles() {
        let rp = resolved();
        let request = req("trois petits chats");
        let events = collect(infer_stream(&rp, &request)).await;

        let reassembled: String = events
            .iter()
            .filter_map(|e| match e {
                Ok(InferEvent::Delta { text }) => Some(text.clone()),
                _ => None,
            })
            .collect();
        let single = infer(&rp, &request);
        match &single.content[0] {
            ContentBlock::Text { text } => assert_eq!(&reassembled, text),
            other => panic!("expected text, got {other:?}"),
        }
        assert!(matches!(events.last(), Some(Ok(InferEvent::Done { .. }))));
        let usage_position = events
            .iter()
            .position(|e| matches!(e, Ok(InferEvent::Usage(_))))
            .expect("usage emitted");
        assert_eq!(usage_position, events.len() - 2, "usage just before done");
    }
}
