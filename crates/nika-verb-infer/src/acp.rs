// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An `infer:` under `run.access.protocol: acp` — ONE turn over the
//! route's attested ACP one-shot profile (`nika_harness::run_acp`), never
//! its direct CLI one-shot and never another route.
//!
//! The session carries the prompt, the system, the model and the native
//! effort (applied and read back before the prompt). Any other explicit
//! task control (`temperature` · `max_tokens` · `thinking` · `vision` · a
//! task-level effort) cannot be applied by the session, so it refuses
//! before any spawn instead of being dropped. The profile's output level is
//! text: a `schema:` task refuses at the meet.

use std::sync::Arc;

use nika_harness::StructuredOutputGrade;
use nika_kernel::ai::harness::{DynAgentBackend, HarnessRequest};

use crate::{HarnessInferOutput, InferInput, InferVerb, VerbInferError, selection};

/// A lent ACP one-shot transport (an embedder's or a test's). It must drive
/// the completion profile ([`nika_harness::drive_one_shot`]); without one,
/// the route's registry adapter is spawned under that profile.
#[derive(Clone)]
pub(crate) struct AcpTransport(pub(crate) Arc<dyn DynAgentBackend>);

impl std::fmt::Debug for AcpTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AcpTransport")
    }
}

impl<H> InferVerb<H> {
    /// One `infer:` over `seat`'s attested ACP one-shot.
    pub(crate) async fn run_on_acp(
        &self,
        seat: &str,
        input: InferInput,
    ) -> Result<HarnessInferOutput, VerbInferError> {
        let need = if input.schema.is_some() {
            StructuredOutputGrade::JsonSchema
        } else {
            StructuredOutputGrade::Text
        };
        let one_shot = nika_harness::meet_acp_one_shot(seat)
            .and_then(|one_shot| one_shot.grade(need))
            .map_err(|refused| VerbInferError::HarnessAccess {
                detail: format!("{refused}; nothing was sent"),
            })?;
        refuse_uncarried(&input, seat)?;
        let requested_model = input
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let requirement = input.requirement.clone();
        // The scratch root is the one-shot's own; this path is replaced.
        let mut request = HarnessRequest::new(input.prompt, std::path::PathBuf::new())
            .with_requested_model(requested_model.clone())
            .with_requested_effort(requirement.as_ref().and_then(|r| r.effort.clone()));
        request.system = input.system;
        let outcome = match &self.acp_transport {
            Some(lent) => {
                one_shot
                    .run_over(lent.0.as_ref(), request, input.timeout)
                    .await
            }
            None => one_shot.run(request, input.timeout).await,
        }
        .map_err(|source| VerbInferError::Harness { source })?;
        let evidence = requirement
            .as_ref()
            .map(|r| selection::acp_evidence(r, &requested_model, &outcome));
        Ok(
            HarnessInferOutput::new(serde_json::Value::String(outcome.output), requested_model)
                .with_selection(evidence),
        )
    }
}

/// The explicit task controls an ACP session cannot apply, refused before
/// any spawn — never dropped.
fn refuse_uncarried(input: &InferInput, seat: &str) -> Result<(), VerbInferError> {
    let mut named: Vec<&'static str> = Vec::new();
    if input.temperature.is_some() {
        named.push("temperature");
    }
    if input.max_tokens.is_some() {
        named.push("max_tokens");
    }
    if input.thinking_budget.is_some() {
        named.push("thinking");
    }
    if input.reasoning_effort.is_some() {
        named.push("reasoning_effort");
    }
    if !input.vision.is_empty() {
        named.push("vision");
    }
    let Some(first) = named.first().copied() else {
        return Ok(());
    };
    Err(VerbInferError::InvalidParam {
        param: first,
        detail: format!(
            "`{seat}` over ACP carries the prompt, system, model and native effort only — this \
             task also sets {}, which the session cannot apply; drop it or declare an API route \
             (nothing was sent)",
            named.join(" · ")
        ),
    })
}
