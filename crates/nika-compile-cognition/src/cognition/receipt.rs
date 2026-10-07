// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One authoring call and what it leaves in the outcome's receipt: the bounded request every
//! authoring call makes, whatever its messages, and the call's journal entry (its role, the
//! digests of what it was shown, its bounds, its result, its usage).

use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn,
    ResponseFormat, Role, StopReason,
};
use serde_json::{Value, json};

use super::{effort, reasoning_record};
use crate::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileOutcome, DiagnosticKind,
};

/// The observer of authoring calls, owned by the provider layer; this module produces its
/// observations.
pub(crate) use nika_providers::authoring::observe;

/// One bounded call under any answer schema (the plan's, the transform's), accounted in the
/// outcome's receipt: the raw response, or None with the finding recorded. It opens at the
/// policy's initial output limit; a reported truncation below the hard ceiling asks the same
/// call once more at the ceiling (`widened_to` on the cut call's entry), a second request
/// journaled and charged like the first. Nothing else is retried.
pub(super) async fn call_with_schema<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    messages: Vec<Message>,
    schema: Value,
    out: &mut CompileOutcome,
) -> Option<InferResponse> {
    let opening = opening_limit(policy, out);
    let widen = opening < policy.max_tokens;
    let kept = widen.then(|| (messages.clone(), schema.clone()));
    let response = send(policy, provider, role, (messages, schema), opening, out).await?;
    let Some((messages, schema)) = kept.filter(|_| response.stop_reason == StopReason::MaxTokens)
    else {
        return Some(response);
    };
    let receipt = out.provenance.authoring.as_mut();
    if let Some(context) = receipt.and_then(|r| r.context.last_mut()) {
        context["widened_to"] = json!(policy.max_tokens);
    }
    let ceiling = policy.max_tokens;
    send(policy, provider, role, (messages, schema), ceiling, out).await
}

/// The output limit a call opens at: the policy's initial limit, or its hard ceiling when none
/// is set or once this compile has widened a cut answer (its journal says so).
fn opening_limit(policy: &AuthoringPolicy, out: &CompileOutcome) -> u32 {
    let widened = (out.provenance.authoring.as_ref())
        .is_some_and(|r| r.context.iter().any(|c| c.get("widened_to").is_some()));
    match policy.initial_max_tokens {
        Some(initial) if !widened => initial.min(policy.max_tokens),
        _ => policy.max_tokens,
    }
}

/// One request at the output limit `cap`, accounted in the outcome's receipt.
async fn send<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    (messages, schema): (Vec<Message>, Value),
    cap: u32,
    out: &mut CompileOutcome,
) -> Option<InferResponse> {
    let entry = context_entry(role, &messages, &schema);
    let prompt = observe::prompt(&messages);
    let identity = |key: &str| entry[key].as_str().unwrap_or_default().to_owned();
    let schema_sha256 = identity("schema_sha256");
    let instruction_sha256 = identity("instruction_sha256");
    let Some(request) = authoring_request(policy, messages, schema, cap) else {
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
        context["max_output_tokens"] = json!(cap);
        context["timeout_ms"] = json!(policy.timeout.as_millis());
    }
    let start = std::time::Instant::now();
    let result = timed(policy, provider, role, request).await;
    // The answer as received, before any decode, to a host's scope (a request never built above
    // made no call and is not observed).
    observe::emit(
        role,
        prompt.as_ref(),
        &schema_sha256,
        &instruction_sha256,
        observe::answered(&result),
    );
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
                timeout_message(policy),
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

/// The provider's answer within the policy's deadline, the call's activity told to a scope.
async fn timed<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    request: InferRequest,
) -> Result<Result<InferResponse, ProviderError>, tokio::time::error::Elapsed> {
    let activity = observe::Activity::started(role, &policy.model);
    let result = tokio::time::timeout(policy.timeout, provider.infer(request)).await;
    if let Some(call) = activity {
        call.finish();
    }
    result
}

fn timeout_message(policy: &AuthoringPolicy) -> String {
    let local_hint = if policy.model.starts_with("ollama/") {
        " Check the local model's allocated context before retrying; waiting longer does not prevent server-side input truncation."
    } else {
        ""
    };
    format!(
        "An authorized authoring call timed out after its {}s limit. No retry occurred.{local_hint}",
        policy.timeout.as_secs_f64()
    )
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

/// The bounded JSON-schema request every authoring call makes, whatever its messages, at the
/// output limit `cap` (never above the policy's ceiling), with the policy's explicit reasoning
/// effort; `None` when that effort has no provider level.
fn authoring_request(
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
