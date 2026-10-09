// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow's authored access selection on the `infer` verb.
//!
//! A declared `run.reasoning.effort` is the route's NATIVE word. On the
//! API path it becomes the wire's level only when the word IS one of the
//! levels the kernel request carries (the wire then still qualifies the
//! exact model and its direct endpoint before a byte leaves); any other
//! word refuses before any request — no alias, no translation. A direct
//! one-shot seat refuses an effort it cannot carry and never satisfies a
//! declared `acp`. The receipt keeps what was requested, what was sent and
//! what the route named apart.

use nika_kernel::ai::provider::ReasoningEffort;
use nika_types::access::{AccessProtocol, AccessRequirement, SelectedValue, SelectionEvidence};

use crate::{InferInput, InferOutput, VerbInferError};

/// The declared effort as the API wire's exact level, or the refusal.
pub(crate) fn with_declared_level(
    mut input: InferInput,
    requirement: &AccessRequirement,
) -> Result<InferInput, VerbInferError> {
    let Some(word) = requirement.effort.as_deref() else {
        return Ok(input);
    };
    let Some(level) = ReasoningEffort::parse(word) else {
        return Err(VerbInferError::InvalidParam {
            param: "reasoning_effort",
            detail: format!(
                "`run.reasoning.effort: {word}` is not a level this API route carries (low · \
                 high · max are its exact words); nothing was sent"
            ),
        });
    };
    if input.reasoning_effort.is_some_and(|asked| asked != level) {
        return Err(VerbInferError::InvalidParam {
            param: "reasoning_effort",
            detail: format!(
                "the task asks another reasoning effort than `run.reasoning.effort: {word}`; \
                 nothing was sent"
            ),
        });
    }
    input.reasoning_effort = Some(level);
    Ok(input)
}

/// The API call's receipt: the protocol, the model and level sent through
/// the request body, and the responder the answer itself named.
pub(crate) fn stamp_api(
    out: InferOutput,
    requirement: &AccessRequirement,
    requested: String,
    wire_model: Option<String>,
) -> InferOutput {
    let responder = out.response.gen_ai.response_model.clone();
    let model = SelectedValue::new()
        .requested(Some(requested))
        .transmitted(Some("model".into()), wire_model);
    let level = requirement
        .effort
        .as_deref()
        .and_then(ReasoningEffort::parse)
        .map(|level| level.word().to_owned());
    let effort = SelectedValue::new()
        .requested(requirement.effort.clone())
        .transmitted(level.as_ref().map(|_| "reasoning_effort".into()), level);
    let evidence = SelectionEvidence::new(Some(AccessProtocol::Api))
        .with_model(model)
        .with_effort(effort)
        .with_responder(responder, "api_response");
    out.with_selection(Some(evidence))
}

/// The direct one-shot's gate: it never satisfies `acp`, and it carries
/// the declared effort only to have the seat refuse it before spawning.
#[cfg(feature = "access-harness")]
pub(crate) fn direct_one_shot(
    requirement: Option<&AccessRequirement>,
    seat: &str,
) -> Result<Option<String>, VerbInferError> {
    let Some(requirement) = requirement else {
        return Ok(None);
    };
    if requirement.protocol == Some(AccessProtocol::Acp) {
        return Err(VerbInferError::HarnessAccess {
            detail: format!(
                "`{seat}` would serve this `infer:` through its direct one-shot, which never \
                 satisfies `run.access.protocol: acp`; nothing was sent"
            ),
        });
    }
    Ok(requirement.effort.clone())
}

/// The direct one-shot's receipt: no protocol, the requested model, and
/// the responder its CLI named (a report, not a proof). What its adapter
/// put on the command line is the adapter's own fact, not restated here.
#[cfg(feature = "access-harness")]
pub(crate) fn direct_evidence(requested: &str, observed: Option<String>) -> SelectionEvidence {
    SelectionEvidence::new(None)
        .with_model(SelectedValue::new().requested(Some(requested.to_owned())))
        .with_responder(observed, "cli_reported")
}

/// Whether the workflow declares the ACP protocol: its `infer:` then rides
/// the route's ACP one-shot, never the direct CLI one.
#[cfg(feature = "access-harness")]
pub(crate) fn declares_acp(requirement: Option<&AccessRequirement>) -> bool {
    requirement.is_some_and(|r| r.protocol == Some(AccessProtocol::Acp))
}

/// The ACP one-shot's receipt, from what the client sent and read back —
/// the law of the `agent:` receipt on the same session: an ACK without an
/// echoed value is no read-back (`configured: null` beside its
/// `accepted_request` evidence), and the responder stays unknown (an ACP
/// prompt result names none).
#[cfg(feature = "access-harness")]
pub(crate) fn acp_evidence(
    requirement: &AccessRequirement,
    requested_model: &str,
    outcome: &nika_kernel::ai::harness::HarnessOutcome,
) -> SelectionEvidence {
    let selection = &outcome.selection;
    let source = outcome
        .observed_model_source
        .map(nika_kernel::ai::harness::ModelProvenance::as_str);
    let configured = outcome
        .observed_model
        .clone()
        .filter(|_| source != Some("accepted_request"));
    let model = SelectedValue::new()
        .requested(Some(requested_model.to_owned()))
        .transmitted(
            selection.model_option.clone(),
            selection.transmitted_model.clone(),
        )
        .configured(configured, source.map(str::to_owned));
    let effort = SelectedValue::new()
        .requested(requirement.effort.clone())
        .transmitted(
            selection.effort_option.clone(),
            selection.transmitted_effort.clone(),
        )
        .configured(
            selection.configured_effort.clone(),
            selection
                .configured_effort_source
                .map(|source| source.as_str().to_owned()),
        );
    SelectionEvidence::new(Some(AccessProtocol::Acp))
        .with_model(model)
        .with_effort(effort)
        .with_changes(selection.changed_mid_turn.clone())
}
