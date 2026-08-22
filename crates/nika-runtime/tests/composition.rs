// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic, clippy::doc_markdown)]

//! Composition at the dispatch seam (spec `14-composition.md`) — the
//! laws the RUNTIME itself judges for `invoke: workflow:`, proven
//! hermetically over a mock [`ChildRunner`] (no filesystem · no child
//! process · the seam contract in isolation):
//!
//! - the depth backstop (`NIKA-SEC-003` · fail-closed BEFORE the runner)
//! - the runner-less refusal (loud `NIKA-COMP-001` · never a no-op)
//! - args render over the scope; outputs become the task value (law 2)
//! - the parent ledger debits the child's spend (laws 5/6)
//! - the remaining budget rides the call (law 6)
//! - the parent's `returns:` fits the child value (law 2 · run half)
//! - a child failure surfaces the CHILD's code (one voice)
//! - a composition refusal is never retried (structural · not transient)
//! - the terminal frame records the child row (law 8) — and because the
//!   frame is itself hash-chained by the trace sink, the parent's
//!   receipt commits to the child's head (law 9 · the file-level twin
//!   is the CLI E2E's).
//!
//! The REAL child execution (files · nested runtimes · trace files) is
//! `nika-cli/tests/composition_e2e.rs`'s — this battery pins the seam.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};

use nika_check::CheckReport;
use nika_event::{Event, EventKind};
use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::child::{
    ChildCall, ChildOutcome, ChildRunRefusal, ChildRunSummary, ChildRunner, MAX_RUN_DEPTH,
};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_schema::raw::RawWorkflow;
use nika_schema::source::FileId;
use nika_schema::{ParseMode, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};

// ─── harness ─────────────────────────────────────────────────────────────

type Respond =
    Box<dyn Fn(&ChildCall) -> Result<ChildOutcome, ChildRunRefusal> + Send + Sync + 'static>;

/// The hermetic seam double — records every call, answers via a closure.
struct MockChildRunner {
    calls: Arc<Mutex<Vec<ChildCall>>>,
    respond: Respond,
}

impl MockChildRunner {
    fn new(respond: Respond) -> (Arc<Self>, Arc<Mutex<Vec<ChildCall>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        (
            Arc::new(Self {
                calls: Arc::clone(&calls),
                respond,
            }),
            calls,
        )
    }
}

impl ChildRunner for MockChildRunner {
    fn run_child<'a>(
        &'a self,
        call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        Box::pin(async move {
            let out = (self.respond)(&call);
            self.calls.lock().expect("mock lock").push(call);
            out
        })
    }
}

/// A green child: `{report: "done"}` outputs · optional cost · a trace row.
fn green_child(cost_usd: Option<f64>) -> Respond {
    Box::new(move |call| {
        Ok(ChildOutcome::new(
            true,
            BTreeMap::from([("report".to_owned(), json!("done"))]),
            cost_usd,
            Some(ChildRunSummary::new(
                call.target.clone(),
                true,
                (
                    Some("child-trace-1".to_owned()),
                    Some("childhead123".to_owned()),
                    Some("childdef456".to_owned()),
                ),
            )),
            None,
            None,
        ))
    })
}

fn parse_and_check(yaml: &str) -> (RawWorkflow, CheckReport) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    // PURE check only — the resolved lane is the CLI's; these fixtures
    // exercise the runtime seam, whose gate is the clean report.
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:#?}");
    (wf, report)
}

#[allow(clippy::type_complexity)]
fn runtime(
    config: RuntimeConfig,
) -> Runtime<
    MockShell,
    nika_kernel_mock::MockToolExecutor,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
> {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(
        nika_kernel_mock::MockToolExecutor::new(),
    )));
    Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        MockClock::new(),
        config,
    )
}

async fn drive(
    yaml: &str,
    rt: Runtime<
        MockShell,
        nika_kernel_mock::MockToolExecutor,
        nika_providers::NoHttp,
        MockProvider,
        MockToolDefinitionProvider,
        MockClock,
    >,
) -> (RunOutcome, Vec<Event>) {
    let (wf, report) = parse_and_check(yaml);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = rt
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("run completes");
    (outcome, sink.into_events())
}

fn str_field<'a>(event: &'a Event, key: &str) -> Option<&'a str> {
    event.fields.iter().find(|f| f.key == key).and_then(|f| {
        if let nika_types::resource::Value::String(s) = &f.value {
            Some(s.as_str())
        } else {
            None
        }
    })
}

