// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One authoring call and what it leaves in the outcome's receipt: the bounded request every
//! authoring call makes, whatever its messages, and the call's journal entry (its role, the
//! digests of what it was shown, its bounds, its result, its usage).

use nika_compile::AuthoringReasoning;
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ReasoningEffort, ResponseFormat, Role,
};
use serde_json::{Value, json};

use crate::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileOutcome, DiagnosticKind,
};

/// The provider level an authoring level names (R4 B16): the same word, or `None` for a level
/// the provider seam does not know, which no call may silently drop.
pub(crate) fn effort(reasoning: AuthoringReasoning) -> Option<ReasoningEffort> {
    ReasoningEffort::parse(reasoning.word())
}

/// One call's reasoning, each fact apart (R4 B16): the level the policy configured, the keys the
/// adapter read back from the body it dispatched (`unobserved` when it reports none, or when no
/// response came), the effort the provider served internally (never observable here), the
/// reasoning tokens it reported (null when unreported) and the model it named.
pub(crate) fn reasoning_record(
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

/// One bounded call under any answer schema (the plan's, the transform's), accounted in the
/// outcome's receipt: the raw response, or None with the finding recorded. Never retries.
pub(super) async fn call_with_schema<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    messages: Vec<Message>,
    schema: Value,
    out: &mut CompileOutcome,
) -> Option<InferResponse> {
    let entry = context_entry(role, &messages, &schema);
    let Some(request) = authoring_request(policy, messages, schema) else {
        crate::finding(
            out,
            DiagnosticKind::Unknown,
            "authoring_provider",
            "The configured reasoning effort has no provider level; no request was sent.",
        );
        return None;
    };
    out.provenance.cognition = AuthoringCognition::ExplicitProvider;
    let receipt = out
        .provenance
        .authoring
        .get_or_insert_with(|| AuthoringReceipt::new(policy.model.clone()));
    receipt.calls += 1;
    receipt.context.push(entry);
    if let Some(context) = receipt.context.last_mut() {
        context["max_output_tokens"] = json!(policy.max_tokens);
        context["timeout_ms"] = json!(policy.timeout.as_millis());
    }
    let start = std::time::Instant::now();
    let result = tokio::time::timeout(policy.timeout, provider.infer(request)).await;
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        receipt.elapsed_ms = receipt.elapsed_ms.saturating_add(elapsed_ms);
        if let Some(context) = receipt.context.last_mut() {
            context["elapsed_ms"] = json!(elapsed_ms);
            context["result"] = match &result {
                Ok(Ok(response)) => json!({
                    "stop_reason": format!("{:?}", response.stop_reason),
                    "usage_reported": response.usage_reported,
                    "input_tokens": response.usage_reported.then_some(response.usage.input_tokens),
                    "output_tokens": response.usage_reported.then_some(response.usage.output_tokens),
                }),
                Ok(Err(ProviderError::AdmissionDenied { .. })) => {
                    json!({"failure_kind": "admission_refused"})
                }
                Ok(Err(_)) => json!({"failure_kind": "provider_error"}),
                Err(_) => json!({"failure_kind": "timeout"}),
            };
            let answered = result.as_ref().ok().and_then(|r| r.as_ref().ok());
            context["reasoning"] = reasoning_record(policy.reasoning, answered);
            context["response"] = answered.map_or(Value::Null, response_identity);
        }
    }
    let response = match result {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            // A local refusal says why in its own words; the kernel's prefix names another door.
            let message = match error {
                ProviderError::AdmissionDenied { reason } => reason,
                other => other.to_string(),
            };
            crate::finding(out, DiagnosticKind::Unknown, "authoring_provider", message);
            return None;
        }
        Err(_) => {
            crate::finding(
                out,
                DiagnosticKind::Unknown,
                "authoring_provider",
                "An authorized authoring call timed out. No retry occurred.",
            );
            return None;
        }
    };
    // The totals keep every usage a call reported; whether they are complete is read from the
    // calls' own results (`authority::usage_complete`), never assumed from a partial sum.
    if let Some(receipt) = out.provenance.authoring.as_mut()
        && response.usage_reported
    {
        receipt.input_tokens =
            Some(receipt.input_tokens.unwrap_or(0) + response.usage.input_tokens);
        receipt.output_tokens =
            Some(receipt.output_tokens.unwrap_or(0) + response.usage.output_tokens);
    }
    Some(response)
}

/// The identity of what one answered call returned (its text blocks, by digest and length, and
/// how many blocks of any kind it held), so a later decode, refusal or repair names the bytes it
/// read. A call that returned nothing records `null`, never an empty answer.
fn response_identity(response: &InferResponse) -> Value {
    let text: String = response
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    json!({
        "sha256": super::knowledge::sha256(&text),
        "bytes": text.len(),
        "blocks": response.content.len(),
    })
}

