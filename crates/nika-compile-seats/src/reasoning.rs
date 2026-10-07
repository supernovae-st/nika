// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoning one call is asked for and the record of what it reported (R4 B16), shared by
//! the decision seats and every authoring call of the seats' doors. Moved from
//! `nika-compile-cognition`'s authoring receipt with the decision seats (ADR-146), unchanged
//! but for their visibility.

use nika_compile::AuthoringReasoning;
use nika_kernel::ai::provider::{InferResponse, ReasoningEffort};
use serde_json::{Value, json};

/// The provider level an authoring level names (R4 B16): the same word, or `None` for a level
/// the provider seam does not know, which no call may silently drop.
#[must_use]
pub fn effort(reasoning: AuthoringReasoning) -> Option<ReasoningEffort> {
    ReasoningEffort::parse(reasoning.word())
}

/// One call's reasoning, each fact apart (R4 B16): the level the policy configured, the keys the
/// adapter read back from the body it dispatched (`unobserved` when it reports none, or when no
/// response came), the effort the provider served internally (never observable here), the
/// reasoning tokens it reported (null when unreported) and the model it named.
#[must_use]
pub fn reasoning_record(
    configured: Option<AuthoringReasoning>,
    response: Option<&InferResponse>,
) -> Value {
    let transmitted = response
        .and_then(|r| r.reasoning_wire.as_ref())
        .map_or_else(
            || json!("unobserved"),
            |wire| json!({"thinking": wire.thinking, "effort": wire.effort}),
        );
    json!({
        "configured": configured.map(AuthoringReasoning::word),
        "transmitted": transmitted,
        "served": "unknown",
        "reasoning_tokens": response
            .filter(|r| r.usage_reported)
            .and_then(|r| r.usage.reasoning_tokens),
        "response_model": response.and_then(|r| r.gen_ai.response_model.clone()),
    })
}
