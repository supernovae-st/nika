// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The ONE composition of « what this run needs » with « what this
//! machine offers » (One Door · wave 1): the checked requirements joined
//! with each task's verb, the `--model` override applied, the `--access`
//! pin carried, this machine's probe rows collected once — resolved into
//! the frozen [`ExecutionAccessPlan`] every door EXECUTES: `nika run`,
//! the answered gate leg, `nika serve`'s resident jobs, an ARM beat.
//! Descended from `nika-cli-host` in wave 1b so the resident door and
//! the CLI door read one resolver (the host crate re-exports these).
//! Before wave 1 the same question was answered five times on one run
//! path, and the answers could disagree.

use nika_check::CheckReport;
use nika_providers::probe::ProviderProbe;
use nika_providers::{ExecutionAccessPlan, ModelNeed, VerbNeeds, resolve_execution_plan_for};
use nika_schema::raw::{RawAction, RawWorkflow};
use nika_types::access::AccessRejection;

/// This machine's access-probe rows: the provider rows (key presence ·
/// endpoint overrides · the locals) PLUS the harness rows when the
/// feature is on — ONE door, so the run's gate, `check`, `explain` and
/// the resident's jobs can never judge different rows.
#[must_use]
pub fn access_probes_env() -> Vec<ProviderProbe> {
    nika_providers::probe::collect_access_probes_env(nika_runtime::compose::config_from_env())
}

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
    resolve_plan_over(wf, report, model_override, pin, &access_probes_env())
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
    resolve_execution_plan_for(&needs, probes, pin, verb_needs(wf))
}

/// The verbs the WORKFLOW carries, whatever its models say (W3-F1: a
/// model-less `infer:` task yields no model need; the pin judge and the
/// readiness layer must still see the infer).
#[must_use]
pub fn verb_needs(wf: &RawWorkflow) -> VerbNeeds {
    let mut infer = false;
    let mut agent = false;
    for task in &wf.tasks {
        match &task.value.action {
            RawAction::Infer(_) => infer = true,
            RawAction::Agent(_) => agent = true,
            _ => {}
        }
    }
    VerbNeeds::new(infer, agent)
}

/// The first `infer:`/`agent:` task whose effective model is EMPTY (no
/// task `model:`, no envelope `model:`) — the seat must supply it, so a
/// run with no seat pinned has no path (W3-F13).
#[must_use]
pub fn first_modelless_task(wf: &RawWorkflow) -> Option<&str> {
    if wf.model.is_some() {
        return None;
    }
    wf.tasks.iter().find_map(|task| match &task.value.action {
        RawAction::Infer(a) if a.model.is_none() => Some(task.value.id.value.as_str()),
        RawAction::Agent(a) if a.model.is_none() => Some(task.value.id.value.as_str()),
        _ => None,
    })
}

/// The ONE machine shape of an access lane (One Door · wave 2 · the W1
/// gauntlet met three): `check --json`'s `access_plan[]`, `run --dry-run
/// --json`'s `access.plans[]` and the trace's boot manifest `access_plan`
/// all carry exactly these rows — `model` · `provider` · `resolved` ·
/// `access` (the id that serves) · `chosen` (its class) · `billing` ·
/// `trust` (declared · discovered · observed · ADR-134) · `pinned` · `rejected[]` with `access` · `dimension` · `layer` ·
/// `witness`. A refused lane carries `resolved: false` and its witnesses.
#[must_use]
pub fn lane_rows(plan: &ExecutionAccessPlan) -> Vec<serde_json::Value> {
    plan.lanes
        .iter()
        .map(|(model, verdict)| match verdict {
            nika_providers::LaneVerdict::Admitted(lane) => serde_json::json!({
                "model": model,
                "provider": lane.plan.provider,
                "resolved": true,
                "access": lane.plan.access,
                "chosen": lane.plan.chosen.as_str(),
                "billing": lane.plan.billing.as_str(),
                "trust": lane.plan.trust.as_str(),
                "pinned": lane.plan.pinned,
                "rejected": rejection_rows(&lane.plan.rejected),
                "outranked": rejection_rows(&lane.plan.outranked),
                "candidates": lane.candidates,
            }),
            nika_providers::LaneVerdict::Refused(refusal) => serde_json::json!({
                "model": model,
                "provider": refusal.provider,
                "resolved": false,
                "rejected": rejection_rows(&refusal.rejected),
            }),
            // `#[non_exhaustive]` · a verdict this build does not know is
            // never rendered as admitted (fail closed on the machine face).
            _ => serde_json::json!({
                "model": model,
                "resolved": false,
                "rejected": [],
                "note": "lane verdict unknown to this build",
            }),
        })
        .collect()
}

fn rejection_rows(rejected: &[AccessRejection]) -> Vec<serde_json::Value> {
    rejected
        .iter()
        .map(|r| {
            serde_json::json!({
                "access": r.access,
                "dimension": r.dimension.as_str(),
                "layer": r.layer.as_str(),
                "witness": r.witness,
            })
        })
        .collect()
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

    /// The one shape: an admitted lane and a refused lane render the
    /// same keys on every machine surface (`resolved` tells them apart).
    #[test]
    fn the_lane_rows_carry_one_shape_for_admitted_and_refused() {
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  a:\n    infer: { prompt: hi }\n  b:\n    infer: { prompt: hi, model: \"mock/echo\" }\n",
        );
        let report = nika_check::check(&wf);
        let probes = [api_probe("mistral", false)];
        let plan = resolve_plan_over(&wf, &report, None, None, &probes);
        let rows = lane_rows(&plan);
        assert_eq!(rows.len(), 2, "{rows:?}");
        let refused = rows
            .iter()
            .find(|r| r["model"] == "mistral/mistral-small-latest")
            .expect("row");
        assert_eq!(refused["resolved"], false);
        assert_eq!(refused["provider"], "mistral");
        assert!(
            refused["rejected"]
                .as_array()
                .is_some_and(|r| !r.is_empty()),
            "{refused}"
        );
        let admitted = rows
            .iter()
            .find(|r| r["model"] == "mock/echo")
            .expect("row");
        assert_eq!(admitted["resolved"], true);
        assert_eq!(admitted["chosen"], "mock");
        assert_eq!(admitted["access"], "mock");
        assert_eq!(admitted["pinned"], false);
        assert_eq!(
            admitted["trust"], "observed",
            "the mock is the engine's own"
        );
    }
}
