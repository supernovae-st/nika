// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! Nested runs on ONE ordinary native stack (spec `14-composition.md`).
//!
//! The composer polls each child's `Runtime::run` inside its parent's dispatch, on the
//! parent's own thread: the borrowed, non-`Send` [`ChildRunner`] future. These proofs
//! drive REAL nested runtimes (hermetic seams · no filesystem) on a 2 MiB thread, the
//! stack a test thread or an embedder's worker gets, so the depth law stays the ONLY
//! nesting limit:
//!
//! - eight child edges return the leaf's exact value: one leaf call, one run per level,
//!   the root frame records its child's forest row
//! - the ninth edge is refused (`NIKA-SEC-003`) before the leaf is ever called
//! - a pending leaf eight levels down is destroyed whole by its root's `timeout:` and by
//!   a dropped run: started once, dropped once, never completed, no late effect
//! - an operator Stop during that pending leaf keeps the documented contract: in-flight
//!   work completes and is counted (a child run does not observe its root's Stop), the
//!   root ends cancelled, and nothing happens after the run returns
//!
//! The production composer's own nesting (files · traces · the CLI) is
//! `nika-cli/tests/composition_e2e.rs`'s.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_check::CheckReport;
use nika_event::{Event, EventKind};
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::child::{
    ChildCall, ChildOutcome, ChildRunRefusal, ChildRunSummary, ChildRunner, MAX_RUN_DEPTH,
};
use nika_runtime::{
    DeterministicStamper, EventSink, RunOutcome, Runtime, RuntimeConfig, RuntimeError, TaskStatus,
    VecSink,
};
use nika_schema::raw::RawWorkflow;
use nika_schema::source::FileId;
use nika_schema::{ParseMode, parse};
use nika_types::cancel::CancelCtx;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::{Value, json};

/// The stack `std::thread` gives by default — a test thread, an embedder's worker.
const ORDINARY_STACK: usize = 2 * 1024 * 1024;

// ─── the leaf every level shares ─────────────────────────────────────────

/// What the deepest task does, and what happened to it.
#[derive(Default)]
struct Leaf {
    /// `Some(d)`: the call stays pending `d` (real time) before its effect.
    delay: Option<Duration>,
    /// Flipped on the leaf's first poll: the operator's Stop, mid-flight.
    stop: Option<CancelCtx>,
    started: AtomicUsize,
    effects: AtomicUsize,
    dropped: AtomicUsize,
}

impl Leaf {
    fn counts(&self) -> (usize, usize, usize) {
        (
            self.started.load(Ordering::SeqCst),
            self.effects.load(Ordering::SeqCst),
            self.dropped.load(Ordering::SeqCst),
        )
    }
}

/// Counts a leaf call destroyed before its effect.
struct DropWitness<'a> {
    leaf: &'a Leaf,
    done: bool,
}

impl Drop for DropWitness<'_> {
    fn drop(&mut self) {
        if !self.done {
            self.leaf.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

impl ToolExecuteDyn for Leaf {
    async fn execute(&self, _call: ToolCall) -> Result<ToolResult, ToolExecError> {
        self.started.fetch_add(1, Ordering::SeqCst);
        let mut witness = DropWitness {
            leaf: self,
            done: false,
        };
        if let Some(stop) = &self.stop {
            stop.cancel();
        }
        if let Some(delay) = self.delay {
            tokio::time::sleep(delay).await;
        }
        self.effects.fetch_add(1, Ordering::SeqCst);
        witness.done = true;
        Ok(ToolResult::success("leaf", "bottom").with_structured(json!("bottom")))
    }
}

// ─── real nesting, hermetic seams ────────────────────────────────────────

type Rt = Runtime<
    MockShell,
    Leaf,
    nika_providers::NoHttp,
    MockProvider,
    MockToolDefinitionProvider,
    MockClock,
>;

fn runtime(leaf: &Arc<Leaf>) -> Rt {
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(leaf)));
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
        RuntimeConfig::default(),
    )
}

