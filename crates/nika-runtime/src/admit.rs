// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Run-admission gates. Every refusal precedes the prologue, so it emits no
//! event and spends no task. Constructors serve runtime and CLI preflights.

use std::collections::BTreeMap;

use nika_check::CheckReport;
use nika_providers::ExecutionAccessPlan;
use nika_providers::probe::ProviderProbe;
use nika_providers::resolve_access::PinRefusal;
use nika_schema::raw::{RawAction, RawTask, RawWorkflow};
use nika_schema::types::VarDecl;
use serde_json::Value;

use crate::errors::RuntimeError;
// The pre-effect model resolver lives beside the static analysis it builds on.
use nika_check::analyzer::rendered_collections;
pub use nika_check::analyzer::resolve_model_expr;
use nika_check::analyzer::resolved_infer_models;
// The `--task` cone cut is a pure graph walk beside the edges it reads.
pub use nika_check::analyzer::scope_to_task;
// The static builtin floor and the unpriced-cloud class read the catalog beside it.
use nika_check::analyzer::priced_builtin_floor;
pub(crate) use nika_check::analyzer::unpriced_cloud_seat;

/// The run's launch gates, in order: the report trust check
/// (audit-before-run) · the required-input preflight below · the budget
/// floor ([`budget_floor_refusal`]) · the MODELS rung over the seats this
/// run uses · the access plan — all refuse BEFORE the prologue, so a
/// refused run emits zero events and spends zero tasks.
pub(crate) fn gates(
    wf: &RawWorkflow,
    report: &CheckReport,
    overrides: &BTreeMap<String, Value>,
    budget: Option<f64>,
    model_override: Option<&str>,
    (access_pin, probes, plan): (Option<&str>, &[ProviderProbe], Option<&ExecutionAccessPlan>),
) -> Result<(), RuntimeError> {
    crate::trust::check_report(wf, report)?;
    if let Some(err) = required_inputs_refusal(wf, overrides) {
        return Err(err);
    }
    if let Some(err) =
        budget_floor_refusal_bound(wf, report, budget, model_override, overrides, false)
    {
        return Err(err);
    }
    if let Some(err) = models_refusal(wf, report, model_override, overrides) {
        return Err(err);
    }
    // One Door · wave 1: a frozen plan IS the access admission — the
    // gate fires on EVERY run that carries one (pinned or not), never
    // only when `--access` was typed. A bare embedder keeps the pin gate.
    match plan {
        Some(plan) => {
            if let Some(err) = plan_refusal(plan) {
                return Err(err);
            }
            // Judge the same effective envelope that produced the plan.
            // A task-local model keeps winning over the operator's default.
            let effective = model_override.map(|model| nika_check::with_model_override(wf, model));
            if let Some(err) = modelless_refusal(effective.as_ref().unwrap_or(wf), plan) {
                return Err(err);
            }
        }
        None => {
            if let Some(err) = access_pin_refusal(wf, report, probes, access_pin, model_override) {
                return Err(err);
            }
        }
    }
    Ok(())
}

/// W3-F13 · an `infer:`/`agent:` task whose effective model is EMPTY
/// (no task `model:`, no envelope `model:`) rides a seat or nothing: with
/// no seat pinned the run has no path, and it says so BEFORE task 1
/// (NIKA-1800) instead of dying in dispatch on an empty model. `check`'s
/// ACCESS layer and the dry-run read the same judge.
/// Pass the effective workflow, with any envelope model override applied via
/// [`nika_check::with_model_override`]; unrelated admitted lanes do not supply
/// a missing task model.
#[must_use]
pub fn modelless_refusal(wf: &RawWorkflow, plan: &ExecutionAccessPlan) -> Option<RuntimeError> {
    if plan.seat.is_some() || plan.pin_refusal.is_some() {
        return None;
    }
    let task = first_modelless_task(wf)?;
    Some(RuntimeError::AccessNoPath {
        message: format!(
            "task `{task}` names no model and no seat is pinned — set `model: \
             <provider/name>` on the task or the envelope, or run with `--access \
             <seat>` (`nika doctor` lists the seats this machine holds)"
        ),
    })
}

