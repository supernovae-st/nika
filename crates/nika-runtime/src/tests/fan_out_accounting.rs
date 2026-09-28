// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! B7 · E17-F1 / E13-F4: the run ledger against the physical requests a
//! dispatch makes. A controlled wire holds every post at a barrier and then
//! answers per item, so which sibling settles first, and whether `fail_fast`
//! or a `timeout:` drops a sent request, is decided by the test, never by DNS
//! or scheduler timing. Every physical request is on the ledger exactly once.
//!
//! B8 · spec 03/17: the same wire tells which items began. An item whose
//! iteration began but was abandoned without a recorded terminal is
//! `cancelled`; only one never polled is `never_started`.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};

use nika_kernel::http::{HttpError, HttpPostDyn, HttpRequest, HttpResponse, HttpStreamResponse};
use nika_kernel::secret::Secret;
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor};
use nika_providers::{InferenceAdmission, ProviderRegistry, ProvidersConfig};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

const FREE: &str = "openrouter/qwen/qwen3.8-27b:free";
/// Catalog-priced (C4's and E17's canonical paid route).
const PAID: &str = "deepseek/deepseek-v4-pro";

/// What the wire does with one post of an item once the barrier opens.
#[derive(Clone, Copy, Debug)]
enum Answer {
    /// A connection failure: the request left, no response came back.
    Refuse,
    /// Never answers: only a dropped future ends it.
    Hang,
    /// Complete usage for the route that was asked.
    Serve,
    /// HTTP 429 with no Retry-After: the provider backs off and re-sends.
    Limit,
}

/// A std-only async gate: `wait` returns once `need` arrivals were counted.
struct Gate {
    need: usize,
    state: Mutex<(usize, Vec<Waker>)>,
}

impl Gate {
    fn new(need: usize) -> Self {
        Self {
            need,
            state: Mutex::new((0, Vec::new())),
        }
    }
    fn arrive(&self) {
        let mut state = self.state.lock().expect("gate");
        state.0 += 1;
        if state.0 >= self.need {
            for waker in state.1.drain(..) {
                waker.wake();
            }
        }
    }
    fn wait(&self) -> impl Future<Output = ()> + '_ {
        std::future::poll_fn(|cx| {
            let mut state = self.state.lock().expect("gate");
            if state.0 >= self.need {
                Poll::Ready(())
            } else {
                state.1.push(cx.waker().clone());
                Poll::Pending
            }
        })
    }
}

