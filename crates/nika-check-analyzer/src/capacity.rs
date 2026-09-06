// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! CAPACITY FIT — the third layer of `nika check` (One Door · wave 2):
//! can the seat satisfy what the task declares? Four laws over the
//! catalog's POSITIVE knowledge ([`catalog_knows`] — a seat the catalog
//! never heard of gets no claim, the mock never does): an
//! `infer.max_tokens` above the seat's max output · a `schema:` on a
//! seat the catalog marks without a JSON mode · an `agent.max_tokens_total`
//! above the seat's context window · a `vision:` input on a seat whose
//! input modalities exclude images. Each finding names the task, the
//! seat, the two numbers (or the capability) and the repair — a static
//! judgment, so the failure is met at `check`, never after a task spent.

use nika_catalog::{JsonMode, Modality};
use nika_schema::Spanned;
use nika_schema::raw::{RawAction, RawWorkflow};

use crate::thinking::catalog_knows;

/// One CAPACITY finding — a declaration the judged seat cannot satisfy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapacityFinding {
    /// The task carrying the declaration.
    pub task: String,
    /// The seat the law judged (`provider/model`).
    pub model: String,
    /// Why — the law violated and the repair.
    pub why: String,
}

impl CapacityFinding {
    /// Construct (INV-019 · `new()` on every `#[non_exhaustive]` struct).
    #[must_use]
    pub fn new(task: String, model: String, why: String) -> Self {
        Self { task, model, why }
    }
}

/// The CAPACITY laws over every infer/agent task with a catalog-known seat.
#[must_use]
pub fn capacity_findings(wf: &RawWorkflow) -> Vec<CapacityFinding> {
    let mut findings = Vec::new();
    for task in &wf.tasks {
        let id = task.value.id.value.as_str();
        match &task.value.action {
            RawAction::Infer(a) => {
                let Some((seat, caps)) = known_seat(wf, a.model.as_ref()) else {
                    continue;
                };
                if let Some(max) = a.max_tokens.as_ref()
                    && let Some(cap) = caps.max_output_tokens
                    && max.value > cap
                {
                    findings.push(CapacityFinding::new(
                        id.to_owned(),
                        seat.clone(),
                        format!(
                            "`max_tokens` ({}) on `{id}` exceeds what `{seat}` can emit in one \
                             answer ({cap}) — the provider clamps or refuses the call; lower \
                             `max_tokens` to at most {cap} or seat a model with a larger output \
                             window",
                            max.value
                        ),
                    ));
                }
                if a.schema.is_some() && caps.json_mode == Some(JsonMode::Unavailable) {
                    findings.push(CapacityFinding::new(
                        id.to_owned(),
                        seat.clone(),
                        format!(
                            "`schema:` on `{id}` asks `{seat}` for structured output the catalog \
                             marks unavailable — the answer cannot be forced to the shape; seat a \
                             model with a JSON mode or drop `schema:`"
                        ),
                    ));
                }
                if !a.vision.is_empty()
                    && !caps.input_modalities.is_empty()
                    && !caps.input_modalities.contains(&Modality::Image)
                {
                    findings.push(CapacityFinding::new(
                        id.to_owned(),
                        seat,
                        format!(
                            "`vision:` on `{id}` sends images to a seat whose input modalities \
                             the catalog lists without `image` — the provider refuses the parts; \
                             seat a vision-capable model or drop `vision:`"
                        ),
                    ));
                }
            }
            RawAction::Agent(a) => {
                let Some((seat, caps)) = known_seat(wf, a.model.as_ref()) else {
                    continue;
                };
                if let Some(total) = a.max_tokens_total.as_ref()
                    && let Some(window) = caps.context_window_tokens
                    && total.value > u64::from(window)
                {
                    findings.push(CapacityFinding::new(
                        id.to_owned(),
                        seat,
                        format!(
                            "`max_tokens_total` ({}) on `{id}` exceeds the context window of the \
                             seat ({window}) — the loop cannot hold that budget in one context; \
                             lower it to at most {window} or seat a larger-window model",
                            total.value
                        ),
                    ));
                }
            }
            _ => {}
        }
    }
    findings
}