/// The first `infer:`/`agent:` task without a task or envelope model.
/// Callers must first apply an operator's effective envelope override through
/// [`nika_check::with_model_override`]; unrelated model lanes do not supply it.
#[must_use]
pub fn first_modelless_task(wf: &RawWorkflow) -> Option<&str> {
    if wf.model.is_some() {
        return None;
    }
    wf.tasks.iter().find_map(|task| match &task.value.action {
        nika_schema::raw::RawAction::Infer(a) if a.model.is_none() => {
            Some(task.value.id.value.as_str())
        }
        nika_schema::raw::RawAction::Agent(a) if a.model.is_none() => {
            Some(task.value.id.value.as_str())
        }
        _ => None,
    })
}

/// The launch refusal a frozen plan carries: the pin judge's own
/// teaching (NIKA-1800..1803) when a pin failed, else the first lane
/// no path survived for — every candidate with its witness (A-8). The
/// ONE constructor the composer's preflight and the runtime gate speak.
#[must_use]
pub fn plan_refusal(plan: &ExecutionAccessPlan) -> Option<RuntimeError> {
    if let Some(refusal) = plan.pin_refusal.clone() {
        return Some(map_pin_refusal(refusal));
    }
    let (model, refusal) = plan.first_refused()?;
    let witnesses: Vec<String> = refusal
        .rejected
        .iter()
        .map(nika_types::access::AccessRejection::witness_line)
        .collect();
    let rendered = if witnesses.is_empty() {
        format!(
            "model `{model}` names provider `{}` — no access candidate exists for it here \
             (`nika doctor` lists the providers this binary drives)",
            refusal.provider
        )
    } else {
        format!(
            "no access path is ready for `{model}` on this machine · {} · nothing ran",
            witnesses.join(" · ")
        )
    };
    Some(RuntimeError::AccessNoPath { message: rendered })
}

/// The budget-floor admission gate — `Some` run-abort error (NIKA-1709)
/// when the workflow's unavoidable cost floor already exceeds the budget
/// the run was launched under. The ONE constructor both admission
/// surfaces speak: the CLI's standalone preflight calls this same
/// function and never reaches `run`, so the gate here is
/// the fail-closed word for every OTHER embedder — the composed child
/// above all, whose budget is the parent's remaining at call time (spec
/// 14 law 6) and which used to RUN where the standalone form refused
/// (the 2026-07-29 composition bypass). A `None` budget never refuses;
/// the mid-run ledger (NIKA-1704) still owns the crossing that the
/// static floor cannot see (gates opening · retries · fan-outs).
///
/// The floor prices the EFFECTIVE model (#342): a `--model` override
/// replaces the envelope default (a per-task `model:` keeps winning), so
/// the gate never fires on the file's model while the run uses another.
///
/// Priced builtins (B24 / issue 1296) fold in on top of the infer
/// envelope: `nika check` still skips `invoke:` (no token bound), but a
/// catalog floor already over the cap must refuse before HTTP — the
/// mid-run NIKA-1704 abort is the spend-then-apologise this gate closes.
///
/// B20 R1 / issue 1297: a `--var` that resolves an envelope or task
/// `model:` CEL is not on the static report. The internal `gates` path
/// passes those bindings so the live id is judged here; this 4-arg form (the CLI
/// preflight) still prices `--model` and the file.
///
/// #1368 · the unresolvable arm: a resolved id the ONE resolver
/// (`nika_providers::resolve_refusal`) refuses — a bare id · an unknown
/// prefix · a cataloged vendor this binary cannot drive — joins the
/// unpriced cloud class here. Such an id floored at $0 and passed ANY
/// budget (the gauntlet's `claude-opus-4.1`): an armed cap cannot
/// bound a seat it cannot name.
#[must_use]
pub fn budget_floor_refusal(
    wf: &RawWorkflow,
    report: &CheckReport,
    budget: Option<f64>,
    model_override: Option<&str>,
) -> Option<RuntimeError> {
    budget_floor_refusal_bound(wf, report, budget, model_override, &BTreeMap::new(), false)
}

/// The same gate for a run SEATED on a harness (`--access codex` and its kin, a subscription
/// the seat owns): an unpriced cloud model is not unknown spend there — the subscription's
/// own plan bounds it and the cap meters the priced builtins only — so the unpriced-cloud arm
/// stands down while the floor of priced work and the unresolvable-id arm still judge.
/// Measured 2026-09-23: `nika run --access codex` with `openai/gpt-6-astra` answered in 17 s
/// and refused NIKA-1709 under any cap.
#[must_use]
pub fn budget_floor_refusal_seated(
    wf: &RawWorkflow,
    report: &CheckReport,
    budget: Option<f64>,
    model_override: Option<&str>,
    seated_on_harness: bool,
) -> Option<RuntimeError> {
    budget_floor_refusal_bound(
        wf,
        report,
        budget,
        model_override,
        &BTreeMap::new(),
        seated_on_harness,
    )
}