fn parse_and_check(yaml: &str) -> (RawWorkflow, CheckReport) {
    let wf = parse(yaml, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:#?}");
    (wf, report)
}

/// Every child run's events, recorded AS EMITTED: a destroyed run keeps what it said.
#[derive(Clone, Default)]
struct Journals(Arc<Mutex<Vec<(u32, Event)>>>);

struct JournalSink {
    depth: u32,
    journals: Journals,
}

impl EventSink for JournalSink {
    fn emit(&mut self, event: Event) {
        self.journals.0.lock().unwrap().push((self.depth, event));
    }
}

/// The production nesting with hermetic seams: resolve the target in an in-memory world,
/// check it, compose a runtime one level deeper under the call's budget and inputs, and
/// run it on the caller's poll, exactly where the production composer runs its child.
#[derive(Clone)]
struct NestedRunner {
    world: Arc<BTreeMap<String, String>>,
    leaf: Arc<Leaf>,
    journals: Journals,
}

impl ChildRunner for NestedRunner {
    fn run_child<'a>(
        &'a self,
        call: ChildCall,
    ) -> Pin<Box<dyn Future<Output = Result<ChildOutcome, ChildRunRefusal>> + 'a>> {
        Box::pin(async move {
            let Some(source) = self.world.get(call.target.trim_start_matches("./")) else {
                return Err(ChildRunRefusal {
                    code: "NIKA-COMP-001".to_owned(),
                    message: format!("no child `{}` in this world", call.target),
                });
            };
            let (wf, report) = parse_and_check(source);
            let child = runtime(&self.leaf)
                .with_child_runner(Arc::new(self.clone()))
                .with_run_depth(call.depth)
                .with_var_overrides(call.args.clone())
                .with_max_cost_usd(call.remaining_budget_usd);
            let mut stamper = DeterministicStamper::new();
            let mut sink = JournalSink {
                depth: call.depth,
                journals: self.journals.clone(),
            };
            let outcome = child
                .run(&wf, &report, &mut stamper, &mut sink)
                .await
                .map_err(|error| ChildRunRefusal {
                    code: error.spec_code(),
                    message: error.to_string(),
                })?;
            let failure = (outcome.settlement.error.as_ref())
                .map(|error| (error.code.clone(), error.message.clone()));
            let row = (Some(format!("depth-{}", call.depth)), None, None);
            Ok(ChildOutcome {
                ok: outcome.ok,
                outputs: outcome.outputs.clone(),
                cost_usd: outcome.total_cost_usd,
                trace: Some(ChildRunSummary::new(call.target.clone(), outcome.ok, row)),
                failure,
            })
        })
    }
}

/// `f0 → … → f{edges}`: each level re-exports the value below it; `f{edges}` calls the
/// leaf. Returns the root source and the world of its descendants (`f1.nika` …).
fn chain(edges: u32, root_timeout: bool) -> (String, BTreeMap<String, String>) {
    let level = |i: u32| {
        if i == edges {
            return format!(
                "nika: f{i}\ntasks:\n  leaf:\n    invoke: {{ tool: \"nika:jq\", args: {{ input: \"bottom\", expression: \".\" }} }}\noutputs:\n  value: ${{{{ tasks.leaf.output }}}}\n"
            );
        }
        let timeout = if i == 0 && root_timeout {
            "    timeout: 1s\n"
        } else {
            ""
        };
        format!(
            "nika: f{i}\ntasks:\n  descend:\n    invoke: {{ workflow: \"./f{next}.nika\" }}\n{timeout}outputs:\n  value: ${{{{ tasks.descend.output.value }}}}\n",
            next = i + 1
        )
    };
    let world = (1..=edges)
        .map(|i| (format!("f{i}.nika"), level(i)))
        .collect();
    (level(0), world)
}

/// What one scenario observed, carried out of its thread.
struct Observed {
    root: Option<Result<RunOutcome, RuntimeError>>,
    events: Vec<Event>,
    journals: Vec<(u32, Event)>,
}

/// The root run of `chain(edges, …)` over `leaf`, optionally under an operator `cancel`.
/// `budget`: drop the root run after this much real time (an embedder abandoning it).
async fn drive(
    (source, world): (String, BTreeMap<String, String>),
    leaf: Arc<Leaf>,
    cancel: Option<CancelCtx>,
    budget: Option<Duration>,
) -> Observed {
    let journals = Journals::default();
    let runner = NestedRunner {
        world: Arc::new(world),
        leaf: Arc::clone(&leaf),
        journals: journals.clone(),
    };
    let mut root = runtime(&leaf).with_child_runner(Arc::new(runner));
    if let Some(cancel) = cancel {
        root = root.with_cancel(cancel);
    }
    let (wf, report) = parse_and_check(&source);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let run = root.run(&wf, &report, &mut stamper, &mut sink);
    let root = match budget {
        None => Some(run.await),
        Some(limit) => tokio::time::timeout(limit, run).await.ok(),
    };
    let journals = journals.0.lock().unwrap().clone();
    Observed {
        root,
        events: sink.into_events(),
        journals,
    }
}