/// The controlled wire. Every post arrives at `arrived` (a barrier over the
/// posts in flight together). An item answers its posts in order from its
/// script (the last answer repeats). An item named by `answering_last` waits
/// until the other items' posts have returned.
struct Held {
    single: bool,
    script: Vec<(&'static str, Vec<Answer>)>,
    arrived: Gate,
    returned: Gate,
    last: Option<&'static str>,
    posts: Mutex<Vec<String>>,
    dropped: AtomicUsize,
}

impl Held {
    fn new(single: bool, script: &[(&'static str, &[Answer])], in_flight: usize) -> Arc<Self> {
        Arc::new(Self {
            single,
            script: script.iter().map(|(i, a)| (*i, a.to_vec())).collect(),
            arrived: Gate::new(in_flight),
            returned: Gate::new(script.len().saturating_sub(1)),
            last: None,
            posts: Mutex::new(Vec::new()),
            dropped: AtomicUsize::new(0),
        })
    }
    fn answering_last(mut self: Arc<Self>, item: &'static str) -> Arc<Self> {
        Arc::get_mut(&mut self).expect("unshared").last = Some(item);
        self
    }
    fn posts(&self) -> Vec<String> {
        self.posts.lock().expect("posts").clone()
    }
    fn dropped(&self) -> usize {
        self.dropped.load(Ordering::SeqCst)
    }
    /// The item this post is for, and which of its posts it is.
    fn post_of(&self, req: &HttpRequest) -> (&'static str, Answer) {
        let body = String::from_utf8_lossy(req.body.as_ref().expect("a body"));
        let (item, answers) = self
            .script
            .iter()
            .find(|(item, _)| body.contains(&format!("say {item}")))
            .unwrap_or_else(|| panic!("an item prompt in {body}"));
        let mut posts = self.posts.lock().expect("posts");
        let nth = posts.iter().filter(|p| p == item).count();
        posts.push((*item).to_owned());
        (item, answers[nth.min(answers.len() - 1)])
    }
}

/// Counts a post whose future was dropped before it returned.
struct InFlight<'a> {
    wire: &'a Held,
    returned: bool,
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        if !self.returned {
            self.wire.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// Complete usage for the model the request named (each vendor's shape), under
/// a response id that names the item, so a call record says which item it was.
fn served(req: &HttpRequest, item: &str) -> HttpResponse {
    let sent: serde_json::Value =
        serde_json::from_slice(req.body.as_ref().expect("a body")).expect("json");
    let usage = if req.url.contains("openrouter") {
        serde_json::json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
            "cost": 0, "is_byok": false})
    } else {
        serde_json::json!({"prompt_tokens": 10, "completion_tokens": 5, "total_tokens": 15,
            "prompt_cache_hit_tokens": 0, "prompt_cache_miss_tokens": 10})
    };
    let body = serde_json::json!({
        "id": format!("fan-fixture-{item}"), "model": sent["model"],
        "choices": [{"message": {"content": "observed"}, "finish_reason": "stop"}],
        "usage": usage
    });
    HttpResponse::new(
        200,
        std::collections::BTreeMap::new(),
        body.to_string().into(),
        req.url.clone(),
    )
}

impl HttpPostDyn for Held {
    fn supports_single_attempt(&self) -> bool {
        self.single
    }
    async fn post(&self, req: HttpRequest) -> Result<HttpResponse, HttpError> {
        let (item, answer) = self.post_of(&req);
        let mut flight = InFlight {
            wire: self,
            returned: false,
        };
        self.arrived.arrive();
        self.arrived.wait().await;
        if self.last == Some(item) {
            self.returned.wait().await;
        }
        let result = match answer {
            Answer::Refuse => Err(HttpError::Connection {
                reason: "fixture: connection refused".into(),
            }),
            Answer::Serve => Ok(served(&req, item)),
            Answer::Limit => Ok(HttpResponse::new(
                429,
                std::collections::BTreeMap::new(),
                "{\"error\":{\"message\":\"rate limited\"}}".into(),
                req.url.clone(),
            )),
            // Only a dropped future ends a hung post.
            Answer::Hang => std::future::pending().await,
        };
        flight.returned = true;
        if self.last != Some(item) {
            self.returned.arrive();
        }
        result
    }
    async fn send_streaming(&self, _: HttpRequest) -> Result<HttpStreamResponse, HttpError> {
        panic!("no route here streams");
    }
}

/// The run's two clients: `normal` for unobserved routes, `bounded` for the
/// ones `account` observes.
struct Wires {
    normal: Arc<Held>,
    bounded: Arc<Held>,
    account: InferenceAdmission,
}

impl Wires {
    fn observed(bounded: Arc<Held>) -> Self {
        Self {
            normal: Held::new(false, &[], 1),
            bounded,
            account: InferenceAdmission::observe_declared_free(),
        }
    }
    fn unobserved(normal: Arc<Held>) -> Self {
        Self {
            normal,
            bounded: Held::new(true, &[], 1),
            account: InferenceAdmission::observe_declared_free(),
        }
    }
}

/// Run `source` over both clients. With `limit`, the caller drops the run at
/// that deadline: no outcome comes back, only the frames written before.
async fn run_within(
    source: &str,
    wires: &Wires,
    limit: Option<std::time::Duration>,
) -> (Option<RunOutcome>, Vec<Event>) {
    let (settled, events) = launch(source, wires, limit, None).await;
    (settled.map(|s| s.expect("the run settles")), events)
}

/// As [`run_within`], keeping a launch refusal, under an optional cap.
async fn launch(
    source: &str,
    wires: &Wires,
    limit: Option<std::time::Duration>,
    cap: Option<f64>,
) -> (Option<Result<RunOutcome, RuntimeError>>, Vec<Event>) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let config = ProvidersConfig::new()
        .with_key("openrouter", Secret::new("fixture"))
        .with_key("deepseek", Secret::new("fixture"));
    let registry = ProviderRegistry::new(Arc::clone(&wires.normal), config)
        .with_inference_admission_http(wires.account.clone(), Arc::clone(&wires.bounded));
    let runtime_config = RuntimeConfig {
        inference_admission: Some(wires.account.clone()),
        ..RuntimeConfig::default()
    };
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(Arc::new(registry), "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        runtime_config,
    )
    .with_max_cost_usd(cap);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let running = runtime.run(&wf, &report, &mut stamper, &mut sink);
    let outcome = match limit {
        None => Some(running.await),
        Some(limit) => tokio::time::timeout(limit, running).await.ok(),
    };
    (outcome, sink.into_events())
}

async fn run(source: &str, wires: &Wires) -> (RunOutcome, Vec<Event>) {
    let (outcome, events) = run_within(source, wires, None).await;
    (outcome.expect("no caller deadline"), events)
}

/// The item statuses of the fan-out's terminal table, in input order.
fn statuses(events: &[Event]) -> Vec<String> {
    let text = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TaskFailed | EventKind::TaskCompleted))
        .find_map(|e| e.str_field("items"))
        .expect("an inline item table on the fan-out's terminal");
    let rows: Vec<serde_json::Value> = serde_json::from_str(text).expect("rows");
    rows.iter()
        .map(|row| {
            assert!(
                row.get("output").is_none(),
                "no item row carries output: {row}"
            );
            row["status"].as_str().expect("a status").to_owned()
        })
        .collect()
}

/// A three-item fan-out over `model`; `task` adds task-level fields. The cap
/// is the reasoning seat's floor (256) that `PAID` needs (B9).
fn fan(fan: &str, model: &str, task: &str) -> String {
    format!(
        "nika: fan\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: ['a', 'b', 'c'], {fan} }}\n{task}    infer: {{ model: '{model}', prompt: 'say ${{{{ item }}}}', max_tokens: 256 }}\n"
    )
}

/// One infer task on `model` asking for item `a`, at the same cap.
fn single(model: &str, task: &str) -> String {
    format!(
        "nika: one\npermits: {{}}\ntasks:\n  ask:\n{task}    infer: {{ model: '{model}', prompt: 'say a', max_tokens: 256 }}\n"
    )
}

fn terminal(events: &[Event]) -> &Event {
    events
        .iter()
        .rfind(|e| e.is_terminal())
        .expect("a terminal frame")
}

/// The terminal ledger: (priced calls, unpriced calls).
fn ledger(events: &[Event]) -> (Option<i64>, Option<i64>) {
    let int = |key: &str| {
        terminal(events)
            .fields
            .iter()
            .find(|f| f.key == key)
            .and_then(|f| match &f.value {
                FieldValue::Int(n) => Some(*n),
                _ => None,
            })
    };
    (int("priced_calls"), int("unpriced_calls"))
}