/// The ONE budget floor over the workflow as the run binds it (B11): the gate
/// the run's own launch applies, for a host that holds the invocation's
/// validated `overrides` (`--var` · `--inputs-json`) before the run starts.
/// It prices [`effective_workflow`]: `--model`, then the bound fans and
/// models. [`budget_floor_refusal`] and [`budget_floor_refusal_seated`] are
/// this gate with no bindings.
#[must_use]
pub fn budget_floor_refusal_bound(
    wf: &RawWorkflow,
    report: &CheckReport,
    budget: Option<f64>,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
    seated_on_harness: bool,
) -> Option<RuntimeError> {
    let budget = budget?;
    if let Some(err) =
        unmeterable_seat_on_resolved_ids(wf, budget, model_override, overrides, seated_on_harness)
    {
        return Some(err);
    }
    let owned;
    let effective = match effective_workflow(wf, model_override, overrides) {
        Some(seated) => {
            owned = nika_check::check(&seated);
            &owned
        }
        None => report,
    };
    if !seated_on_harness && let Some(err) = unpriced_cloud_cap_refusal(effective, budget) {
        return Some(err);
    }
    if !seated_on_harness && let Some(message) = effective.cost.zero_budget_refusal(budget) {
        return Some(RuntimeError::BudgetFloor { message });
    }
    let floor = effective.cost.min_path_total_usd + priced_builtin_floor(wf);
    let message = floor_refusal(floor, budget)?;
    Some(RuntimeError::BudgetFloor { message })
}

/// The workflow as this run seats it before any effect (B9): the operator's
/// `--model` in the envelope (a task's own `model:` keeps winning), then every
/// fan over an input the invocation binds iterating the bound value (B11 · a
/// bound value never falls back to the default), then every `model:` its
/// bindings, a declared default or a const decide, as literals. `None` when
/// that is the file itself. A seat only the run decides stays an expression,
/// judged at dispatch. Public so a host's budget warnings describe the same
/// workflow its floor prices.
#[must_use]
pub fn effective_workflow(
    wf: &RawWorkflow,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
) -> Option<RawWorkflow> {
    let seated = model_override.map(|m| nika_check::with_model_override(wf, m));
    let bound = rendered_collections(seated.as_ref().unwrap_or(wf), overrides).or(seated);
    nika_check::analyzer::rendered_models(bound.as_ref().unwrap_or(wf), overrides).or(bound)
}

/// The MODELS rung `nika check` applies to a literal seat, over every seat
/// this run uses before any effect, literal or rendered: the ONE resolver's
/// refusal (through a declared default, as the check's rung reads it), then
/// the thinking and capacity laws. A report judged over a template never saw
/// the value a binding gave it, so the run re-derives the rung here and
/// refuses NIKA-1707 with the rung's own words.
fn models_refusal(
    wf: &RawWorkflow,
    report: &CheckReport,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
) -> Option<RuntimeError> {
    let seated = effective_workflow(wf, model_override, overrides);
    let owned;
    let (wf, report) = match &seated {
        Some(seated) => {
            owned = nika_check::check(seated);
            (seated, &owned)
        }
        None => (wf, report),
    };
    let mut why: Vec<String> = report
        .requirements
        .models
        .iter()
        .filter_map(|m| {
            let judged = match nika_check::static_literal_of(wf, &m.model) {
                Some(default) => default.as_str()?,
                None if m.model.contains("${{") => return None,
                None => m.model.as_str(),
            };
            let refusal = nika_providers::resolve_refusal(judged)?;
            Some(format!("`{judged}` · {}", refusal.why))
        })
        .collect();
    why.extend(nika_check::thinking_findings(wf).into_iter().map(|f| f.why));
    why.extend(nika_check::capacity_findings(wf).into_iter().map(|f| f.why));
    let why = (!why.is_empty()).then(|| why.join(" · "))?;
    let detail = format!("the MODELS rung refuses a seat this run uses: {why}");
    Some(RuntimeError::ReportMismatch { detail })
}

