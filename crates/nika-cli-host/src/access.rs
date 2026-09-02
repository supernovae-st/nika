// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE composition of « what this run needs » with « what this
//! machine offers » (One Door · wave 1): the checked requirements joined
//! with each task's verb, the `--model` override applied, the `--access`
//! pin carried, this machine's probe rows collected once — resolved into
//! the frozen [`ExecutionAccessPlan`] that `check`, `run`, `--dry-run`,
//! the announce and the boot manifest all PROJECT. Before this module
//! the same question was answered five times on one run path, and the
//! answers could disagree.

use nika_check::CheckReport;
use nika_providers::probe::ProviderProbe;
use nika_providers::{ExecutionAccessPlan, ModelNeed, resolve_execution_plan};
use nika_schema::raw::{RawAction, RawWorkflow};

/// The verbs that read each static model — the checked requirements
/// (task `model:` ?? envelope) joined with the action kind of every
/// task that resolves to the model. The eligibility facts a harness
/// candidate is judged against (an ACP-only seat drives `agent:`,
/// never a one-shot `infer:`).
#[must_use]
pub fn model_needs(wf: &RawWorkflow, report: &CheckReport) -> Vec<ModelNeed> {
    report
        .requirements
        .models
        .iter()
        .map(|req| {
            let (infer, agent) = req.tasks.iter().fold((false, false), |(infer, agent), id| {
                let action = wf
                    .tasks
                    .iter()
                    .find(|task| task.value.id.value == *id)
                    .map(|task| &task.value.action);
                match action {
                    Some(RawAction::Infer(_)) => (true, agent),
                    Some(RawAction::Agent(_)) => (infer, true),
                    _ => (infer, agent),
                }
            });
            ModelNeed::new(req.model.clone(), infer, agent)
        })
        .collect()
}

/// Resolve the frozen plan for one execution attempt over THIS
/// machine's probe rows (provider rows + harness rows · presence only,
/// no socket): the EFFECTIVE models (`--model` applied · a per-task
/// `model:` keeps winning) with their verbs, under the `--access` pin.
#[must_use]
pub fn resolve_plan(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    pin: Option<&str>,
) -> ExecutionAccessPlan {
    let probes = crate::probe::access_probes_with_harness();
    resolve_plan_over(wf, report, model_override, pin, &probes)
}

/// [`resolve_plan`] over INJECTED probe rows — the pure half (tests
/// drive this; the process environment is never read here).
#[must_use]
pub fn resolve_plan_over(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    pin: Option<&str>,
    probes: &[ProviderProbe],
) -> ExecutionAccessPlan {
    let needs = match model_override {
        Some(model) => {
            let swapped = nika_check::with_model_override(wf, model);
            let report = nika_check::check(&swapped);
            model_needs(&swapped, &report)
        }
        None => model_needs(wf, report),
    };
    resolve_execution_plan(&needs, probes, pin)
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]
mod tests {
    use nika_providers::probe::{ExecutionLocus, ProviderReadiness};
    use nika_types::access::AccessClass;

    use super::*;

    fn parse(src: &str) -> RawWorkflow {
        nika_schema::parse(
            src,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
    }

    fn api_probe(id: &str, key_present: bool) -> ProviderProbe {
        ProviderProbe::new(
            id,
            true,
            key_present,
            format!("{}_API_KEY", id.to_uppercase()),
            false,
            ProviderReadiness::new(
                true,
                key_present,
                None,
                None,
                true,
                ExecutionLocus::Cloud,
                AccessClass::Api,
            ),
            "https://api.example.com",
        )
    }

    /// The needs join the requirement's tasks with their verbs: one
    /// model read by an infer AND an agent task carries both flags.
    #[test]
    fn needs_join_each_model_with_the_verbs_that_read_it() {
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  a:\n    infer: { prompt: hi }\n  b:\n    agent: { prompt: go, tools: [] }\n  c:\n    infer: { prompt: hi, model: \"mock/echo\" }\n",
        );
        let report = nika_check::check(&wf);
        let mut needs = model_needs(&wf, &report);
        needs.sort_by(|a, b| a.model.cmp(&b.model));
        assert_eq!(needs.len(), 2);
        assert_eq!(needs[0].model, "mistral/mistral-small-latest");
        assert!(needs[0].infer && needs[0].agent, "{:?}", needs[0]);
        assert_eq!(needs[1].model, "mock/echo");
        assert!(needs[1].infer && !needs[1].agent, "{:?}", needs[1]);
    }

    /// `--model` swaps the ENVELOPE model before the plan is resolved —
    /// the plan speaks about the run, never about the file (the shipped
    /// door announced the file's model under `--model mock/echo`).
    #[test]
    fn the_override_is_what_the_plan_resolves() {
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  a:\n    infer: { prompt: hi }\n",
        );
        let report = nika_check::check(&wf);
        let probes = [api_probe("mistral", false)];
        let file_plan = resolve_plan_over(&wf, &report, None, None, &probes);
        assert!(
            !file_plan.is_admitted(),
            "no mistral key → the file's lane refuses"
        );
        let run_plan = resolve_plan_over(&wf, &report, Some("mock/echo"), None, &probes);
        assert!(run_plan.is_admitted(), "the run rides mock");
        assert!(run_plan.lane("mock/echo").is_some());
        assert!(run_plan.lane("mistral/mistral-small-latest").is_none());
    }
}
