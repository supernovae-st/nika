// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Pure finite-call analysis for the production composer; never execution authority.
use nika_check::analyzer::static_args::{ConstStrings, judgeable_arg};
use nika_schema::raw::{RawAction, RawInferAction, RawInvokeAction, RawWorkflow};
use std::path::{Component, Path, PathBuf};

/// Upper bound on requests for a checked, static, single-route text workflow.
/// Includes every schema re-ask made by the stock production `InferVerb`.
/// This observation grants no spending, filesystem or tool authority. The host
/// must still obtain fresh scoped consent and re-observe its source and inputs;
/// the runtime and provider admission account enforce each actual invocation.
/// # Errors
/// Unsupported shape, invalid DAG/permits or a model outside the selected route.
pub fn request_bound(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    unknown_routes: usize,
) -> Result<u32, String> {
    if unknown_routes != 1 || plan.lanes.len() != 1 || !plan.is_admitted() {
        return Err("unknown-cost Run requires one exact admitted API route".into());
    }
    if !wf.secrets.is_empty() {
        return Err("unknown-cost Run cannot bind external secret inputs in this review".into());
    }
    let consts = ConstStrings::of(wf);
    let mut requests = 0_u32;
    for task in &wf.tasks {
        let task = &task.value;
        if task.for_each.is_some() || task.retry.is_some() || task.on_error.is_some() {
            return Err("unknown-cost Run does not support fan-out, retry or recovery".into());
        }
        match &task.action {
            RawAction::Infer(action) => {
                requests = requests
                    .checked_add(infer_bound(wf, action, plan)?)
                    .ok_or("request bound overflow")?;
            }
            RawAction::Invoke(action) => match action.tool().map(|t| t.value.as_str()) {
                Some("nika:read" | "nika:write") => {
                    project_file_path(&consts, action)?;
                }
                // BuiltinDispatcher routes assert directly to core_tools::assert:
                // a boolean check with no I/O. Canonical pure-call exemptions still apply.
                Some("nika:jq" | "nika:assert") => {}
                _ => {
                    return Err(
                        "unknown-cost Run supports only direct infer and local read/write/jq/assert; no nested workflow or other tools".into(),
                    );
                }
            },
            _ => return Err("unknown-cost Run does not support exec or agent inference".into()),
        }
    }
    let report = nika_check::check(wf);
    if !report.is_clean() || report.waves.iter().flatten().count() != wf.tasks.len() {
        return Err(
            "unknown-cost Run requires a clean checked DAG; monetary choice cannot grant effects"
                .into(),
        );
    }
    if report.waves.iter().any(|wave| {
        wave.iter()
            .filter(|&&i| matches!(wf.tasks[i].value.action, RawAction::Infer(_)))
            .count()
            > 1
    }) {
        return Err(
            "unknown-cost Run requires sequential infer waves; parallel calls are unsupported"
                .into(),
        );
    }
    if requests == 0 {
        return Err("unknown-cost Run has no statically bounded direct infer".into());
    }
    Ok(requests)
}

fn infer_bound(
    wf: &RawWorkflow,
    action: &RawInferAction,
    plan: &nika_providers::ExecutionAccessPlan,
) -> Result<u32, String> {
    let model = action
        .model
        .as_ref()
        .or(wf.model.as_ref())
        .ok_or("unknown-cost Run requires a literal selected model")?;
    if model.value.contains("${{") || !plan.lanes.contains_key(&model.value) {
        return Err("unknown-cost Run model differs from the selected static route".into());
    }
    if action
        .max_tokens
        .as_ref()
        .is_none_or(|n| n.value == 0 || n.value > 8192)
        || action.thinking.is_some()
        || !action.vision.is_empty()
    {
        return Err(
            "unknown-cost Run requires text, max_tokens 1..8192, no thinking/vision".into(),
        );
    }
    // Runtime::compose uses InferVerb::new. Every schema re-ask traverses the
    // SAME admission account; admitted transport never retries a failed call.
    Ok(1 + if action.schema.is_some() {
        u32::from(nika_verb_infer::DEFAULT_SCHEMA_RETRY_BUDGET)
    } else {
        0
    })
}

/// Resolve only a literal or immutable bare-const path confined lexically to a project.
/// Does not open the path or attest containment through filesystem links.
/// # Errors
/// Dynamic, missing, absolute or parent-traversing path. Hosts still need fresh
/// descriptor-rooted observations and the existing effect permits.
pub fn project_file_path(
    consts: &ConstStrings,
    action: &RawInvokeAction,
) -> Result<PathBuf, String> {
    let path = judgeable_arg(consts, action, "path")
        .ok_or("local file step requires a literal or immutable const path")?;
    let path = Path::new(path.strip_prefix("./").unwrap_or(&path));
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains("${{")
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err(
            "unknown-cost Run file paths must be static files confined to the project".into(),
        );
    }
    Ok(path.into())
}

#[cfg(test)]
mod tests;
