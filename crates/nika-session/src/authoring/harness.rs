// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Subscription transport composition only; the native Compiler still owns
//! creation, revision, questions, whole-answer validation and source grants.
use super::{AuthoringError, CompileOutcome, CompileRequest};

#[cfg(feature = "access-harness")]
pub(super) fn compile(
    adapter: &str,
    model: Option<&str>,
    request: &CompileRequest,
) -> Result<CompileOutcome, AuthoringError> {
    let harness = nika_harness::authoring::HarnessAuthoring::meet(adapter, model)
        .map_err(AuthoringError::Seat)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| AuthoringError::Runtime(e.to_string()))?;
    let mut out = runtime.block_on(Box::pin(nika_onboard::compile::compile_with_cognition(
        request,
        nika_onboard::compile::Cognition {
            provider: Some(&harness),
            seat: None,
        },
    )))?;
    if let Some(receipt) = out.provenance.authoring.as_mut() {
        receipt.backend = Some(harness.descriptor().map_err(AuthoringError::Seat)?);
    }
    Ok(out)
}

#[cfg(not(feature = "access-harness"))]
pub(super) fn compile(
    adapter: &str,
    _: Option<&str>,
    _: &CompileRequest,
) -> Result<CompileOutcome, AuthoringError> {
    Err(AuthoringError::Seat(format!(
        "subscription authoring `{adapter}` requires access-harness in this build"
    )))
}