/// The fan-out parent's own frame (the one task frame these workflows write):
/// its durable call records and its unknown-call count, as written.
fn parent_calls(events: &[Event]) -> (Vec<serde_json::Value>, Option<i64>) {
    let parent = events
        .iter()
        .find(|e| matches!(e.kind, EventKind::TaskFailed | EventKind::TaskCompleted))
        .expect("the fan-out's own frame");
    let calls = parent
        .str_field("inference_calls")
        .map_or_else(Vec::new, |text| {
            serde_json::from_str(text).expect("a call list")
        });
    let unknown = parent
        .fields
        .iter()
        .find(|f| f.key == "cost_unknown_calls")
        .and_then(|f| match &f.value {
            FieldValue::Int(n) => Some(*n),
            _ => None,
        });
    (calls, unknown)
}

/// Every task frame's field `key`, in the order the frames were written.
fn task_fields<'e>(events: &'e [Event], key: &str) -> Vec<&'e str> {
    events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TaskFailed | EventKind::TaskCompleted))
        .filter_map(|e| e.str_field(key))
        .collect()
}

fn sent_attempts(account: &InferenceAdmission) -> usize {
    account
        .snapshot()
        .expect("the account reads")
        .attempts
        .iter()
        .filter(|a| a.sent)
        .count()
}

const REFUSE: &[Answer] = &[Answer::Refuse];
const HANG: &[Answer] = &[Answer::Hang];
const SERVE: &[Answer] = &[Answer::Serve];

/// E17-F1 · all three posts are in flight together; item a's connection fails
/// while b and c are still waiting, so `fail_fast` drops two sent siblings.
/// Every physical send stays on the ledger: three calls whose charge is unknown.
#[tokio::test]
async fn fail_fast_keeps_every_sent_sibling_on_the_ledger() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", REFUSE), ("b", HANG), ("c", HANG)],
        3,
    ));
    let (outcome, events) = run(&fan("max_parallel: 3", FREE, ""), &wires).await;
    assert!(!outcome.ok, "item a failed: {outcome:?}");
    assert_eq!(wires.bounded.posts().len(), 3, "three physical sends");
    assert_eq!(
        wires.bounded.dropped(),
        2,
        "fail_fast dropped b and c in flight"
    );
    assert_eq!(sent_attempts(&wires.account), 3);
    assert_eq!(
        ledger(&events),
        (Some(0), Some(3)),
        "{:?}",
        terminal(&events)
    );
    assert_eq!(
        statuses(&events),
        ["failed", "cancelled", "cancelled"],
        "b and c began and were abandoned"
    );
}

/// The control: the same three sends, but a answers only after b and c
/// return. Both orders count the same physical sends, and nothing twice.
/// B10 · spec 03: the collector no longer waits for a in input order. c, the
/// arrival that opens the held gate, is answered first; its refusal stops the
/// batch at once, and a (still waiting for its answer) and b are dropped in
/// flight: `cancelled`, never `never_started`, each send one unknown charge.
#[tokio::test]
async fn siblings_that_settle_first_count_the_same_sends() {
    let wires = Wires::observed(
        Held::new(true, &[("a", REFUSE), ("b", REFUSE), ("c", REFUSE)], 3).answering_last("a"),
    );
    let (outcome, events) = run(&fan("max_parallel: 3", FREE, ""), &wires).await;
    assert!(!outcome.ok);
    assert_eq!(wires.bounded.posts().len(), 3);
    assert_eq!(wires.bounded.dropped(), 2, "a and b were dropped in flight");
    assert_eq!(sent_attempts(&wires.account), 3);
    assert_eq!(ledger(&events), (Some(0), Some(3)));
    assert_eq!(statuses(&events), ["cancelled", "cancelled", "failed"]);
}

/// B10 · D6 · spec 03 immediate fail-fast: `max_parallel: 2`, a held, b
/// failing at once, c queued. Before B10 the collector waited for a in input
/// order (its whole timeout; here, with none, forever) and then read b's
/// finished failure as `cancelled`. Now the first failure to complete stops
/// the fan: a is dropped in flight (`cancelled`, one unknown charge), b's
/// failure is recorded and becomes the parent error, c never starts.
#[tokio::test]
async fn the_first_failure_to_complete_stops_the_fan_whatever_its_index() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", HANG), ("b", REFUSE), ("c", SERVE)],
        2,
    ));
    let bound = Some(std::time::Duration::from_secs(5));
    let (settled, events) = run_within(&fan("max_parallel: 2", FREE, ""), &wires, bound).await;
    let outcome = settled.expect("b's failure stops the fan: nothing waits for a");
    assert!(!outcome.ok);
    let mut posts = wires.bounded.posts();
    posts.sort();
    assert_eq!(posts, ["a", "b"], "c never started");
    assert_eq!(wires.bounded.dropped(), 1, "a was dropped in flight");
    assert_eq!(
        ledger(&events),
        (Some(0), Some(2)),
        "{:?}",
        terminal(&events)
    );
    assert_eq!(statuses(&events), ["cancelled", "failed", "never_started"]);
    let error = outcome.records["ask"]
        .error
        .as_ref()
        .expect("the parent error");
    assert!(error.message.contains("[1] b"), "{}", error.message);
}

