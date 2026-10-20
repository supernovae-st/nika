// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! B7's twin for tool receipts: a dispatch dropped before it returns (a task
//! `timeout:`, a `fail_fast` sibling) debits the known cost of every tool
//! success it received, once; a returned dispatch carries its own.
//!
//! An `mcp:` call runs on its own worker thread, which stores the result in
//! the handoff BEFORE it wakes the task. The test waker counts only the wakes
//! issued from that thread, so a publication is observed, never guessed by a
//! sleep or a closure-side signal. The task runs through the real per-task
//! pipeline and the real settle, polled by hand below the wave; the injected
//! clock fires the task timer in the poll where the test decides.

use std::future::Future;
use std::sync::{Arc, Condvar, Mutex};
use std::task::{Context, Poll, Wake, Waker};
use std::time::{Duration, Instant, SystemTime};

use nika_event::EventKind;
use nika_kernel::CancelCtx;
use nika_kernel::ai::provider::{ContentBlock, InferResponse, StopReason, TokenUsage, ToolDef};
use nika_kernel::clock::ClockDyn;
use nika_kernel::tool_executor::{ToolCall, ToolResult, ToolRunStart};
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::{InvokeVerb, McpPlane};
use serde_json::json;

use super::*;

/// The `timeout:` of the timed fixtures; the clock sleeps no other length.
const LIMIT: Duration = Duration::from_secs(1);
/// A fixture deadlock bound: a missing notification fails the test, never decides it.
const BOUND: Duration = Duration::from_secs(10);

const TIMED_INVOKE: &str = "nika: unread-ready\npermits: { tools: [\"mcp:s/paid\"] }\ntasks:\n  paid:\n    timeout: \"1s\"\n    invoke: { tool: \"mcp:s/paid\" }\n";
const FAIL_FAST: &str = "nika: unread-ready-fan\npermits: { tools: [\"mcp:s/paid\"] }\ntasks:\n  fan:\n    for_each: { items: [\"boom\", \"paid\"], max_parallel: 2, fail_fast: true }\n    invoke: { tool: \"mcp:s/paid\", args: { item: \"${{ item }}\" } }\n";
const TIMED_AGENT: &str = "nika: unread-ready-agent\nmodel: mock/agent\npermits: { tools: [\"mcp:s/paid\"] }\ntasks:\n  scout:\n    timeout: \"1s\"\n    agent: { prompt: \"Use the offered source.\", tools: [\"mcp:s/paid\"] }\n";

/// A counter another thread can wait on.
#[derive(Default)]
struct Count {
    value: Mutex<usize>,
    changed: Condvar,
}

impl Count {
    fn bump(&self) {
        *self.value.lock().expect("count") += 1;
        self.changed.notify_all();
    }

    fn get(&self) -> usize {
        *self.value.lock().expect("count")
    }

    /// Wait until `n` were counted; a fixture deadlock fails the test.
    fn reach(&self, n: usize, what: &str) {
        let value = self.value.lock().expect("count");
        let (value, waited) = self
            .changed
            .wait_timeout_while(value, BOUND, |value| *value < n)
            .expect("count");
        assert!(!waited.timed_out(), "{what}: {} of {n}", *value);
    }
}

/// The fixture's `mcp:` calls: each begins, then waits until the test
/// releases its item. `boom` fails ordinarily; any other item succeeds and
/// reports 2.5 USD.
#[derive(Default)]
struct Calls {
    began: Count,
    released: Mutex<Vec<String>>,
    opened: Condvar,
    stops: Mutex<Vec<CancelCtx>>,
    /// An item the clock releases at its next `now()`, from inside a poll.
    deferred: Mutex<Option<&'static str>>,
}

impl Calls {
    fn release(&self, item: &str) {
        self.released
            .lock()
            .expect("released")
            .push(item.to_owned());
        self.opened.notify_all();
    }

    fn defer_release(&self, item: &'static str) {
        *self.deferred.lock().expect("deferred") = Some(item);
    }

