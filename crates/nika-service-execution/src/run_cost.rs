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

/// Check's mirror of the Run's monetary admission (descended from the host's
/// `run_cost::readiness`, C6): `None` when a Run of `wf` under `plan` needs no
/// fresh choice, otherwise why it is not run ready. An unknown-cost route is
/// never ready (it needs a fresh finite-call choice); a declared-free route is
/// ready only in a shape its observation admits. It admits nothing itself.
#[must_use]
pub fn readiness(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
) -> Option<String> {
    let routes = match unknown_routes(plan, config) {
        Ok(routes) => routes,
        Err(why) => return Some(format!("Run monetary admission is unresolved: {why}")),
    };
    if routes.is_empty() {
        // Check mirrors the Run's refusal of what a declared-free observation
        // cannot admit: never run ready, never a known zero.
        return declared_free_shape(wf, plan, config, None)
            .err()
            .map(|refusal| format!("Run cannot observe its declared-free route: {refusal}"));
    }
    Some(match request_bound(wf, plan, routes.len()) {
        Ok(bound) => format!(
            "USD cost is unknown: Run requires a fresh finite-call choice for at most {bound} requests, including schema re-asks; Check has not admitted spend or effects"
        ),
        Err(why) => format!("USD cost is unknown; Run cannot obtain a bounded choice: {why}"),
    })
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
    run_time_routes(wf, plan, config, overrides).map(|routes| routes.dynamic)
}

/// The run-time model facts of one workflow, from the one resolved-id walk.
#[derive(Default)]
struct RunTimeRoutes {
    /// A task's own `model:` is an expression, or a nested workflow exists.
    dynamic: bool,
    /// A nested `workflow:` invoke: its routes are judged at its dispatch.
    nested: bool,
    /// Tasks whose model only the run decides (an upstream output, CEL).
    undecided: Vec<String>,
    /// Values decided before any effect, as (task, model).
    decided: Vec<(String, String)>,
}

fn run_time_routes(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
    overrides: &std::collections::BTreeMap<String, serde_json::Value>,
) -> Result<RunTimeRoutes, RunTimeModelRefusal> {
    let mut routes = RunTimeRoutes::default();
    for spanned in &wf.tasks {
        let task = &spanned.value;
        let (declared, infer) = match &task.action {
            RawAction::Infer(a) => (a.model.as_ref(), Some(a)),
            RawAction::Agent(a) => (a.model.as_ref(), None),
            RawAction::Invoke(a) => {
                routes.nested |= a.tool().is_none();
                routes.dynamic |= a.tool().is_none();
                continue;
            }
            _ => continue,
        };
        let Some(expr) = declared.filter(|m| m.value.contains("${{")) else {
            continue;
        };
        routes.dynamic = true;
        let Some(model) = nika_runtime::resolve_model_expr(&expr.value, wf, overrides, Some(task))
        else {
            routes.undecided.push(task.id.value.clone());
            continue;
        };
        let task = task.id.value.clone();
        routes.decided.push((task.clone(), model.clone()));
        if nika_providers::resolve_refusal(&model).is_some()
            || plan.seat_for(&model).is_some()
            || api_class(&model) != nika_types::access::AccessClass::Api
        {
            continue;
        }
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
    Ok(routes)
}

/// The access class of a `provider/model` id (the ONE derivation).
fn api_class(model: &str) -> nika_types::access::AccessClass {
    let provider = model.split_once('/').map_or(model, |(p, _)| p);
    nika_providers::profile::access_class_for(nika_providers::canonical_provider(provider))
}

/// A readiness blocker: a stable kind slug, the registered NIKA code when
/// the law has one, the input, model or task it names, and one sentence
/// that never carries an input value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ReadinessBlocker {
    /// The stable kind (`input_unbound` · `input_refused` · `free_shape` ·
    /// `unknown_cost_unreviewable` · `model_route_unresolved` · `budget_floor`).
    pub kind: &'static str,
    /// The registered code when the law has one (`NIKA-1708` · `NIKA-1709`).
    pub code: Option<&'static str>,
    /// The input, model or task the blocker names.
    pub subject: Option<String>,
    /// The binding's reason class ([`crate::inputs::BindingFault::as_str`]).
    pub reason: Option<&'static str>,
    /// One value-free sentence.
    pub message: String,
}

