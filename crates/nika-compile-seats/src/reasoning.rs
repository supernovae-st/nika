// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The reasoning one call is asked for and the record of what it reported (R4 B16), shared by
//! the decision seats and every authoring call of the seats' doors. Moved from
//! `nika-compile-cognition`'s authoring receipt with the decision seats (ADR-146), unchanged
//! but for their visibility; the bounded request every authoring call makes and the pieces of
//! its journal entry (what it was shown, what it returned, a payload withheld) followed at that
//! crate's size cap (2026-10-08).

use nika_compile::surface::sha256;
use nika_compile::{AuthoringPolicy, AuthoringReasoning, CompileOutcome};
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ReasoningEffort, ResponseFormat, Role,
};
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

/// The identity of what one answered call returned (its text blocks, by digest and length, and
/// how many blocks of any kind it held), so a later decode, refusal or repair names the bytes it
/// read. A call that returned nothing records `null`, never an empty answer.
#[must_use]
pub fn response_identity(response: &InferResponse) -> Value {
    let text: String = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    json!({
        "sha256": sha256(&text),
        "bytes": text.len(),
        "blocks": response.content.len(),
    })
}

/// A refused or ignored model payload as a record keeps it: its digest and length, its shape (the
/// JSON type, the door's own `known` keys it carries and how many other keys) and the reason it
/// was not used — never its text, which may echo anything the model was shown.
#[must_use]
pub fn withheld(text: &str, known: &[&str], reason: &str) -> Value {
    let shape = match serde_json::from_str::<Value>(text) {
        Ok(Value::Object(map)) => {
            let mut keys: Vec<&str> = known
                .iter()
                .copied()
                .filter(|key| map.contains_key(*key))
                .collect();
            keys.sort_unstable();
            json!({"type": "object", "known_keys": keys, "other_keys": map.len() - keys.len()})
        }
        Ok(Value::Array(items)) => json!({"type": "array", "items": items.len()}),
        Ok(_) => json!({"type": "scalar"}),
        Err(_) => json!({"type": "not_json"}),
    };
    json!({
        "withheld": true,
        "sha256": sha256(text),
        "bytes": text.len(),
        "shape": shape,
        "reason": reason,
    })
}

/// What one call received: its role, the sha256 of its instruction (the system message)
/// and of its answer schema, the bytes of its messages, and the references sent with it.
#[must_use]
pub fn context_entry(role: &str, messages: &[Message], schema: &Value) -> Value {
    let sha = sha256;
    let text_of = |m: &Message| -> String {
        m.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    };
    let instruction = messages
        .iter()
        .find(|m| matches!(m.role, Role::System))
        .map(text_of)
        .unwrap_or_default();
    let bytes: usize = messages.iter().map(|m| text_of(m).len()).sum();
    json!({
        "call": role,
        "instruction_sha256": sha(&instruction),
        "schema_sha256": sha(&schema.to_string()),
        "message_bytes": bytes,
        "references": [],
    })
}

/// The bounded JSON-schema request every authoring call makes, whatever its messages, at the
/// output limit `cap` (never above the policy's ceiling), with the policy's explicit reasoning
/// effort; `None` when that effort has no provider level.
#[must_use]
pub fn authoring_request(
    policy: &AuthoringPolicy,
    messages: Vec<Message>,
    schema: Value,
    cap: u32,
) -> Option<InferRequest> {
    let mut infer = InferRequest::new(&policy.model, messages);
    infer.max_tokens = Some(cap.min(policy.max_tokens));
    infer.timeout = Some(policy.timeout);
    infer.response_format = ResponseFormat::JsonSchema(schema);
    if let Some(reasoning) = policy.reasoning {
        infer.reasoning_effort = Some(effort(reasoning)?);
    }
    Some(infer)
}

/// The semantic object a call's answer proposed, on that call's journal entry. An object that
/// decoded as the door's closed shape (`known` are its keys) is kept exactly; a refused one is
/// [`withheld`]: a model's arbitrary text never reaches the shared record. Data for forensics,
/// never read back as a plan.
pub fn record_proposed(out: &mut CompileOutcome, object: &str, decoded: bool, known: &[&str]) {
    if let Some(call) = out
        .provenance
        .authoring
        .as_mut()
        .and_then(|receipt| receipt.context.last_mut())
    {
        call["proposed"] = if decoded {
            json!({
                "decoded": true,
                "sha256": sha256(object),
                "object": serde_json::from_str::<Value>(object).ok(),
            })
        } else {
            let mut kept = withheld(object, known, "not the door's closed shape; never read");
            kept["decoded"] = json!(false);
            kept
        };
    }
}

/// The references a call's messages actually carried, on the journal entry of the call made
/// after `before` entries (R4 A11, E36): a call that was never journaled is left alone.
pub fn stamp_references(out: &mut CompileOutcome, before: usize, receipts: &Value) {
    if let Some(receipt) = out.provenance.authoring.as_mut()
        && receipt.context.len() > before
        && let Some(entry) = receipt.context.last_mut()
    {
        entry["references"] = receipts.clone();
    }
}

/// The number of journaled calls so far.
#[must_use]
pub fn journaled(out: &CompileOutcome) -> usize {
    out.provenance
        .authoring
        .as_ref()
        .map_or(0, |receipt| receipt.context.len())
}