const CALL: &str = "\
nika: parent
const:
  target_url: \"https://example.com\"
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
      args: { url: \"${{ const.target_url }}\", depth: 2 }
";

// ─── the battery ─────────────────────────────────────────────────────────

/// A runtime WITHOUT a composed child surface refuses LOUDLY (the
/// NoWorkflow precedent: never a silent no-op) — and the refusal is the
/// composition code, not a generic crash.
#[tokio::test]
async fn runner_less_workflow_call_refuses_loudly() {
    let (outcome, _) = drive(CALL, runtime(RuntimeConfig::default())).await;
    assert!(!outcome.ok);
    let rec = &outcome.records["audit"];
    assert_eq!(rec.status, TaskStatus::Failure);
    let err = rec.error.as_ref().expect("failure carries the error");
    assert_eq!(err.code, "NIKA-COMP-001");
    assert!(
        err.message.contains("no child-workflow surface"),
        "{}",
        err.message
    );
}

/// The budget-floor launch gate (NIKA-1709 · the 2026-07-29 composition
/// bypass closed at admission): a runtime launched under a budget its
/// floor already crosses aborts BEFORE the prologue — `BudgetFloor`, and
/// the sink saw ZERO events (zero spend). This is the gate a composed
/// child now passes through with the parent's remaining (law 6) — the
/// CLI preflight never reaches it, so the seam battery pins it.
#[tokio::test]
async fn a_floor_above_budget_aborts_at_admission_with_zero_events() {
    let (wf, report) = parse_and_check(
        "nika: m\ntasks:\n  \
         a:\n    infer: { prompt: hi, max_tokens: 1000000, model: \"anthropic/claude-sonnet-5\" }\n",
    );
    assert!(
        report.cost.min_path_total_usd > 0.000_001,
        "the fixture's floor dwarfs the budget"
    );
    let rt = runtime(RuntimeConfig::new(None, 0).with_max_cost_usd(Some(0.000_001)));
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let err = rt
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect_err("a floor above the budget refuses at admission");
    assert!(
        matches!(err, nika_runtime::RuntimeError::BudgetFloor { .. }),
        "{err:?}"
    );
    assert!(err.to_string().starts_with("NIKA-1709"), "{err}");
    assert!(
        sink.into_events().is_empty(),
        "a refusal at admission emits zero events — zero spend"
    );
}

/// The depth backstop (`NIKA-SEC-003`) refuses FAIL-CLOSED — before the
/// runner is even consulted (zero I/O past the bound).
#[tokio::test]
async fn depth_gate_refuses_before_the_runner() {
    let (runner, calls) = MockChildRunner::new(green_child(None));
    let rt = runtime(RuntimeConfig::default())
        .with_child_runner(runner)
        .with_run_depth(MAX_RUN_DEPTH); // the next call would be MAX+1
    let (outcome, _) = drive(CALL, rt).await;
    assert!(!outcome.ok);
    let err = outcome.records["audit"].error.as_ref().expect("error");
    assert_eq!(err.code, "NIKA-SEC-003");
    assert!(
        err.message.contains("run-recursion bound"),
        "{}",
        err.message
    );
    assert!(
        calls.lock().expect("lock").is_empty(),
        "the gate fires BEFORE the runner (fail-closed)"
    );
}

/// Law 2 at the seam: args render over the parent scope before the call;
/// the child's outputs object IS the task value. Law 8: the terminal
/// frame carries the child row — chain head included (law 9's frame-
/// level commitment; the file-level chain is the CLI E2E's).
#[tokio::test]
async fn args_render_and_outputs_become_the_task_value() {
    let (runner, calls) = MockChildRunner::new(green_child(None));
    let rt = runtime(RuntimeConfig::default()).with_child_runner(runner);
    let (outcome, events) = drive(CALL, rt).await;
    assert!(outcome.ok, "{:?}", outcome.records["audit"].error);

    // args rendered: ${{ const.target_url }} → the literal
    let seen = calls.lock().expect("lock");
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].target, "./child.nika.yaml");
    assert_eq!(seen[0].depth, 1, "root is 0 · its child is 1");
    assert_eq!(
        seen[0].args,
        BTreeMap::from([
            ("depth".to_owned(), json!(2)),
            ("url".to_owned(), json!("https://example.com")),
        ]),
    );
    assert_eq!(
        seen[0].remaining_budget_usd, None,
        "no budget declared → nothing to inherit"
    );

    // outputs → the task value
    assert_eq!(outcome.records["audit"].output, json!({"report": "done"}));

    // the terminal frame records the forest row
    let completed = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("audit"))
        .expect("task_completed");
    let child_row = str_field(completed, "child").expect("child field rides the frame");
    let row: Value = serde_json::from_str(child_row).expect("child row is JSON");
    assert_eq!(row["target"], "./child.nika.yaml");
    assert_eq!(row["chain_head"], "childhead123");
    assert_eq!(row["def_hash"], "childdef456");
    assert_eq!(row["outcome"], "success");
    assert_eq!(
        str_field(completed, "note"),
        Some("invoke · workflow:./child.nika.yaml")
    );
}

