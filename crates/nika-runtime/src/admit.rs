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
        }
        None => {
            if let Some(err) = access_pin_refusal(wf, report, probes, access_pin, model_override) {
                return Err(err);
            }
        }
    }
    Ok(())
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
mod tests {
    use std::sync::Arc;

    use nika_kernel::tool_executor::ToolResult;
    use nika_kernel_mock::{
        MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
    };
    use nika_verb_agent::AgentVerb;
    use nika_verb_exec::ExecVerb;
    use nika_verb_infer::InferVerb;
    use nika_verb_invoke::InvokeVerb;
    use serde_json::json;

    use super::*;
    use crate::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};

    type MockRuntime = Runtime<
        MockShell,
        MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    >;

    fn runtime_with(shell: MockShell) -> MockRuntime {
        runtime_with_tools(shell, MockToolExecutor::new()).0
    }

    fn runtime_with_tools(
        shell: MockShell,
        executor: MockToolExecutor,
    ) -> (MockRuntime, MockToolExecutor) {
        let probe = executor.clone();
        let provider = MockProvider::new("mock");
        let invoke = Arc::new(InvokeVerb::new(Arc::new(executor)));
        let runtime = Runtime::new(
            ExecVerb::new(Arc::new(shell)),
            Arc::clone(&invoke),
            InferVerb::new(
                Arc::new(nika_providers::ProviderRegistry::without_http(
                    nika_providers::ProvidersConfig::new(),
                )),
                "mock/echo",
            ),
            AgentVerb::new(
                Arc::new(provider),
                invoke,
                Arc::new(MockToolDefinitionProvider::new()),
                "mock/echo",
            ),
            MockClock::new(),
            RuntimeConfig::default(),
        );
        (runtime, probe)
    }

    /// One `nika:image_generate` task. Check's infer envelope is $0
    /// (invoke is skipped); the catalog floor is the admission number.
    fn image_generate_wf(provider: &str) -> String {
        format!(
            "nika: b24\npermits: {{ tools: [\"nika:image_generate\"], fs: {{ write: [\"./out/**\"] }} }}\ntasks:\n  og:\n    invoke: {{ tool: \"nika:image_generate\", args: {{ provider: {provider}, prompt: \"a monarch butterfly\", output_dir: \"./out\" }} }}\n"
        )
    }

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .unwrap_or_else(|_| panic!("fixture must parse"))
    }

    /// One `exec` task inside its declared boundary (clean report) · the
    /// `inputs:` block varies per case. NOTHING reads the input — the
    /// preflight judges the DECLARATION ⊕ the overrides, never the reads.
    fn fixture(inputs: &str) -> String {
        format!(
            "nika: admit\n{inputs}permits: {{ exec: [\"true\"] }}\ntasks:\n  t:\n    exec: {{ command: [\"true\"] }}\n"
        )
    }

    async fn run(runtime: &MockRuntime, wf: &RawWorkflow) -> RunOutcome {
        let report = nika_check::check(wf);
        assert!(report.is_clean(), "the fixture checks clean");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        runtime
            .run(wf, &report, &mut stamper, &mut sink)
            .await
            .unwrap_or_else(|_| panic!("run must settle"))
    }

    /// A refused run ABORTS at the preflight — the error, for inspection.
    /// The refusal precedes the prologue (fail-closed): the sink MUST
    /// stay empty (the trust-gate pin, mirrored once here).
    async fn run_refused(runtime: &MockRuntime, wf: &RawWorkflow) -> RuntimeError {
        let report = nika_check::check(wf);
        assert!(report.is_clean(), "the fixture checks clean");
        let mut stamper = DeterministicStamper::new();
        let mut sink = VecSink::new();
        let err = runtime
            .run(wf, &report, &mut stamper, &mut sink)
            .await
            .err()
            .unwrap_or_else(|| panic!("a refused run must never reach dispatch"));
        assert!(
            sink.events().is_empty(),
            "refusal must happen before any event, including the prologue"
        );
        err
    }

    #[test]
    fn required_without_default_or_override_is_the_missing_set() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n  region: { type: string, required: true }\n  limit: { type: integer, default: 3 }\n  note: { type: string }\n",
        ));
        let err = required_inputs_refusal(&wf, &BTreeMap::new()).expect("two unsatisfied");
        let RuntimeError::MissingRequiredInputs { missing, declared } = &err else {
            panic!("expected MissingRequiredInputs");
        };
        // Declaration order — the author's own reading order.
        assert_eq!(missing, &["needle", "region"]);
        assert_eq!(declared, &["needle", "region", "limit", "note"]);
    }

    #[test]
    fn satisfied_cases_pass_the_predicate() {
        let no_override = BTreeMap::new();
        // A declared `default:` satisfies (required or not).
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true, default: \"x\" }\n",
        ));
        assert!(required_inputs_refusal(&wf, &no_override).is_none());
        // A `--var` override satisfies a default-less required input.
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n",
        ));
        let overrides = BTreeMap::from([("needle".to_owned(), json!("ok"))]);
        assert!(required_inputs_refusal(&wf, &overrides).is_none());
        // A NON-required input without a default is unaffected (an unbound
        // OPTIONAL read stays the read-time NIKA-VAR-001).
        let wf = parse(&fixture("inputs:\n  note: { type: string }\n"));
        assert!(required_inputs_refusal(&wf, &no_override).is_none());
        // No `inputs:` block at all — nothing to refuse.
        let wf = parse(&fixture(""));
        assert!(required_inputs_refusal(&wf, &no_override).is_none());
    }

    /// (a) The #603 mechanism, refused: a default-less `required: true`
    /// input with no override ABORTS the run at admission — before one
    /// event, one task, one effect (the mock shell is the spend probe).
    #[tokio::test]
    async fn a_missing_required_input_is_refused_before_any_event() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n",
        ));
        let shell = MockShell::new(); // EMPTY — any execution is a bug
        let probe = shell.clone();
        let runtime = runtime_with(shell);
        let err = run_refused(&runtime, &wf).await;
        assert_eq!(err.spec_code(), "NIKA-1708");
        let msg = err.to_string();
        assert!(msg.contains("`needle`"), "the input must be named");
        assert!(
            msg.contains("--var needle=<value>"),
            "the remediation must ride"
        );
        assert!(
            probe.executed_commands().is_empty(),
            "not one task may execute"
        );
    }

    /// (b) The positive control: a `--var` override on the required input
    /// passes admission and the run completes (F4 — the override IS the
    /// input's value).
    #[tokio::test]
    async fn a_var_override_satisfies_the_admission_gate() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true }\n",
        ));
        let shell = MockShell::new().enqueue_ok("ok");
        let probe = shell.clone();
        let runtime = runtime_with(shell)
            .with_var_overrides(BTreeMap::from([("needle".to_owned(), json!("ok"))]));
        let outcome = run(&runtime, &wf).await;
        assert!(outcome.ok, "the satisfied gate is inert — no false refusal");
        assert_eq!(probe.executed_commands().len(), 1, "the exec ran");
    }

    /// (c) The author-side satisfaction: a declared `default:` passes
    /// admission with NO override.
    #[tokio::test]
    async fn a_declared_default_satisfies_the_admission_gate() {
        let wf = parse(&fixture(
            "inputs:\n  needle: { type: string, required: true, default: \"x\" }\n",
        ));
        let shell = MockShell::new().enqueue_ok("ok");
        let probe = shell.clone();
        let runtime = runtime_with(shell);
        let outcome = run(&runtime, &wf).await;
        assert!(outcome.ok, "a defaulted required input never refuses");
        assert_eq!(probe.executed_commands().len(), 1, "the exec ran");
    }

    // ── `--task` scope (descended from the run verb 2026-07-22) ──────

    /// `--task` scope · the diamond proves ancestors-only semantics: the
    /// target + transitive upstream survive · siblings and downstream drop
    /// · outputs clear (they may read unscoped tasks).
    #[test]
    fn scope_to_task_keeps_the_ancestor_cone() {
        let yaml = "nika: diamond\nmodel: mock/echo\ntasks:\n  discover:\n    invoke: { tool: \"nika:glob\", args: { pattern: \"*.md\" } }\n  stats:\n    with:\n      found: ${{ tasks.discover.output }}\n    infer: { prompt: \"count ${{ with.found }}\" }\n  digest:\n    with:\n      found: ${{ tasks.discover.output }}\n    infer: { prompt: \"sum ${{ with.found }}\" }\n  report:\n    with:\n      stats: ${{ tasks.stats.output }}\n      digest: ${{ tasks.digest.output }}\n    infer: { prompt: \"merge ${{ with.stats }} ${{ with.digest }}\" }\noutputs:\n  all: ${{ tasks.report.output }}\n";
        let wf = parse(yaml);

        let stats_only = scope_to_task(wf.clone(), "stats").expect("stats scopes");
        let ids: Vec<&str> = stats_only
            .tasks
            .iter()
            .map(|t| t.value.id.value.as_str())
            .collect();
        assert_eq!(
            ids,
            vec!["discover", "stats"],
            "target + its one ancestor · document order"
        );
        assert!(stats_only.outputs.is_empty(), "outputs drop under scope");

        let full = scope_to_task(wf.clone(), "report").expect("report scopes");
        assert_eq!(full.tasks.len(), 4, "the sink's cone is the whole diamond");

        let err = scope_to_task(wf, "nope")
            .err()
            .unwrap_or_else(|| panic!("unknown task id must be refused"));
        assert!(
            err.contains("nope") && err.contains("discover"),
            "names the id + the available set"
        );
    }

    /// The scoped sub-workflow re-checks CLEAN — the plan/waves/cost the
    /// run renders describe exactly the cone, not the original file.
    #[test]
    fn scoped_workflow_rechecks_clean() {
        let yaml = "nika: pair\nmodel: mock/echo\ntasks:\n  a:\n    infer: { prompt: \"hi\" }\n  b:\n    with:\n      prev: ${{ tasks.a.output }}\n    infer: { prompt: \"use ${{ with.prev }}\" }\n";
        let wf = parse(yaml);
        let sub = scope_to_task(wf, "a").expect("a scopes");
        let report = nika_check::check(&sub);
        assert!(
            report.is_clean(),
            "the cone stands alone (no dangling refs)"
        );
        assert_eq!(sub.tasks.len(), 1);
    }

    // ── the budget floor (descended from the run verb 2026-07-22) ────

    #[test]
    fn floor_above_budget_refuses_with_both_numbers() {
        let msg = floor_refusal(0.000_019, 0.000_001).expect("refuses");
        assert!(msg.contains("$0.000019"), "floor must ride");
        assert!(msg.contains("$0.000001"), "budget must ride");
        assert!(msg.contains("refusing to start"));
        assert!(msg.contains("nika check"), "must point at the envelope");
    }

    #[test]
    fn floor_at_or_under_budget_passes() {
        // Spending exactly the budget is not over it (mirrors the
        // ledger's crossing semantics).
        assert!(floor_refusal(0.05, 0.05).is_none());
        assert!(floor_refusal(0.0, 0.05).is_none());
        assert!(floor_refusal(0.0, 0.0).is_none());
    }

    /// The admission form (NIKA-1709 · the 2026-07-29 composition bypass
    /// closed): a run launched under a budget its floor already crosses is
    /// aborted by the LAUNCH GATES — before the prologue, for EVERY
    /// embedder (the composed child included). `None` budget never fires,
    /// and a floor at/under the budget admits.
    #[test]
    fn budget_floor_refusal_is_the_embedders_gate() {
        let wf = parse(
            "nika: m\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000, model: \"anthropic/claude-sonnet-5\" }\n",
        );
        let report = nika_check::check(&wf);
        assert!(
            report.cost.min_path_total_usd > 0.000_001,
            "the fixture's floor dwarfs the budget"
        );
        let err = budget_floor_refusal(&wf, &report, Some(0.000_001), None)
            .expect("a floor above the budget refuses at admission");
        assert!(
            matches!(err, RuntimeError::BudgetFloor { .. }),
            "must use the launch-abort variant"
        );
        assert!(err.to_string().starts_with("NIKA-1709"));
        assert!(err.to_string().contains("refusing to start"));
        // The sparing arms: no budget · a floor that fits.
        assert!(budget_floor_refusal(&wf, &report, None, None).is_none());
        assert!(budget_floor_refusal(&wf, &report, Some(999.0), None).is_none());
    }

    /// The gate prices the EFFECTIVE model (#342's law at the admission
    /// layer): a file on mock overridden to a priced model refuses; a
    /// priced file overridden to mock passes.
    #[test]
    fn the_gate_prices_the_model_the_run_will_use() {
        let yaml = "nika: m\nmodel: \"mock/echo\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000 }\n";
        let wf = parse(yaml);
        let report = nika_check::check(&wf);
        assert_eq!(
            report.cost.min_path_total_usd, 0.0,
            "the file's floor is zero"
        );
        let err = budget_floor_refusal(
            &wf,
            &report,
            Some(0.000_001),
            Some("anthropic/claude-sonnet-5"),
        );
        assert!(
            matches!(err, Some(RuntimeError::BudgetFloor { .. })),
            "the overridden effective model's floor must trip the gate"
        );

        let yaml = "nika: m\nmodel: \"anthropic/claude-sonnet-5\"\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000 }\n";
        let wf = parse(yaml);
        let report = nika_check::check(&wf);
        assert!(
            budget_floor_refusal(&wf, &report, Some(0.000_001), Some("mock/echo")).is_none(),
            "the effective mock floor is zero — the file's priced floor never fires"
        );
    }

    /// B24 / issue 1296: check's envelope skips `invoke:`, so a priced
    /// `nika:image_generate` (xAI workhorse $0.02) used to launch, hit
    /// HTTP, then abort NIKA-1704. The admission floor must refuse
    /// BEFORE the executor is called (NIKA-1709 · zero events · zero spend).
    #[test]
    fn priced_image_builtin_floor_exceeds_a_tiny_cap() {
        let wf = parse(&image_generate_wf("xai"));
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "the fixture must check clean");
        assert_eq!(
            report.cost.min_path_total_usd, 0.0,
            "check still skips invoke — the hole this gate closes"
        );
        let err = budget_floor_refusal(&wf, &report, Some(0.001), None)
            .expect("a $0.02 builtin floor refuses a $0.001 cap");
        assert_eq!(err.spec_code(), "NIKA-1709");
        let msg = err.to_string();
        assert!(msg.contains("refusing to start"));
        assert!(msg.contains("$0.020000"), "catalog floor must ride");
        assert!(msg.contains("$0.001000"), "cap must ride");
        assert!(
            !msg.contains("spent $"),
            "preflight must not report post-spend state"
        );
        assert!(budget_floor_refusal(&wf, &report, Some(1.00), None).is_none());
        assert!(budget_floor_refusal(&wf, &report, None, None).is_none());
    }

    /// B20 / issue 1297: an unpriced CLOUD seat under `--max-cost-usd`
    /// must refuse before the prologue. The canary is a gemini id the
    /// snapshot does not price — mock/local stay the sparing arms.
    fn unpriced_cloud_wf() -> String {
        "nika: b20\nmodel: gemini/nika-b20-unpriced-canary\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16 }\n"
            .to_owned()
    }

    #[test]
    fn unpriced_cloud_plus_cap_refuses_to_start() {
        let wf = parse(&unpriced_cloud_wf());
        let report = nika_check::check(&wf);
        assert!(
            report.is_clean(),
            "the canary must check clean so admission owns the refusal"
        );
        let ep = report
            .data_journey
            .model_endpoints
            .iter()
            .find(|e| e.task == "ping")
            .expect("canary endpoint");
        assert!(!ep.priced, "canary must stay unpriced");
        assert_eq!(ep.locus, nika_check::EndpointLocus::Cloud);
        let err = budget_floor_refusal(&wf, &report, Some(0.01), None)
            .expect("unpriced cloud + cap 0.01 must refuse");
        assert_eq!(err.spec_code(), "NIKA-1709");
        let msg = err.to_string();
        assert!(msg.contains("refusing to start"));
        assert!(msg.contains("unpriced"));
        assert!(msg.contains("nika-b20-unpriced-canary"));
        assert!(msg.contains("0.010000"), "cap must ride");
        assert!(
            budget_floor_refusal(&wf, &report, None, None).is_none(),
            "no cap → unpriced cloud may still start"
        );
    }

    #[test]
    fn mock_plus_cap_still_admits() {
        let wf = parse(
            "nika: b20-mock\nmodel: mock/echo\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16 }\n",
        );
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "mock rehearsal must check clean");
        assert!(
            budget_floor_refusal(&wf, &report, Some(0.01), None).is_none(),
            "mock is a proven zero — a cap must not refuse the rehearsal"
        );
        assert!(budget_floor_refusal(&wf, &report, None, None).is_none());
    }

    #[test]
    fn local_unpriced_without_cap_still_admits() {
        let wf = parse(
            "nika: b20-local\nmodel: ollama/llama3\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 16 }\n",
        );
        let report = nika_check::check(&wf);
        let ep = report
            .data_journey
            .model_endpoints
            .iter()
            .find(|e| e.task == "ping")
            .expect("local endpoint");
        assert!(!ep.priced, "local must stay unpriced-not-free");
        assert_eq!(ep.locus, nika_check::EndpointLocus::Local);
        assert!(
            budget_floor_refusal(&wf, &report, None, None).is_none(),
            "local unpriced without a cap still runs"
        );
        assert!(
            budget_floor_refusal(&wf, &report, Some(0.01), None).is_none(),
            "a cap on local watts is not a cloud-price bound"
        );
    }

    #[test]
    fn priced_gemini_flash_under_a_generous_cap_admits() {
        let wf = parse(
            "nika: b20-flash\nmodel: gemini/gemini-2.5-flash\npermits: {}\ntasks:\n  ping:\n    infer: { prompt: \"PONG\", max_tokens: 256 }\n",
        );
        let report = nika_check::check(&wf);
        let ep = report
            .data_journey
            .model_endpoints
            .iter()
            .find(|e| e.task == "ping")
            .expect("flash endpoint");
        assert!(ep.priced, "flash must be the snapshot row");
        assert!(
            budget_floor_refusal(&wf, &report, Some(0.20), None).is_none(),
            "priced flash under $0.20 must start"
        );
    }

    #[tokio::test]
    async fn unpriced_cloud_under_cap_refuses_before_any_infer() {
        let wf = parse(&unpriced_cloud_wf());
        let runtime = runtime_with(MockShell::new()).with_max_cost_usd(Some(0.01));
        let err = run_refused(&runtime, &wf).await;
        assert_eq!(err.spec_code(), "NIKA-1709");
        let msg = err.to_string();
        assert!(msg.contains("refusing to start"));
        assert!(msg.contains("unpriced"));
    }

    #[test]
    fn mock_image_builtin_has_no_static_floor() {
        let wf = parse(&image_generate_wf("mock"));
        let report = nika_check::check(&wf);
        assert!(report.is_clean(), "mock image fixture must check clean");
        assert!(
            budget_floor_refusal(&wf, &report, Some(0.001), None).is_none(),
            "mock/local image is unpriced — a tight cap must not refuse the rehearsal"
        );
    }

    /// Mutation pair with `priced_image_builtin_floor_exceeds_a_tiny_cap`:
    /// cap 0.001 refuses before any tool call; cap 1.00 lets the stub through.
    #[tokio::test]
    async fn priced_image_builtin_under_tiny_cap_refuses_before_the_executor() {
        let wf = parse(&image_generate_wf("xai"));
        let executor = MockToolExecutor::new();
        let (runtime, probe) = runtime_with_tools(MockShell::new(), executor);
        let runtime = runtime.with_max_cost_usd(Some(0.001));
        let err = run_refused(&runtime, &wf).await;
        assert_eq!(err.spec_code(), "NIKA-1709");
        assert!(
            probe.captured_calls().is_empty(),
            "no executor call may be recorded"
        );
        let msg = err.to_string();
        assert!(!msg.contains("cost_usd"));
        assert!(!msg.contains("spent $0.02"));
    }

    #[tokio::test]
    async fn priced_image_builtin_under_generous_cap_reaches_the_stub() {
        let wf = parse(&image_generate_wf("xai"));
        let executor =
            MockToolExecutor::new().enqueue_ok(ToolResult::success("og", r#"{"ok":true}"#));
        let (runtime, probe) = runtime_with_tools(MockShell::new(), executor);
        let runtime = runtime.with_max_cost_usd(Some(1.00));
        let outcome = run(&runtime, &wf).await;
        assert!(outcome.ok, "cap 1.00 admits a $0.02 floor");
        assert_eq!(probe.captured_calls().len(), 1, "the stub must run once");
        assert_eq!(probe.captured_calls()[0].name, "nika:image_generate");
    }

    /// The gates' order: trust → required inputs → budget floor (a run
    /// that would fail the input gate never reaches the budget verdict).
    #[test]
    fn gates_fire_the_budget_floor_after_the_input_gate() {
        let wf = parse(
            "nika: m\ninputs:\n  needed: { type: string, required: true }\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 1000000, model: \"anthropic/claude-sonnet-5\" }\n",
        );
        let report = nika_check::check(&wf);
        let err = gates(
            &wf,
            &report,
            &BTreeMap::new(),
            Some(0.000_001),
            None,
            (None, &[], None),
        )
        .expect_err("both gates could fire — the input gate speaks first");
        assert!(
            matches!(err, RuntimeError::MissingRequiredInputs { .. }),
            "the input gate must precede"
        );
        // With the input satisfied, the budget floor is the word.
        let mut overrides = BTreeMap::new();
        overrides.insert("needed".to_owned(), Value::String("x".to_owned()));
        let err = gates(
            &wf,
            &report,
            &overrides,
            Some(0.000_001),
            None,
            (None, &[], None),
        )
        .expect_err("the budget gate fires once inputs are satisfied");
        assert!(
            matches!(err, RuntimeError::BudgetFloor { .. }),
            "the budget gate must then own the refusal"
        );
    }

    /// A hermetic probe row — hand-built, zero env reads (the gate must
    /// be judgeable without ambient state · instrument law).
    fn access_probe(
        id: &str,
        requires_key: bool,
        key_present: bool,
        access: nika_types::access::AccessClass,
    ) -> ProviderProbe {
        use nika_providers::probe::{ExecutionLocus, ProviderReadiness};
        ProviderProbe::new(
            id,
            requires_key,
            key_present,
            format!("{}_API_KEY", id.to_uppercase()),
            false,
            ProviderReadiness::new(
                true,
                !requires_key || key_present,
                None,
                None,
                true,
                ExecutionLocus::classify(None, "https://api.example.com"),
                access,
            ),
            "https://api.example.com",
        )
    }

    fn mistral_wf() -> (RawWorkflow, CheckReport) {
        let wf = parse(
            "nika: t\ntasks:\n  s:\n    infer: { prompt: \"x\", model: \"mistral/mistral-small-latest\" }\n",
        );
        let report = nika_check::check(&wf);
        (wf, report)
    }

    #[test]
    fn no_pin_never_gates_access() {
        let (wf, report) = mistral_wf();
        // Even a machine with ZERO configured paths admits without a
        // pin — P2 changes nothing for single-access installs.
        let (pin, probes) = (
            None,
            &[access_probe(
                "mistral",
                true,
                false,
                nika_types::access::AccessClass::Api,
            )],
        );
        assert!(access_pin_refusal(&wf, &report, probes, pin, None).is_none());
    }

    #[test]
    fn an_unknown_access_token_teaches_before_resolution() {
        let (wf, report) = mistral_wf();
        let (pin, probes) = (Some("locale"), &[]);
        let err = access_pin_refusal(&wf, &report, probes, pin, None).expect("typo refused");
        assert!(matches!(err, RuntimeError::AccessUnknownToken { .. }));
        assert!(err.to_string().contains("locale"));
        assert!(err.to_string().contains("NIKA-1802"));
    }

    #[test]
    fn a_class_pin_no_candidate_matches_refuses_1801() {
        let (wf, report) = mistral_wf();
        let (pin, probes) = (
            Some("local"),
            &[access_probe(
                "mistral",
                true,
                true,
                nika_types::access::AccessClass::Api,
            )],
        );
        let err = access_pin_refusal(&wf, &report, probes, pin, None).expect("pin unsatisfied");
        assert!(matches!(err, RuntimeError::AccessPinUnsatisfied { .. }));
        assert!(err.to_string().contains("never a substitute"));
    }

    #[test]
    fn a_known_cli_token_without_the_binary_is_1803_not_1802() {
        let (wf, report) = mistral_wf();
        let err = access_pin_refusal(&wf, &report, &[], Some("claude-code"), None)
            .expect("known token refused");
        assert!(matches!(err, RuntimeError::AccessUnavailable { .. }));
        assert!(err.to_string().contains("NIKA-1803"));
        assert!(!err.to_string().contains("NIKA-1802"));
    }

    #[cfg(feature = "access-harness")]
    #[test]
    fn a_ready_claude_agent_seat_refuses_infer_with_the_attestation_witness() {
        let wf = parse(
            "nika: t\nmodel: anthropic/claude-sonnet-4-6\ntasks:\n  s:\n    infer: { prompt: \"x\" }\n",
        );
        let report = nika_check::check(&wf);
        let probe = access_probe(
            "claude-code",
            false,
            true,
            nika_types::access::AccessClass::Harness,
        )
        .with_serves(vec!["anthropic".to_owned()]);
        let err = access_pin_refusal(&wf, &report, &[probe], Some("claude-code"), None)
            .expect("ACP alone is not an infer-grade proof");
        assert!(matches!(err, RuntimeError::AccessNoPath { .. }));
        let witness = err.to_string();
        for term in [
            "claude-code",
            "single_turn",
            "no_implicit_tools",
            "structured_output",
            "model_identity",
        ] {
            assert!(witness.contains(term), "missing witness term: {term}");
        }
    }

    #[cfg(feature = "access-harness")]
    #[test]
    fn codex_infer_grade_pin_admits_an_infer_only_workflow() {
        let (wf, report) = mistral_wf();
        let probe = access_probe(
            "codex",
            false,
            true,
            nika_types::access::AccessClass::Harness,
        )
        .with_serves(vec!["mistral".to_owned()]);
        assert!(access_pin_refusal(&wf, &report, &[probe], Some("codex"), None).is_none());
    }

    #[test]
    fn access_harness_with_only_api_probes_is_1803_not_api_keys() {
        let (wf, report) = mistral_wf();
        let (pin, probes) = (
            Some("harness"),
            &[access_probe(
                "mistral",
                true,
                true,
                nika_types::access::AccessClass::Api,
            )],
        );
        let err = access_pin_refusal(&wf, &report, probes, pin, None).expect("no runtime");
        assert!(matches!(err, RuntimeError::AccessUnavailable { .. }));
        assert!(err.to_string().contains("NIKA-1803"));
        assert!(!err.to_string().contains("API_KEY"));
    }

    #[test]
    fn a_pinned_path_failing_admission_refuses_1800_with_the_fix_var() {
        let (wf, report) = mistral_wf();
        let (pin, probes) = (
            Some("api"),
            &[access_probe(
                "mistral",
                true,
                false,
                nika_types::access::AccessClass::Api,
            )],
        );
        let err = access_pin_refusal(&wf, &report, probes, pin, None).expect("path inadmissible");
        assert!(matches!(err, RuntimeError::AccessNoPath { .. }));
        assert!(err.to_string().contains("MISTRAL_API_KEY unset"));
    }

    #[test]
    fn a_satisfied_pin_admits() {
        let wf = parse(
            "nika: t\ntasks:\n  s:\n    infer: { prompt: \"x\", model: \"ollama/llama3.2\" }\n",
        );
        let report = nika_check::check(&wf);
        let (pin, probes) = (
            Some("local"),
            &[access_probe(
                "ollama",
                false,
                false,
                nika_types::access::AccessClass::Local,
            )],
        );
        assert!(access_pin_refusal(&wf, &report, probes, pin, None).is_none());
    }

    #[test]
    fn a_mock_pin_keeps_the_rehearsal_runnable() {
        let wf =
            parse("nika: t\ntasks:\n  s:\n    infer: { prompt: \"x\", model: \"mock/echo\" }\n");
        let report = nika_check::check(&wf);
        // Probes exclude the mock backend by design — the gate must
        // synthesize its keyless candidate, or the rehearsal dies.
        let (pin, probes) = (Some("mock"), &[]);
        assert!(access_pin_refusal(&wf, &report, probes, pin, None).is_none());
    }

    #[cfg(feature = "access-harness")]
    #[test]
    fn a_mock_model_refuses_a_harness_pin_before_any_live_seat() {
        let wf =
            parse("nika: t\ntasks:\n  s:\n    infer: { prompt: \"x\", model: \"mock/echo\" }\n");
        let report = nika_check::check(&wf);
        let probe = access_probe(
            "codex",
            false,
            true,
            nika_types::access::AccessClass::Harness,
        );
        let err = access_pin_refusal(&wf, &report, &[probe], Some("harness"), None)
            .expect("mock must never substitute a live harness");
        assert!(matches!(err, RuntimeError::AccessPinUnsatisfied { .. }));
        let witness = err.to_string();
        assert!(witness.contains("mock/echo"));
        assert!(witness.contains("--access mock"));
        assert!(witness.contains("never a live substitute"));
    }

    #[test]
    fn the_model_override_is_what_the_pin_judges() {
        // The ENVELOPE model is mistral (api) — the override sends the
        // run to ollama (local): a `local` pin must judge the OVERRIDE
        // (#342's law), so it admits. A per-task `model:` would keep
        // winning over the override — hence the envelope-form fixture.
        let wf = parse(
            "nika: t\nmodel: mistral/mistral-small-latest\ntasks:\n  s:\n    infer: { prompt: \"x\" }\n",
        );
        let report = nika_check::check(&wf);
        let (pin, probes) = (
            Some("local"),
            &[
                access_probe("mistral", true, true, nika_types::access::AccessClass::Api),
                access_probe(
                    "ollama",
                    false,
                    false,
                    nika_types::access::AccessClass::Local,
                ),
            ],
        );
        assert!(
            access_pin_refusal(&wf, &report, probes, pin, Some("ollama/llama3.2")).is_none(),
            "the override's provider satisfies the pin"
        );
        // And WITHOUT the override the same pin refuses (mistral is api).
        assert!(access_pin_refusal(&wf, &report, probes, pin, None).is_some());
    }

    fn breakdown_of(yaml: &str) -> String {
        let wf = parse(yaml);
        unbounded_breakdown(&nika_check::check(&wf).cost)
    }

    #[test]
    fn breakdown_names_each_reason_not_the_fixed_disjunction() {
        // A priced-but-unbounded task must read « no max_tokens », not
        // « unpriced model » — the operator sees which is FIXABLE. The
        // unpriced specimen is a sovereign local (unpriced-never-free);
        // mock stopped qualifying when it became a proven zero (A-02).
        let msg = breakdown_of(
            "nika: m\ntasks:\n  \
             a:\n    infer: { prompt: hi, model: \"anthropic/claude-sonnet-5\" }\n  \
             b:\n    infer: { prompt: hi, max_tokens: 100, model: \"ollama/llama3\" }\n",
        );
        assert!(msg.contains("2 task(s)"));
        assert!(
            msg.contains("1 with no `max_tokens`"),
            "the priced-unbounded task must be classified"
        );
        assert!(
            msg.contains("1 on an unpriced model"),
            "the local task must be classified"
        );
    }

    #[test]
    fn breakdown_counts_only_the_unbounded_tasks() {
        // Bounded tasks are never in the tally — and the mock plane is
        // bounded BY CONSTRUCTION now (a proven zero · A-02), so it
        // rides along as a second exclusion specimen. id b carries
        // max_tokens so its reason is NoPrice (sovereign local), not
        // NoTokenLimit — proving the unpriced bucket AND both
        // exclusions in one shot.
        let msg = breakdown_of(
            "nika: m\ntasks:\n  \
             a:\n    infer: { prompt: hi, max_tokens: 100, model: \"anthropic/claude-sonnet-5\" }\n  \
             b:\n    infer: { prompt: hi, max_tokens: 100, model: \"ollama/llama3\" }\n  \
             c:\n    infer: { prompt: hi, max_tokens: 100, model: \"mock/echo\" }\n",
        );
        assert!(
            msg.contains("1 task(s)"),
            "only the unpriced local task must count"
        );
        assert!(msg.contains("unpriced model"));
    }
}