/// B9 phase C · the pre-send guard for a seat only the run decides (a task
/// output, an answer, an item), which the launch gates could not judge.
/// Dispatch calls it with the rendered `seat` before any provider or seat
/// request. It judges this task as ONE call in the effective workflow (the
/// envelope with the operator's `--model`, the seat rendered, its loop
/// multiplicity cleared and its gate open, every other field kept) with the
/// launch gates' own laws, and returns the refusal's code and words: the
/// MODELS rung (NIKA-INFER-004 when the finding names it, the reasoning
/// seat's cap floor, else NIKA-INFER-001), then, under a cap, an unpriced
/// cloud seat off a harness or a floor above `remaining` (NIKA-1704).
/// `remaining` is the ledger read at call time, the cap minus KNOWN spend:
/// a snapshot, never a reservation. Unknown charges make it an upper bound,
/// so a refusal is certain while a pass proves no fit, and siblings in flight
/// may still cross together (the ledger's wave-boundary abort owns that).
pub(crate) fn run_decided_refusal(
    (wf, task): (&RawWorkflow, &RawTask),
    (overrides, model_override): (&BTreeMap<String, Value>, Option<&str>),
    seat: &str,
    remaining: Option<f64>,
    on_harness: bool,
) -> Option<(&'static str, String)> {
    let expr = match &task.action {
        RawAction::Infer(action) => action.model.as_ref(),
        RawAction::Agent(action) => action.model.as_ref(),
        _ => None,
    }?;
    if resolve_model_expr(&expr.value, wf, overrides, Some(task)).is_some() {
        return None; // a literal, or a seat the launch gates judged
    }
    let mut one =
        model_override.map_or_else(|| wf.clone(), |m| nika_check::with_model_override(wf, m));
    one.tasks.retain(|t| t.value.id.value == task.id.value);
    for t in &mut one.tasks {
        (t.value.for_each, t.value.when) = (None, None);
        let model = match &mut t.value.action {
            RawAction::Infer(action) => action.model.as_mut(),
            RawAction::Agent(action) => action.model.as_mut(),
            _ => None,
        };
        if let Some(model) = model {
            seat.clone_into(&mut model.value);
        }
    }
    let why: Vec<String> = nika_check::thinking_findings(&one)
        .into_iter()
        .map(|f| f.why)
        .chain(
            nika_check::capacity_findings(&one)
                .into_iter()
                .map(|f| f.why),
        )
        .collect();
    let before = "refused before the provider request";
    if !why.is_empty() {
        let floor = why.iter().any(|w| w.contains("NIKA-INFER-004"));
        let code = if floor {
            "NIKA-INFER-004"
        } else {
            "NIKA-INFER-001"
        };
        return Some((code, format!("{before}: {}", why.join(" · "))));
    }
    let remaining = remaining?;
    if !on_harness && unpriced_cloud_seat(seat) {
        let why = format!("{before}: cloud model `{seat}` is unpriced, so the cap cannot bound it");
        return Some(("NIKA-1704", why));
    }
    let floor = nika_check::check(&one).cost.min_path_total_usd;
    let why = format!(
        "{before}: this call's cost floor ${floor:.6} exceeds the ${remaining:.6} left under \
         --max-cost-usd (a snapshot at call time, never a reservation)"
    );
    (floor > remaining).then_some(("NIKA-1704", why))
}

/// B20 / issue 1297: `--max-cost-usd` cannot bound a cloud seat the
/// catalog does not price. Refuse before the prologue (zero events,
/// zero spend). Mock and local unpriced seats are the sparing arms —
/// they never trip this gate.
fn unpriced_cloud_cap_refusal(report: &CheckReport, budget: f64) -> Option<RuntimeError> {
    let unpriced: Vec<String> = report
        .data_journey
        .model_endpoints
        .iter()
        .filter(|endpoint| endpoint.locus == nika_check::EndpointLocus::Cloud && !endpoint.priced)
        .map(|endpoint| endpoint.model.clone())
        .collect();
    unpriced_cloud_message(&unpriced, budget)
}

