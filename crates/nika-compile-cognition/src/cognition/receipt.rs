// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! One authoring call and what it leaves in the outcome's receipt: the bounded request every
//! authoring call makes, whatever its messages, and the call's journal entry (its role, the
//! digests of what it was shown, its bounds, its result, its usage).

use nika_kernel::ai::provider::{
    InferRequest, InferResponse, Message, ProviderError, ProviderInferDyn, StopReason,
};
use serde_json::{Value, json};

use super::reasoning_record;
use nika_compile_seats::reasoning::{authoring_request, context_entry, response_identity};

use crate::{
    AuthoringCognition, AuthoringPolicy, AuthoringReceipt, CompileOutcome, DiagnosticKind,
};
/// A refused or ignored model payload as a record keeps it ([`withheld`](nika_compile_seats::reasoning::withheld)).
pub(super) use nika_compile_seats::reasoning::{
    journaled, record_proposed, stamp_references, withheld,
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
            context["result"] = match observe::answered(&result) {
                Ok(response) => json!({
                    "stop_reason": format!("{:?}", response.stop_reason),
                    "usage_reported": response.usage_reported,
                    "input_tokens": response.usage_reported.then_some(response.usage.input_tokens),
                    "output_tokens": response.usage_reported.then_some(response.usage.output_tokens),
                }),
                Err(observe::Failure::AdmissionRefused) => {
                    json!({"failure_kind": "admission_refused"})
                }
                Err(observe::Failure::Timeout) => json!({"failure_kind": "timeout"}),
                Err(_) => json!({"failure_kind": "provider_error"}),
            };
            let answered = result.as_ref().ok().and_then(|r| r.as_ref().ok());
            context["reasoning"] = reasoning_record(policy.reasoning, answered);
            context["response"] = answered.map_or(Value::Null, response_identity);
            // What actually left for this call, from the provider's own per-dispatch record (the
            // transport never re-sends one): a call cut by its deadline sent one; a harness counts
            // its own invocations, its requests unknown here.
            context["requests_sent"] =
                if nika_providers::authoring::policy::harness_route(&policy.model) {
                    Value::Null
                } else {
                    json!(match &result {
                        Ok(Ok(response)) => response.inference_calls.len(),
                        Ok(Err(error)) => error.inference_calls().len(),
                        Err(_) => 1,
                    })
                };
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

/// The provider's answer within the policy's deadline, the call's activity told to a scope. A
/// harness bounds its own call by the silence it allows (the request's timeout), never by a total.
async fn timed<P: ProviderInferDyn>(
    policy: &AuthoringPolicy,
    provider: &P,
    role: &'static str,
    request: InferRequest,
) -> Result<Result<InferResponse, ProviderError>, tokio::time::error::Elapsed> {
    let activity = observe::Activity::started(role, &policy.model);
    let result = if nika_providers::authoring::policy::harness_route(&policy.model) {
        Ok(provider.infer(request).await)
    } else {
        tokio::time::timeout(policy.timeout, provider.infer(request)).await
    };
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

#[cfg(test)]
mod tests {
    use super::{PLAN_KEYS, withheld};
    use nika_kernel::ai::provider::{
        ContentBlock, InferRequest, InferResponse, ProviderError, ProviderInferDyn, StopReason,
        TokenUsage,
    };
    use serde_json::{Value, json};
    use std::time::Duration;

    /// A seat that answers after a second, twenty times the policy's total below.
    struct Late;
    impl ProviderInferDyn for Late {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            tokio::time::sleep(Duration::from_secs(1)).await;
            let text = vec![ContentBlock::Text { text: "{}".into() }];
            let usage = TokenUsage::new(0, 0);
            Ok(InferResponse::new(text, usage, StopReason::EndTurn))
        }
    }

    /// A seat that never answers.
    struct Silent;
    impl ProviderInferDyn for Silent {
        async fn infer(&self, _: InferRequest) -> Result<InferResponse, ProviderError> {
            std::future::pending().await
        }
    }

    /// A harness route is left to its own deadline, which bounds its silence, never its total:
    /// an answer later than the policy's total is accepted. An API route keeps that total.
    #[tokio::test]
    async fn only_an_api_route_is_cut_at_the_policy_total() {
        let total = Duration::from_millis(50);
        let call = |model: &str| {
            let policy = super::AuthoringPolicy::new(model, 256, total);
            (policy, InferRequest::new(model, Vec::new()))
        };
        let (policy, request) = call("codex/gpt-6");
        let answered = super::timed(&policy, &Late, "document", request).await;
        assert!(matches!(&answered, Ok(Ok(_))), "{answered:?}");
        let (policy, request) = call("openai/gpt-oss-120b");
        let cut = super::timed(&policy, &Silent, "document", request).await;
        assert!(cut.is_err(), "{cut:?}");
    }

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