/// Run `scenario` on an ordinary 2 MiB thread under a current-thread executor with its
/// time driver: the shape of every `nika run`. A nested run that exhausted the stack
/// would abort the whole process here.
fn on_ordinary_stack<F>(name: &str, scenario: impl FnOnce() -> F + Send + 'static) -> Observed
where
    F: Future<Output = Observed>,
{
    std::thread::Builder::new()
        .name(name.to_owned())
        .stack_size(ORDINARY_STACK)
        .spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_time()
                .build()
                .expect("current-thread executor")
                .block_on(scenario())
        })
        .expect("the scenario thread starts")
        .join()
        .expect("the scenario returns")
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

/// The depths that emitted `kind`, in emission order.
fn depths_of(journals: &[(u32, Event)], kind: EventKind) -> Vec<u32> {
    journals
        .iter()
        .filter(|(_, event)| event.kind == kind)
        .map(|(depth, _)| *depth)
        .collect()
}

// ─── the battery ─────────────────────────────────────────────────────────

/// Eight child edges, each a full nested run polled inside its parent's dispatch: the
/// root returns the leaf's exact value, the leaf runs once, every level runs once and
/// settles green deepest-first, and the root frame records its child's forest row.
#[test]
fn eight_nested_runs_return_the_leaf_value_on_an_ordinary_stack() {
    let leaf = Arc::new(Leaf::default());
    let probe = Arc::clone(&leaf);
    let observed = on_ordinary_stack("nested-eight", move || {
        drive(chain(MAX_RUN_DEPTH, false), probe, None, None)
    });
    let outcome = observed
        .root
        .expect("not abandoned")
        .expect("the root settles");
    assert!(outcome.ok, "{:?}", outcome.records["descend"].error);
    assert_eq!(outcome.outputs.get("value"), Some(&json!("bottom")));
    assert_eq!(
        leaf.counts(),
        (1, 1, 0),
        "one leaf call, one effect, none dropped"
    );
    assert_eq!(
        depths_of(&observed.journals, EventKind::WorkflowCompleted),
        (1..=MAX_RUN_DEPTH).rev().collect::<Vec<_>>(),
        "one green run per level, the deepest settles first"
    );
    let completed = observed
        .events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted && str_field(e, "task") == Some("descend"))
        .expect("the root call completed");
    let row: Value =
        serde_json::from_str(str_field(completed, "child").expect("the forest row rides"))
            .expect("the row is JSON");
    assert_eq!(row["target"], "./f1.nika");
    assert_eq!(row["trace_id"], "depth-1");
    assert_eq!(row["outcome"], "success");
}

/// The ninth edge is the bound's: refused fail-closed at depth eight, before its runner
/// is consulted, so the leaf is never called; the code rides up every level verbatim.
#[test]
fn the_ninth_edge_is_refused_before_the_leaf_runs() {
    let leaf = Arc::new(Leaf::default());
    let probe = Arc::clone(&leaf);
    let observed = on_ordinary_stack("nested-nine", move || {
        drive(chain(MAX_RUN_DEPTH + 1, false), probe, None, None)
    });
    let outcome = observed
        .root
        .expect("not abandoned")
        .expect("the root settles");
    assert!(!outcome.ok);
    let error = outcome.records["descend"]
        .error
        .as_ref()
        .expect("the refusal");
    assert_eq!(error.code, "NIKA-SEC-003", "one voice: {}", error.message);
    assert_eq!(leaf.counts(), (0, 0, 0), "the leaf never ran");
    let depths: BTreeSet<u32> = observed.journals.iter().map(|(depth, _)| *depth).collect();
    assert_eq!(
        depths,
        (1..=MAX_RUN_DEPTH).collect(),
        "no run deeper than the bound"
    );
    let refused = observed
        .journals
        .iter()
        .find(|(depth, e)| *depth == MAX_RUN_DEPTH && e.kind == EventKind::TaskFailed)
        .expect("the deepest admitted run refuses its call");
    assert!(
        str_field(&refused.1, "detail").is_some_and(|d| d.contains("NIKA-SEC-003")),
        "{:?}",
        refused.1
    );
}