    fn take_deferred(&self) -> Option<&'static str> {
        self.deferred.lock().expect("deferred").take()
    }

    fn wait_for_release(&self, item: &str) {
        let released = self.released.lock().expect("released");
        let (released, waited) = self
            .opened
            .wait_timeout_while(released, BOUND, |released| {
                !released.iter().any(|done| done == item)
            })
            .expect("released");
        drop(released);
        assert!(!waited.timed_out(), "the test never released {item}");
    }

    fn stops(&self) -> Vec<CancelCtx> {
        self.stops.lock().expect("stops").clone()
    }

    fn plane(self: &Arc<Self>) -> McpPlane {
        let calls = Arc::clone(self);
        McpPlane::stoppable(
            Arc::new(move |call: ToolCall, stop: &CancelCtx| {
                assert!(stop.begin());
                calls.stops.lock().expect("stops").push(stop.clone());
                calls.began.bump();
                let item = call.input["item"].as_str().unwrap_or("paid").to_owned();
                calls.wait_for_release(&item);
                Ok(if item == "boom" {
                    ToolResult::error(call.id.to_string(), "ordinary failure")
                } else {
                    ToolResult::success(call.id.to_string(), "done")
                        .with_structured(json!({"cost_usd": 2.5, "ok": true}))
                })
            }),
            Arc::new(|| vec![ToolDef::new("mcp:s/paid", "fixture tool", json!({}))]),
        )
    }
}

/// Counts the wakes issued from an `mcp:` worker thread: the handoff stores
/// the result before it wakes the task, so each one is a publication.
#[derive(Default)]
struct Publications(Count);

impl Wake for Publications {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        if std::thread::current().name() == Some("nika-mcp-call") {
            self.0.bump();
        }
    }
}

/// What the task timer does when first polled: in the same poll, right
/// after the attempt it races (`race_budget` polls the attempt first).
#[derive(Clone, Copy)]
enum Timer {
    /// No task here is timed: a sleep is a fixture error.
    Absent,
    /// Fire once the call began, while it is still unresolved.
    OnceBegun,
    /// Release `paid`, wait for its publication, then fire before the
    /// attempt is polled again.
    AfterPublication,
}

#[derive(Clone)]
struct Schedule {
    base: Instant,
    timer: Timer,
    calls: Arc<Calls>,
    published: Arc<Publications>,
}

impl ClockDyn for Schedule {
    /// A deferred item is released here, inside the poll that reads the
    /// clock, and its publication awaited before the poll goes on.
    fn now(&self) -> Instant {
        if let Some(item) = self.calls.take_deferred() {
            let before = self.published.0.get();
            self.calls.release(item);
            self.published
                .0
                .reach(before + 1, "the deferred result was never published");
        }
        self.base
    }

    fn system_now(&self) -> SystemTime {
        SystemTime::UNIX_EPOCH
    }

    async fn sleep(&self, duration: Duration) {
        assert_eq!(duration, LIMIT, "only the task timer sleeps here");
        match self.timer {
            Timer::Absent => panic!("no task here is timed"),
            Timer::OnceBegun => self.calls.began.reach(1, "the call never began"),
            Timer::AfterPublication => {
                let before = self.published.0.get();
                self.calls.release("paid");
                self.published
                    .0
                    .reach(before + 1, "the paid result was never published");
            }
        }
    }
}

/// The test's step between the first poll and the next.
type Between<'a> = &'a dyn Fn(&Calls, &Publications);

/// One fixture run, settled.
struct Ran {
    record: TaskRecord,
    sink: VecSink,
    spend: crate::ledger::LedgerSnapshot,
    ledger: crate::ledger::RunLedger,
    calls: Arc<Calls>,
    published: Arc<Publications>,
}

/// Poll the task's pipeline by hand. Without `between`, the first poll must
/// settle it; with it, the first poll must wait, `between` runs, and the
/// next poll must settle it.
fn drive(
    mut pipeline: std::pin::Pin<Box<dyn Future<Output = crate::task::Finish> + '_>>,
    calls: &Calls,
    published: &Arc<Publications>,
    between: Option<Between<'_>>,
) -> crate::task::Finish {
    let waker = Waker::from(Arc::clone(published));
    let mut cx = Context::from_waker(&waker);
    match (pipeline.as_mut().poll(&mut cx), between) {
        (Poll::Ready(finish), None) => finish,
        (Poll::Pending, Some(between)) => {
            between(calls, published);
            match pipeline.as_mut().poll(&mut cx) {
                Poll::Ready(finish) => finish,
                Poll::Pending => panic!("the scheduled task settles on its next poll"),
            }
        }
        (Poll::Ready(_), Some(_)) => panic!("the call was expected to wait"),
        (Poll::Pending, None) => panic!("the timer settles the task in its first poll"),
    }
}