/// B10 · a sibling that completes before a slower one fails keeps its own
/// terminal. a hangs until its `timeout:` drops it; b and c answer at once.
/// Before B10 the collector waited for a in input order and dropped b's and
/// c's finished answers (`cancelled`, never started); now both are `ok`.
#[tokio::test]
async fn a_completed_success_is_not_relabelled_by_a_slower_failure() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", HANG), ("b", SERVE), ("c", SERVE)],
        1,
    ));
    let source = fan("max_parallel: 2", FREE, "    timeout: \"300ms\"\n");
    let (outcome, events) = run(&source, &wires).await;
    assert!(!outcome.ok);
    assert_eq!(statuses(&events), ["failed", "ok", "ok"]);
    assert_eq!(
        wires.bounded.dropped(),
        1,
        "only a's timeout dropped a send"
    );
    assert_eq!(
        ledger(&events),
        (Some(2), Some(1)),
        "{:?}",
        terminal(&events)
    );
}

/// `max_parallel: 1`: a fails, and b and c are never started. Nothing is
/// counted for a send that never happened.
#[tokio::test]
async fn queued_items_that_never_start_are_not_counted() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", REFUSE), ("b", SERVE), ("c", SERVE)],
        1,
    ));
    let (outcome, events) = run(&fan("max_parallel: 1", FREE, ""), &wires).await;
    assert!(!outcome.ok);
    assert_eq!(wires.bounded.posts(), ["a"], "b and c never started");
    assert_eq!(wires.bounded.dropped(), 0);
    assert_eq!(sent_attempts(&wires.account), 1);
    assert_eq!(ledger(&events), (Some(0), Some(1)));
    assert_eq!(
        statuses(&events),
        ["failed", "never_started", "never_started"]
    );
}

/// `fail_fast: false` settles every item: a's unknown charge and b's and c's
/// complete zero-cost usage, each counted once.
#[tokio::test]
async fn without_fail_fast_every_item_settles_once() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", REFUSE), ("b", SERVE), ("c", SERVE)],
        3,
    ));
    let (outcome, events) = run(&fan("max_parallel: 3, fail_fast: false", FREE, ""), &wires).await;
    assert!(!outcome.ok, "item a still fails the task");
    assert_eq!(wires.bounded.dropped(), 0);
    assert_eq!(sent_attempts(&wires.account), 3);
    assert_eq!(
        ledger(&events),
        (Some(2), Some(1)),
        "{:?}",
        terminal(&events)
    );
    assert_eq!(statuses(&events), ["failed", "ok", "ok"]);
}

/// A `timeout:` drops every hung attempt: each request it had sent stays on
/// the ledger as an unknown charge (it used to ride nowhere).
#[tokio::test]
async fn a_timeout_keeps_the_requests_it_dropped() {
    let wires = Wires::observed(Held::new(true, &[("a", HANG), ("b", HANG), ("c", HANG)], 3));
    let (outcome, events) = run(
        &fan("max_parallel: 3", FREE, "    timeout: \"300ms\"\n"),
        &wires,
    )
    .await;
    assert!(!outcome.ok, "the items timed out");
    assert_eq!(wires.bounded.posts().len(), 3);
    assert_eq!(wires.bounded.dropped(), 3, "every post was dropped");
    assert_eq!(
        ledger(&events),
        (Some(0), Some(3)),
        "{:?}",
        terminal(&events)
    );
    assert_eq!(
        statuses(&events),
        ["failed", "cancelled", "cancelled"],
        "a's timeout is recorded; b and c began and were abandoned"
    );
}

/// A later item fails first while the first item is still in flight. B10 ·
/// spec 03: the collector no longer waits for a in input order. b's failure,
/// the first to complete, stops the batch at once; c, completed before the
/// stop, keeps its `ok` row; a is dropped in flight (`cancelled`, not a
/// timeout). b's failure and c's zero-cost completion stay counted beside
/// a's unknown charge.
#[tokio::test]
async fn a_later_item_failing_first_keeps_every_count() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", HANG), ("b", REFUSE), ("c", SERVE)],
        3,
    ));
    let (outcome, events) = run(
        &fan("max_parallel: 3", FREE, "    timeout: \"300ms\"\n"),
        &wires,
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(
        wires.bounded.dropped(),
        1,
        "only a was in flight at the end"
    );
    assert_eq!(
        ledger(&events),
        (Some(1), Some(2)),
        "{:?}",
        terminal(&events)
    );
    assert_eq!(
        statuses(&events),
        ["cancelled", "failed", "ok"],
        "a abandoned in flight, b's failure and c's completion both recorded"
    );
}

/// A paid route without an observer: a sibling dropped in flight used to
/// vanish from the ledger with whatever it cost. Now it is an unknown charge.
#[tokio::test]
async fn a_dropped_paid_sibling_is_an_unknown_charge_not_nothing() {
    let wires = Wires::unobserved(Held::new(
        false,
        &[("a", REFUSE), ("b", HANG), ("c", HANG)],
        3,
    ));
    let (outcome, events) = run(&fan("max_parallel: 3", PAID, ""), &wires).await;
    assert!(!outcome.ok);
    assert_eq!(wires.normal.posts().len(), 3);
    assert_eq!(wires.normal.dropped(), 2);
    assert_eq!(
        ledger(&events),
        (Some(0), Some(3)),
        "{:?}",
        terminal(&events)
    );
    assert_eq!(statuses(&events), ["failed", "cancelled", "cancelled"]);
}