/// The resolved-id walk (W0-D-R1): after the run model is known —
/// CLI `--model`, envelope/task CEL that a `--var` or a declared
/// default fills, the envelope literal — an unpriced cloud seat
/// under a cap refuses even when `nika check` named a priced default.
///
/// #1368 · the stronger arm: an id the ONE resolver
/// ([`nika_providers::resolve_refusal`] — the MODELS rung's own
/// predicate, so check ≡ run by construction) REFUSES is not merely
/// unpriced — an armed cap cannot bound a seat the binary cannot even
/// name. That id used to skip BOTH arms below (`unpriced_cloud_seat`
/// spares an unknown provider by construction), floor at $0, and pass
/// ANY budget — the gauntlet's `claude-opus-4.1` dot variant ran with
/// zero budget protection, dying at dispatch (or « succeeding » under
/// `on_error: skip`) after the cap had silently disarmed.
fn unmeterable_seat_on_resolved_ids(
    wf: &RawWorkflow,
    budget: f64,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
    seated_on_harness: bool,
) -> Option<RuntimeError> {
    let mut unresolvable: Vec<(String, String)> = Vec::new();
    let mut unpriced: Vec<String> = Vec::new();
    for model in resolved_infer_models(wf, model_override, overrides) {
        // The resolver's refusal is the stronger claim, judged first: a
        // cataloged vendor this binary cannot drive (the azure class) is
        // unresolvable HERE, not merely unpriced.
        if let Some(refusal) = nika_providers::resolve_refusal(&model) {
            unresolvable.push((model, refusal.why));
        } else if unpriced_cloud_seat(&model) {
            unpriced.push(model);
        }
    }
    // Seated on a harness, an unpriced cloud id is the subscription's to bound; an id this
    // binary cannot resolve still refuses.
    unresolvable_seat_message(&unresolvable, budget).or_else(|| {
        (!seated_on_harness)
            .then(|| unpriced_cloud_message(&unpriced, budget))
            .flatten()
    })
}

/// #1368 · the unresolvable arm's refusal: every seat with the resolver's
/// own why verbatim (the `<provider>/<model>` contract · the pasteable
/// repair · the did-you-mean — the MODELS rung's teaching, never a second
/// phrasing), then the budget law and the two honest ways out (pin a
/// catalog seat · drop the cap for a local/mock rehearsal).
fn unresolvable_seat_message(
    unresolvable: &[(String, String)],
    budget: f64,
) -> Option<RuntimeError> {
    if unresolvable.is_empty() {
        return None;
    }
    let models = unresolvable
        .iter()
        .map(|(model, why)| format!("`{model}` — {why}"))
        .collect::<Vec<_>>()
        .join(" · ");
    Some(RuntimeError::BudgetFloor {
        message: format!(
            "refusing to start: model {models}. --max-cost-usd ${budget:.6} cannot bound \
             a model this binary cannot resolve — an uncataloged id is not free, it is \
             unmetered, and a budget it disarms is no budget. Pin a `<provider>/<model>` \
             catalog seat (`nika catalog` lists the runnable providers under LOCAL and \
             CLOUD), or drop the cap for a local/mock rehearsal.\n"
        ),
    })
}

fn unpriced_cloud_message(unpriced: &[String], budget: f64) -> Option<RuntimeError> {
    if unpriced.is_empty() {
        return None;
    }
    let models = unpriced.join(", ");
    Some(RuntimeError::BudgetFloor {
        message: format!(
            "refusing to start: cloud model {models} is unpriced — \
             --max-cost-usd ${budget:.6} cannot bound unknown spend \
             (`nika check` reports priced: false). Pick a priced catalog \
             seat, or drop the cap for a local/mock rehearsal.\n"
        ),
    })
}

/// The missing-required-input refusal — `Some` run-abort error when a
/// `required: true` input has neither a declared `default:` nor an
/// operator override, `None` when every required input is satisfied.
/// The ONE constructor both admission surfaces (the runtime's launch
/// gate · the CLI's input gauntlet) speak.
#[must_use]
pub fn required_inputs_refusal(
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
) -> Option<RuntimeError> {
    let missing: Vec<String> = wf
        .inputs
        .iter()
        .filter(|(key, decl)| {
            matches!(
                decl,
                VarDecl::Typed {
                    required: true,
                    default: None,
                    ..
                }
            ) && !overrides.contains_key(&key.value)
        })
        .map(|(key, _)| key.value.clone())
        .collect();
    if missing.is_empty() {
        return None;
    }
    let declared = wf.inputs.iter().map(|(key, _)| key.value.clone()).collect();
    Some(RuntimeError::MissingRequiredInputs { missing, declared })
}

/// `Some(refusal)` when the `--max-cost-usd` floor exceeds the budget —
/// pure, so the operator-facing gate is unit-testable. A floor AT the
/// budget passes (spending exactly the budget is not over it). The
/// budget floor is a launch gate of the same family as
/// [`required_inputs_refusal`] (refuse BEFORE any spend — descended
/// from the run verb's budget preflight 2026-07-22).
#[must_use]
pub fn floor_refusal(floor: f64, budget: f64) -> Option<String> {
    (floor > budget).then(|| {
        format!(
            "refusing to start: the workflow's unavoidable cost floor \
             ${floor:.6} exceeds --max-cost-usd ${budget:.6} (cheapest \
             static path · gates closed · first-try) — raise the budget \
             or trim the workflow (`nika check` shows the envelope)\n"
        )
    })
}

