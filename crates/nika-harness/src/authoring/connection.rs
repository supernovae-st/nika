// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! One selected completion transport for conversational and Compiler consumers.
use nika_kernel::ai::provider::{ContentBlock, InferRequest, Message, ProviderInferDyn, Role};
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
        HarnessTransport::Acp if adapter == "claude-code" => {},
        _ => return Err("this ACP authoring adapter has no attested pre-execution empty-tools profile; choose explicitly another connection; no fallback".into()),
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
    let wire_model = validate_selection(adapter, model, transport)?;
    if transport == HarnessTransport::Native {
        let seat = crate::meet_infer_grade(adapter, crate::StructuredOutputGrade::Text)
            .map_err(|e| e.to_string())?;
        let out = seat
            .run(crate::HarnessInferRequest::new(prompt, wire_model))
            .await
            .map_err(|e| e.to_string())?;
        return Ok((out.output, out.usage_observed));
    }
    let seat = super::HarnessAuthoring::meet_with_transport(adapter, model, transport)?;
    let request = InferRequest::new(
        wire_model,
        vec![Message::new(
            Role::User,
            vec![ContentBlock::Text {
                text: prompt.into(),
            }],
        )],
    );
    let mut response = seat.infer(request).await.map_err(|e| e.to_string())?;
    if response.content.len() != 1 {
        return Err("ACP authoring returned ambiguous content".into());
    }
    match response.content.pop() {
        Some(ContentBlock::Text { text }) => Ok((text, false)),
        _ => Err("ACP authoring did not return text".into()),
    }
}
