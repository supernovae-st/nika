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

/// A task that uses an exact catalog-declared-free route in a shape its
/// provider observation cannot admit. That observation takes bounded text only
/// (no tools, thinking, vision or memory; a literal output bound the tariff
/// covers). Host-side and static like [`RunShapeError`]: Check and Run refuse
/// before any effect, and nothing unsupported becomes a known zero.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "task `{task}` uses the declared-free route {model} with {shape}; that route admits bounded text only: no tools, thinking or vision, and a literal max_tokens within its tariff"
)]
#[non_exhaustive]
pub struct FreeShapeRefusal {
    /// The task that uses the route.
    pub task: String,
    /// The route's model, as the workflow names it.
    pub model: String,
    /// What the route cannot admit (`an agent loop` · `thinking` · `vision` ·
    /// `an output bound outside its tariff`).
    pub shape: &'static str,
}

/// Whether an admitted API lane of `plan` is an exact catalog-declared-free
/// route, every task that uses one being a shape its observation admits.
/// `Ok(false)` means no such route: the Run keeps today's composition, and no
/// account or text guard reaches its paid, local or mock lanes.
/// `model_override` is the Run's `--model` (Check passes the effective file).
/// # Errors
/// The first task that uses a declared-free route with an unsupported shape.
pub fn declared_free_shape(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
    model_override: Option<&str>,
) -> Result<bool, FreeShapeRefusal> {
    let free: std::collections::BTreeMap<&str, u32> = plan
        .admitted()
        .filter(|(_, lane)| lane.plan.chosen == nika_types::access::AccessClass::Api)
        .filter_map(|(model, _)| Some((model, declared_free_max(model, config)?)))
        .collect();
    let fallback = model_override.or(wf.model.as_ref().map(|m| m.value.as_str()));
    for task in &wf.tasks {
        let (declared, infer) = match &task.value.action {
            RawAction::Infer(a) => (a.model.as_ref(), Some(a)),
            RawAction::Agent(a) => (a.model.as_ref(), None),
            _ => continue,
        };
        let model = declared.map(|m| m.value.as_str()).or(fallback);
        let Some((model, &max)) = model.and_then(|m| free.get_key_value(m)) else {
            continue;
        };
        // This first slice admits direct infer only: an agent loop (tools,
        // turns, its own output bounds) stays refused on a free route.
        let shape = infer.map_or(Some("an agent loop"), |a| free_infer_shape(a, max));
        if let Some(shape) = shape {
            return Err(FreeShapeRefusal {
                task: task.value.id.value.clone(),
                model: (*model).to_owned(),
                shape,
            });
        }
    }
    Ok(!free.is_empty())
}

/// The admitted API lanes of `plan` whose USD cost is unknown: the routes
/// only a fresh unknown-cost choice may admit. One predicate
/// (`admission::unknown_cost_route`) judges these and a route rendered at
/// run time alike (descended from the host's Run review, 2026-09-28).
/// # Errors
/// A lane neither an unknown-cost route nor a native price can judge.
pub fn unknown_routes(
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
) -> Result<Vec<(String, nika_providers::admission::CostRoute)>, String> {
    let mut unknown = Vec::new();
    for (model, lane) in plan.admitted() {
        if lane.plan.chosen == nika_types::access::AccessClass::Api
            && let Some(route) =
                nika_providers::admission::unknown_cost_route(model, config.clone())?
        {
            unknown.push((model.to_owned(), route));
        }
    }
    Ok(unknown)
}

/// The output bound of `model`'s exact catalog-declared-free tariff, if any.
fn declared_free_max(model: &str, config: &nika_providers::ProvidersConfig) -> Option<u32> {
    let route = nika_providers::admission::CostRoute::observe(model, config.clone()).ok()?;
    nika_providers::InferenceAdmission::qualify(&route.provider, &route.model, &route.endpoint)
        .ok()
        .filter(|tariff| tariff.is_declared_free())
        .map(|tariff| tariff.max_output_tokens)
}