/// Tally the unbounded tasks BY THEIR ACTUAL reason (the report carries
/// `unbounded_reason` per task) instead of parroting the fixed
/// disjunction — a priced-but-unbounded task read « unpriced model »,
/// which misleads (the fixable one is `no max_tokens`, not the model).
/// The operator sees WHICH kind they have, and which is fixable.
/// (Descended from the run verb's budget preflight 2026-07-22.)
#[must_use]
pub fn unbounded_breakdown(cost: &nika_check::CostCeiling) -> String {
    use nika_check::UnboundedReason;

    let (mut no_tokens, mut unpriced, mut unknown_iters) = (0_usize, 0_usize, 0_usize);
    let mut run_time = 0_usize;
    for t in cost.tasks.iter().filter(|t| t.usd.is_none()) {
        match t.unbounded_reason {
            Some(UnboundedReason::NoTokenLimit) => no_tokens += 1,
            // B9 · a seat only the run decides has no price YET: never
            // « unpriced », which reads as free.
            Some(UnboundedReason::NoPrice)
                if t.model.as_deref().is_none_or(|m| m.contains("${{")) =>
            {
                run_time += 1;
            }
            Some(UnboundedReason::NoPrice) => unpriced += 1,
            // A task with no price AND no ceiling records ONE reason
            // (NoPrice wins in the check ladder); UnknownIterations, an
            // unclassified None, and any FUTURE reason (the enum is
            // #[non_exhaustive]) all count as the generic bucket.
            _ => unknown_iters += 1,
        }
    }
    let total = no_tokens + unpriced + run_time + unknown_iters;
    let mut parts = Vec::new();
    if no_tokens > 0 {
        parts.push(format!("{no_tokens} with no `max_tokens`"));
    }
    if unpriced > 0 {
        parts.push(format!("{unpriced} on an unpriced model"));
    }
    if run_time > 0 {
        parts.push(format!("{run_time} on a model decided at run time"));
    }
    if unknown_iters > 0 {
        parts.push(format!("{unknown_iters} with unknown iterations"));
    }
    format!(
        "{total} task(s) have no static ceiling ({})",
        parts.join(" · ")
    )
}

/// Judge an explicit access pin against every statically known model.
/// Templated models remain the dispatch layer's responsibility.
#[must_use]
pub fn access_pin_refusal(
    wf: &RawWorkflow,
    report: &CheckReport,
    probes: &[ProviderProbe],
    access_pin: Option<&str>,
    model_override: Option<&str>,
) -> Option<RuntimeError> {
    let pin = access_pin?;
    let has_infer = wf
        .tasks
        .iter()
        .any(|task| matches!(&task.value.action, nika_schema::raw::RawAction::Infer(_)));
    let has_agent = wf
        .tasks
        .iter()
        .any(|task| matches!(&task.value.action, nika_schema::raw::RawAction::Agent(_)));
    let models: Vec<String> = match model_override {
        Some(m) => nika_check::check(&nika_check::with_model_override(wf, m))
            .requirements
            .models
            .iter()
            .map(|r| r.model.clone())
            .collect(),
        None => report
            .requirements
            .models
            .iter()
            .map(|r| r.model.clone())
            .collect(),
    };
    // Templated models are not admission-time facts.
    let judged = models
        .iter()
        .map(String::as_str)
        .filter(|m| !m.contains("${{"));
    nika_providers::refuse_pin_for_verbs(judged, probes, pin, has_infer, has_agent)
        .map(map_pin_refusal)
}

fn map_pin_refusal(refusal: PinRefusal) -> RuntimeError {
    match refusal {
        PinRefusal::UnknownToken { message } => RuntimeError::AccessUnknownToken { message },
        PinRefusal::PinUnsatisfied { message } => RuntimeError::AccessPinUnsatisfied { message },
        PinRefusal::NoPath { message } => RuntimeError::AccessNoPath { message },
        PinRefusal::Unavailable { message } => RuntimeError::AccessUnavailable { message },
        // Future classes fail closed until mapped explicitly.
        _ => RuntimeError::AccessNoPath {
            message: "access pin refusal could not be classified".to_owned(),
        },
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::float_cmp
)]
mod tests;
