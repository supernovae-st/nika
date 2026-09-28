// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

//! B10 · spec 03 immediate fail-fast, in COMPLETION order. Under `fail_fast`
//! the first iteration failure to complete stops the fan at once, whatever
//! its index: a completed failure or success keeps its own row, an item that
//! began without one is `cancelled`, an item never polled is `never_started`,
//! and successful outputs stay in input order.
//!
//! The tool below is scripted by the item token and logs what it began,
//! finished and saw dropped, so each assertion reads what actually ran. No
//! sleep decides a result: `hold*` ends only when the runtime drops it,
//! `<x>@<y>` answers once item `<y>` has finished, `<x>&<n>` once `n` calls
//! have begun, `fail*` fails and anything else answers `answer <token>`. A
//! run that waited on a dropped item would exceed its 10 s bound and fail.

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nika_event::EventKind;
use nika_kernel::tool_executor::{ToolCall, ToolExecError, ToolExecuteDyn, ToolResult};
use nika_kernel_mock::{MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, VecSink};
use nika_types::resource::Value as FieldValue;
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::Value;

#[derive(Default)]
struct Scripted {
    began: Mutex<Vec<String>>,
    finished: Mutex<Vec<String>>,
    dropped: AtomicUsize,
}

/// Counts a call whose future the runtime dropped before it answered.
struct Live<'a>(&'a Scripted, bool);

impl Drop for Live<'_> {
    fn drop(&mut self) {
        if !self.1 {
            self.0.dropped.fetch_add(1, SeqCst);
        }
    }
}

impl Scripted {
    fn began(&self) -> BTreeSet<String> {
        self.began.lock().expect("began").iter().cloned().collect()
    }

