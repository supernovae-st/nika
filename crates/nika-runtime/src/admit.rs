// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Run-admission gates. Every refusal precedes the prologue, so it emits no
//! event and spends no task. Constructors serve runtime and CLI preflights.

use std::collections::BTreeMap;

use nika_check::CheckReport;
use nika_providers::probe::ProviderProbe;
use nika_providers::resolve_access::PinRefusal;
use nika_schema::raw::RawWorkflow;
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
    access_pin: Option<&str>,
    probes: &[ProviderProbe],
) -> Result<(), RuntimeError> {
    crate::trust::check_report(wf, report)?;
    if let Some(err) = required_inputs_refusal(wf, overrides) {
        return Err(err);
    }
    if let Some(err) = budget_floor_refusal(wf, report, budget, model_override) {
        return Err(err);
    }
    if let Some(err) = access_pin_refusal(wf, report, probes, access_pin, model_override) {
        return Err(err);
    }
    Ok(())
}

/// The budget-floor admission gate — `Some` run-abort error (NIKA-1709)
/// when the workflow's unavoidable cost floor already exceeds the budget
/// the run was launched under. The ONE constructor both admission
/// surfaces speak: the CLI's standalone preflight prints this same
/// [`floor_refusal`] text and never reaches `run`, so the gate here is
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
#[must_use]
pub fn budget_floor_refusal(
    wf: &RawWorkflow,
    report: &CheckReport,
    budget: Option<f64>,
    model_override: Option<&str>,
) -> Option<RuntimeError> {
    let floor = match model_override {
        Some(m) => {
            nika_check::check(&nika_check::with_model_override(wf, m))
                .cost
                .min_path_total_usd
        }
        None => report.cost.min_path_total_usd,
    };
    let message = floor_refusal(floor, budget?)?;
    Some(RuntimeError::BudgetFloor { message })
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
        other => RuntimeError::AccessNoPath {
            message: format!("{other:?}"),
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
        let executor = MockToolExecutor::new();
        let provider = MockProvider::new("mock");
        let invoke = Arc::new(InvokeVerb::new(Arc::new(executor)));
        Runtime::new(
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
        )
    }

    fn parse(yaml: &str) -> RawWorkflow {
        nika_schema::parse(
            yaml,
            nika_schema::FileId::new(0),
            nika_schema::ParseMode::Strict,
        )
        .expect("fixture parses")
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
            .expect("run settles")
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
            .expect_err("an unsatisfied required input never reaches dispatch");
        assert!(
            sink.events().is_empty(),
            "refused BEFORE any event — not even the prologue: {:?}",
            sink.events()
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
            panic!("expected MissingRequiredInputs, got {err:?}");
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
        assert!(msg.contains("`needle`"), "the input is named: {msg}");
        assert!(msg.contains("--var needle=<value>"), "the fix rides: {msg}");
        assert!(
            probe.executed_commands().is_empty(),
            "not one task spent: {:?}",
            probe.executed_commands()
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

        let err = scope_to_task(wf, "nope").expect_err("unknown id refused");
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
        assert!(msg.contains("$0.000019"), "floor rides: {msg}");
        assert!(msg.contains("$0.000001"), "budget rides: {msg}");
        assert!(msg.contains("refusing to start"), "{msg}");
        assert!(msg.contains("nika check"), "points at the envelope: {msg}");
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
            "the launch-abort variant: {err:?}"
        );
        assert!(err.to_string().starts_with("NIKA-1709"), "{err}");
        assert!(err.to_string().contains("refusing to start"), "{err}");
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
            "the overridden (effective) model's floor trips the gate: {err:?}"
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
            None,
            &[],
        )
        .expect_err("both gates could fire — the input gate speaks first");
        assert!(
            matches!(err, RuntimeError::MissingRequiredInputs { .. }),
            "the input gate precedes: {err:?}"
        );
        // With the input satisfied, the budget floor is the word.
        let mut overrides = BTreeMap::new();
        overrides.insert("needed".to_owned(), Value::String("x".to_owned()));
        let err = gates(&wf, &report, &overrides, Some(0.000_001), None, None, &[])
            .expect_err("the budget gate fires once inputs are satisfied");
        assert!(
            matches!(err, RuntimeError::BudgetFloor { .. }),
            "the budget gate then owns the refusal: {err:?}"
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
        assert!(
            matches!(err, RuntimeError::AccessUnknownToken { .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("locale"), "{err}");
        assert!(err.to_string().contains("NIKA-1802"), "{err}");
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
        assert!(
            matches!(err, RuntimeError::AccessPinUnsatisfied { .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("never a substitute"), "{err}");
    }

    #[test]
    fn a_known_cli_token_without_the_binary_is_1803_not_1802() {
        let (wf, report) = mistral_wf();
        let err = access_pin_refusal(&wf, &report, &[], Some("claude-code"), None)
            .expect("known token refused");
        assert!(
            matches!(err, RuntimeError::AccessUnavailable { .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("NIKA-1803"), "{err}");
        assert!(!err.to_string().contains("NIKA-1802"), "{err}");
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
        assert!(matches!(err, RuntimeError::AccessNoPath { .. }), "{err:?}");
        let witness = err.to_string();
        for term in [
            "claude-code",
            "single_turn",
            "no_implicit_tools",
            "structured_output",
            "model_identity",
        ] {
            assert!(witness.contains(term), "missing {term}: {witness}");
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
        assert!(
            matches!(err, RuntimeError::AccessUnavailable { .. }),
            "{err:?}"
        );
        assert!(err.to_string().contains("NIKA-1803"), "{err}");
        assert!(!err.to_string().contains("API_KEY"), "{err}");
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
        assert!(matches!(err, RuntimeError::AccessNoPath { .. }), "{err:?}");
        assert!(err.to_string().contains("MISTRAL_API_KEY unset"), "{err}");
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
        assert!(msg.contains("2 task(s)"), "{msg}");
        assert!(
            msg.contains("1 with no `max_tokens`"),
            "the priced-unbounded task: {msg}"
        );
        assert!(
            msg.contains("1 on an unpriced model"),
            "the local task: {msg}"
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
            "only the unpriced local task: {msg}"
        );
        assert!(msg.contains("unpriced model"), "{msg}");
    }
}
