// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Run-admission gates. Every refusal precedes the prologue, so it emits no
//! event and spends no task. Constructors serve runtime and CLI preflights.

use std::collections::BTreeMap;

use nika_check::CheckReport;
use nika_providers::ExecutionAccessPlan;
use nika_providers::probe::ProviderProbe;
use nika_providers::resolve_access::PinRefusal;
use nika_schema::raw::{ForEachValue, RawAction, RawTask, RawWorkflow};
use nika_schema::types::VarDecl;
use serde_json::Value;

use crate::errors::RuntimeError;

/// The run's launch gates, in order: the report trust check
/// (audit-before-run) · the required-input preflight below · the budget
/// floor ([`budget_floor_refusal`]) — all refuse BEFORE the prologue, so a
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
    if let Some(err) = budget_floor_at(wf, report, budget, model_override, overrides) {
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
            if let Some(err) = modelless_refusal(wf, plan) {
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

/// The first `infer:`/`agent:` task with an EMPTY effective model.
fn first_modelless_task(wf: &RawWorkflow) -> Option<&str> {
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
    budget_floor_at(wf, report, budget, model_override, &BTreeMap::new())
}

fn budget_floor_at(
    wf: &RawWorkflow,
    report: &CheckReport,
    budget: Option<f64>,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
) -> Option<RuntimeError> {
    let budget = budget?;
    if let Some(err) = unmeterable_seat_on_resolved_ids(wf, budget, model_override, overrides) {
        return Some(err);
    }
    let owned;
    let effective = match model_override {
        Some(m) => {
            owned = nika_check::check(&nika_check::with_model_override(wf, m));
            &owned
        }
        None => report,
    };
    if let Some(err) = unpriced_cloud_cap_refusal(effective, budget) {
        return Some(err);
    }
    let floor = effective.cost.min_path_total_usd + priced_builtin_floor(wf);
    let message = floor_refusal(floor, budget)?;
    Some(RuntimeError::BudgetFloor { message })
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
    unresolvable_seat_message(&unresolvable, budget)
        .or_else(|| unpriced_cloud_message(&unpriced, budget))
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

fn resolved_infer_models(
    wf: &RawWorkflow,
    model_override: Option<&str>,
    overrides: &BTreeMap<String, Value>,
) -> Vec<String> {
    let envelope = wf.model.as_ref().map(|m| m.value.as_str());
    let default = model_override
        .map(str::to_owned)
        .or_else(|| envelope.and_then(|expr| resolve_model_expr(expr, wf, overrides, None)));
    wf.tasks
        .iter()
        .filter_map(|task| {
            let declared = match &task.value.action {
                RawAction::Infer(action) => action.model.as_ref().map(|m| m.value.as_str()),
                RawAction::Agent(action) => action.model.as_ref().map(|m| m.value.as_str()),
                _ => return None,
            };
            match declared {
                Some(expr) => resolve_model_expr(expr, wf, overrides, Some(&task.value)),
                None => default.clone(),
            }
        })
        .collect()
}

fn resolve_model_expr(
    expr: &str,
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    task: Option<&RawTask>,
) -> Option<String> {
    if !expr.contains("${{") {
        return Some(expr.to_owned());
    }
    if let Some(joined) = concat_model_expr(expr, wf, overrides, task) {
        return Some(joined);
    }
    if let Some((authority, name)) = nika_check::analyzer::bare_static_ref(expr)
        && authority == "inputs."
        && let Some(value) = overrides.get(name).and_then(Value::as_str)
    {
        return Some(value.to_owned());
    }
    if let Some(from_with) = with_alias(expr, wf, overrides, task) {
        return Some(from_with);
    }
    nika_check::static_literal_of(wf, expr)?
        .as_str()
        .map(str::to_owned)
}

/// `${{ inputs.provider }}/${{ inputs.name }}` — both sides resolve, the
/// slash is the catalog seat spelling (N01 / issue 1319).
fn concat_model_expr(
    expr: &str,
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    task: Option<&RawTask>,
) -> Option<String> {
    let (left, right) = expr.split_once('/')?;
    if !left.contains("${{") || !right.contains("${{") {
        return None;
    }
    let left = resolve_model_expr(left, wf, overrides, task)?;
    let right = resolve_model_expr(right, wf, overrides, task)?;
    if left.contains("${{") || right.contains("${{") {
        return None;
    }
    Some(format!("{left}/{right}"))
}

/// `${{ with.model }}` follows the task's `with:` alias (N01).
fn with_alias(
    expr: &str,
    wf: &RawWorkflow,
    overrides: &BTreeMap<String, Value>,
    task: Option<&RawTask>,
) -> Option<String> {
    let inner = expr.trim().strip_prefix("${{")?.strip_suffix("}}")?.trim();
    let name = inner.strip_prefix("with.")?;
    if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let task = task?;
    let (_, bound) = task.with.iter().find(|(k, _)| k.value == name)?;
    let next = bound.value.as_str()?;
    resolve_model_expr(next, wf, overrides, None)
}

/// A recognized third-party cloud seat with no snapshot row. Unknown
/// providers stay unknown (never promoted to cloud — but NOT spared:
/// the resolved-id walk's unresolvable arm refuses them under a cap
/// through [`nika_providers::resolve_refusal`], #1368). Mock and local
/// are the sparing arms — unpriced, never this class.
pub(crate) fn unpriced_cloud_seat(model: &str) -> bool {
    if model == "mock" || model.starts_with("mock/") {
        return false;
    }
    let provider = model.split_once('/').map_or(model, |(p, _)| p);
    let Some(entry) = nika_catalog::find_provider(provider) else {
        return false;
    };
    let local = entry
        .tags
        .iter()
        .any(|tag| matches!(tag, nika_catalog::Tag::Local))
        || entry
            .data_policy
            .is_some_and(|policy| policy.zdr == "local");
    if local {
        return false;
    }
    nika_catalog::find_pricing_for(model).is_none()
}

/// Unavoidable catalog spend of priced `invoke:` tasks (cheapest path:
/// `when:` closed → $0 · first-try · known `n:` · known `for_each`
/// length). Templated provider/`n` and expression `for_each` stay off
/// this floor — the mid-run ledger still owns what statics cannot see.
fn priced_builtin_floor(wf: &RawWorkflow) -> f64 {
    wf.tasks.iter().map(|t| invoke_static_floor(&t.value)).sum()
}

fn invoke_static_floor(task: &RawTask) -> f64 {
    if task.when.is_some() {
        return 0.0;
    }
    let RawAction::Invoke(inv) = &task.action else {
        return 0.0;
    };
    let Some(tool) = inv.tool() else {
        return 0.0;
    };
    let Some(args) = inv.args.as_ref() else {
        return 0.0;
    };
    let Some(provider) = static_provider(&args.value) else {
        return 0.0;
    };
    let Some(per) = nika_catalog::builtin_provider_floor_usd(&tool.value, provider) else {
        return 0.0;
    };
    per * static_n(&args.value) * static_iterations(task)
}

fn static_provider(args: &Value) -> Option<&str> {
    if let Some(provider) = args.get("provider").and_then(Value::as_str) {
        return static_literal(provider);
    }
    let model = args.get("model").and_then(Value::as_str)?;
    let model = static_literal(model)?;
    model.contains("grok-imagine").then_some("xai")
}

fn static_literal(s: &str) -> Option<&str> {
    (!s.contains("${{")).then_some(s)
}

fn static_n(args: &Value) -> f64 {
    #[allow(clippy::cast_precision_loss)] // image `n:` is capped at 10
    args.get("n")
        .and_then(Value::as_u64)
        .map_or(1.0, |n| n.max(1) as f64)
}

fn static_iterations(task: &RawTask) -> f64 {
    match task.for_each.as_ref().map(|f| &f.value) {
        None => 1.0,
        Some(ForEachValue::List(arr)) => {
            #[allow(clippy::cast_precision_loss)] // literal list length is a task count
            {
                arr.as_array().map_or(1, Vec::len) as f64
            }
        }
        // Unknown count: cheapest path cannot claim a floor (NIKA-1704).
        Some(ForEachValue::Expression(_)) => 0.0,
        #[allow(
            clippy::unreachable,
            reason = "non_exhaustive future variant — enum and runtime ship together"
        )]
        _ => unreachable!("unsupported for_each form"),
    }
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

/// `--task` scoping — the ancestor-cone cut behind the regenerate-one-
/// block move (its gate + re-check live in the run verb; this is the
/// pure graph walk · descended from the run verb 2026-07-22 — DAG
/// assembly is the runtime's family, the launch-gate module its home).
///
/// Ancestors must run — their outputs feed the target's bindings; nothing
/// downstream or sibling executes. Document order is preserved (stable
/// waves) and workflow `outputs:` drop (they may reference tasks outside
/// the scope — the target's own output IS the point of the run). Unknown
/// ids fail with the available set (environment class · exit 3 · before
/// any effect — the same lane as an unknown `--var` key).
///
/// # Errors
///
/// A human-readable refusal naming the declared task ids.
pub fn scope_to_task(mut wf: RawWorkflow, target: &str) -> Result<RawWorkflow, String> {
    use std::collections::{BTreeSet, VecDeque};

    let mut deps_of: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for t in &wf.tasks {
        deps_of.insert(
            t.value.id.value.as_str().to_owned(),
            nika_check::analyzer::edges::producer_ids(&t.value),
        );
    }
    if !deps_of.contains_key(target) {
        let known = deps_of.keys().cloned().collect::<Vec<_>>().join(" · ");
        return Err(format!(
            "--task `{target}` names no task in this workflow — tasks: {known}"
        ));
    }

    let mut keep: BTreeSet<String> = BTreeSet::new();
    let mut queue: VecDeque<String> = VecDeque::from([target.to_owned()]);
    while let Some(id) = queue.pop_front() {
        if !keep.insert(id.clone()) {
            continue;
        }
        if let Some(deps) = deps_of.get(&id) {
            for d in deps {
                queue.push_back(d.clone());
            }
        }
    }

    wf.tasks
        .retain(|t| keep.contains(t.value.id.value.as_str()));
    wf.outputs.clear();
    Ok(wf)
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
    for t in cost.tasks.iter().filter(|t| t.usd.is_none()) {
        match t.unbounded_reason {
            Some(UnboundedReason::NoTokenLimit) => no_tokens += 1,
            Some(UnboundedReason::NoPrice) => unpriced += 1,
            // A task with no price AND no ceiling records ONE reason
            // (NoPrice wins in the check ladder); UnknownIterations, an
            // unclassified None, and any FUTURE reason (the enum is
            // #[non_exhaustive]) all count as the generic bucket.
            _ => unknown_iters += 1,
        }
    }
    let total = no_tokens + unpriced + unknown_iters;
    let mut parts = Vec::new();
    if no_tokens > 0 {
        parts.push(format!("{no_tokens} with no `max_tokens`"));
    }
    if unpriced > 0 {
        parts.push(format!("{unpriced} on an unpriced model"));
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