/// Run the workflow's one task through the real pipeline and settle it.
fn run(source: &str, timer: Timer, provider: MockProvider, between: Option<Between<'_>>) -> Ran {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture checks: {:?}", report.findings);
    let calls = Arc::new(Calls::default());
    let published = Arc::new(Publications::default());
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    assert!(invoke.install_mcp_plane(calls.plane()));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(ProviderRegistry::without_http(ProvidersConfig::default())),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(provider),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/agent",
        ),
        Schedule {
            base: Instant::now(),
            timer,
            calls: Arc::clone(&calls),
            published: Arc::clone(&published),
        },
        RuntimeConfig::default(),
    );
    let (values, strings) = (BTreeMap::new(), BTreeMap::new());
    let (records, types) = (BTreeMap::new(), BTreeMap::new());
    let bags = crate::bags::ValueBags {
        inputs: &values,
        consts: &values,
        secrets: &values,
        run: None,
        bound: None,
    };
    let resume =
        crate::resume::ResumeContext::of(&wf, &values, None, &strings, &strings, None, &strings);
    let ledger = crate::ledger::RunLedger::new(None);
    let task = &wf.tasks[0].value;
    let pipeline = Box::pin(runtime.run_task_pipeline(
        task,
        &wf,
        &records,
        bags,
        wf.permits.as_ref().map(|p| &p.value),
        &types,
        &resume,
        &ledger,
        nika_cap::JqClock::at(nika_types::timestamp::Timestamp::EPOCH),
        ToolRunStart::new(0),
    ));
    let finish = drive(pipeline, &calls, &published, between);
    let spend = ledger.snapshot();
    let mut settled = BTreeMap::new();
    let (mut ok, mut hits, mut sink) = (true, Vec::new(), VecSink::new());
    crate::settle::settle(
        finish,
        &mut settled,
        &mut ok,
        &mut hits,
        &mut DeterministicStamper::new(),
        &mut sink,
    );
    assert!(hits.is_empty(), "no cache credit");
    Ran {
        record: settled.remove(&task.id.value).expect("the task settled"),
        sink,
        spend,
        ledger,
        calls,
        published,
    }
}

/// Failed as `cause`, with no output, success frame or resume trio.
fn assert_failed_without_credit(ran: &Ran, cause: TerminalCause, code: &str) {
    assert_eq!(ran.record.status, TaskStatus::Failure);
    assert_eq!(ran.record.cause, cause);
    assert_eq!(ran.record.error.as_ref().expect("an error").code, code);
    assert_eq!(ran.record.output, Value::Null);
    let events = ran.sink.events();
    assert!(!events.iter().any(|e| e.kind == EventKind::TaskCompleted));
    let failed: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::TaskFailed)
        .collect();
    assert_eq!(failed.len(), 1);
    for field in [
        crate::resume::fields::DEF_HASH,
        crate::resume::fields::INPUT_HASH,
        crate::resume::fields::OUTPUT,
    ] {
        assert!(
            failed[0].str_field(field).is_none(),
            "no resume field {field}"
        );
    }
}

/// 2.5 USD on the ledger from one priced leaf, under `source` when named.
fn assert_debited_once(spend: &crate::ledger::LedgerSnapshot, source: Option<&str>) {
    assert!(
        (spend.spent_usd - 2.5).abs() < 1e-9,
        "2.5 USD once, not {}",
        spend.spent_usd
    );
    assert!(spend.any_priced);
    assert_eq!(spend.priced_calls, 1);
    assert_eq!(spend.unpriced_calls, 0);
    if let Some(source) = source {
        assert_eq!(spend.by_source.len(), 1, "{:?}", spend.by_source);
        assert!((spend.by_source[source] - 2.5).abs() < 1e-9);
    }
}

#[test]
fn a_timeout_debits_a_ready_unread_mcp_success_once() {
    let ran = run(
        TIMED_INVOKE,
        Timer::AfterPublication,
        MockProvider::new("mock"),
        None,
    );
    assert_failed_without_credit(&ran, TerminalCause::Timeout, "NIKA-TIMEOUT-001");
    // A completed call is not an abandoned one: no stopped call, no cancel.
    assert!(ran.spend.stopped_calls.is_empty());
    assert!(!ran.calls.stops()[0].is_cancelled());
    assert_debited_once(&ran.spend, Some("mcp:s/paid"));
    assert_debited_once(&ran.ledger.snapshot(), Some("mcp:s/paid"));
}