/// B9 · the same paid fan-out under the reasoning seat's cap floor (64 <
/// 256) is the MODELS rung's refusal at admission: NIKA-1707 before the
/// prologue, zero frames, zero requests on either client, nothing sent on
/// the account.
#[tokio::test]
async fn a_reasoning_seat_under_its_cap_floor_is_refused_before_the_prologue() {
    let wires = Wires::unobserved(Held::new(
        false,
        &[("a", SERVE), ("b", SERVE), ("c", SERVE)],
        3,
    ));
    let source = fan("max_parallel: 3", PAID, "").replace("max_tokens: 256", "max_tokens: 64");
    let (settled, events) = launch(&source, &wires, None, None).await;
    let Some(Err(RuntimeError::ReportMismatch { detail })) = &settled else {
        panic!("the MODELS rung refuses at admission: {settled:?}");
    };
    assert!(
        detail.contains("too small for reasoning seat `deepseek/deepseek-v4-pro`"),
        "{detail}"
    );
    assert!(events.is_empty(), "no prologue, no frame");
    assert!(wires.normal.posts().is_empty() && wires.bounded.posts().is_empty());
    assert_eq!(sent_attempts(&wires.account), 0);
}

/// Mixed routes rendered per item: a paid completion keeps its exact known
/// price, a dropped free sibling and a refused one stay unknown. B10 · c's
/// refusal completes first and stops the batch at once, so b is dropped in
/// flight (`cancelled`) instead of waiting for its timeout; the counts do not
/// move.
#[tokio::test]
async fn mixed_routes_keep_the_known_subtotal_and_every_unknown() {
    let normal = Held::new(false, &[("a", SERVE)], 1);
    let bounded = Held::new(true, &[("b", HANG), ("c", REFUSE)], 2);
    let wires = Wires {
        normal,
        bounded,
        account: InferenceAdmission::observe_run(),
    };
    let source = format!(
        "nika: mixed\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: [{{n: 'a', m: '{PAID}'}}, {{n: 'b', m: '{FREE}'}}, {{n: 'c', m: '{FREE}'}}], max_parallel: 3 }}\n    timeout: \"300ms\"\n    infer: {{ model: '${{{{ item.m }}}}', prompt: 'say ${{{{ item.n }}}}', max_tokens: 256 }}\n"
    );
    let (outcome, events) = run(&source, &wires).await;
    assert!(!outcome.ok);
    assert_eq!(wires.normal.posts(), ["a"], "{events:#?}");
    assert_eq!(wires.bounded.posts().len(), 2);
    assert_eq!(
        ledger(&events),
        (Some(1), Some(2)),
        "{:?}",
        terminal(&events)
    );
    let spent = terminal(&events)
        .fields
        .iter()
        .find(|f| f.key == "total_cost_usd")
        .and_then(|f| match &f.value {
            FieldValue::Float(usd) => Some(*usd),
            _ => None,
        })
        .expect("the paid completion is priced");
    assert!(
        spent > 0.0,
        "the known subtotal is the paid call's own price"
    );
    assert_eq!(statuses(&events), ["ok", "cancelled", "failed"]);
}

/// A provider-level retry: HTTP 429, a bounded backoff, then a re-send that
/// hangs until the task's timeout drops it. Both physical requests count.
#[tokio::test]
async fn a_provider_retry_keeps_both_physical_requests() {
    let wires = Wires::unobserved(Held::new(
        false,
        &[("a", &[Answer::Limit, Answer::Hang])],
        1,
    ));
    let (outcome, events) = run(&single(PAID, "    timeout: \"2500ms\"\n"), &wires).await;
    assert!(!outcome.ok);
    assert_eq!(wires.normal.posts(), ["a", "a"], "the 429 and its re-send");
    assert_eq!(wires.normal.dropped(), 1, "the re-send was dropped");
    assert_eq!(
        ledger(&events),
        (Some(0), Some(2)),
        "{:?}",
        terminal(&events)
    );
}

/// An authored `retry:`: the first attempt's failure is folded when it
/// returns, the second is dropped by the timeout. Each attempt counts once.
#[tokio::test]
async fn an_authored_retry_counts_each_attempt_once() {
    let wires = Wires::unobserved(Held::new(
        false,
        &[("a", &[Answer::Refuse, Answer::Hang])],
        1,
    ));
    let (outcome, events) = run(
        &single(
            PAID,
            "    retry: { max_attempts: 2, backoff_ms: 10 }\n    timeout: \"1s\"\n",
        ),
        &wires,
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(wires.normal.posts(), ["a", "a"]);
    assert_eq!(wires.normal.dropped(), 1);
    assert_eq!(
        ledger(&events),
        (Some(0), Some(2)),
        "{:?}",
        terminal(&events)
    );
}

/// The caller drops the whole run mid-flight: there is no terminal frame to
/// read, nothing panics, and the shared account keeps every sent attempt as
/// an unknown charge (the run's own ledger ends with the run). B8: no item
/// row is written for the abandoned batch, neither `cancelled` nor
/// `never_started`: the frames stop where the run stopped.
#[tokio::test]
async fn a_dropped_run_leaves_its_sent_attempts_on_the_account() {
    let wires = Wires::observed(Held::new(true, &[("a", HANG), ("b", HANG), ("c", HANG)], 3));
    let (settled, events) = run_within(
        &fan("max_parallel: 3", FREE, ""),
        &wires,
        Some(std::time::Duration::from_millis(300)),
    )
    .await;
    assert!(settled.is_none(), "the caller gave up first");
    assert_eq!(wires.bounded.dropped(), 3);
    let receipt = wires.account.snapshot().expect("the account reads");
    assert_eq!(receipt.unknown_calls, 3, "{receipt:?}");
    assert_eq!(sent_attempts(&wires.account), 3);
    assert_eq!(wires.bounded.posts().len(), 3, "every item began");
    assert!(
        events.iter().any(|e| e.kind == EventKind::TaskScheduled),
        "the run was under way: {events:#?}"
    );
    assert!(
        events
            .iter()
            .all(|e| e.str_field("items").is_none() && !e.is_terminal()),
        "no fabricated item table and no terminal: {events:#?}"
    );
}

/// B8 acceptance · three items, `max_parallel: 2`. The wire holds a and b
/// until both arrived (the barrier), then a's connection fails while b hangs.
/// `fail_fast` abandons b in flight; c was never polled. The wire's own post
/// log is the callback: a and b began, c did not.
#[tokio::test]
async fn a_stopped_batch_tells_cancelled_from_never_started() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", REFUSE), ("b", HANG), ("c", SERVE)],
        2,
    ));
    let (outcome, events) = run(&fan("max_parallel: 2", FREE, ""), &wires).await;
    assert!(!outcome.ok, "item a failed: {outcome:?}");
    assert_eq!(
        wires.bounded.posts(),
        ["a", "b"],
        "c never reached the wire"
    );
    assert_eq!(wires.bounded.dropped(), 1, "b was abandoned in flight");
    assert_eq!(statuses(&events), ["failed", "cancelled", "never_started"]);
    assert_eq!(
        ledger(&events),
        (Some(0), Some(2)),
        "b's send stays an unknown charge"
    );
}

