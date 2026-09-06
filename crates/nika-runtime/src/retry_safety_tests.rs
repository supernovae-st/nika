// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! The effect-safe retry law (#1371) end-to-end, through the REAL attempt
//! loop over the real `BuiltinDispatcher` (mock http seam · zero I/O):
//!
//! - a declared `retry:` on a keyless mutating `nika:fetch` never reaches
//!   the loop — `NIKA-SEC-016` dirties the report and the admission trust
//!   gate refuses the run BEFORE any socket (zero requests sent);
//! - resolved keyless mutating fetch failures keep one attempt even when
//!   `on_codes` matches, retaining their error and fired evidence;
//! - with an `idempotency-key` header the declared retry fires under the
//!   existing name-based law; this mock does not prove receiver deduplication;
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
    settle_with_events(yaml, http)
        .await
        .map(|(outcome, _)| outcome)
}

async fn settle_with_events(
    yaml: &str,
    http: &MockHttp,
) -> Result<(RunOutcome, Vec<nika_event::Event>), RuntimeError> {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    let outcome = fetch_runtime(http)
        .run(&wf, &report, &mut stamper, &mut sink)
        .await?;
    Ok((outcome, sink.into_events()))
}

type FetchRuntime = Runtime<
    MockShell,
    FetchDispatcher,
    nika_providers::NoHttp,
    MockProvider,
    FetchDispatcher,
    nika_kernel_mock::MockClock,
>;

fn fetch_runtime(http: &MockHttp) -> FetchRuntime {
    let dispatcher = dispatcher(http);
    let invoke = Arc::new(InvokeVerb::new(Arc::clone(&dispatcher)));
    let registry = Arc::new(nika_providers::ProviderRegistry::without_http(
        nika_providers::ProvidersConfig::default(),
    ));
    Runtime::new(
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
    )
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

/// A plain keyless POST retains its non-transient failure classification.
/// The resolved-method tests below exercise the explicit retry declaration.
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
        "one recorded dispatch for this plain invocation"
    );
}

/// A declared key retains the existing retry policy: three attempts and
/// three recorded calls. Receiver deduplication is outside this mock.
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
        "every attempt is recorded; receiver deduplication is not modeled"
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

/// A resolved effect-capable request cannot be replayed just because its
/// successful response body fails local extraction and matches `on_codes`.
/// The mock counts dispatches only; it does not model receiver deduplication.
#[tokio::test]
async fn resolved_keyless_post_extraction_failure_is_not_replayed() {
    let http = MockHttp::new()
        .enqueue_ok(200, b"not-json".to_vec())
        .enqueue_ok(200, b"not-json".to_vec())
        .enqueue_ok(200, b"not-json".to_vec());
    let yaml = [
        HEAD,
        r#"    retry:
      max_attempts: 3
      backoff_ms: 1
      backoff_strategy: fixed
      jitter: false
      on_codes: [NIKA-BUILTIN-FETCH-001]
    invoke:
      tool: "nika:fetch"
      args:
        url: "https://api.example.com/fixture"
        method: "${{ 'POST' }}"
        body: { id: 1 }
        mode: jq
        jq: ".id"
"#,
    ]
    .concat();
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "resolved method is judged at runtime: {report:#?}"
    );
    let outcome = settle(&yaml, &http)
        .await
        .expect("workflow failure is data");
    assert!(!outcome.ok);
    let rec = &outcome.records["charge"];
    let err = rec.error.as_ref().expect("extraction failure is recorded");
    assert_eq!(err.code, "NIKA-BUILTIN-FETCH-001");
    assert!(!err.transient);
    let sent = http.sent_requests();
    assert!(
        sent.iter()
            .all(|request| request.method == nika_kernel::HttpMethod::Post)
    );
    assert_eq!(
        sent.len(),
        1,
        "local extraction failure must not repeat the resolved keyless POST"
    );
    assert_eq!(rec.attempts, Some(1));
}

