// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `mock` provider — deterministic · zero network · zero key.
//!
//! This is the `hello.yaml` first-run surface (install → `nika run` in 60
//! seconds with NO API key): a stable echo of the last user message, fixed
//! usage arithmetic, and a 3-delta synthetic stream. Determinism is the
//! contract — engine tests and conformance fixtures rely on byte-stable
//! output for identical input.

use std::collections::VecDeque;
use std::pin::Pin;
use std::task::{Context, Poll};

use futures_core::Stream;
use nika_kernel::ai::provider::{
    ContentBlock, InferEvent, InferEventStream, InferRequest, InferResponse, ProviderError, Role,
    StopReason, TokenUsage,
};

use crate::registry::ResolvedProvider;

/// Deterministic single-shot response.
pub(crate) fn infer<H>(rp: &ResolvedProvider<H>, request: &InferRequest) -> InferResponse {
    let echo = last_user_text(request);
    let text = format!("mock({model}) · {echo}", model = rp.wire_model());
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
    let text = match full.content.first() {
        Some(ContentBlock::Text { text }) => text.clone(),
        _ => String::new(),
    };
    let mut events = VecDeque::new();
    for piece in split_in_three(&text) {
        if !piece.is_empty() {
            events.push_back(Ok(InferEvent::Delta { text: piece }));
        }
    }
    events.push_back(Ok(InferEvent::Usage(full.usage.clone())));
    events.push_back(Ok(InferEvent::Done {
        stop_reason: StopReason::EndTurn,
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
