// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One selected completion transport for conversational and Compiler consumers.
use nika_kernel::ai::provider::{
    ContentBlock, InferRequest, Message, ProviderInferDyn, ReasoningEffort, Role,
};
use nika_types::access::HarnessTransport;

/// Validate a selection without spawning a product or probing credentials.
/// # Errors
/// The model namespace or requested connection is not supported.
pub fn validate_selection(
    adapter: &str,
    model: Option<&str>,
    transport: HarnessTransport,
) -> Result<String, String> {
    #[cfg(not(unix))]
    if transport == HarnessTransport::Acp {
        return Err("ACP authoring is not admitted on this platform: owned descendant cleanup is not attested".into());
    }
    match transport {
        HarnessTransport::Native => {},
        // An audited completion profile: Claude Code (no tools at all) or Codex (tool
        // surfaces closed and read back at spawn; `apply_patch` confined to the scratch, a
        // tool beat refusing the answer).
        HarnessTransport::Acp if super::acp::Profile::for_seat(adapter).is_some() => {},
        _ => return Err("this ACP authoring adapter has no audited completion profile; choose explicitly another connection; no fallback".into()),
    }
    super::model_argument(adapter, model)
}

/// One conversational completion using the selected connection; no workflow or fallback.
/// Native retains its infer-grade contract, including the existing Codex conversation door.
/// # Errors
/// Invalid identity, transport refusal, deadline or non-text response.
pub async fn reason(
    adapter: &str,
    model: Option<&str>,
    transport: HarnessTransport,
    prompt: &str,
) -> Result<(String, bool), String> {
    reason_with_effort(adapter, model, transport, prompt, None).await
}

/// [`reason`] asking an explicit reasoning effort. Over ACP the effort travels with the request:
/// the session applies it through its advertised reasoning option and reads it back. A direct
/// (native) connection cannot carry it and refuses before any call, never dropping it.
/// # Errors
/// As [`reason`], and an effort asked of a native connection.
pub async fn reason_with_effort(
    adapter: &str,
    model: Option<&str>,
    transport: HarnessTransport,
    prompt: &str,
    effort: Option<ReasoningEffort>,
) -> Result<(String, bool), String> {
    let wire_model = validate_selection(adapter, model, transport)?;
    if transport == HarnessTransport::Native {
        if let Some(effort) = effort {
            return Err(format!(
                "the direct connection of `{adapter}` cannot carry the explicit reasoning effort `{}` · nothing was sent",
                effort.word()
            ));
        }
        let seat = crate::meet_infer_grade(adapter, crate::StructuredOutputGrade::Text)
            .map_err(|e| e.to_string())?;
        let out = seat
            .run(crate::HarnessInferRequest::new(prompt, wire_model))
            .await
            .map_err(|e| e.to_string())?;
        return Ok((out.output, out.usage_observed));
    }
    let seat = super::HarnessAuthoring::meet_with_transport(adapter, model, transport)?;
    let request = conversational(wire_model, prompt, effort);
    let mut response = seat.infer(request).await.map_err(|e| e.to_string())?;
    if response.content.len() != 1 {
        return Err("ACP authoring returned ambiguous content".into());
    }
    match response.content.pop() {
        Some(ContentBlock::Text { text }) => Ok((text, false)),
        _ => Err("ACP authoring did not return text".into()),
    }
}

/// The one-message request a conversational completion sends, with the effort it asks.
fn conversational(
    wire_model: String,
    prompt: &str,
    effort: Option<ReasoningEffort>,
) -> InferRequest {
    let mut request = InferRequest::new(
        wire_model,
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text {
                text: prompt.into(),
            }],
        )],
    );
    request.reasoning_effort = effort;
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversational_request_carries_the_effort_it_asks_and_no_other() {
        let asked = conversational("m".into(), "hello", Some(ReasoningEffort::Max));
        assert_eq!(asked.reasoning_effort, Some(ReasoningEffort::Max));
        assert_eq!(
            conversational("m".into(), "hello", None).reasoning_effort,
            None
        );
    }

    #[tokio::test]
    async fn a_native_connection_refuses_an_effort_before_any_call() {
        // Refused before the seat is met: no process starts, no byte is sent.
        let refused = reason_with_effort(
            "codex",
            None,
            HarnessTransport::Native,
            "hello",
            Some(ReasoningEffort::Max),
        )
        .await;
        assert!(
            matches!(&refused, Err(why) if why.contains("cannot carry the explicit reasoning effort `max`")
                && why.contains("nothing was sent")),
            "{refused:?}"
        );
    }
}