impl ReadinessBlocker {
    /// Construct (INV-019).
    #[must_use]
    pub fn new(
        kind: &'static str,
        code: Option<&'static str>,
        subject: Option<String>,
        message: String,
    ) -> Self {
        Self {
            kind,
            code,
            subject,
            reason: None,
            message,
        }
    }
}

/// The program side of an unattended Run (`nika arm fire`): the binding,
/// required-input, model and cost law a scheduled fire meets, judged
/// read-only. `model_cost_ready` is `None` when a route is judged only at
/// dispatch (a nested workflow, a model only the run decides) or when no
/// access path is known without probing harness seats: unknown, never true.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ScheduledProgram {
    /// Whether every binding fits and every required input has a source.
    pub required_inputs_ready: bool,
    /// Model and cost admission: `Some(false)` on a blocker, `None` unknown.
    pub model_cost_ready: Option<bool>,
    /// The blockers of both axes, in order.
    pub blockers: Vec<ReadinessBlocker>,
    /// What readiness cannot derive, in words.
    pub unknowns: Vec<String>,
    /// The machine document: `required_inputs` · `optional_inputs` ·
    /// `undeclared_bindings` · `unbound_inputs` · `model_summary` ·
    /// `authority_summary` (the R4 89 receipt fields this owner judges).
    pub document: serde_json::Value,
}

/// Judge `wf` as an unattended fire binds it: `bindings` are the beat's
/// `KEY=VALUE` pairs and `ceiling_usd` its per-tick plafond; the review
/// channel is unavailable, as for `nika arm fire`. Read-only: nothing is
/// claimed or journaled, and access is observed by key presence only, so
/// harness seats are never spawned (they stay unknown).
#[must_use]
pub fn scheduled_program(
    wf: &RawWorkflow,
    report: &nika_check::CheckReport,
    config: &nika_providers::ProvidersConfig,
    bindings: &[String],
    ceiling_usd: f64,
) -> ScheduledProgram {
    let checks = crate::inputs::check_bindings(bindings, wf);
    let mut blockers = binding_blockers(wf, &checks);
    // The required-input law over the pairs that bind, so one refused
    // pair never hides (or fakes) another input's missing source.
    let fitting: Vec<String> = bindings
        .iter()
        .zip(&checks)
        .filter(|(_, check)| check.fault.is_none())
        .map(|(pair, _)| pair.clone())
        .collect();
    let overrides = crate::inputs::parse_var_overrides(&fitting, wf)
        .map(|bound| bound.values)
        .unwrap_or_default();
    if let Some(refusal) = nika_runtime::required_inputs_refusal(wf, &overrides) {
        let missing = match &refusal {
            nika_runtime::RuntimeError::MissingRequiredInputs { missing, .. } => missing.clone(),
            _ => Vec::new(),
        };
        // An input whose binding is refused is reported once, as refused.
        let refused = |input: &str| {
            checks
                .iter()
                .any(|check| check.input == input && check.fault.is_some())
        };
        for input in missing.into_iter().filter(|input| !refused(input)) {
            blockers.push(ReadinessBlocker::new(
                "input_unbound",
                Some("NIKA-1708"),
                Some(input.clone()),
                format!("required input `{input}` has no binding source (schedule literal, declared environment or workflow default)"),
            ));
        }
    }
    let required_inputs_ready = blockers.is_empty();
    let (model_blockers, unknowns, routes) =
        model_cost(wf, report, config, &overrides, ceiling_usd);
    let model_cost_ready = if !model_blockers.is_empty() {
        Some(false)
    } else if unknowns.is_empty() {
        Some(true)
    } else {
        None
    };
    blockers.extend(model_blockers);
    let mut document = inputs_document(wf, &checks);
    document["model_summary"] = serde_json::json!({
        "ready": model_cost_ready,
        "ceiling_usd": ceiling_usd,
        "review_channel": "unavailable",
        "routes": routes,
        "unknown": unknowns,
    });
    document["authority_summary"] = authority_document(wf);
    ScheduledProgram {
        required_inputs_ready,
        model_cost_ready,
        blockers,
        unknowns,
        document,
    }
}

