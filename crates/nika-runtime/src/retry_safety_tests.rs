// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect-safe retry law (#1371) end-to-end, through the REAL attempt
//! loop over the real `BuiltinDispatcher` (mock http seam · zero I/O):
//!
//! - a declared `retry:` on a keyless mutating `nika:fetch` never reaches
//!   the loop — `NIKA-SEC-016` dirties the report and the admission trust
//!   gate refuses the run BEFORE any socket (zero requests sent);
//! - the runtime belt for an embedder past the authored-file gate: a
//!   keyless POST's failure lands `transient: false` in the error record
//!   (one attempt — the very flag the retry loop's eligibility reads);
//! - with an `idempotency-key` header the declared retry fires (the
//!   receiver dedups the replay);
//! - GET retry behavior is unchanged (read-only methods retry free).

#![allow(clippy::expect_used, clippy::panic)]

use super::*;
use nika_kernel_mock::{MockFs, MockHttp, MockProvider, MockShell};

type FetchDispatcher = nika_builtin::BuiltinDispatcher<
    MockFs,
    MockHttp,
    nika_kernel_mock::MockClock,
    nika_builtin::NullEmitter,
    nika_builtin::NonInteractive,
    nika_builtin::NoWorkflow,
>;

fn dispatcher(http: &MockHttp) -> Arc<FetchDispatcher> {
    Arc::new(nika_builtin::BuiltinDispatcher::new(
        Arc::new(MockFs::new()),
        Arc::new(http.clone()),
        Arc::new(nika_kernel_mock::MockClock::new()),
        Arc::new(nika_builtin::NullEmitter::default()),
        Arc::new(nika_builtin::NonInteractive::default()),
        Arc::new(nika_builtin::NoWorkflow::default()),
    ))
}

/// Settle one single-task fetch workflow against the mock http seam —
/// the run's own Result (a refused admission is data here, never a panic).
async fn settle(yaml: &str, http: &MockHttp) -> Result<RunOutcome, RuntimeError> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let dispatcher = dispatcher(http);
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new())),
        Arc::clone(&invoke),
        InferVerb::new(registry, "mock/echo"),
        AgentVerb::new(
            Arc::new(MockProvider::new("mock")),
            invoke,
            Arc::clone(&dispatcher),
            "mock/echo",
        ),
        nika_kernel_mock::MockClock::new(),
        RuntimeConfig::default(),
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime.run(&wf, &report, &mut stamper, &mut sink).await
}

const HEAD: &str = "nika: w\npermits:\n  net: { http: [\"api.example.com\"] }\n  tools: [\"nika:fetch\"]\ntasks:\n  charge:\n";

/// THE issue's static half (#1371): `retry×3` on a keyless POST checked
/// GREEN on 0.116.2 and triple-charged. The report is now dirty
/// (NIKA-SEC-016) and the admission trust gate refuses the run BEFORE the
/// prologue — the wire ledger shows ZERO calls where it showed three.
#[tokio::test]
async fn retried_keyless_post_is_refused_before_any_socket() {
    let http = MockHttp::new()
        .enqueue_ok(500, Vec::new())
        .enqueue_ok(500, Vec::new())
        .enqueue_ok(500, Vec::new());
    let yaml = format!(
        "{HEAD}    retry: {{ max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false }}\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://api.example.com/charge\", method: POST, body: {{ a: 1 }} }}\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        !report.is_clean(),
        "the declared retry on a keyless POST is the SEC-016 refusal"
    );
    assert_eq!(report.retry_safety_findings.len(), 1);
    let err = settle(&yaml, &http)
        .await
        .expect_err("a dirty report never reaches the attempt loop");
    assert!(
        matches!(err, RuntimeError::DirtyReport),
        "the trust gate owns the refusal: {err:?}"
    );
    assert!(
        http.sent_requests().is_empty(),
        "zero calls — the refusal precedes any socket (the issue's ledger showed 3)"
    );
}

