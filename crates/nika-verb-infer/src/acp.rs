// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! An `infer:` under `run.access.protocol: acp` — ONE turn over the
//! route's attested ACP one-shot profile (`nika_harness::run_acp`), never
//! its direct CLI one-shot and never another route.
//!
//! The session carries the prompt, the system, the model, the native
//! effort (applied and read back before the prompt) and the task's
//! `vision:` files as ACP image blocks (sent only to an agent whose
//! `initialize` advertises image prompts). Any other explicit task control
//! (`temperature` · `max_tokens` · `thinking` · a task-level effort · a
//! `vision:` URL, which Nika would have to fetch) cannot be applied by the
//! session, so it refuses before any spawn instead of being dropped.
//!
//! The profile's own output level is text. A `schema:` task meets its need
//! through the ENGINE (spec 02 §Subscription harness access, conjunct 3):
//! the prompt carries the schema, the terminal answer is validated here, an
//! answer that does not validate is never the task output (`NIKA-INFER-002`),
//! and each repair is a new one-shot turn under the same profile — a fresh
//! session that receives the earlier answer and the errors as text.

use std::sync::Arc;

use nika_harness::StructuredOutputGrade;
use nika_kernel::ai::harness::{DynAgentBackend, HarnessOutcome, HarnessRequest};

use crate::{HarnessInferOutput, InferInput, InferVerb, VerbInferError, selection, structured};

/// A lent ACP one-shot transport (an embedder's or a test's). It must drive
/// the route's completion profile ([`nika_harness::AcpOneShot::drive`]); without one,
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
        // The profile's own level: text. A schema is the engine's to enforce (below).
        let one_shot = nika_harness::meet_acp_one_shot(seat)
            .and_then(|one_shot| one_shot.grade(StructuredOutputGrade::Text))
            .map_err(|refused| VerbInferError::HarnessAccess {
                detail: format!("{refused}; nothing was sent"),
            })?;
        refuse_uncarried(&input, seat)?;
        let validator = match input.schema.as_ref() {
            Some(schema) => Some(structured::compile_schema(schema).map_err(|detail| {
                VerbInferError::InvalidParam {
                    param: "schema",
                    detail,
                }
            })?),
            None => None,
        };
        let images = crate::vision::prompt_images(&input.vision)?;
        let requested_model = input
            .model
            .clone()
            .unwrap_or_else(|| self.default_model.clone());
        let requirement = input.requirement.clone();
        let asked = match &input.schema {
            Some(schema) => format!(
                "{prompt}\n\nReply with ONLY a JSON value that satisfies this JSON Schema, \
                 no prose, no code fences:\n{rendered}",
                prompt = input.prompt,
                rendered = structured::render_schema(schema)
            ),
            None => input.prompt.clone(),
        };
        let (timeout, system) = (input.timeout, input.system.clone());
        let turn = |prompt: String| {
            // The scratch root is the one-shot's own; this path is replaced.
            let mut request = HarnessRequest::new(prompt, std::path::PathBuf::new())
                .with_requested_model(requested_model.clone())
                .with_requested_effort(requirement.as_ref().and_then(|r| r.effort.clone()))
                .with_images(images.clone());
            request.system.clone_from(&system);
            let transport = self.acp_transport.clone();
            async move {
                match transport {
                    Some(lent) => one_shot.run_over(lent.0.as_ref(), request, timeout).await,
                    None => one_shot.run(request, timeout).await,
                }
                .map_err(|source| VerbInferError::Harness { source })
            }
        };
        let mut outcome = turn(asked.clone()).await?;
        let value = match (input.schema.as_ref(), validator.as_ref()) {
            (Some(schema), Some(validator)) => {
                let mut attempts: u32 = 1;
                loop {
                    let errors = match structured::extract_and_validate(
                        &outcome.output,
                        validator,
                        schema,
                    ) {
                        structured::Validation::Valid(value) => break value,
                        structured::Validation::Invalid(errors) => errors,
                    };
                    if attempts > u32::from(self.schema_retry_budget) {
                        return Err(VerbInferError::SchemaValidation {
                            attempts,
                            detail: errors.join("; "),
                            spend: Box::default(),
                        });
                    }
                    attempts += 1;
                    outcome = turn(repair_prompt(&asked, &outcome, &errors, schema)).await?;
                }
            }
            _ => serde_json::Value::String(outcome.output.clone()),
        };
        let evidence = requirement
            .as_ref()
            .map(|r| selection::acp_evidence(r, &requested_model, &outcome));
        Ok(HarnessInferOutput::new(value, requested_model).with_selection(evidence))
    }
}

/// A repair turn's prompt: a one-shot session holds no memory of the earlier turn, so the
/// request carries the original ask, the answer that missed the schema and the errors.
fn repair_prompt(
    asked: &str,
    previous: &HarnessOutcome,
    errors: &[String],
    schema: &serde_json::Value,
) -> String {
    format!(
        "{asked}\n\nYour previous reply was:\n{answer}\n\n{repair}",
        answer = previous.output,
        repair = structured::retry_message(errors, schema)
    )
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
    if input
        .vision
        .iter()
        .any(|part| matches!(part, crate::VisionPart::Url { .. }))
    {
        named.push("vision URL");
    }
    let Some(first) = named.first().copied() else {
        return Ok(());
    };
    Err(VerbInferError::InvalidParam {
        param: if first == "vision URL" {
            "vision"
        } else {
            first
        },
        detail: format!(
            "`{seat}` over ACP carries the prompt, system, model, native effort, a `schema:` Nika \
             validates and `vision:` files — this task also sets {}, which the session cannot \
             apply (a `vision:` URL would need Nika to fetch it: point it at a local file); drop \
             it or declare an API route (nothing was sent)",
            named.join(" · ")
        ),
    })
}