/// The catalog-KNOWN seat a capacity law judges: the task's `model:` or
/// the envelope's (a templated seat through its declared default), never
/// the mock, never a seat the catalog has no positive row for.
fn known_seat(
    wf: &RawWorkflow,
    model: Option<&Spanned<String>>,
) -> Option<(String, nika_catalog::ModelCapabilities)> {
    let seat = model.or(wf.model.as_ref())?;
    let judged = if seat.value.contains("${{") {
        let literal = crate::static_literal_of(wf, &seat.value)?;
        literal.as_str()?.to_owned()
    } else {
        seat.value.clone()
    };
    let (provider, name) = judged.split_once('/')?;
    if provider == "mock" || !catalog_knows(provider, name, &judged) {
        return None;
    }
    // The limits live on the provider ROW (required there); a capability
    // rule may override them (optional there) — one merged view.
    let mut caps = nika_catalog::model_capabilities(provider, name);
    let row = nika_catalog::find_provider(provider)
        .and_then(|p| p.models.iter().find(|m| m.id == name || m.model == name));
    if let Some(row) = row {
        caps.max_output_tokens = caps.max_output_tokens.or(Some(row.max_output_tokens));
        caps.context_window_tokens = caps
            .context_window_tokens
            .or(Some(row.context_window_tokens));
    }
    Some((judged, caps))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn parse(src: &str) -> RawWorkflow {
        nika_schema::parse(
            src,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    /// `max_tokens` above the seat's max output names both numbers and
    /// the repair; the same seat under its cap is silent.
    #[test]
    fn an_output_cap_above_the_seat_max_is_a_finding() {
        let over = parse(
            "nika: t\nmodel: openai/gpt-5.2\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 200000 }\n",
        );
        let findings = capacity_findings(&over);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].task, "a");
        assert_eq!(findings[0].model, "openai/gpt-5.2");
        assert!(
            findings[0].why.contains("200000") && findings[0].why.contains("128000"),
            "{}",
            findings[0].why
        );
        let under = parse(
            "nika: t\nmodel: openai/gpt-5.2\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 256, schema: { type: object } }\n",
        );
        assert!(capacity_findings(&under).is_empty());
    }

    /// An agent's total budget above the seat's context window is a
    /// finding — through the envelope model, like the thinking laws.
    #[test]
    fn an_agent_budget_above_the_context_window_is_a_finding() {
        let wf = parse(
            "nika: t\nmodel: openai/gpt-5.2\ntasks:\n  go:\n    agent: { prompt: hi, tools: [], max_tokens_total: 300000 }\n",
        );
        let findings = capacity_findings(&wf);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(
            findings[0].why.contains("300000") && findings[0].why.contains("272000"),
            "{}",
            findings[0].why
        );
    }

    /// `vision:` on a seat the catalog lists as text-only is a finding.
    #[test]
    fn vision_on_a_text_only_seat_is_a_finding() {
        let wf = parse(
            "nika: t\ntasks:\n  see:\n    infer:\n      model: deepseek/deepseek-chat\n      prompt: describe\n      max_tokens: 256\n      vision: [{ source: file, path: \"./x.png\" }]\n",
        );
        let findings = capacity_findings(&wf);
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert!(findings[0].why.contains("`vision:`"), "{}", findings[0].why);
    }

    /// The mock and a seat the catalog never heard of get no claim —
    /// the MODELS rung owns the unknown provider, the mock is unmetered.
    #[test]
    fn the_mock_and_an_unknown_seat_get_no_claim() {
        let mock = parse(
            "nika: t\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 999999 }\n",
        );
        assert!(capacity_findings(&mock).is_empty());
        let unknown = parse(
            "nika: t\nmodel: openai/never-a-model-9\ntasks:\n  a:\n    infer: { prompt: hi, max_tokens: 999999 }\n",
        );
        assert!(capacity_findings(&unknown).is_empty());
    }
}
