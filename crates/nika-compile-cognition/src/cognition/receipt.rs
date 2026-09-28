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