/// Control · every item completes: every row is `ok`, nothing is
/// cancelled or never started, and each settled send is priced once.
#[tokio::test]
async fn a_completed_batch_has_no_cancelled_row() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", SERVE), ("b", SERVE), ("c", SERVE)],
        2,
    ));
    let (outcome, events) = run(&fan("max_parallel: 2", FREE, ""), &wires).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(statuses(&events), ["ok", "ok", "ok"]);
    assert_eq!(ledger(&events), (Some(3), Some(0)));
}

/// Paging keeps the vocabulary: a stopped batch too large for one frame
/// pages its rows, and the terminal always counts `items_cancelled`.
#[tokio::test]
async fn a_paged_stopped_batch_counts_its_cancelled_rows() {
    let wires = Wires::observed(Held::new(true, &[("a", REFUSE), ("b", HANG)], 2));
    let tail: Vec<String> = (0..1500).map(|n| format!("'n{n:04}'")).collect();
    let source = format!(
        "nika: fan\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: ['a', 'b', {}], max_parallel: 2 }}\n    infer: {{ model: '{FREE}', prompt: 'say ${{{{ item }}}}', max_tokens: 64 }}\n",
        tail.join(", ")
    );
    let (outcome, events) = run(&source, &wires).await;
    assert!(!outcome.ok);
    assert_eq!(wires.bounded.posts(), ["a", "b"]);
    let pages: Vec<Vec<serde_json::Value>> = events
        .iter()
        .filter(|e| e.kind == EventKind::TaskItems)
        .map(|e| serde_json::from_str(e.str_field("items").expect("rows")).expect("json"))
        .collect();
    assert!(pages.len() > 1, "the table pages");
    let rows: Vec<&serde_json::Value> = pages.iter().flatten().collect();
    assert_eq!(rows.len(), 1502);
    assert_eq!(rows[0]["status"], "failed");
    assert_eq!(rows[1]["status"], "cancelled");
    assert!(rows[2..].iter().all(|r| r["status"] == "never_started"));
    let done = events
        .iter()
        .find(|e| e.kind == EventKind::TaskFailed)
        .expect("the fan-out terminal");
    for (key, value) in [
        ("items_total", 1502),
        ("items_ok", 0),
        ("items_recovered", 0),
        ("items_failed", 1),
        ("items_cancelled", 1),
        ("items_never_started", 1500),
    ] {
        assert_eq!(done.int_field(key), Some(value), "{key}");
    }
    assert!(done.str_field("items").is_none(), "paged, not inline");
}

// ─── B9 phase C · the pre-send guard over siblings and retries ──────────────

