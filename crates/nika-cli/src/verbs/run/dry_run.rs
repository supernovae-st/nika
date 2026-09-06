// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--dry-run` lane: model swap, refusal, then one honest preview.
//! The access half is a PROJECTION of the frozen plan the run would
//! execute (One Door · wave 1): the preview refuses exactly where the
//! run refuses and shows exactly the lanes the run rides — it resolves
//! nothing of its own.

use nika_check::CheckReport;
use nika_providers::ExecutionAccessPlan;
use nika_schema::raw::RawWorkflow;
use nika_types::access::AccessClass;

use super::{RunVerdict, exit};
use crate::Theme;

/// Apply `--model` and re-check the envelope the preview will describe.
pub(super) fn swap(
    wf: &RawWorkflow,
    model: &str,
) -> Result<(RawWorkflow, CheckReport), Box<(RawWorkflow, CheckReport)>> {
    let swapped = crate::verbs::with_model_override(wf, model);
    let mut report = nika_check::check(&swapped);
    crate::verbs::stamp_judged_semantic(&swapped, &mut report);
    let models = crate::verbs::check::models_rung::unresolvable_models(&report, &swapped);
    let thinking = crate::verbs::check::models_rung::thinking_findings(&swapped);
    if report.is_clean() && models.findings.is_empty() && thinking.is_empty() {
        Ok((swapped, report))
    } else {
        Err(Box::new((swapped, report)))
    }
}

/// Preview the overridden pair, or emit the same refusal as `nika check`.
/// `plan` is the run's frozen access plan, already resolved over the
/// effective models (the `--model` override applied by the resolver).
#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub(super) fn lane(
    file: &str,
    source: &str,
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: &nika_schema::ResolvedSkills,
    repair_target: nika_display::check_render::RepairTarget,
    model_override: Option<&str>,
    plan: &ExecutionAccessPlan,
    json: bool,
    theme: Theme,
    output_json: bool,
) -> RunVerdict {
    if let Some(model) = model_override {
        match swap(wf, model) {
            Ok((wf, report)) => {
                if let Some(verdict) = access_refusal(plan, output_json) {
                    return verdict;
                }
                return RunVerdict::bare(verdict(file, &wf, &report, plan, json, theme));
            }
            Err(refused) => {
                let (wf, report) = *refused;
                let out = crate::verbs::check::run_admitted_pair(
                    source,
                    file,
                    repair_target,
                    &wf,
                    &report,
                    skills,
                    json,
                    theme,
                );
                super::epilogue::emit_diagnostic(&out.text, output_json);
                return RunVerdict::bare(out.code);
            }
        }
    }
    if let Some(verdict) = access_refusal(plan, output_json) {
        return verdict;
    }
    RunVerdict::bare(verdict(file, wf, report, plan, json, theme))
}

/// The SAME refusal the runtime's admission belt speaks
/// ([`nika_runtime::plan_refusal`]): an unsatisfied pin, or a lane with
/// no ready access path on this machine — before any task, on every lane.
fn access_refusal(plan: &ExecutionAccessPlan, output_json: bool) -> Option<RunVerdict> {
    nika_runtime::plan_refusal(plan).map(|error| {
        RunVerdict::bare(super::epilogue::env_refusal(
            &error.to_string(),
            output_json,
        ))
    })
}

fn verdict(
    file: &str,
    wf: &RawWorkflow,
    report: &CheckReport,
    plan: &ExecutionAccessPlan,
    json: bool,
    theme: Theme,
) -> u8 {
    if json {
        let payload = project_access(nika_check::plan::payload(file, wf, report), plan);
        println!("{payload:#}");
        return exit::OK;
    }
    // Wave 2 (W1 gauntlet · the seated preview hid the plan): the text
    // preview ALWAYS carries the static plan, then one access line per
    // lane — the same lines the run announces.
    let rendered = crate::verbs::inspect::render_pair(wf, report, theme);
    if !rendered.text.is_empty() {
        println!("{}", rendered.text.trim_end());
    }
    // W3-F1 · the pin line reads the SAME judge the run's admission
    // reads: a refused pin is printed as refused and the preview exits
    // like the run would (3), never « admission satisfied » for a seat
    // this machine lacks. W3-F13 · a model-less infer with no seat has
    // no path, and the preview says so before task 1.
    let refusal =
        nika_runtime::plan_refusal(plan).or_else(|| nika_runtime::modelless_refusal(wf, plan));
    if let Some(pin) = &plan.pin {
        if plan.pin_refusal.is_some()
            && let Some(err) = &refusal
        {
            println!("access: pinned `{pin}` · REFUSED · {err} → the run refuses before task 1");
        } else {
            println!("access: pinned `{pin}` · admitted · seat present · sign-in judged at run");
        }
    }
    if plan.pin_refusal.is_none()
        && let Some(err) = &refusal
    {
        println!("access: REFUSED · {err} → the run refuses before task 1");
    }
    for (model, lane) in plan.admitted() {
        let seat = if lane.plan.chosen == AccessClass::Harness {
            " · subscription seat · usage billed to the seat"
        } else {
            ""
        };
        println!(
            "access: {model} → {} ({} · {}){seat}",
            lane.plan.access,
            lane.plan.chosen.as_str(),
            lane.plan.billing.as_str()
        );
    }
    println!("\n  dry-run · plan only · no effects executed");
    if refusal.is_some() {
        return exit::ENV;
    }
    exit::OK
}