/// The root's `timeout:` destroys the whole pending chain: the leaf eight levels down
/// started once and was dropped once, no level completed, and the leaf's effect never
/// happens, not even after its own delay has passed.
#[test]
fn a_root_timeout_destroys_a_pending_leaf_eight_levels_down() {
    let leaf = Arc::new(Leaf {
        delay: Some(Duration::from_millis(300)),
        ..Leaf::default()
    });
    let probe = Arc::clone(&leaf);
    let observed = on_ordinary_stack("nested-timeout", move || {
        drive(chain(MAX_RUN_DEPTH, true), probe, None, None)
    });
    let outcome = observed
        .root
        .expect("not abandoned")
        .expect("the root settles");
    let error = outcome.records["descend"]
        .error
        .as_ref()
        .expect("the timeout");
    assert_eq!(error.code, "NIKA-TIMEOUT-001", "{}", error.message);
    assert_eq!(outcome.records["descend"].status, TaskStatus::Failure);
    assert_eq!(
        leaf.counts(),
        (1, 0, 1),
        "reached, then destroyed before its effect"
    );
    assert!(
        depths_of(&observed.journals, EventKind::WorkflowCompleted).is_empty()
            && depths_of(&observed.journals, EventKind::TaskCompleted).is_empty(),
        "no level settled after the deadline"
    );
    assert_eq!(
        depths_of(&observed.journals, EventKind::WorkflowStarted),
        (1..=MAX_RUN_DEPTH).collect::<Vec<_>>(),
        "every level had started"
    );
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(
        leaf.counts(),
        (1, 0, 1),
        "no late effect after the leaf's own delay"
    );
}

/// An embedder abandoning the root run (dropping its future) destroys the pending chain
/// the same way: one start, one drop, no completion, no late effect.
#[test]
fn a_dropped_root_run_destroys_a_pending_leaf_eight_levels_down() {
    let leaf = Arc::new(Leaf {
        delay: Some(Duration::from_millis(400)),
        ..Leaf::default()
    });
    let probe = Arc::clone(&leaf);
    let observed = on_ordinary_stack("nested-dropped", move || {
        drive(
            chain(MAX_RUN_DEPTH, false),
            probe,
            None,
            Some(Duration::from_millis(100)),
        )
    });
    assert!(
        observed.root.is_none(),
        "the root run was abandoned, not settled"
    );
    assert_eq!(
        leaf.counts(),
        (1, 0, 1),
        "reached, then destroyed before its effect"
    );
    assert!(
        depths_of(&observed.journals, EventKind::TaskCompleted).is_empty(),
        "no level completed"
    );
    std::thread::sleep(Duration::from_millis(700));
    assert_eq!(
        leaf.counts(),
        (1, 0, 1),
        "no late effect after the leaf's own delay"
    );
}

/// An operator Stop while the leaf eight levels down is pending: the documented Stop
/// contract (in-flight work completes and is counted) holds through the nesting, since a
/// child run does not observe its root's Stop. The leaf's one effect lands BEFORE the
/// root returns, the root ends cancelled at its wave boundary, and nothing follows.
#[test]
fn an_operator_stop_lets_the_pending_descendant_finish_then_cancels() {
    let cancel = CancelCtx::new();
    let leaf = Arc::new(Leaf {
        delay: Some(Duration::from_millis(100)),
        stop: Some(cancel.clone()),
        ..Leaf::default()
    });
    let probe = Arc::clone(&leaf);
    let observed = on_ordinary_stack("nested-stop", move || {
        drive(chain(MAX_RUN_DEPTH, false), probe, Some(cancel), None)
    });
    let outcome = observed
        .root
        .expect("not abandoned")
        .expect("the root settles");
    assert!(outcome.cancelled, "the root ends cancelled by the operator");
    assert!(!outcome.ok);
    assert_eq!(outcome.records["descend"].status, TaskStatus::Success);
    assert!(
        observed
            .events
            .iter()
            .any(|e| e.kind == EventKind::WorkflowCancelled),
        "the root's terminal frame"
    );
    assert_eq!(
        leaf.counts(),
        (1, 1, 0),
        "the in-flight leaf completed once"
    );
    assert_eq!(
        depths_of(&observed.journals, EventKind::WorkflowCompleted),
        (1..=MAX_RUN_DEPTH).rev().collect::<Vec<_>>(),
        "each child run settled on its own; none observed the root's Stop"
    );
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(
        leaf.counts(),
        (1, 1, 0),
        "nothing happens after the run returns"
    );
}
