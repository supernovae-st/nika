// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The versioned, effect-free run-plan projection.

use nika_schema::raw::RawWorkflow;

use crate::CheckReport;

/// Project the checked workflow into the machine-readable dry-run contract.
#[must_use]
pub fn payload(file: &str, wf: &RawWorkflow, report: &CheckReport) -> serde_json::Value {
    let ids: Vec<&str> = wf.tasks.iter().map(|t| t.value.id.value.as_str()).collect();
    let waves: Vec<Vec<&str>> = report
        .waves
        .iter()
        .map(|wave| {
            wave.iter()
                .filter_map(|&index| ids.get(index).copied())
                .collect()
        })
        .collect();
    let tasks: Vec<serde_json::Value> = wf
        .tasks
        .iter()
        .map(|task| {
            serde_json::json!({
                "id": task.value.id.value,
                "verb": task.value.action.verb(),
            })
        })
        .collect();
    serde_json::json!({
        "plan_version": 1,
        "workflow": wf.workflow.as_ref().map(|name| name.value.as_str()),
        "file": file,
        "dry_run": true,
        "effects_executed": false,
        "waves": waves,
        "tasks": tasks,
        "cost": report.cost,
        "permits": report.permits,
        "requirements": report.requirements,
    })
}