/// Laws 5/6: the child's metered spend debits the PARENT ledger (one
/// count · attributed to the call) and the remaining budget rides the
/// call for the child to inherit.
#[tokio::test]
async fn child_spend_debits_the_parent_ledger_and_remaining_rides() {
    let (runner, calls) = MockChildRunner::new(green_child(Some(0.25)));
    let rt =
        runtime(RuntimeConfig::default().with_max_cost_usd(Some(2.0))).with_child_runner(runner);
    let (outcome, _) = drive(CALL, rt).await;
    assert!(outcome.ok);
    assert_eq!(
        calls.lock().expect("lock")[0].remaining_budget_usd,
        Some(2.0),
        "the child inherits the parent's remaining at call time (law 6)"
    );
    assert!(
        (outcome.total_cost_usd.expect("metered") - 0.25).abs() < 1e-9,
        "the child's spend is the parent's spend (law 5)"
    );
    assert!(!outcome.budget_exceeded);
}

/// Law 6's gate: a child whose spend crosses the parent budget trips
/// the parent ledger — downstream admission stops.
#[tokio::test]
async fn child_spend_crossing_the_budget_trips_the_parent() {
    let (runner, _calls) = MockChildRunner::new(green_child(Some(1.5)));
    let rt =
        runtime(RuntimeConfig::default().with_max_cost_usd(Some(1.0))).with_child_runner(runner);
    let (outcome, _) = drive(CALL, rt).await;
    assert!(outcome.budget_exceeded, "the parent ledger tripped");
}

/// Law 2, run half: the parent's `returns:` judges the child's outputs
/// object with the ONE type core — a misfit is the SAME `NIKA-TYPE-101`
/// every verb speaks.
#[tokio::test]
async fn returns_contract_judges_the_child_value() {
    let yaml = "\
nika: parent
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
    returns: { object: { report: integer } }
";
    let (runner, _) = MockChildRunner::new(green_child(None)); // report: \"done\" — a string
    let rt = runtime(RuntimeConfig::default()).with_child_runner(runner);
    let (outcome, _) = drive(yaml, rt).await;
    assert!(!outcome.ok);
    let err = outcome.records["audit"].error.as_ref().expect("error");
    assert_eq!(err.code, "NIKA-TYPE-101");
}

/// One voice: a failed child surfaces the CHILD's own spec code on the
/// parent task, with the trace pointer in the message.
#[tokio::test]
async fn child_failure_surfaces_the_child_code() {
    let (runner, _) = MockChildRunner::new(Box::new(|call| {
        Ok(ChildOutcome::new(
            false,
            BTreeMap::new(),
            Some(0.1),
            Some(ChildRunSummary::new(
                call.target.clone(),
                false,
                (Some("t-9".to_owned()), Some("headxyz".to_owned()), None),
            )),
            Some(("NIKA-EXEC-002".to_owned(), "exit 3".to_owned())),
            None,
        ))
    }));
    let rt =
        runtime(RuntimeConfig::default().with_max_cost_usd(Some(5.0))).with_child_runner(runner);
    let (outcome, _) = drive(CALL, rt).await;
    assert!(!outcome.ok);
    let err = outcome.records["audit"].error.as_ref().expect("error");
    assert_eq!(
        err.code, "NIKA-EXEC-002",
        "the child's own code — one voice"
    );
    assert!(err.message.contains("./child.nika.yaml"), "{}", err.message);
    assert!(err.message.contains("headxyz"), "{}", err.message);
    assert!(
        !err.transient,
        "a settled child failure is never blind-retried by the parent"
    );
    assert!(
        (outcome.total_cost_usd.expect("metered") - 0.1).abs() < 1e-9,
        "a failed child's spend still debits (billed-then-failed is real money)"
    );
}

/// A composition refusal is structural — the retry policy never replays
/// it (transient: false), so the runner is consulted exactly once.
#[tokio::test]
async fn refusals_are_not_retried() {
    let yaml = "\
nika: parent
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
    retry: { max_attempts: 3 }
";
    let (runner, calls) = MockChildRunner::new(Box::new(|_| {
        Err(ChildRunRefusal {
            code: "NIKA-COMP-001".to_owned(),
            message: "cannot read child".to_owned(),
        })
    }));
    let rt = runtime(RuntimeConfig::default()).with_child_runner(runner);
    let (outcome, _) = drive(yaml, rt).await;
    assert!(!outcome.ok);
    assert_eq!(
        calls.lock().expect("lock").len(),
        1,
        "a structural refusal is never replayed"
    );
}

/// The parent's declared boundary rides the call (laws 3/4 input) —
/// the runner composes `child ∩ parent` from it.
#[tokio::test]
async fn parent_permits_ride_the_call() {
    let yaml = "\
nika: parent
permits:
  net:
    http: [\"api.example.com\"]
  tools: [\"nika:*\"]
tasks:
  audit:
    invoke:
      workflow: \"./child.nika.yaml\"
";
    let (runner, calls) = MockChildRunner::new(green_child(None));
    let rt = runtime(RuntimeConfig::default()).with_child_runner(runner);
    let (outcome, _) = drive(yaml, rt).await;
    assert!(outcome.ok, "{:?}", outcome.records["audit"].error);
    let seen = calls.lock().expect("lock");
    let permits = seen[0].parent_permits.as_ref().expect("boundary rides");
    assert_eq!(
        permits.net.as_ref().expect("net").http,
        vec!["api.example.com".to_owned()]
    );
}
