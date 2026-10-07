// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Closed metadata and private returned Text are separate projections.

use nika_onboard::compile::observe::{Answered, AuthoringObservation, Failure, TextBlocks};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::policy::CapturePolicy;

/// Only identities needed to correlate a later closed receipt; never retained raw Text.
pub(super) struct Identity {
    pub ordinal: u32,
    role: String,
    schema: String,
    response: Value,
    failure: Option<&'static str>,
}

fn failure(failure: Failure) -> &'static str {
    match failure {
        Failure::Timeout => "timeout",
        Failure::AdmissionRefused => "admission_refused",
        Failure::ProviderError => "provider_error",
        _ => "unknown",
    }
}

fn hash(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub(super) fn observation(
    call: &AuthoringObservation<'_>,
    policy: &CapturePolicy,
) -> Option<(Value, Identity, bool)> {
    if call.role.len() > 64
        || !hash(call.prompt_sha256)
        || !hash(call.schema_sha256)
        || !hash(call.instruction_sha256)
        || call.prompt_shape_sha256.is_some_and(|digest| !hash(digest))
    {
        return None;
    }
    let (model, model_withheld) = policy.metadata(call.response_model);
    let (detail, detail_withheld) = policy.metadata(call.stop_reason_detail);
    let stop_kind = call.stop_reason_kind.map(|kind| match kind {
        "end_turn" | "max_tokens" | "stop_sequence" | "tool_use" | "content_filter" => kind,
        _ => "unknown",
    });
    let returned_metadata = json!({
        "source": "same_returned_callback",
        "instruction_sha256": call.instruction_sha256,
        "prompt_shape_sha256": call.prompt_shape_sha256,
        "prompt_messages": call.prompt_messages,
        "prompt_text_blocks": call.prompt_text_blocks,
        "prompt_other_blocks": call.prompt_other_blocks,
        "prompt_text_bytes": call.prompt_text_bytes,
        "usage_reported": call.usage_reported,
        "input_tokens": call.input_tokens,
        "output_tokens": call.output_tokens,
        "reasoning_tokens": call.reasoning_tokens,
        "response_model": model,
        "response_model_withheld": model_withheld,
        "stop_reason_kind": stop_kind,
        "stop_reason_detail": detail,
        "stop_reason_detail_withheld": detail_withheld,
    });
    let mut identity = Identity {
        ordinal: call.ordinal,
        role: call.role.to_owned(),
        schema: call.schema_sha256.to_owned(),
        response: Value::Null,
        failure: None,
    };
    let (response, withheld) = match &call.answered {
        Answered::Text {
            blocks,
            other_blocks,
            framed_sha256,
        } => returned(blocks, *other_blocks, framed_sha256, policy, &mut identity),
        Answered::NoResponse(why) => {
            identity.failure = Some(failure(*why));
            (
                json!({"state": "no_response",
            "failure": failure(*why)}),
                false,
            )
        }
        _ => (
            json!({"state": "unknown",
        "withheld_reason": "observer_version"}),
            true,
        ),
    };
    Some((
        json!({
            "capture_version": 1,
            "kind": "call",
            "ordinal": call.ordinal,
            "role": call.role,
            "prompt_sha256": call.prompt_sha256,
            "schema_sha256": call.schema_sha256,
            "response": response,
            "request_started": null,
            "physical_send": "unknown",
            "decode": null,
            "returned_metadata": returned_metadata,
            "outcome_metadata": "pending_closed_receipt_correlation",
        }),
        identity,
        withheld,
    ))
}

/// The returned Text record: bounded and admitted whole before any copy, else withheld whole
/// with its original counts and framed identity.
fn returned(
    blocks: &TextBlocks<'_>,
    other_blocks: usize,
    framed_sha256: &str,
    policy: &CapturePolicy,
    identity: &mut Identity,
) -> (Value, bool) {
    let bytes = blocks
        .iter()
        .try_fold(0usize, |sum, text| sum.checked_add(text.len()));
    let mut concatenated = Sha256::new();
    for text in blocks.iter() {
        concatenated.update(text.as_bytes());
    }
    identity.response = json!({
        "sha256": format!("{:x}", concatenated.finalize()),
        "bytes": bytes,
        "blocks": blocks.len().checked_add(other_blocks),
    });
    let reason = if bytes.is_none_or(|bytes| bytes > 256 * 1024) || blocks.len() > 1024 {
        Some("returned_text_bound")
    } else {
        // Only the host builds a view vector, after the 1024-block/256KiB bounds.
        let bounded: Vec<&str> = blocks.iter().collect();
        (!hash(framed_sha256)
            || !policy.admits(&bounded.concat())
            || !blocks.iter().all(|text| policy.admits(text)))
        .then_some("host_withheld")
    };
    // Retained Text is cloned only after bounds and host admission above.
    let text = reason
        .is_none()
        .then(|| json!(blocks.iter().collect::<Vec<_>>()));
    (
        json!({
            "state": "returned",
            "text": text,
            "text_blocks": blocks.len(),
            "other_blocks": other_blocks,
            "text_bytes": bytes,
            "framed_sha256": hash(framed_sha256).then_some(framed_sha256),
            "withheld_reason": reason,
        }),
        reason.is_some(),
    )
}

impl Identity {
    pub(super) fn matches(&self, entry: &Value) -> bool {
        entry["call"].as_str() == Some(self.role.as_str())
            && entry["schema_sha256"].as_str() == Some(self.schema.as_str())
            && entry.get("response") == Some(&self.response)
            && entry["result"]["failure_kind"].as_str() == self.failure
    }

    pub(super) fn metadata(&self, entry: &Value, policy: &CapturePolicy) -> Value {
        let result = &entry["result"];
        let reasoning = &entry["reasoning"];
        let (model, model_withheld) = policy.metadata(reasoning["response_model"].as_str());
        let (stop, stop_withheld) = policy.metadata(result["stop_reason"].as_str());
        let (configured, configured_withheld) = policy.metadata(reasoning["configured"].as_str());
        let instruction = entry["instruction_sha256"]
            .as_str()
            .filter(|value| hash(value));
        json!({
            "capture_version": 1,
            "kind": "call_metadata",
            "ordinal": self.ordinal,
            "binding": "ordinal_role_schema_response",
            "instruction_sha256": instruction,
            "message_bytes": entry["message_bytes"].as_u64(),
            "max_output_tokens": entry["max_output_tokens"].as_u64(),
            "timeout_ms": entry["timeout_ms"].as_u64(),
            "elapsed_ms": entry["elapsed_ms"].as_u64(),
            "response_model": model,
            "response_model_withheld": model_withheld,
            "stop_reason": stop,
            "stop_reason_withheld": stop_withheld,
            "usage_reported": result["usage_reported"].as_bool(),
            "input_tokens": result["input_tokens"].as_u64(),
            "output_tokens": result["output_tokens"].as_u64(),
            "reasoning_tokens": reasoning["reasoning_tokens"].as_u64(),
            "configured_effort": configured,
            "configured_effort_withheld": configured_withheld,
            "decoded": entry["proposed"]["decoded"].as_bool(),
        })
    }
}