/// The semantic object a call's answer proposed, on that call's journal entry. An object that
/// decoded as the door's closed shape (`known` are its keys) is kept exactly; a refused one is
/// [`withheld`]: a model's arbitrary text never reaches the shared record. Data for forensics,
/// never read back as a plan.
pub(super) fn record_proposed(
    out: &mut CompileOutcome,
    object: &str,
    decoded: bool,
    known: &[&str],
) {
    if let Some(call) = out
        .provenance
        .authoring
        .as_mut()
        .and_then(|receipt| receipt.context.last_mut())
    {
        call["proposed"] = if decoded {
            json!({
                "decoded": true,
                "sha256": super::knowledge::sha256(object),
                "object": serde_json::from_str::<Value>(object).ok(),
            })
        } else {
            let mut kept = withheld(object, known, "not the door's closed shape; never read");
            kept["decoded"] = json!(false);
            kept
        };
    }
}

/// The keys of the closed plan shape (`assets/plan_schema.json`): a refused proposal keeps only
/// these names, by shape.
pub(super) const PLAN_KEYS: &[&str] = &[
    "steps",
    "effects",
    "obligations",
    "constraints",
    "unknowns",
    "regions",
    "approval_bypass",
];

/// A refused or ignored model payload as a record keeps it: its digest and length, its shape (the
/// JSON type, the door's own `known` keys it carries and how many other keys) and the reason it
/// was not used — never its text, which may echo anything the model was shown.
pub(super) fn withheld(text: &str, known: &[&str], reason: &str) -> Value {
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
        "sha256": super::knowledge::sha256(text),
        "bytes": text.len(),
        "shape": shape,
        "reason": reason,
    })
}

/// The references a call's messages actually carried, on the journal entry of the call made
/// after `before` entries (R4 A11, E36): a call that was never journaled is left alone.
pub(super) fn stamp_references(out: &mut CompileOutcome, before: usize, receipts: &Value) {
    if let Some(receipt) = out.provenance.authoring.as_mut()
        && receipt.context.len() > before
        && let Some(entry) = receipt.context.last_mut()
    {
        entry["references"] = receipts.clone();
    }
}

/// The number of journaled calls so far.
pub(super) fn journaled(out: &CompileOutcome) -> usize {
    out.provenance
        .authoring
        .as_ref()
        .map_or(0, |receipt| receipt.context.len())
}

/// What one call received: its role, the sha256 of its instruction (the system message)
/// and of its answer schema, the bytes of its messages, and the references sent with it.
fn context_entry(role: &str, messages: &[Message], schema: &Value) -> Value {
    let sha = super::knowledge::sha256;
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

/// The bounded JSON-schema request every authoring call makes, whatever its messages, with the
/// policy's explicit reasoning effort; `None` when that effort has no provider level.
fn authoring_request(
    policy: &AuthoringPolicy,
    messages: Vec<Message>,
    schema: Value,
) -> Option<InferRequest> {
    let mut infer = InferRequest::new(&policy.model, messages);
    infer.max_tokens = Some(policy.max_tokens);
    infer.timeout = Some(policy.timeout);
    infer.response_format = ResponseFormat::JsonSchema(schema);
    if let Some(reasoning) = policy.reasoning {
        infer.reasoning_effort = Some(effort(reasoning)?);
    }
    Some(infer)
}

#[cfg(test)]
mod tests {
    use super::{PLAN_KEYS, withheld};
    use serde_json::{Value, json};

    #[test]
    fn the_plan_keys_are_the_closed_schema_keys() {
        let schema: Value =
            serde_json::from_str(include_str!("../../assets/plan_schema.json")).unwrap();
        let mut keys: Vec<&str> = schema["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        let mut known = PLAN_KEYS.to_vec();
        known.sort_unstable();
        assert_eq!(keys, known);
    }

    #[test]
    fn a_withheld_payload_keeps_its_digest_and_shape_never_its_text() {
        let canary = "sk-withheld-canary-42";
        for text in [
            json!({"steps": [], "api_key": canary}).to_string(),
            json!({canary: 1}).to_string(),
            json!([canary, canary]).to_string(),
            format!("\"{canary}\""),
            format!("not json {canary}"),
        ] {
            let kept = withheld(&text, PLAN_KEYS, "refused");
            assert!(!kept.to_string().contains(canary), "{kept}");
            assert_eq!(kept["sha256"], crate::cognition::knowledge::sha256(&text));
            assert_eq!(kept["bytes"], text.len());
            assert_eq!(kept["withheld"], true);
        }
        let kept = withheld(
            &json!({"steps": [], "api_key": 1}).to_string(),
            PLAN_KEYS,
            "r",
        );
        assert_eq!(
            kept["shape"],
            json!({"type": "object", "known_keys": ["steps"], "other_keys": 1})
        );
    }
}