/// The runtime belt (#1371): a keyless POST past the authored-file gate
/// (an embedder that skipped the check) sees ONE attempt — the failure
/// record is `transient: false`, the flag the retry loop's eligibility
/// reads (a post-commit 500 is ambiguous: never blind-replayed).
#[tokio::test]
async fn keyless_post_failure_is_not_retryable_at_run() {
    let http = MockHttp::new().enqueue_ok(500, Vec::new());
    let yaml = format!(
        "{HEAD}    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://api.example.com/charge\", method: POST, body: {{ a: 1 }} }}\n"
    );
    let outcome = settle(&yaml, &http)
        .await
        .expect("the run completes (a workflow failure is data)");
    assert!(!outcome.ok);
    let rec = &outcome.records["charge"];
    assert_eq!(rec.status, TaskStatus::Failure);
    let err = rec.error.as_ref().expect("the failure carries its record");
    assert_eq!(err.code, "NIKA-BUILTIN-FETCH-001");
    assert!(
        !err.transient,
        "the ambiguous post-commit failure is never retry-eligible"
    );
    assert_eq!(rec.attempts, Some(1), "one attempt — never a blind replay");
    assert_eq!(
        http.sent_requests().len(),
        1,
        "one call on the wire ledger — the issue's triple charge is impossible"
    );
}

/// With an `idempotency-key` header the declared retry fires (the
/// receiver dedups the replay — the machinery the issue proved working):
/// 3 attempts · 3 calls · the run still fails honestly.
#[tokio::test]
async fn keyed_post_retry_fires_and_the_ledger_admits_every_call() {
    let http = MockHttp::new()
        .enqueue_ok(500, Vec::new())
        .enqueue_ok(500, Vec::new())
        .enqueue_ok(500, Vec::new());
    let yaml = format!(
        "{HEAD}    retry: {{ max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false }}\n    invoke:\n      tool: \"nika:fetch\"\n      args:\n        url: \"https://api.example.com/charge\"\n        method: POST\n        headers: {{ idempotency-key: \"order-7\" }}\n        body: {{ a: 1 }}\n"
    );
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "the keyed shape is the legal one: {report:#?}"
    );
    let outcome = settle(&yaml, &http)
        .await
        .expect("the run completes (a workflow failure is data)");
    assert!(!outcome.ok);
    let rec = &outcome.records["charge"];
    assert_eq!(rec.status, TaskStatus::Failure);
    let err = rec.error.as_ref().expect("the failure carries its record");
    assert!(err.transient, "the keyed call keeps the spec status table");
    assert_eq!(rec.attempts, Some(3), "the declared retry fires");
    assert_eq!(
        http.sent_requests().len(),
        3,
        "every attempt is a call the receiver dedups on the key"
    );
}

/// GET retry behavior is UNCHANGED (#1371): a read-only method replays
/// nothing, so the spec status table applies without any key.
#[tokio::test]
async fn get_retry_behavior_is_unchanged() {
    let http = MockHttp::new()
        .enqueue_ok(500, Vec::new())
        .enqueue_ok(500, Vec::new())
        .enqueue_ok(500, Vec::new());
    let yaml = format!(
        "{HEAD}    retry: {{ max_attempts: 3, backoff_ms: 1, backoff_strategy: fixed, jitter: false }}\n    invoke:\n      tool: \"nika:fetch\"\n      args: {{ url: \"https://api.example.com/items\", method: GET }}\n"
    );
    let outcome = settle(&yaml, &http)
        .await
        .expect("the run completes (a workflow failure is data)");
    let rec = &outcome.records["charge"];
    let err = rec.error.as_ref().expect("the failure carries its record");
    assert!(err.transient, "GET + 5xx stays transient");
    assert_eq!(rec.attempts, Some(3), "GET retries free");
    assert_eq!(http.sent_requests().len(), 3);
}
