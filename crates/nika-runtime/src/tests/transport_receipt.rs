// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The transport's account of a metered call on the sealed trace: one round-trip per call,
//! never re-sent by the transport (the money admission's contract). A seat answering 429 ends
//! the call at its first answer, with nothing waited and nothing re-sent (the author's `retry:`
//! decides), and a first-time answer stamps `attempts: 1` on `task_completed`.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nika_kernel::secret::Secret;
use nika_kernel_mock::{
    MockHttp, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_schema::types::{RunClock, RunDecl};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_invoke::InvokeVerb;

use super::*;

/// The measured rate-limit body (an openai-compat 429 · transient).
const RATE_LIMITED: &str =
    r#"{"error":{"code":"rate_limit_exceeded","type":"requests","message":"slow down"}}"#;

/// A minimal openai-compat success with a usage block (the priced
/// seat's meter — `refuse_unusable_response` fails closed without it).
const OPENAI_OK: &str = r#"{"id":"cc","model":"gpt-4o-mini-2024-07-18",
    "choices":[{"message":{"content":"ok"},"finish_reason":"stop"}],
    "usage":{"prompt_tokens":7,"completion_tokens":3}}"#;

/// One infer on a keyed cloud seat under the virtual clock.
const VIRTUAL_CLOCK_INFER: &str = "nika: transport-receipt\n\
     model: openai/gpt-4o-mini\n\
     run: { clock: virtual }\n\
     permits: {}\n\
     tasks:\n  \
     ask:\n    \
     infer: { prompt: \"hello\", max_tokens: 16 }\n";

/// Run the fixture over a canned wire, the runtime's clock taken from the SAME declared-clock
/// seams the composition root uses (`RunSeams::clock`).
async fn run_over(http: MockHttp) -> (RunOutcome, Vec<Event>) {
    let wf = nika_schema::parse(
        VIRTUAL_CLOCK_INFER,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(report.is_clean(), "fixture passes the ladder: {report:?}");
    let seams = crate::compose::RunSeams::of(Some(&RunDecl::new(None, Some(RunClock::Virtual))));
    assert!(seams.clock.as_virtual().is_some(), "the fixture's clock");
    let registry = Arc::new(ProviderRegistry::new(
        Arc::new(http),
        ProvidersConfig::new().with_key("openai", Secret::new("sk-test")),
    ));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        nika_verb_infer::InferVerb::new(registry, "openai/gpt-4o-mini"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::new(MockToolDefinitionProvider::new()),
            "mock/echo",
        ),
        seams.clock.clone(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("the run completes");
    (outcome, sink.into_events())
}

fn completed(events: &[Event]) -> &Event {
    events
        .iter()
        .find(|e| e.kind == EventKind::TaskCompleted)
        .expect("a TaskCompleted frame")
}

fn int(frame: &Event, key: &str) -> Option<i64> {
    frame
        .fields
        .iter()
        .find(|f| f.key == key)
        .map(|f| match &f.value {
            FieldValue::Int(n) => *n,
            other => panic!("{key} is not an int: {other:?}"),
        })
}

fn text<'e>(frame: &'e Event, key: &str) -> Option<&'e str> {
    frame
        .fields
        .iter()
        .find(|f| f.key == key)
        .map(|f| match &f.value {
            FieldValue::String(s) => s.as_str(),
            other => panic!("{key} is not a string: {other:?}"),
        })
}

/// A 429 with `Retry-After: 2`, a 200 queued behind it: the call ends at its first answer.
/// The wire saw ONE request (nothing re-sent), nothing was waited, and the task fails with
/// the transient rejection instead of settling green on a request the admission never counted.
#[tokio::test]
async fn a_rate_limited_call_ends_at_its_first_answer_and_nothing_is_re_sent() {
    let http = MockHttp::new()
        .enqueue_ok_with_headers(429, [("Retry-After", "2")], RATE_LIMITED)
        .enqueue_ok(200, OPENAI_OK);
    let sent = http.clone();
    let start = Instant::now(); // seam-bypass-ok: test-only wall-clock measure proving nothing was waited
    let (outcome, events) = run_over(http).await;
    let wall = start.elapsed();
    assert!(!outcome.ok, "the rejected call fails its task: {outcome:?}");
    assert_eq!(sent.sent_requests().len(), 1, "nothing re-sent");
    assert!(
        wall < Duration::from_secs(1),
        "nothing waited (took {wall:?})"
    );
    assert!(
        events.iter().any(|e| e.kind == EventKind::TaskFailed),
        "{events:?}"
    );
    assert!(!events.iter().any(|e| e.kind == EventKind::TaskCompleted));
}

/// The common case — a seat that answers first time — reads
/// `attempts: 1` and no wait fields: a reader greps `retried_on` for
/// exactly the retried calls, and `attempts` is never absent on a wire
/// call (absent would read as « the verb reported no transport »).
#[tokio::test]
async fn a_first_time_answer_reads_one_attempt_and_no_wait() {
    let http = MockHttp::new().enqueue_ok(200, OPENAI_OK);
    let (outcome, events) = run_over(http).await;
    assert!(outcome.ok, "{outcome:?}");
    let frame = completed(&events);
    assert_eq!(int(frame, "attempts"), Some(1), "{:?}", frame.fields);
    assert_eq!(int(frame, "waited_ms"), None, "{:?}", frame.fields);
    assert_eq!(text(frame, "retried_on"), None, "{:?}", frame.fields);
}