/// One value-free blocker per binding that cannot bind.
fn binding_blockers(
    wf: &RawWorkflow,
    checks: &[crate::inputs::BindingCheck],
) -> Vec<ReadinessBlocker> {
    use crate::inputs::BindingFault;
    checks
        .iter()
        .filter_map(|check| {
            let fault = check.fault?;
            let input = &check.input;
            let door = check
                .env
                .as_deref()
                .map_or(String::new(), |var| format!(" ← @env:{var}"));
            let message = match fault {
                BindingFault::Malformed => "a schedule binding is not KEY=VALUE".to_owned(),
                BindingFault::UnknownInput => {
                    let declared: Vec<&str> =
                        wf.inputs.iter().map(|(k, _)| k.value.as_str()).collect();
                    format!(
                        "binding `{input}` names no declared input (declared: {})",
                        declared.join(" · ")
                    )
                }
                BindingFault::EnvNameMissing => format!("binding `{input}`: `@env:` names no variable"),
                BindingFault::EnvUndeclaredInCi => format!(
                    "binding `{input}`{door}: under CI the workflow must declare the variable in `permits.env:`"
                ),
                BindingFault::EnvUnset => {
                    format!("binding `{input}`{door}: the variable is not set (or empty)")
                }
                BindingFault::TypeMismatch => format!(
                    "binding `{input}`{door}: the value does not fit the declared type `{}` (withheld)",
                    declared_type(wf, input)
                ),
            };
            let mut blocker =
                ReadinessBlocker::new("input_refused", None, Some(input.clone()), message);
            blocker.reason = Some(fault.as_str());
            Some(blocker)
        })
        .collect()
}