/// The `access` block of the JSON preview — the admitted lanes of the
/// frozen plan, and the subscription-quota cost basis when a seat rides
/// the run. A plan with no pin and no lane leaves the payload untouched
/// (a file with no inference has nothing to say about access).
fn project_access(mut payload: serde_json::Value, plan: &ExecutionAccessPlan) -> serde_json::Value {
    // Wave 2 · the ONE lane-row shape (`check --json` · this preview ·
    // the boot manifest carry the same rows).
    let rows = nika_service_execution::access::lane_rows(plan);
    if plan.pin.is_none() && rows.is_empty() {
        return payload;
    }
    let seated = plan
        .admitted()
        .any(|(_, lane)| lane.plan.chosen == AccessClass::Harness);
    payload["access"] = serde_json::json!({
        "requested": plan.pin,
        "resolved": plan.is_admitted(),
        "seat": plan.seat,
        "plans": rows,
    });
    if seated {
        let tasks = payload["cost"]["tasks"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|task| {
                serde_json::json!({
                    "task": task.get("task").cloned().unwrap_or(serde_json::Value::Null),
                    "requested_model": task
                        .get("model")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    "budget": "subscription_quota",
                })
            })
            .collect::<Vec<_>>();
        payload["cost"] = serde_json::json!({
            "basis": "subscription_quota",
            "tasks": tasks,
        });
    }
    payload
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use nika_providers::{ExecutionAccessPlan, LaneVerdict, ResolvedLane};
    use nika_types::access::{AccessClass, AccessPlan, BillingClass};
    use serde_json::json;

    fn seated_plan() -> ExecutionAccessPlan {
        let lane = ResolvedLane::new(
            AccessPlan::new(
                "openai/gpt-5.2",
                "openai",
                "codex",
                AccessClass::Harness,
                BillingClass::Unknown,
                true,
                Vec::new(),
            ),
            2,
        );
        ExecutionAccessPlan::new(
            BTreeMap::from([("openai/gpt-5.2".to_owned(), LaneVerdict::Admitted(lane))]),
            Some("codex".to_owned()),
            Some("codex".to_owned()),
            None,
        )
    }

    #[test]
    fn subscription_preview_names_request_and_contains_no_numeric_meter() {
        let payload = json!({
            "cost": {
                "bounded_total_usd": 0.25,
                "tasks": [{
                    "task": "answer",
                    "model": "openai/gpt-5.2",
                    "max_tokens": 400,
                    "usd": 0.25
                }]
            }
        });

        let projected = super::project_access(payload, &seated_plan());
        let wire = projected.to_string();
        assert_eq!(projected["access"]["requested"], "codex");
        assert_eq!(projected["access"]["seat"], "codex");
        assert_eq!(projected["access"]["plans"][0]["model"], "openai/gpt-5.2");
        assert_eq!(projected["access"]["plans"][0]["access"], "codex");
        assert_eq!(projected["access"]["plans"][0]["chosen"], "harness");
        assert_eq!(projected["cost"]["basis"], "subscription_quota");
        for forbidden in ["usd", "max_tokens", "responding_model"] {
            assert!(!wire.contains(forbidden), "{forbidden} leaked: {wire}");
        }
        assert!(
            projected["cost"]["tasks"][0]["budget"]
                .as_str()
                .is_some_and(|budget| budget == "subscription_quota")
        );
    }

    /// No pin, no lane (a file with no inference): the payload is left
    /// exactly as the planner wrote it — no empty `access` block.
    #[test]
    fn a_planless_payload_is_left_alone() {
        let payload = json!({ "cost": { "bounded_total_usd": 0.0, "tasks": [] } });
        let projected = super::project_access(payload.clone(), &ExecutionAccessPlan::default());
        assert_eq!(projected, payload);
    }

    /// An unpinned API lane still shows in the machine preview — the
    /// plan speaks whenever it has something to say (`requested` null).
    #[test]
    fn an_unpinned_lane_is_projected_with_a_null_request() {
        let lane = ResolvedLane::new(
            AccessPlan::new(
                "mock/echo",
                "mock",
                "mock",
                AccessClass::Mock,
                BillingClass::Local,
                false,
                Vec::new(),
            ),
            1,
        );
        let plan = ExecutionAccessPlan::new(
            BTreeMap::from([("mock/echo".to_owned(), LaneVerdict::Admitted(lane))]),
            None,
            None,
            None,
        );
        let projected = super::project_access(json!({ "cost": { "tasks": [] } }), &plan);
        assert!(projected["access"]["requested"].is_null());
        assert_eq!(projected["access"]["resolved"], true);
        assert_eq!(projected["access"]["plans"][0]["chosen"], "mock");
        assert!(
            projected["cost"].get("basis").is_none(),
            "no seat, no quota basis"
        );
    }
}
