// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
//! Pure finite-call analysis for the production composer; never execution authority.
use nika_check::analyzer::static_args::{ConstStrings, judgeable_arg};
use nika_schema::raw::{RawAction, RawInferAction, RawInvokeAction, RawWorkflow};
use std::path::{Component, Path, PathBuf};

/// Why a workflow has no finite unknown-cost Run shape. The variant is the
/// reason a host can match; `Display` is the unchanged refusal it renders.
/// A host-side static observation: it never enters the workflow or verb plane.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RunShapeError {
    /// Not exactly one admitted API route with an unknown USD cost.
    #[error("unknown-cost Run requires one exact admitted API route")]
    Route,
    /// The workflow declares external secret inputs.
    #[error("unknown-cost Run cannot bind external secret inputs in this review")]
    Secrets,
    /// A task fans out, retries or recovers.
    #[error("unknown-cost Run does not support fan-out, retry or recovery")]
    Control,
    /// An infer names no literal model.
    #[error("unknown-cost Run requires a literal selected model")]
    Model,
    /// An infer's model is dynamic or outside the selected route.
    #[error("unknown-cost Run model differs from the selected static route")]
    OtherModel,
    /// An infer is not bounded text: `max_tokens` 1..=8192, no thinking or vision.
    #[error("unknown-cost Run requires text, max_tokens 1..8192, no thinking/vision")]
    InferShape,
    /// The summed request bound does not fit a `u32`.
    #[error("request bound overflow")]
    Overflow,
    /// A tool other than local read/write, jq or assert, or a nested workflow.
    #[error(
        "unknown-cost Run supports only direct infer and local read/write/jq/assert; no nested workflow or other tools"
    )]
    Tool,
    /// An exec or agent step.
    #[error("unknown-cost Run does not support exec or agent inference")]
    Action,
    /// The workflow does not check clean as one complete DAG.
    #[error("unknown-cost Run requires a clean checked DAG; monetary choice cannot grant effects")]
    Unchecked,
    /// Two infers share a wave.
    #[error("unknown-cost Run requires sequential infer waves; parallel calls are unsupported")]
    Parallel,
    /// No direct infer to bound.
    #[error("unknown-cost Run has no statically bounded direct infer")]
    NoInfer,
    /// A local file path is neither a literal nor an immutable bare const.
    #[error("local file step requires a literal or immutable const path")]
    DynamicPath,
    /// A local file path is empty, absolute or leaves the project lexically.
    #[error("unknown-cost Run file paths must be static files confined to the project")]
    UnconfinedPath,
}

/// Upper bound on requests for a checked, static, single-route text workflow.
/// Includes every schema re-ask made by the stock production `InferVerb`.
/// This observation grants no spending, filesystem or tool authority. The host
/// must still obtain fresh scoped consent and re-observe its source and inputs;
/// the runtime and provider admission account enforce each actual invocation.
/// # Errors
/// A [`RunShapeError`]: unsupported shape, invalid DAG/permits or a model
/// outside the selected route.
pub fn request_bound(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    unknown_routes: usize,
) -> Result<u32, RunShapeError> {
    if unknown_routes != 1 || plan.lanes.len() != 1 || !plan.is_admitted() {
        return Err(RunShapeError::Route);
    }
    if !wf.secrets.is_empty() {
        return Err(RunShapeError::Secrets);
    }
    let consts = ConstStrings::of(wf);
    let mut requests = 0_u32;
    for task in &wf.tasks {
        let task = &task.value;
        if task.for_each.is_some() || task.retry.is_some() || task.on_error.is_some() {
            return Err(RunShapeError::Control);
        }
        match &task.action {
            RawAction::Infer(action) => {
                requests = add_requests(requests, infer_bound(wf, action, plan)?)?;
            }
            RawAction::Invoke(action) => match action.tool().map(|t| t.value.as_str()) {
                Some("nika:read" | "nika:write") => {
                    project_file_path(&consts, action)?;
                }
                // BuiltinDispatcher routes assert directly to core_tools::assert:
                // a boolean check with no I/O. Canonical pure-call exemptions still apply.
                Some("nika:jq" | "nika:assert") => {}
                _ => return Err(RunShapeError::Tool),
            },
            _ => return Err(RunShapeError::Action),
        }
    }
    let report = nika_check::check(wf);
    if !report.is_clean() || report.waves.iter().flatten().count() != wf.tasks.len() {
        return Err(RunShapeError::Unchecked);
    }
    if report.waves.iter().any(|wave| {
        wave.iter()
            .filter(|&&i| matches!(wf.tasks[i].value.action, RawAction::Infer(_)))
            .count()
            > 1
    }) {
        return Err(RunShapeError::Parallel);
    }
    if requests == 0 {
        return Err(RunShapeError::NoInfer);
    }
    Ok(requests)
}

fn add_requests(total: u32, requests: u32) -> Result<u32, RunShapeError> {
    total.checked_add(requests).ok_or(RunShapeError::Overflow)
}

fn infer_bound(
    wf: &RawWorkflow,
    action: &RawInferAction,
    plan: &nika_providers::ExecutionAccessPlan,
) -> Result<u32, RunShapeError> {
    let model = action
        .model
        .as_ref()
        .or(wf.model.as_ref())
        .ok_or(RunShapeError::Model)?;
    if model.value.contains("${{") || !plan.lanes.contains_key(&model.value) {
        return Err(RunShapeError::OtherModel);
    }
    if action
        .max_tokens
        .as_ref()
        .is_none_or(|n| n.value == 0 || n.value > 8192)
        || action.thinking.is_some()
        || !action.vision.is_empty()
    {
        return Err(RunShapeError::InferShape);
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
/// [`RunShapeError::DynamicPath`] for a dynamic or missing path;
/// [`RunShapeError::UnconfinedPath`] for an empty, absolute or parent-traversing
/// one. Hosts still need fresh descriptor-rooted observations and the existing
/// effect permits.
pub fn project_file_path(
    consts: &ConstStrings,
    action: &RawInvokeAction,
) -> Result<PathBuf, RunShapeError> {
    let path = judgeable_arg(consts, action, "path").ok_or(RunShapeError::DynamicPath)?;
    let path = Path::new(path.strip_prefix("./").unwrap_or(&path));
    if path.as_os_str().is_empty()
        || path.to_string_lossy().contains("${{")
        || path
            .components()
            .any(|p| !matches!(p, Component::Normal(_)))
    {
        return Err(RunShapeError::UnconfinedPath);
    }
    Ok(path.into())
}

#[cfg(test)]
mod tests;