/// The one-call floor launch prices for `PAID` at the fixtures' cap.
fn paid_floor() -> f64 {
    let wf = nika_schema::parse(
        &single(PAID, ""),
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    nika_check::check(&wf).cost.min_path_total_usd
}

/// Two paid items whose seat the item decides, run `parallel` at a time.
fn siblings(parallel: u32, first: &str, second: &str) -> String {
    format!(
        "nika: siblings\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: [{{n: '{first}', m: '{PAID}'}}, {{n: '{second}', m: '{PAID}'}}], max_parallel: {parallel}, fail_fast: false }}\n    infer: {{ model: '${{{{ item.m }}}}', prompt: 'say ${{{{ item.n }}}}', max_tokens: 256 }}\n"
    )
}

/// B9 phase C · the guard reads the ledger's snapshot when a sibling starts,
/// and reserves nothing. The cap holds one call's floor plus half of one
/// served request's KNOWN price. Started together (both held in flight),
/// each guard reads the same snapshot and both requests are sent. Queued,
/// the second reads the snapshot after the first's known price and is
/// refused before its request, whichever item goes first.
#[tokio::test]
async fn siblings_read_the_snapshot_they_start_with_and_reserve_nothing() {
    let price = {
        let wires = Wires::unobserved(Held::new(false, &[("a", SERVE)], 1));
        let (outcome, _) = run(&single(PAID, ""), &wires).await;
        outcome
            .total_cost_usd
            .expect("a served paid request is priced")
    };
    let cap = paid_floor() + price / 2.0;
    let wires = Wires::unobserved(Held::new(false, &[("a", SERVE), ("b", SERVE)], 2));
    let deadline = Some(std::time::Duration::from_secs(10));
    let (settled, _) = launch(&siblings(2, "a", "b"), &wires, deadline, Some(cap)).await;
    let outcome = settled
        .expect("both were sent, so the held pair released")
        .expect("the run settles");
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(
        wires.normal.posts().len(),
        2,
        "no reservation between siblings"
    );
    for (first, second) in [("a", "b"), ("b", "a")] {
        let wires = Wires::unobserved(Held::new(false, &[("a", SERVE), ("b", SERVE)], 1));
        let (settled, events) = launch(&siblings(1, first, second), &wires, None, Some(cap)).await;
        let outcome = settled.expect("no deadline").expect("the run settles");
        assert_eq!(wires.normal.posts(), [first], "queued after {first}");
        assert_eq!(statuses(&events), ["ok", "failed"]);
        let error = outcome.records["ask"]
            .error
            .as_ref()
            .expect("the refused sibling");
        assert_eq!(error.code, "NIKA-1704", "{}", error.message);
    }
}

/// A provider-level retry on a route only the run decides: the guard judges
/// the dispatch ONCE, and both physical requests (the 429 and its served
/// re-send) are on the ledger, as for a literal seat.
#[tokio::test]
async fn a_provider_retry_after_the_guard_keeps_both_physical_requests() {
    let wires = Wires::unobserved(Held::new(
        false,
        &[("a", &[Answer::Limit, Answer::Serve])],
        1,
    ));
    let source = format!(
        "nika: one\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: [{{n: 'a', m: '{PAID}'}}] }}\n    infer: {{ model: '${{{{ item.m }}}}', prompt: 'say ${{{{ item.n }}}}', max_tokens: 256 }}\n"
    );
    let (settled, events) = launch(&source, &wires, None, Some(paid_floor() * 1.5)).await;
    let outcome = settled.expect("no deadline").expect("the run settles");
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wires.normal.posts(), ["a", "a"], "the 429 and its re-send");
    assert_eq!(
        ledger(&events),
        (Some(1), Some(1)),
        "{:?}",
        terminal(&events)
    );
}

/// A request dropped in flight is an unknown charge the observed account
/// keeps (B7); the account then refuses a later route only the run decides
/// before any byte, whatever the guard's snapshot says.
#[tokio::test]
async fn a_dropped_request_closes_the_account_for_a_later_run_decided_route() {
    let source = format!(
        "nika: closure\npermits: {{}}\ntasks:\n  first:\n    timeout: \"300ms\"\n    infer: {{ model: '{FREE}', prompt: 'say a', max_tokens: 64 }}\n  ask:\n    after: {{ first: failure }}\n    for_each: {{ items: [{{n: 'b', m: '{FREE}'}}] }}\n    infer: {{ model: '${{{{ item.m }}}}', prompt: 'say ${{{{ item.n }}}}', max_tokens: 64 }}\n"
    );
    let wires = Wires::observed(Held::new(true, &[("a", HANG), ("b", SERVE)], 1));
    let (outcome, _) = run(&source, &wires).await;
    assert_eq!(
        wires.bounded.posts(),
        ["a"],
        "the account closed on the unknown charge"
    );
    assert_eq!(wires.bounded.dropped(), 1);
    let receipt = wires.account.snapshot().expect("the account reads");
    assert_eq!(receipt.unknown_calls, 1, "{receipt:?}");
    assert!(outcome.records["ask"].error.is_some(), "{outcome:?}");
}

/// E33 · E32 finding 1: a fan-out's calls used to reach no frame. The parent
/// now carries every completed iteration's call records once, in input order
/// whatever order they completed in (a answers last), and invents no meter,
/// transport or single-route field of its own.
#[tokio::test]
async fn a_fan_out_parent_records_every_completed_call_once_in_input_order() {
    let wires = Wires::unobserved(
        Held::new(false, &[("a", SERVE), ("b", SERVE), ("c", SERVE)], 3).answering_last("a"),
    );
    let (outcome, events) = run(&fan("max_parallel: 3", PAID, ""), &wires).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wires.normal.posts().len(), 3);
    let (calls, unknown) = parent_calls(&events);
    let ids: Vec<Option<&str>> = calls.iter().map(|c| c["request_id"].as_str()).collect();
    assert_eq!(
        ids,
        [
            Some("fan-fixture-a"),
            Some("fan-fixture-b"),
            Some("fan-fixture-c")
        ],
        "{calls:?}"
    );
    assert_eq!(unknown, Some(0), "every paid call has its estimate");
    assert_eq!(ledger(&events), (Some(3), Some(0)));
    let parent = events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("the parent frame");
    for key in [
        "tokens_in",
        "tokens_out",
        "attempts",
        "model_served",
        "response_id",
        "pricing_route",
    ] {
        assert!(
            parent.fields.iter().all(|f| f.key != key),
            "{key} is not invented on the parent"
        );
    }
}

/// A provider retry inside a fan-out: the 429 and its re-send are two physical
/// requests, and the parent keeps both beside b's and c's. The 429's unknown
/// charge is the parent's one unknown call, as it is the ledger's.
#[tokio::test]
async fn a_provider_retry_inside_a_fan_out_keeps_both_requests_on_the_parent() {
    let wires = Wires::unobserved(Held::new(
        false,
        &[
            ("a", &[Answer::Limit, Answer::Serve]),
            ("b", SERVE),
            ("c", SERVE),
        ],
        1,
    ));
    let (outcome, events) = run(&fan("max_parallel: 1", PAID, ""), &wires).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(wires.normal.posts(), ["a", "a", "b", "c"]);
    let (calls, unknown) = parent_calls(&events);
    assert_eq!(calls.len(), 4, "{calls:?}");
    assert_eq!(unknown, Some(1), "{calls:?}");
    assert_eq!(
        ledger(&events),
        (Some(3), Some(1)),
        "{:?}",
        terminal(&events)
    );
}

