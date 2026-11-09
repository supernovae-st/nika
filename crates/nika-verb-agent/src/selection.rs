// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The workflow's authored access selection on the `agent` verb.
//!
//! On a harness seat the native effort travels to the session, which
//! applies it through its own reasoning option and reads it back before
//! the prompt (`nika-harness`); the receipt keeps what was asked, sent and
//! read back apart, and the responder stays unknown (an ACP prompt result
//! names none). On the native loop the effort becomes every turn's exact
//! request level, or refuses before the first call — no alias, ever.
//!
//! A declared service tier travels the same way on a seat: the session binds
//! the word to its adapter's own speed option after the effort, or refuses.
//! The native loop binds no speed control, so it refuses a declared tier
//! before its first call.

#[cfg(feature = "access-harness")]
use nika_kernel::ai::harness::HarnessSelection;
use nika_kernel::ai::provider::ReasoningEffort;
#[cfg(feature = "access-harness")]
use nika_types::access::ServiceTierEvidence;
use nika_types::access::{AccessProtocol, AccessRequirement, SelectedValue, SelectionEvidence};

use crate::VerbAgentError;

/// The declared effort as the native loop's exact request level. A declared
/// service tier refuses here: this API loop binds no speed control, and a
/// strict selection is never dropped.
pub(crate) fn native_level(
    requirement: Option<&AccessRequirement>,
) -> Result<Option<ReasoningEffort>, VerbAgentError> {
    if let Some(tier) = requirement.and_then(|r| r.service_tier) {
        return Err(VerbAgentError::InvalidParam {
            param: "service_tier",
            detail: format!(
                "`run.access.service_tier: {tier}` cannot ride this API route: no speed control \
                 is bound for it (declare an agent seat that offers one, or remove the key to \
                 keep the route's own tier); no model call was made"
            ),
        });
    }
    let Some(word) = requirement.and_then(|r| r.effort.as_deref()) else {
        return Ok(None);
    };
    ReasoningEffort::parse(word)
        .map(Some)
        .ok_or_else(|| VerbAgentError::InvalidParam {
            param: "reasoning_effort",
            detail: format!(
                "`run.reasoning.effort: {word}` is not a level this API route carries (low · \
                 high · max are its exact words); no model call was made"
            ),
        })
}

/// The native loop's receipt: the API protocol, the requested model and
/// the exact level every turn carried (the wire refuses an unqualified
/// one before a byte leaves). No read-back exists on an API.
pub(crate) fn native_evidence(
    requirement: &AccessRequirement,
    requested_model: Option<String>,
) -> SelectionEvidence {
    let level = requirement
        .effort
        .as_deref()
        .and_then(ReasoningEffort::parse)
        .map(|level| level.word().to_owned());
    SelectionEvidence::new(Some(AccessProtocol::Api))
        .with_model(SelectedValue::new().requested(requested_model))
        .with_effort(
            SelectedValue::new()
                .requested(requirement.effort.clone())
                .transmitted(level.as_ref().map(|_| "reasoning_effort".into()), level),
        )
}

/// The ACP session's receipt, from what the client sent and read back.
#[cfg(feature = "access-harness")]
pub(crate) fn harness_evidence(
    requirement: &AccessRequirement,
    requested_model: Option<String>,
    configured_model: Option<String>,
    model_source: Option<&'static str>,
    selection: &HarnessSelection,
) -> SelectionEvidence {
    // An ACK without an echoed value is not a read-back: the model then
    // stays `configured: null` beside its `accepted_request` evidence.
    let configured_model = configured_model.filter(|_| model_source != Some("accepted_request"));
    let model = SelectedValue::new()
        .requested(requested_model)
        .transmitted(
            selection.model_option.clone(),
            selection.transmitted_model.clone(),
        )
        .configured(configured_model, model_source.map(str::to_owned));
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
        .with_service_tier(tier_evidence(requirement, selection))
}

/// The declared service tier as the word the session request carries.
#[cfg(feature = "access-harness")]
pub(crate) fn requested_tier(requirement: Option<&AccessRequirement>) -> Option<String> {
    let tier = requirement.and_then(|r| r.service_tier)?;
    Some(tier.as_str().to_owned())
}

/// The service-tier block of the receipt, only when the file declared a tier:
/// the word asked, what the session offered for its speed option, the native
/// value sent and the one read back. The tier a turn carried and the tier
/// that served it are not facts an ACP session states, so they stay unknown.
#[cfg(feature = "access-harness")]
fn tier_evidence(
    requirement: &AccessRequirement,
    selection: &HarnessSelection,
) -> Option<ServiceTierEvidence> {
    let tier = requirement.service_tier?;
    Some(
        ServiceTierEvidence::new(tier)
            .offered(
                selection.service_tier_option.clone(),
                selection.offered_service_tier.clone(),
            )
            .transmitted(selection.transmitted_service_tier.clone())
            .configured(
                selection.configured_service_tier.clone(),
                selection
                    .configured_service_tier_source
                    .map(|source| source.as_str().to_owned()),
            ),
    )
}
