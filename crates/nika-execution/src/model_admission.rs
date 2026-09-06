// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effective-model judgments shared by snapshot admission and execution.

use nika_schema::raw::RawWorkflow;

/// Judge model resolution, thinking and capacity before any workflow effect.
///
/// An override replaces only the envelope default; explicit task models keep
/// their authority. This uses the same pure judges as the MODELS rung and
/// never probes credentials or calls a provider. A template without a known
/// literal default remains unjudged, as it does on the check surface.
#[must_use]
pub fn model_admission_findings(
    workflow: &RawWorkflow,
    model_override: Option<&str>,
) -> Vec<String> {
    let effective = model_override.map(|model| nika_check::with_model_override(workflow, model));
    let workflow = effective.as_ref().unwrap_or(workflow);
    let report = nika_check::check(workflow);
    let mut findings = Vec::new();
    for requirement in &report.requirements.models {
        let model = if requirement.model.contains("${{") {
            let Some(model) = nika_check::static_literal_of(workflow, &requirement.model)
                .and_then(serde_json::Value::as_str)
            else {
                continue;
            };
            model
        } else {
            requirement.model.as_str()
        };
        if let Some(refusal) = nika_providers::resolve_refusal(model) {
            findings.push(format!("model `{model}`: {}", refusal.why));
        }
    }
    findings.extend(
        nika_check::thinking_findings(workflow)
            .into_iter()
            .map(|finding| format!("task `{}`: {}", finding.task, finding.why)),
    );
    findings.extend(
        nika_check::capacity_findings(workflow)
            .into_iter()
            .map(|finding| format!("task `{}`: {}", finding.task, finding.why)),
    );
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    fn workflow(model: &str, tokens: u32, task_model: Option<&str>) -> RawWorkflow {
        let task_model = task_model.map_or_else(String::new, |m| format!(", model: {m}"));
        nika_schema::parse(
            &format!(
                "nika: capacity\nmodel: {model}\ntasks:\n  say:\n    infer: {{ prompt: hi, max_tokens: {tokens}{task_model} }}\n"
            ),
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    #[test]
    fn the_same_judges_refuse_reasoning_floor_capacity_and_unknown_provider() {
        for (model, cap, expected) in [
            ("openai/gpt-5.2", 32, "max_tokens"),
            ("openai/gpt-5.2", 200_000, "exceeds"),
            ("not-a-provider/model", 512, "model"),
        ] {
            let findings = model_admission_findings(&workflow(model, cap, None), None);
            assert!(!findings.is_empty(), "{model}/{cap} must refuse");
            assert!(findings.join(" ").contains(expected), "{findings:?}");
        }
    }

    #[test]
    fn an_override_repairs_the_envelope_but_never_an_explicit_task_model() {
        let envelope = workflow("openai/gpt-5.2", 32, None);
        assert!(model_admission_findings(&envelope, Some("mock/echo")).is_empty());
        let task = workflow("mock/echo", 32, Some("openai/gpt-5.2"));
        assert!(!model_admission_findings(&task, Some("mock/echo")).is_empty());
    }
}
