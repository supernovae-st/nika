// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Subscription transport composition only; the native Compiler still owns
//! creation, revision, questions, whole-answer validation and source grants.
use super::{AuthoringError, CompileOutcome, CompileRequest};

#[cfg(feature = "access-harness")]
pub(super) fn compile(
    adapter: &str,
    model: Option<&str>,
    transport: nika_types::access::HarnessTransport,
    request: &CompileRequest,
    decision: Option<super::decision::SessionSeat>,
    (host, catalog): (
        Option<&dyn nika_onboard::compile::rehearse::Rehearse>,
        Option<&dyn nika_compile_seats::foundry::ComponentCatalog>,
    ),
) -> Result<CompileOutcome, AuthoringError> {
    let harness =
        nika_harness::authoring::HarnessAuthoring::meet_with_transport(adapter, model, transport)
            .map_err(AuthoringError::Seat)?;
    let mut out = super::complete(Box::pin(
        nika_compile_cognition::compile_with_cognition_composed(
            request,
            nika_onboard::compile::Cognition {
                provider: Some(&harness),
                seat: decision
                    .as_ref()
                    .map(|s| s as &dyn nika_onboard::compile::decide::DecisionSeat),
            },
            host,
            catalog,
        ),
    ))??;
    super::decision::finish(&mut out, request, decision);
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        receipt.backend = Some(harness.descriptor().map_err(AuthoringError::Seat)?);
    }
    Ok(out)
}

#[cfg(not(feature = "access-harness"))]
pub(super) fn compile(
    adapter: &str,
    _: Option<&str>,
    _: nika_types::access::HarnessTransport,
    _: &CompileRequest,
    _: Option<super::decision::SessionSeat>,
    _: (
        Option<&dyn nika_onboard::compile::rehearse::Rehearse>,
        Option<&dyn nika_compile_seats::foundry::ComponentCatalog>,
    ),
) -> Result<CompileOutcome, AuthoringError> {
    Err(AuthoringError::Seat(format!(
        "subscription authoring `{adapter}` requires access-harness in this build"
    )))
}
