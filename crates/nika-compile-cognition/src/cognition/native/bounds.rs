// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Reported output truncation may consume a repair, within the original hard ceiling.
use super::{AuthoringPolicy, Talk, knowledge};
use nika_kernel::ai::provider::{ContentBlock, InferResponse, Message, Role, StopReason};
use serde_json::json;

pub(super) fn expand(
    response: &InferResponse,
    round: u32,
    policy: &mut AuthoringPolicy,
    hard_max_tokens: u32,
    talk: &mut Talk,
) -> bool {
    if response.stop_reason != StopReason::MaxTokens
        || !response.usage_reported
        || round >= policy.repairs.min(5)
        || policy.max_tokens >= hard_max_tokens
    {
        return false;
    }
    let text: String = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let previous = policy.max_tokens;
    policy.max_tokens = previous.saturating_mul(2).min(hard_max_tokens);
    talk.rounds.push(json!({
        "round": round,
        "answer": "cut at initial output limit",
        "response_sha256": knowledge::sha256(&text),
        "output_tokens": response.usage.output_tokens,
        "previous_max_tokens": previous,
        "next_max_tokens": policy.max_tokens,
        "hard_max_tokens": hard_max_tokens,
        "repair": "larger answer within original hard ceiling and repair count",
    }));
    talk.messages.push(Message::text(Role::User, json!({
        "kind": "answer_truncated",
        "instruction": "The previous response reached its output limit and was not a complete candidate. Return a complete answer for the same original request, answers and context. Nothing from the truncated response was accepted or executed.",
        "max_output_tokens": policy.max_tokens,
    }).to_string()));
    true
}