fn resolved_fetch_yaml(method: Option<&str>, headers: serde_json::Value, attempts: u32) -> String {
    let mut args = serde_json::json!({
        "url": "https://api.example.com/fixture",
        "mode": "jq",
        "jq": ".id",
    });
    args["headers"] = headers;
    if let Some(method) = method {
        args["method"] = serde_json::Value::String(format!("${{{{ '{method}' }}}}"));
    }
    let invoke = serde_json::json!({"tool": "nika:fetch", "args": args});
    format!(
        "{HEAD}    retry: {{ max_attempts: {attempts}, backoff_ms: 1, backoff_strategy: fixed, jitter: false, on_codes: [NIKA-BUILTIN-FETCH-001] }}\n    invoke: {invoke}\n"
    )
}

#[tokio::test]
async fn resolved_mutating_methods_keep_one_attempt_across_failure_stages() {
    for method in ["POST", "put", "PATCH", "DELETE", "poſt", "poﬆ"] {
        for stage in ["extraction", "status", "transport"] {
            let mut http = MockHttp::new();
            for _ in 0..3 {
                http = match stage {
                    "extraction" => http.enqueue_ok(200, b"not-json".to_vec()),
                    "status" => http.enqueue_ok(503, Vec::new()),
                    _ => http.enqueue_err(nika_kernel::HttpError::Timeout { duration_ms: 1 }),
                };
            }
            let yaml = resolved_fetch_yaml(Some(method), serde_json::json!({}), 3);
            let outcome = settle(&yaml, &http).await.expect("failure is data");
            let record = &outcome.records["charge"];
            assert!(!outcome.ok, "{method}/{stage}");
            assert_eq!(record.attempts, Some(1), "{method}/{stage}");
            let error = record.error.as_ref().expect("recorded failure");
            assert_eq!(error.code, "NIKA-BUILTIN-FETCH-001");
            assert!(!error.transient);
            assert_eq!(http.sent_requests().len(), 1, "{method}/{stage}");
            assert_eq!(http.remaining(), 2, "unused responses stay queued");
        }
    }
}

#[tokio::test]
async fn resolved_read_methods_and_declared_keys_keep_on_codes_retries() {
    let cases = [
        (Some("GET"), serde_json::json!({})),
        (Some("HEAD"), serde_json::json!({})),
        (None, serde_json::json!({})),
        (
            Some("POST"),
            serde_json::json!({"Idempotency-Key": "fixture-1"}),
        ),
        (
            Some("PUT"),
            serde_json::json!({"IDEMPOTENCY-KEY": "fixture-1"}),
        ),
        (
            Some("PATCH"),
            serde_json::json!({"idempotency-key": "fixture-1"}),
        ),
        (
            Some("DELETE"),
            serde_json::json!({"idempotency-key": "fixture-1"}),
        ),
    ];
    for (method, headers) in cases {
        let http = MockHttp::new()
            .enqueue_ok(200, b"not-json".to_vec())
            .enqueue_ok(200, b"not-json".to_vec())
            .enqueue_ok(200, b"not-json".to_vec());
        let yaml = resolved_fetch_yaml(method, headers, 3);
        let outcome = settle(&yaml, &http).await.expect("failure is data");
        let record = &outcome.records["charge"];
        assert!(!outcome.ok);
        assert_eq!(record.attempts, Some(3), "{method:?}");
        assert_eq!(http.sent_requests().len(), 3, "{method:?}");
        assert_eq!(http.remaining(), 0);
    }
}