/// Why a Run refuses, before any effect, a task `model:` expression whose
/// value its inputs, declared defaults or const already decide.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RunTimeModelRefusal {
    /// The value is a declared-free route in a shape its observation cannot admit.
    #[error(transparent)]
    FreeShape(FreeShapeRefusal),
    /// The value is an API route the Run's review calls unknown-cost.
    #[error(
        "task `{task}` resolves its model to {model} before any effect; that route's USD cost is unknown, and a fresh unknown-cost choice binds only a literal `model:`"
    )]
    UnknownCost {
        /// The task.
        task: String,
        /// The resolved route.
        model: String,
    },
}

/// Judge each infer/agent task whose own `model:` is an expression at the
/// value the runtime's resolved-id walk gives it before any effect (`overrides`
/// over declared defaults, const, `with:`), exactly as that literal would be
/// judged: a declared-free route in a shape its observation cannot admit, or
/// an API route with an unknown USD cost, refuses. A value only the run decides
/// (an upstream output, an item) is left to the Run observer at dispatch:
/// refused before provider bytes, after earlier effects. A seated or local
/// value never reaches the registry. `Ok(true)`: such a task exists, or a
/// nested `workflow:` whose routes no root plan sees, so the Run binds that
/// observer (its children share it); that enforcement is at dispatch only.
/// # Errors
/// The first decided value the Run would refuse.
pub fn run_time_models(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
    overrides: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<bool, RunTimeModelRefusal> {
    let mut dynamic = false;
    for task in &wf.tasks {
        let (declared, infer) = match &task.value.action {
            RawAction::Infer(a) => (a.model.as_ref(), Some(a)),
            RawAction::Agent(a) => (a.model.as_ref(), None),
            RawAction::Invoke(a) => {
                dynamic |= a.tool().is_none();
                continue;
            }
            _ => continue,
        };
        let Some(expr) = declared.filter(|m| m.value.contains("${{")) else {
            continue;
        };
        dynamic = true;
        let Some(model) =
            nika_runtime::resolve_model_expr(&expr.value, wf, overrides, Some(&task.value))
                .filter(|m| {
                    nika_providers::resolve_refusal(m).is_none() && plan.seat_for(m).is_none()
                })
                .filter(|m| {
                    let provider = m.split_once('/').map_or(m.as_str(), |(p, _)| p);
                    nika_providers::profile::access_class_for(nika_providers::canonical_provider(
                        provider,
                    )) == nika_types::access::AccessClass::Api
                })
        else {
            continue;
        };
        let task = task.value.id.value.clone();
        if let Some(max) = declared_free_max(&model, config) {
            let shape = infer.map_or(Some("an agent loop"), |a| free_infer_shape(a, max));
            if let Some(shape) = shape {
                return Err(RunTimeModelRefusal::FreeShape(FreeShapeRefusal {
                    task,
                    model,
                    shape,
                }));
            }
        } else if !matches!(
            nika_providers::admission::unknown_cost_route(&model, config.clone()),
            Ok(None)
        ) {
            return Err(RunTimeModelRefusal::UnknownCost { task, model });
        }
    }
    Ok(dynamic)
}

/// What one infer asks that a declared-free observation cannot admit: the
/// same request fields the provider's bounded-text guard refuses (a thinking
/// budget reaches the wire only when enabled), and an output bound the tariff
/// does not cover.
fn free_infer_shape(action: &RawInferAction, max: u32) -> Option<&'static str> {
    if action
        .thinking
        .as_ref()
        .is_some_and(|t| t.value.enabled && t.value.budget_tokens.is_some())
    {
        Some("thinking")
    } else if !action.vision.is_empty() {
        Some("vision")
    } else if action
        .max_tokens
        .as_ref()
        .is_none_or(|n| n.value == 0 || n.value > max)
    {
        Some("an output bound outside its tariff")
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
