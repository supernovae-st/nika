// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Request assembly — the transcript the loop opens with, the per-turn
//! `InferRequest`, the final schema-constrained re-ask, and the BM25
//! routing query. Moved VERBATIM out of `lib.rs` (the loop file) so the
//! loop file stays within its 1,500-line budget; behavior is pinned by
//! the tests that already cover each builder through the loop.

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, Message, ResponseFormat, Role, ToolDef,
};

use crate::AgentInput;

/// One turn's request — the live transcript + this turn's routed tools.
/// `defs` is consumed (the router already handed us an owned `Vec`) so a
/// large fail-open universe isn't cloned a second time per turn.
pub(crate) fn build_request(
    model: &str,
    messages: Vec<Message>,
    input: &AgentInput,
    defs: Vec<ToolDef>,
) -> InferRequest {
    let mut request = InferRequest::new(model, messages);
    request.temperature = input.temperature;
    request.tools = defs;
    request
}

/// The FINAL schema-constrained re-ask request (BUG#11): tools OFF (the
/// schema constraint and tool-calling do not reliably coexist in one
/// request across providers — anthropic rejects `response_format`,
/// openai/gemini are fragile), with the schema wired natively when the
/// provider supports it (`infer`+schema parity · `build_request` mirror).
pub(crate) fn schema_request(
    model: &str,
    messages: Vec<Message>,
    input: &AgentInput,
    schema: &serde_json::Value,
    native: bool,
) -> InferRequest {
    let mut request = InferRequest::new(model, messages);
    request.temperature = input.temperature;
    // tools deliberately left empty (default) — see the doc comment.
    if native {
        request.response_format = ResponseFormat::JsonSchema(schema.clone());
    }
    request
}

/// Per-component character budgets for the routing query — the live tail
/// (`last_text` · `last_observations`) must always reach the ranker, so a
/// long prompt can't evict it via a single tail-truncating cap.
const QUERY_PROMPT_CHARS: usize = 2048;
const QUERY_TEXT_CHARS: usize = 1024;
const QUERY_OBS_CHARS: usize = 1024;

/// Build the BM25 routing query from the live task context, each piece
/// bounded independently (a 100 KB prompt can't crowd out the model's
/// last words or the last observations — the signal routing ranks on).
pub(crate) fn routing_query(prompt: &str, last_text: &str, last_observations: &str) -> String {
    let take = |s: &str, n: usize| s.chars().take(n).collect::<String>();
    format!(
        "{} {} {}",
        take(prompt, QUERY_PROMPT_CHARS),
        take(last_text, QUERY_TEXT_CHARS),
        take(last_observations, QUERY_OBS_CHARS),
    )
}

/// The opening transcript: optional system, then the user prompt.
pub(crate) fn opening_messages(input: &AgentInput) -> Vec<Message> {
    let mut messages = Vec::with_capacity(2);
    if let Some(system) = &input.system {
        messages.push(Message::text(Role::System, system.clone()));
    }
    messages.push(Message::text(Role::User, input.prompt.clone()));
    messages
}

/// Concatenate a response's text blocks (the assistant's words).
pub(crate) fn joined_text(content: &[ContentBlock]) -> String {
    content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}