#[tokio::test]
async fn resolved_keyless_first_success_and_single_attempt_are_unchanged() {
    for attempts in [1, 3] {
        let http = MockHttp::new().enqueue_ok(200, br#"{"id":7}"#.to_vec());
        let yaml = resolved_fetch_yaml(Some("POST"), serde_json::json!({}), attempts);
        let outcome = settle(&yaml, &http).await.expect("success is data");
        assert!(outcome.ok);
        let record = &outcome.records["charge"];
        assert_eq!(record.attempts, Some(1));
        assert_eq!(record.output, serde_json::json!(7));
        assert_eq!(http.sent_requests().len(), 1);
    }
}

#[tokio::test]
async fn assert_on_codes_still_retries_its_non_transient_failure() {
    let yaml = r#"nika: polling
permits:
  tools: ["nika:assert"]
tasks:
  guard:
    retry: { max_attempts: 3, backoff_ms: 1, jitter: false, on_codes: [NIKA-BUILTIN-ASSERT-001] }
    invoke: { tool: "nika:assert", args: { condition: false, message: "pending" } }
"#;
    let http = MockHttp::new();
    let outcome = settle(yaml, &http).await.expect("failure is data");
    let record = &outcome.records["guard"];
    assert_eq!(record.attempts, Some(3));
    assert!(!record.error.as_ref().expect("assert error").transient);
    assert!(http.sent_requests().is_empty());
}

#[tokio::test]
async fn replay_veto_keeps_fired_evidence_and_schedules_no_backoff() {
    let http = MockHttp::new().enqueue_ok(200, b"not-json".to_vec());
    let yaml = resolved_fetch_yaml(Some("POST"), serde_json::json!({}), 3);
    let (outcome, events) = settle_with_events(&yaml, &http)
        .await
        .expect("failure is data");
    assert!(!outcome.ok);
    assert!(
        !events
            .iter()
            .any(|e| e.kind == nika_event::EventKind::TaskRetrying)
    );
    let terminal = events
        .iter()
        .find(|e| e.kind == nika_event::EventKind::TaskFailed)
        .expect("task failure event");
    let field = |key: &str| {
        terminal
            .fields
            .iter()
            .find(|f| f.key == key)
            .map(|f| &f.value)
    };
    assert!(field("preview_digest").is_some());
    assert_eq!(field("preview_digest"), field("commit_digest"));
    assert!(field("divergence").is_none());
}

#[test]
#[allow(clippy::float_cmp)] // Exact binary fractions pin the debit, not a numerical approximation.
fn replay_veto_debits_the_attempt_and_preserves_its_failure() {
    use crate::dispatch::{
        FailedDispatch,
        commit::{CommitAttestation, CommitEvidence},
    };

    let yaml = resolved_fetch_yaml(Some("POST"), serde_json::json!({}), 3);
    let wf = nika_schema::parse(
        &yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let evidence = CommitEvidence::Fired(Box::new(CommitAttestation {
        preview_digest: "same-judged-request".to_owned(),
        commit_digest: "same-judged-request".to_owned(),
    }));
    let failed = FailedDispatch {
        usage: None,
        record: crate::record::TaskErrorRecord::new(
            "NIKA-BUILTIN-FETCH-001",
            "recorded failure",
            false,
        ),
        retry_forbidden: true,
        cost_usd: Some(0.25),
        cost_source: Some("fixture".to_owned()),
        cost_unpriced: None,
        evidence: Some(evidence.clone()),
        access: None,
    };
    let ledger = crate::ledger::RunLedger::new(Some(10.0));
    let mut total_cost = Some(0.125);
    let mut unpriced = None;
    let runtime = fetch_runtime(&MockHttp::new());
    let result = runtime.failed_attempt_delay(
        &wf.tasks[0].value,
        failed,
        &ledger,
        &mut total_cost,
        &mut unpriced,
        1,
        3,
        "charge",
    );
    let Err(failure) = result else {
        panic!("a replay veto must not produce a retry delay");
    };
    assert_eq!(ledger.snapshot().spent_usd, 0.25);
    assert_eq!(ledger.snapshot().priced_calls, 1);
    assert_eq!(failure.cost_usd, Some(0.375));
    assert_eq!(total_cost, Some(0.375));
    assert_eq!(failure.record.code, "NIKA-BUILTIN-FETCH-001");
    assert_eq!(failure.record.message, "recorded failure");
    assert_eq!(failure.evidence, Some(evidence));
}
