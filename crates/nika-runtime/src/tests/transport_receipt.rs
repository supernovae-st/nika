// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The transport's account of a metered call on the sealed trace (the
//! product-convergence war room · L4): a seat that answered only after
//! the provider layer's bounded backoff stamps `attempts` · `waited_ms`
//! · `retried_on` on `task_completed`, and the wait rides the run's
//! declared clock — a `run: { clock: virtual }` run waits zero wall
//! seconds on a `Retry-After: 2`.

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

/// Run the fixture over a canned wire, the registry's backoff and the
/// runtime's clock both taken from the SAME declared-clock seams the
/// composition root uses (`RunSeams::backoff` · `RunSeams::clock`).
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
    let registry = Arc::new(
        ProviderRegistry::new(
            Arc::new(http),
            ProvidersConfig::new().with_key("openai", Secret::new("sk-test")),
        )
        .with_backoff(seams.backoff()),
    );
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

/// A 429 with `Retry-After: 2` then a 200: the task settles green, the
/// frame says two round-trips, the 2000 ms the seat asked for, and the
/// status waited on — and the whole run takes well under the 2 s a
/// real sleep would cost, because the backoff rode the virtual clock.
#[tokio::test]
async fn a_retried_call_stamps_its_transport_on_the_frame_without_sleeping() {
    let http = MockHttp::new()
        .enqueue_ok_with_headers(429, [("Retry-After", "2")], RATE_LIMITED)
        .enqueue_ok(200, OPENAI_OK);
    let sent = http.clone();
    let start = Instant::now();
    let (outcome, events) = run_over(http).await;
    let wall = start.elapsed();
    assert!(outcome.ok, "the retried seat settles green: {outcome:?}");
    assert_eq!(
        sent.sent_requests().len(),
        2,
        "one re-send of the same call"
    );
    assert!(
        wall < Duration::from_secs(1),
        "the virtual clock never slept the 2 s Retry-After (took {wall:?})"
    );

    let frame = completed(&events);
    assert_eq!(int(frame, "attempts"), Some(2), "{:?}", frame.fields);
    assert_eq!(int(frame, "waited_ms"), Some(2000), "{:?}", frame.fields);
    assert_eq!(text(frame, "retried_on"), Some("429"), "{:?}", frame.fields);
    // The meters still ride beside the transport facts.
    assert_eq!(int(frame, "tokens_in"), Some(7));
    assert_eq!(int(frame, "tokens_out"), Some(3));
    assert_eq!(text(frame, "model_served"), Some("gpt-4o-mini-2024-07-18"));
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