#[test]
fn a_fail_fast_drop_debits_a_ready_unread_sibling_once() {
    // Both items wait at their calls; `boom` publishes its failure. The
    // next poll takes `boom`; while it settles, the clock releases `paid`,
    // whose publication wakes the collector that poll registered (queued
    // behind `boom`). The collector fails fast and drops `paid` unread.
    let boom_first = |calls: &Calls, published: &Publications| {
        calls.began.reach(2, "both items began");
        calls.release("boom");
        published.0.reach(1, "boom's failure was never published");
        calls.defer_release("paid");
    };
    let ran = run(
        FAIL_FAST,
        Timer::Absent,
        MockProvider::new("mock"),
        Some(&boom_first),
    );
    assert_eq!(ran.calls.take_deferred(), None, "paid was released in-poll");
    assert_eq!(ran.published.0.get(), 2);
    assert_failed_without_credit(&ran, TerminalCause::VerbError, "NIKA-451");
    let failed = ran
        .sink
        .events()
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed)
        .expect("a task_failed frame");
    let items: Value =
        serde_json::from_str(failed.str_field("items").expect("the item table")).expect("json");
    assert_eq!(items[1]["item"], "paid");
    assert_eq!(
        items[1]["status"], "cancelled",
        "an unread success earns no row"
    );
    assert!(ran.spend.stopped_calls.is_empty());
    assert_debited_once(&ran.spend, Some("mcp:s/paid"));
}

#[test]
fn an_agent_timeout_debits_its_ready_unread_tool_once() {
    let provider = MockProvider::new("mock").enqueue_response(InferResponse::new(
        vec![ContentBlock::ToolUse {
            id: "agent-paid-1".into(),
            name: "mcp:s/paid".into(),
            input: json!({}),
        }],
        TokenUsage::default(),
        StopReason::ToolUse,
    ));
    let ran = run(TIMED_AGENT, Timer::AfterPublication, provider.clone(), None);
    assert_failed_without_credit(&ran, TerminalCause::Timeout, "NIKA-TIMEOUT-001");
    assert_eq!(provider.captured_requests().len(), 1);
    assert!(ran.spend.stopped_calls.is_empty());
    // An agent's returned tool spend rides its resolved model, unknown
    // once the dispatch is dropped: the debit is unattributed.
    assert_debited_once(&ran.spend, None);
}

#[test]
fn a_consumed_mcp_success_is_debited_once() {
    let consume = |calls: &Calls, published: &Publications| {
        calls.release("paid");
        published.0.reach(1, "the paid result was never published");
    };
    let source = TIMED_INVOKE.replace("    timeout: \"1s\"\n", "");
    let ran = run(
        &source,
        Timer::Absent,
        MockProvider::new("mock"),
        Some(&consume),
    );
    assert_eq!(ran.record.status, TaskStatus::Success);
    assert_eq!(ran.record.output, json!({"cost_usd": 2.5, "ok": true}));
    assert_debited_once(&ran.spend, Some("mcp:s/paid"));
}

#[test]
fn a_contract_rejected_mcp_success_is_debited_once() {
    let consume = |calls: &Calls, published: &Publications| {
        calls.release("paid");
        published.0.reach(1, "the paid result was never published");
    };
    let source = TIMED_INVOKE.replace("    timeout: \"1s\"\n", "    returns: integer\n");
    let ran = run(
        &source,
        Timer::Absent,
        MockProvider::new("mock"),
        Some(&consume),
    );
    assert_failed_without_credit(&ran, TerminalCause::VerbError, "NIKA-TYPE-101");
    assert_debited_once(&ran.spend, Some("mcp:s/paid"));
}

#[test]
fn an_unresolved_call_stays_unknown_and_costs_nothing() {
    let ran = run(
        TIMED_INVOKE,
        Timer::OnceBegun,
        MockProvider::new("mock"),
        None,
    );
    assert_failed_without_credit(&ran, TerminalCause::Timeout, "NIKA-TIMEOUT-001");
    let [stopped] = ran.spend.stopped_calls.as_slice() else {
        panic!("one stopped call: {:?}", ran.spend.stopped_calls);
    };
    assert_eq!(stopped.tool, "mcp:s/paid");
    assert_eq!(
        stopped.outcome,
        nika_event::settlement::StopOutcome::Unknown
    );
    assert!(ran.calls.stops()[0].is_cancelled());
    // A success arriving after the drop decided `unknown` invents no cost.
    ran.calls.release("paid");
    ran.published
        .0
        .reach(1, "the late result was never published");
    for spend in [&ran.spend, &ran.ledger.snapshot()] {
        assert!(!spend.any_priced);
        assert_eq!(spend.priced_calls, 0);
        assert!(spend.spent_usd.abs() < 1e-12);
    }
}
