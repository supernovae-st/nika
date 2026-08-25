// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The `--dry-run` lane: model swap, refusal, then one honest preview.

use nika_check::CheckReport;
use nika_schema::raw::RawWorkflow;
use nika_types::access::{AccessClass, AccessPlan};

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
#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub(super) fn lane(
    file: &str,
    source: &str,
    wf: &RawWorkflow,
    report: &CheckReport,
    skills: &nika_schema::ResolvedSkills,
    repair_target: nika_display::check_render::RepairTarget,
    model_override: Option<&str>,
    access_pin: Option<&str>,
    json: bool,
    theme: Theme,
    output_json: bool,
) -> RunVerdict {
    if let Some(model) = model_override {
        match swap(wf, model) {
            Ok((wf, report)) => {
                if let Some(verdict) = access_refusal(&wf, &report, access_pin, output_json) {
                    return verdict;
                }
                return RunVerdict::bare(verdict(file, &wf, &report, access_pin, json, theme));
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
    if let Some(verdict) = access_refusal(wf, report, access_pin, output_json) {
        return verdict;
    }
    RunVerdict::bare(verdict(file, wf, report, access_pin, json, theme))
}

fn access_refusal(
    wf: &RawWorkflow,
    report: &CheckReport,
    access_pin: Option<&str>,
    output_json: bool,
) -> Option<RunVerdict> {
    let probes = nika_cli_host::probe::access_probes_with_harness();
    nika_runtime::access_pin_refusal(wf, report, &probes, access_pin, None).map(|error| {
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
    access_pin: Option<&str>,
    json: bool,
    theme: Theme,
) -> u8 {
    let plans = preview_access_plans(report, access_pin);
    let seated = plans
        .values()
        .any(|plan| plan.chosen == AccessClass::Harness);
    if json {
        let payload = project_access(
            nika_check::plan::payload(file, wf, report),
            access_pin,
            &plans,
        );
        println!("{payload:#}");
    } else if seated {
        println!("{file} · subscription-seat preview");
        for plan in plans.values() {
            println!(
                "  requested {} → seat {} · subscription quota",
                plan.model, plan.access
            );
        }
    } else {
        if let Some(pin) = access_pin {
            println!("access: pinned `{pin}` · admission satisfied");
        }
        let plan = crate::verbs::inspect::render_pair(wf, report, theme);
        if !plan.text.is_empty() {
            println!("{}", plan.text.trim_end());
        }
        println!("\n  dry-run · plan only · no effects executed");
    }
    exit::OK
}

fn preview_access_plans(
    report: &CheckReport,
    access_pin: Option<&str>,
) -> std::collections::BTreeMap<String, AccessPlan> {
    let Some(pin) = access_pin else {
        return std::collections::BTreeMap::new();
    };
    let models: Vec<String> = report
        .requirements
        .models
        .iter()
        .map(|model| model.model.clone())
        .collect();
    let probes = nika_cli_host::probe::access_probes_with_harness();
    nika_providers::access_plan_map(&models, &probes, Some(pin))
}

fn project_access(
    mut payload: serde_json::Value,
    access_pin: Option<&str>,
    plans: &std::collections::BTreeMap<String, AccessPlan>,
) -> serde_json::Value {
    let Some(pin) = access_pin else {
        return payload;
    };
    let rows: Vec<serde_json::Value> = plans
        .values()
        .map(|plan| {
            serde_json::json!({
                "requested_model": plan.model,
                "access": plan.access,
                "class": plan.chosen.as_str(),
                "billing": plan.billing.as_str(),
            })
        })
        .collect();
    let seated = plans
        .values()
        .any(|plan| plan.chosen == AccessClass::Harness);
    payload["access"] = serde_json::json!({
        "requested": pin,
        "resolved": !plans.is_empty(),
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

    use nika_types::access::{AccessClass, AccessPlan, BillingClass};
    use serde_json::json;

    #[test]
    fn subscription_preview_names_request_and_contains_no_numeric_meter() {
        let plans = BTreeMap::from([(
            "openai/gpt-5.2".to_owned(),
            AccessPlan::new(
                "openai/gpt-5.2",
                "openai",
                "codex",
                AccessClass::Harness,
                BillingClass::IncludedQuota,
                true,
                Vec::new(),
            ),
        )]);
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

        let projected = super::project_access(payload, Some("codex"), &plans);
        let wire = projected.to_string();
        assert_eq!(projected["access"]["requested"], "codex");
        assert_eq!(
            projected["access"]["plans"][0]["requested_model"],
            "openai/gpt-5.2"
        );
        assert_eq!(projected["access"]["plans"][0]["access"], "codex");
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
}