    /// Wait on an EVENT, never on a duration deciding the result: the 1 ms
    /// polls are only how a lock-based fixture observes it (5 s ceiling).
    async fn until(&self, ready: impl Fn(&Self) -> bool, what: &str) {
        for _ in 0..5_000 {
            if ready(self) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
        panic!("the fixture starved waiting for {what}");
    }
}

impl ToolExecuteDyn for Scripted {
    async fn execute(&self, call: ToolCall) -> Result<ToolResult, ToolExecError> {
        let token = call.input["expression"].as_str().unwrap_or("?").to_owned();
        self.began.lock().expect("began").push(token.clone());
        let mut live = Live(self, false);
        if token.starts_with("hold") {
            std::future::pending::<()>().await;
        }
        if let Some((_, after)) = token.split_once('@') {
            let seen = |tool: &Self| {
                tool.finished
                    .lock()
                    .expect("done")
                    .iter()
                    .any(|t| t == after)
            };
            self.until(seen, after).await;
        }
        if let Some((_, n)) = token.split_once('&') {
            let n: usize = n.parse().expect("a count");
            let what = format!("{n} calls to begin");
            self.until(|tool| tool.began.lock().expect("began").len() >= n, &what)
                .await;
        }
        live.1 = true;
        self.finished.lock().expect("done").push(token.clone());
        if token.starts_with("fail") {
            return Err(ToolExecError::ExecutionFailed {
                name: "nika:jq".into(),
                reason: format!("fixture failure {token}"),
            });
        }
        Ok(ToolResult::success(
            call.id.as_str(),
            format!("answer {token}"),
        ))
    }
}

/// A fan over `items` of the scripted tool; `fan` completes `for_each:`,
/// `task` adds task-level fields.
fn fan(items: &[&str], fan: &str, task: &str) -> String {
    let items: Vec<String> = items.iter().map(|i| format!("\"{i}\"")).collect();
    format!(
        "nika: fan\npermits: {{ tools: [\"nika:jq\"] }}\ntasks:\n  fan:\n    for_each: {{ items: [{}], {fan} }}\n{task}    invoke: {{ tool: \"nika:jq\", args: {{ input: {{}}, expression: \"${{{{ item }}}}\" }} }}\noutputs:\n  answers: ${{{{ tasks.fan.output }}}}\n",
        items.join(", ")
    )
}

async fn run(source: &str, tool: &Arc<Scripted>) -> (RunOutcome, VecSink) {
    let wf = nika_schema::parse(
        source,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(tool)));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(
            Arc::new(ProviderRegistry::without_http(ProvidersConfig::default())),
            "mock/echo",
        ),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        nika_clock::DeclaredClock::system(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let running = runtime.run(&wf, &report, &mut stamper, &mut sink);
    let outcome = tokio::time::timeout(Duration::from_secs(10), running)
        .await
        .expect("the fan settles: nothing waits on an item the stop dropped")
        .expect("the run settles");
    (outcome, sink)
}

/// The fan's item table (inline, or its pages in order).
fn rows(sink: &VecSink) -> Vec<Value> {
    let text = |value: &FieldValue| match value {
        FieldValue::String(s) => s.clone(),
        other => panic!("items is a JSON text, got {other:?}"),
    };
    let mut rows = Vec::new();
    for event in sink.events() {
        let terminal = matches!(event.kind, EventKind::TaskFailed | EventKind::TaskCompleted);
        if (event.kind == EventKind::TaskItems || terminal)
            && let Some(field) = event.fields.iter().find(|f| f.key == "items")
        {
            let page: Vec<Value> = serde_json::from_str(&text(&field.value)).expect("rows");
            rows.extend(page);
        }
    }
    rows
}

fn statuses(sink: &VecSink) -> Vec<String> {
    rows(sink)
        .iter()
        .map(|row| row["status"].as_str().expect("a status").to_owned())
        .collect()
}

fn terminal(sink: &VecSink) -> nika_event::Event {
    sink.events()
        .iter()
        .find(|e| matches!(e.kind, EventKind::TaskFailed | EventKind::TaskCompleted))
        .cloned()
        .expect("the fan's terminal frame")
}

/// Every row agrees with the tool's own log: an item the tool never saw
/// begin is `never_started`, one it saw begin is `failed`, `ok` or
/// `cancelled`, and a dropped call never reads as finished.
fn lawful(sink: &VecSink, tool: &Scripted) {
    let began = tool.began();
    for row in rows(sink) {
        let item = row["item"].as_str().expect("an item");
        let status = row["status"].as_str().expect("a status");
        if began.contains(item) {
            assert!(["failed", "ok", "cancelled"].contains(&status), "{row}");
        } else {
            assert_eq!(status, "never_started", "{row}");
        }
    }
}

/// D6: `max_parallel: 2`, the first item held, the second failing at once,
/// the third queued. The failure stops the fan at once: the held item is
/// dropped in flight (`cancelled`), the failure keeps its row and is the
/// parent error, the queued item never starts. Before B10 the collector
/// waited for the held item in input order and this run never settled.
#[tokio::test]
async fn a_later_failure_that_completes_first_stops_the_fan() {
    let tool = Arc::new(Scripted::default());
    let (outcome, sink) = run(&fan(&["hold", "fail", "ok"], "max_parallel: 2", ""), &tool).await;
    assert!(!outcome.ok);
    assert_eq!(statuses(&sink), ["cancelled", "failed", "never_started"]);
    assert_eq!(tool.began(), BTreeSet::from(["hold".into(), "fail".into()]));
    assert_eq!(tool.dropped.load(SeqCst), 1, "the held call was dropped");
    let error = outcome.records["fan"]
        .error
        .as_ref()
        .expect("the parent error");
    assert!(error.message.contains("[1] fail"), "{}", error.message);
    assert_eq!(rows(&sink)[1]["code"], error.code.as_str());
    lawful(&sink, &tool);
}

/// Reversed: the same shape with the held item last. The failure still
/// stops the fan; the answer that completed before it keeps its `ok` row.
#[tokio::test]
async fn the_reversed_order_stops_on_the_same_failure() {
    let tool = Arc::new(Scripted::default());
    let source = fan(&["ok", "fail&2", "hold"], "max_parallel: 2", "");
    let (outcome, sink) = run(&source, &tool).await;
    assert!(!outcome.ok);
    let words = statuses(&sink);
    assert_eq!(words[..2], ["ok", "failed"], "{words:?}");
    lawful(&sink, &tool);
}

/// A first-index failure stops the fan the same way: every row agrees with
/// what the tool saw begin.
#[tokio::test]
async fn a_first_index_failure_stops_the_fan() {
    let tool = Arc::new(Scripted::default());
    let (outcome, sink) = run(&fan(&["fail", "hold", "ok"], "max_parallel: 2", ""), &tool).await;
    assert!(!outcome.ok);
    assert_eq!(statuses(&sink)[0], "failed");
    assert_eq!(statuses(&sink)[2], "never_started");
    lawful(&sink, &tool);
}

/// Two failures completing in the same instant: which one the collector
/// meets first is not a contract, so the assertion is the lawful one. At
/// least one is recorded `failed`, each is `failed` or `cancelled`, the
/// queued item never starts, and the parent error is a recorded failure.
#[tokio::test]
async fn simultaneous_failures_keep_a_lawful_record() {
    let tool = Arc::new(Scripted::default());
    let source = fan(&["failA&2", "failB&2", "ok"], "max_parallel: 2", "");
    let (outcome, sink) = run(&source, &tool).await;
    assert!(!outcome.ok);
    let words = statuses(&sink);
    assert!(
        words[..2].iter().all(|w| w == "failed" || w == "cancelled"),
        "{words:?}"
    );
    assert!(words[..2].iter().any(|w| w == "failed"), "{words:?}");
    assert_eq!(words[2], "never_started");
    let error = outcome.records["fan"]
        .error
        .as_ref()
        .expect("the parent error");
    let failed: Vec<Value> = rows(&sink)
        .into_iter()
        .filter(|r| r["status"] == "failed")
        .collect();
    assert!(
        failed
            .iter()
            .any(|r| r["message"] == error.message.as_str()),
        "{error:?}"
    );
    lawful(&sink, &tool);
}

/// Successful outputs stay in input order whatever order they complete in:
/// here the last item finishes first and the first finishes last.
#[tokio::test]
async fn outputs_keep_input_order_whatever_completes_first() {
    let tool = Arc::new(Scripted::default());
    let source = fan(&["a@b@c", "b@c", "c"], "max_parallel: 3", "");
    let (outcome, sink) = run(&source, &tool).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(
        *tool.finished.lock().expect("done"),
        ["c", "b@c", "a@b@c"],
        "the completion order is the reverse of the input"
    );
    let answers = outcome.records["fan"].output.clone();
    assert_eq!(
        answers,
        serde_json::json!(["answer a@b@c", "answer b@c", "answer c"])
    );
    assert_eq!(statuses(&sink), ["ok", "ok", "ok"]);
}

/// A success that completed before a slower sibling's timeout keeps its
/// row: the per-iteration `timeout:` fails the held item, and the answers
/// already recorded are never relabelled `cancelled`.
#[tokio::test]
async fn a_completed_success_survives_a_slower_timeout() {
    let tool = Arc::new(Scripted::default());
    let source = fan(
        &["hold", "ok", "okb"],
        "max_parallel: 2",
        "    timeout: \"300ms\"\n",
    );
    let (outcome, sink) = run(&source, &tool).await;
    assert!(!outcome.ok);
    assert_eq!(statuses(&sink), ["failed", "ok", "ok"]);
    lawful(&sink, &tool);
}

/// In flight at the stop reads `cancelled`, queued reads `never_started`.
#[tokio::test]
async fn in_flight_items_are_cancelled_and_queued_ones_never_start() {
    let tool = Arc::new(Scripted::default());
    let source = fan(&["hold", "fail&2", "ok", "okb"], "max_parallel: 2", "");
    let (outcome, sink) = run(&source, &tool).await;
    assert!(!outcome.ok);
    assert_eq!(
        statuses(&sink),
        ["cancelled", "failed", "never_started", "never_started"]
    );
    lawful(&sink, &tool);
}

/// Positive control · `fail_fast: false` runs every item, keeps each
/// failure's row, and never cancels a sibling.
#[tokio::test]
async fn keep_going_runs_every_item() {
    let tool = Arc::new(Scripted::default());
    let source = fan(
        &["fail", "ok@fail", "okb"],
        "max_parallel: 2, fail_fast: false",
        "",
    );
    let (outcome, sink) = run(&source, &tool).await;
    assert!(!outcome.ok, "a failure still fails the fan");
    assert_eq!(statuses(&sink), ["failed", "ok", "ok"]);
    assert_eq!(tool.dropped.load(SeqCst), 0, "nothing was cancelled");
}

/// Positive control · an item recovered by `on_error:` is not a failure:
/// the fan stays green with the fallback at the item's index.
#[tokio::test]
async fn a_recovered_item_does_not_stop_the_fan() {
    let tool = Arc::new(Scripted::default());
    let source = fan(
        &["fail", "ok"],
        "max_parallel: 2",
        "    on_error: { recover: \"fallback\" }\n",
    );
    let (outcome, sink) = run(&source, &tool).await;
    assert!(outcome.ok, "{outcome:?}");
    assert_eq!(statuses(&sink), ["recovered", "ok"]);
    assert_eq!(
        outcome.records["fan"].output,
        serde_json::json!(["fallback", "answer ok"])
    );
}

/// `max_parallel: 1` runs in order and stops at the failure: the item
/// after it never starts, and nothing before it is relabelled.
#[tokio::test]
async fn max_parallel_one_stops_at_the_failure() {
    let tool = Arc::new(Scripted::default());
    let (outcome, sink) = run(&fan(&["ok", "fail", "okb"], "max_parallel: 1", ""), &tool).await;
    assert!(!outcome.ok);
    assert_eq!(statuses(&sink), ["ok", "failed", "never_started"]);
    assert_eq!(*tool.began.lock().expect("began"), ["ok", "fail"]);
}

/// More than 1500 items: the table pages, and a later failure that
/// completes first still stops the batch, with every row accounted for.
#[tokio::test]
async fn a_paged_batch_stopped_by_a_later_failure_accounts_for_every_row() {
    let tool = Arc::new(Scripted::default());
    let tail: Vec<String> = (2..1600).map(|n| format!("ok-{n:04}")).collect();
    let mut items = vec!["hold", "fail&2"];
    items.extend(tail.iter().map(String::as_str));
    let (outcome, sink) = run(&fan(&items, "max_parallel: 2", ""), &tool).await;
    assert!(!outcome.ok);
    let pages = sink
        .events()
        .iter()
        .filter(|e| e.kind == EventKind::TaskItems)
        .count();
    assert!(pages > 1, "the table pages");
    let words = statuses(&sink);
    assert_eq!(words.len(), 1600);
    assert_eq!(words[..2], ["cancelled", "failed"]);
    assert!(words[2..].iter().all(|w| w == "never_started"));
    let done = terminal(&sink);
    for (key, value) in [
        ("items_total", 1600),
        ("items_ok", 0),
        ("items_failed", 1),
        ("items_cancelled", 1),
        ("items_never_started", 1598),
    ] {
        assert_eq!(done.int_field(key), Some(value), "{key}");
    }
    lawful(&sink, &tool);
}