/// Without `fail_fast` every item settles: a's refused request is recorded once
/// beside b's and c's completions, and the parent's unknown count is a's.
#[tokio::test]
async fn a_refused_item_keeps_its_call_beside_the_served_ones_on_the_parent() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", REFUSE), ("b", SERVE), ("c", SERVE)],
        3,
    ));
    let (outcome, events) = run(&fan("max_parallel: 3, fail_fast: false", FREE, ""), &wires).await;
    assert!(!outcome.ok, "item a still fails the task");
    let (calls, unknown) = parent_calls(&events);
    let ids: Vec<Option<&str>> = calls.iter().map(|c| c["request_id"].as_str()).collect();
    assert_eq!(
        ids,
        [None, Some("fan-fixture-b"), Some("fan-fixture-c")],
        "{calls:?}"
    );
    assert_eq!(unknown, Some(1));
    assert_eq!(ledger(&events), (Some(2), Some(1)));
}

/// `fail_fast` drops b and c in flight: their sends stay on the ledger and the
/// account (E17-F1), but no transport report ever returns for them, so the
/// parent records a's call only and invents none for b or c.
#[tokio::test]
async fn a_sibling_dropped_in_flight_leaves_no_invented_call_on_the_parent() {
    let wires = Wires::observed(Held::new(
        true,
        &[("a", REFUSE), ("b", HANG), ("c", HANG)],
        3,
    ));
    let (outcome, events) = run(&fan("max_parallel: 3", FREE, ""), &wires).await;
    assert!(!outcome.ok);
    assert_eq!(
        (wires.bounded.posts().len(), wires.bounded.dropped()),
        (3, 2)
    );
    assert_eq!(ledger(&events), (Some(0), Some(3)));
    assert_eq!(sent_attempts(&wires.account), 3);
    let (calls, unknown) = parent_calls(&events);
    assert_eq!(calls.len(), 1, "a's call only: {calls:?}");
    assert_eq!(unknown, Some(1));
}

/// The serial control: the same three prompts as three tasks record the same
/// call records, byte for byte, as the fan-out's parent does.
#[tokio::test]
async fn three_serial_tasks_record_the_calls_the_fan_out_parent_records() {
    let serial = format!(
        "nika: serial\npermits: {{}}\ntasks:\n  a:\n    infer: {{ model: '{PAID}', prompt: 'say a', max_tokens: 256 }}\n  b:\n    after: {{ a: success }}\n    infer: {{ model: '{PAID}', prompt: 'say b', max_tokens: 256 }}\n  c:\n    after: {{ b: success }}\n    infer: {{ model: '{PAID}', prompt: 'say c', max_tokens: 256 }}\n"
    );
    let script: &[(&'static str, &[Answer])] = &[("a", SERVE), ("b", SERVE), ("c", SERVE)];
    let wires = Wires::unobserved(Held::new(false, script, 1));
    let (outcome, events) = run(&serial, &wires).await;
    assert!(outcome.ok, "{outcome:?}");
    let serial_calls: Vec<serde_json::Value> = task_fields(&events, "inference_calls")
        .into_iter()
        .flat_map(|text| serde_json::from_str::<Vec<serde_json::Value>>(text).expect("calls"))
        .collect();
    let fanned = Wires::unobserved(Held::new(false, script, 1));
    let (outcome, events) = run(&fan("max_parallel: 1", PAID, ""), &fanned).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(serial_calls.len(), 3);
    assert_eq!(parent_calls(&events).0, serial_calls);
    assert_eq!(wires.normal.posts(), fanned.normal.posts());
}

/// A fan-out whose sends never return a transport report writes no call
/// evidence and invents none (the fake-zero law): a's attempt times out and b
/// and c are dropped in flight, while the ledger keeps all three sends as
/// unknown charges. An empty collection is skipped before any iteration and
/// writes none either.
#[tokio::test]
async fn a_fan_out_whose_calls_never_returned_writes_no_call_evidence() {
    let wires = Wires::observed(Held::new(true, &[("a", HANG), ("b", HANG), ("c", HANG)], 3));
    let (outcome, events) = run(
        &fan("max_parallel: 3", FREE, "    timeout: \"300ms\"\n"),
        &wires,
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(
        (wires.bounded.posts().len(), wires.bounded.dropped()),
        (3, 3)
    );
    assert_eq!(ledger(&events), (Some(0), Some(3)));
    assert_eq!(parent_calls(&events), (Vec::new(), None));
    let empty = format!(
        "nika: empty\npermits: {{}}\ntasks:\n  ask:\n    for_each: {{ items: [] }}\n    infer: {{ model: '{PAID}', prompt: 'say ${{{{ item }}}}', max_tokens: 256 }}\n"
    );
    let quiet = Wires::unobserved(Held::new(false, &[], 1));
    let (outcome, events) = run(&empty, &quiet).await;
    assert!(outcome.ok, "{outcome:?}");
    assert!(quiet.normal.posts().is_empty());
    for key in ["inference_calls", "cost_unknown_calls"] {
        assert!(
            events.iter().all(|e| e.fields.iter().all(|f| f.key != key)),
            "{key} rode a frame: {events:?}"
        );
    }
}