fn declared_type(wf: &RawWorkflow, input: &str) -> String {
    wf.inputs
        .iter()
        .find(|(key, _)| key.value == input)
        .map_or_else(String::new, |(_, decl)| match decl {
            nika_schema::types::VarDecl::Typed { r#type, .. } => {
                nika_schema::types::type_expr_display(&r#type.value)
            }
            nika_schema::types::VarDecl::Untyped(_) => "untyped".to_owned(),
        })
}

/// Every declared input with its binding source and status (required,
/// then optional), every binding that names no declared input, and the
/// required inputs left unbound (R4 71: required minus bound). Never a value.
fn inputs_document(wf: &RawWorkflow, checks: &[crate::inputs::BindingCheck]) -> serde_json::Value {
    use nika_schema::types::VarDecl;
    let (mut required, mut optional, mut unbound) = (Vec::new(), Vec::new(), Vec::new());
    for (name, decl) in &wf.inputs {
        let (is_required, has_default) = match decl {
            VarDecl::Typed {
                required, default, ..
            } => (*required, default.is_some()),
            VarDecl::Untyped(_) => (false, true),
        };
        let check = checks.iter().rev().find(|check| check.input == name.value);
        let (source, status) = match check {
            Some(check) => (
                if check.env.is_some() {
                    Some("declared_environment")
                } else {
                    Some("schedule_literal")
                },
                if check.fault.is_some() {
                    "refused"
                } else {
                    "bound"
                },
            ),
            None if has_default => (Some("workflow_default"), "bound"),
            None => (None, if is_required { "unbound" } else { "absent" }),
        };
        let entry = serde_json::json!({
            "name": name.value,
            "required": is_required,
            "binding_source": source,
            "environment_variable": check.and_then(|check| check.env.clone()),
            "binding_status": status,
            "reason": check.and_then(|check| check.fault).map(crate::inputs::BindingFault::as_str),
        });
        if !is_required {
            optional.push(entry);
            continue;
        }
        if status != "bound" {
            unbound.push(name.value.clone());
        }
        required.push(entry);
    }
    let undeclared: Vec<serde_json::Value> = checks
        .iter()
        .filter(|check| !wf.inputs.iter().any(|(name, _)| name.value == check.input))
        .map(|check| {
            serde_json::json!({
                "name": check.input,
                "reason": check.fault.map(crate::inputs::BindingFault::as_str),
            })
        })
        .collect();
    serde_json::json!({
        "required_inputs": required,
        "optional_inputs": optional,
        "undeclared_bindings": undeclared,
        "unbound_inputs": unbound,
    })
}

/// The model and cost law of an unattended fire over the plan the fire
/// resolves (key presence only). A model the inputs decide is written in as
/// the literal it renders to, so its access, its admission (resolution,
/// thinking, capacity) and the budget floor judge it exactly as they would
/// judge that literal `model:`.
fn model_cost(
    wf: &RawWorkflow,
    report: &nika_check::CheckReport,
    config: &nika_providers::ProvidersConfig,
    overrides: &std::collections::BTreeMap<String, serde_json::Value>,
    ceiling_usd: f64,
) -> (Vec<ReadinessBlocker>, Vec<String>, Vec<serde_json::Value>) {
    let mut blockers = Vec::new();
    let mut unknowns = Vec::new();
    let registry = nika_providers::ProviderRegistry::without_http(config.clone());
    let probes = nika_providers::probe::collect_provider_probes(&registry);
    let static_plan = crate::access::resolve_plan_over(wf, report, None, None, &probes);
    let decided = decided_routes(
        wf,
        &static_plan,
        config,
        overrides,
        &mut blockers,
        &mut unknowns,
    );
    let literal = (!decided.is_empty()).then(|| with_decided_models(wf, &decided));
    let literal_report = literal.as_ref().map(nika_check::check);
    let (wf, report) = match (&literal, &literal_report) {
        (Some(literal), Some(literal_report)) => (literal, literal_report),
        _ => (wf, report),
    };
    let plan = crate::access::resolve_plan_over(wf, report, None, None, &probes);
    let routes = plan
        .lanes
        .keys()
        .map(|model| route_row(model, &decided, config))
        .collect();
    if let Some(refusal) = nika_runtime::plan_refusal(&plan) {
        unknowns.push(format!(
            "{refusal} (readiness observes API keys only; harness seats are not probed)"
        ));
    }
    match unknown_routes(&plan, config) {
        Err(why) => blockers.push(ReadinessBlocker::new(
            "model_route_unresolved",
            None,
            None,
            why,
        )),
        Ok(unknown) => blockers.extend(
            unknown
                .into_iter()
                .map(|(model, _)| unknown_cost_blocker(model)),
        ),
    }
    if let Err(refusal) = declared_free_shape(wf, &plan, config, None) {
        blockers.push(ReadinessBlocker::new(
            "free_shape",
            None,
            Some(refusal.model.clone()),
            refusal.to_string(),
        ));
    }
    if literal.is_some() {
        // The literal admission a captured `model:` already met.
        blockers.extend(
            nika_execution::model_admission_findings(wf, None)
                .into_iter()
                .map(|finding| {
                    ReadinessBlocker::new("model_admission_refused", None, None, finding)
                }),
        );
    }
    if let Some(error) =
        nika_runtime::budget_floor_refusal_seated(wf, report, Some(ceiling_usd), None, false)
    {
        blockers.push(ReadinessBlocker::new(
            "budget_floor",
            Some("NIKA-1709"),
            None,
            error.to_string(),
        ));
    }
    (blockers, unknowns, routes)
}

/// The run-time walk's facts: an unknown for what only a dispatch decides,
/// a blocker for a decided route the Run refuses, the decided (task, model).
fn decided_routes(
    wf: &RawWorkflow,
    plan: &nika_providers::ExecutionAccessPlan,
    config: &nika_providers::ProvidersConfig,
    overrides: &std::collections::BTreeMap<String, serde_json::Value>,
    blockers: &mut Vec<ReadinessBlocker>,
    unknowns: &mut Vec<String>,
) -> Vec<(String, String)> {
    match run_time_routes(wf, plan, config, overrides) {
        Ok(run_time) => {
            unknowns.extend(run_time.undecided.iter().map(|task| {
                format!(
                    "task `{task}`: its model is decided only at run time and judged at dispatch"
                )
            }));
            if run_time.nested {
                unknowns.push(
                    "a nested workflow's routes are judged only at its own dispatch".to_owned(),
                );
            }
            run_time.decided
        }
        Err(RunTimeModelRefusal::UnknownCost { model, .. }) => {
            blockers.push(unknown_cost_blocker(model));
            Vec::new()
        }
        Err(refusal) => {
            blockers.push(ReadinessBlocker::new(
                "free_shape",
                None,
                None,
                refusal.to_string(),
            ));
            Vec::new()
        }
    }
}

/// `wf` with each decided task's `model:` expression replaced by the literal
/// it renders to (the value only; spans and every other field unchanged).
fn with_decided_models(wf: &RawWorkflow, decided: &[(String, String)]) -> RawWorkflow {
    let mut literal = wf.clone();
    for spanned in &mut literal.tasks {
        let task = &mut spanned.value;
        let Some((_, model)) = decided.iter().find(|(id, _)| *id == task.id.value) else {
            continue;
        };
        let slot = match &mut task.action {
            RawAction::Infer(action) => action.model.as_mut(),
            RawAction::Agent(action) => action.model.as_mut(),
            _ => None,
        };
        if let Some(expr) = slot {
            expr.value.clone_from(model);
        }
    }
    literal
}

fn unknown_cost_blocker(model: String) -> ReadinessBlocker {
    let message = format!(
        "model `{model}` has an unknown USD cost; a scheduled fire cannot obtain a fresh unknown-cost choice"
    );
    ReadinessBlocker::new("unknown_cost_unreviewable", None, Some(model), message)
}

fn route_row(
    model: &str,
    decided: &[(String, String)],
    config: &nika_providers::ProvidersConfig,
) -> serde_json::Value {
    let task = decided
        .iter()
        .find(|(_, decided)| decided == model)
        .map(|(task, _)| task);
    serde_json::json!({
        "model": model,
        "source": if task.is_some() { "run_time" } else { "static" },
        "task": task,
        "class": route_class(model, config),
    })
}

/// The admission class of one route, from the providers' one predicate.
fn route_class(model: &str, config: &nika_providers::ProvidersConfig) -> &'static str {
    use nika_types::access::AccessClass;
    if nika_providers::resolve_refusal(model).is_some() {
        return "unresolvable";
    }
    match api_class(model) {
        AccessClass::Mock => "mock",
        AccessClass::Local => "local_unpriced",
        AccessClass::Api => {
            match nika_providers::admission::unknown_cost_route(model, config.clone()) {
                Ok(None) if declared_free_max(model, config).is_some() => "declared_free_observed",
                Ok(None) => "priced",
                Ok(Some(_)) => "unknown_cost",
                Err(_) => "unresolved_cost",
            }
        }
        _ => "other_access",
    }
}

/// What an unattended fire would need beyond the program: the declared
/// boundary, secret references (never values), human gates, and the
/// authorities readiness never acquires.
fn authority_document(wf: &RawWorkflow) -> serde_json::Value {
    let permits = wf
        .permits
        .as_ref()
        .map(|permits| serde_json::to_value(&permits.value).unwrap_or(serde_json::Value::Null));
    let secrets: Vec<serde_json::Value> = wf
        .secrets
        .iter()
        .map(|(name, reference)| {
            serde_json::json!({
                "name": name.value,
                "source": format!("{:?}", reference.value.source).to_lowercase(),
            })
        })
        .collect();
    let gates: Vec<&str> = wf
        .tasks
        .iter()
        .filter(|task| {
            matches!(&task.value.action, RawAction::Invoke(action)
                if action.tool().is_some_and(|tool| tool.value == "nika:prompt"))
        })
        .map(|task| task.value.id.value.as_str())
        .collect();
    serde_json::json!({
        "permits": permits,
        "secrets": secrets,
        "human_gates": gates,
        "activation": "not_acquired",
        "monetary": "not_acquired",
    })
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
