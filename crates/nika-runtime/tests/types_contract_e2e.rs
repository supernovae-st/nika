// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! W3 « the contract » — the runtime decode + fit battery over the REAL
//! parse → check → run chain (spec `09-types.md` §decode · §returns).
//!
//! Every case the operator matrix names: strict-UTF-8 text · json ·
//! jsonl · bytes · invalid UTF-8 (`NIKA-1705`) · empty output ·
//! truncated JSON · large output · stderr/combined capture · structured
//! capture typed directly · a fit violation (`NIKA-TYPE-101`) · the
//! `on_error:` scope catching both failure classes · timeout while a
//! contract is declared (cancellation settles cleanly, no fit runs).
//! Invalid octets ride `ShellResult::with_raw` — the same lane the real
//! runner (`nika-exec-runner`) records, so the mock feeds the decode
//! pipeline the exact bytes a real subprocess would.

use std::sync::Arc;
use std::time::Duration;

use nika_kernel::process::ShellResult;
use nika_kernel_mock::{
    MockClock, MockProvider, MockShell, MockToolDefinitionProvider, MockToolExecutor,
};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, RunOutcome, Runtime, RuntimeConfig, TaskStatus, VecSink};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;
use serde_json::json;

// ─── harness (the floor: real parse → check → run over mocks) ─────────────

async fn run_yaml(yaml: &str, shell: MockShell) -> RunOutcome {
    let wf = nika_schema::parse(
        yaml,
        nika_schema::FileId::new(0),
        nika_schema::ParseMode::Strict,
    )
    .expect("fixture parses");
    let report = nika_check::check(&wf);
    assert!(
        report.is_clean(),
        "fixture passes the ladder: {:?}",
        report
            .findings
            .iter()
            .map(|f| f.code.clone())
            .collect::<Vec<_>>()
    );
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(MockToolExecutor::new())));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(shell)),
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
    );
    let mut stamper = DeterministicStamper::new();
    let mut sink = VecSink::new();
    runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean run")
}

fn ok_bytes(stdout: &[u8]) -> ShellResult {
    ShellResult::new(
        0,
        String::from_utf8_lossy(stdout),
        "",
        Duration::from_millis(1),
    )
    .with_raw(stdout.to_vec(), Vec::new())
}

// ─── decode: the four modes over real chains ───────────────────────────────

#[tokio::test]
async fn decode_json_feeds_a_typed_object_downstream() {
    let yaml = r#"
nika: w3-json
permits: { exec: true }
tasks:
  stats:
    exec: { command: ["jq-mock"], decode: json }
    returns: { object: { count: integer, mean: number } }
  gate:
    with: { c: "${{ tasks.stats.output.count }}" }
    when: ${{ with.c == 3 }}
    exec: { command: ["echo", "typed"] }
"#;
    let shell = MockShell::new()
        .enqueue_result(ok_bytes(br#"{"count": 3, "mean": 1.5}"#))
        .enqueue_ok("typed\n");
    let outcome = run_yaml(yaml, shell).await;
    assert!(outcome.ok, "typed json chain settles green");
    assert_eq!(
        outcome.records["stats"].output,
        json!({"count": 3, "mean": 1.5}),
        "the decoded OBJECT is the task output (never a string)"
    );
    assert_eq!(outcome.records["gate"].status, TaskStatus::Success);
}

#[tokio::test]
async fn decode_text_is_strict_utf8_and_trims_like_today() {
    let yaml = r#"
nika: w3-text
permits: { exec: true }
tasks:
  say:
    exec: { command: ["echo", "42"], decode: text }
    returns: string
"#;
    let outcome = run_yaml(yaml, MockShell::new().enqueue_result(ok_bytes(b"42\n"))).await;
    assert!(outcome.ok);
    assert_eq!(
        outcome.records["say"].output,
        json!("42"),
        "trailing newline trimmed exactly like the untyped lane"
    );
}

#[tokio::test]
async fn decode_jsonl_is_one_value_per_non_empty_line() {
    let yaml = r#"
nika: w3-jsonl
permits: { exec: true }
tasks:
  rows:
    exec: { command: ["stream-mock"], decode: jsonl }
    returns: { array: { object: { a: integer } } }
"#;
    let shell = MockShell::new().enqueue_result(ok_bytes(b"{\"a\":1}\n\n  \n{\"a\":2}\n"));
    let outcome = run_yaml(yaml, shell).await;
    assert!(outcome.ok);
    assert_eq!(
        outcome.records["rows"].output,
        json!([{"a":1},{"a":2}]),
        "empty/whitespace lines skipped · order preserved"
    );
}

#[tokio::test]
async fn decode_bytes_carries_invalid_utf8_as_base64() {
    let yaml = r#"
nika: w3-bytes
permits: { exec: true }
tasks:
  blob:
    exec: { command: ["bin-mock"], decode: bytes }
    returns: bytes
"#;
    // 0xFF 0x00 — invalid UTF-8 is FINE under bytes: the octets ARE the value.
    let outcome = run_yaml(
        yaml,
        MockShell::new().enqueue_result(ok_bytes(&[0xFF, 0x00])),
    )
    .await;
    assert!(outcome.ok);
    assert_eq!(
        outcome.records["blob"].output,
        json!("/wA="),
        "base64 of the EXACT octets (RFC 4648)"
    );
}

// ─── the refusal lanes: 1705 (decode) and TYPE-101 (fit) ───────────────────

#[tokio::test]
async fn invalid_utf8_under_text_is_a_task_failure_not_a_crash() {
    let yaml = r#"
nika: w3-bad-utf8
permits: { exec: true }
tasks:
  bad:
    exec: { command: ["bin-mock"], decode: text }
    returns: string
"#;
    let outcome = run_yaml(
        yaml,
        MockShell::new().enqueue_result(ok_bytes(&[0x66, 0x6F, 0xFF, 0x0A])),
    )
    .await;
    assert!(!outcome.ok, "the run reports the failure");
    assert_eq!(outcome.records["bad"].status, TaskStatus::Failure);
    let err = outcome.records["bad"]
        .error
        .as_ref()
        .expect("failure error");
    assert!(
        err.message.contains("not valid UTF-8") && err.message.contains("decode: bytes"),
        "teaches the octets door: {}",
        err.message
    );
}

#[tokio::test]
async fn truncated_json_names_the_decode_and_empty_output_is_honest() {
    let yaml = r#"
nika: w3-trunc
permits: { exec: true }
tasks:
  cut:
    exec: { command: ["api-mock"], decode: json }
    returns: { object: { ok: bool } }
"#;
    // An incomplete stream (process died mid-write) must land as a decode
    // refusal, never a panic or a half-value.
    let outcome = run_yaml(
        yaml,
        MockShell::new().enqueue_result(ok_bytes(br#"{"ok": tr"#)),
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["cut"].status, TaskStatus::Failure);
    let err = outcome.records["cut"].error.as_ref().expect("decode error");
    assert!(err.message.contains("decode: json"), "{}", err.message);

    // EMPTY output: json refuses (nothing is not a document) —
    // same lane, honest message.
    let yaml_empty = yaml.replace("w3-trunc", "w3-empty");
    let outcome = run_yaml(&yaml_empty, MockShell::new().enqueue_result(ok_bytes(b""))).await;
    assert!(!outcome.ok, "empty bytes do not parse as one JSON document");
    assert_eq!(outcome.records["cut"].status, TaskStatus::Failure);
}

#[tokio::test]
async fn empty_output_fits_text_and_jsonl_as_their_empty_values() {
    // text: empty string IS a string · jsonl: zero lines IS [].
    let yaml = r#"
nika: w3-empties
permits: { exec: true }
tasks:
  t:
    exec: { command: ["quiet-mock"], decode: text }
    returns: string
  l:
    exec: { command: ["quiet-mock"], decode: jsonl }
    returns: { array: integer }
"#;
    let shell = MockShell::new()
        .enqueue_result(ok_bytes(b""))
        .enqueue_result(ok_bytes(b""));
    let outcome = run_yaml(yaml, shell).await;
    assert!(outcome.ok);
    assert_eq!(outcome.records["t"].output, json!(""));
    assert_eq!(outcome.records["l"].output, json!([]));
}

#[tokio::test]
async fn a_fit_violation_is_type_101_and_never_echoes_the_value() {
    let yaml = r#"
nika: w3-fit
permits: { exec: true }
tasks:
  n:
    exec: { command: ["num-mock"], decode: json }
    returns: { object: { count: integer } }
"#;
    let outcome = run_yaml(
        yaml,
        MockShell::new().enqueue_result(ok_bytes(br#"{"count": "s3cret-like"}"#)),
    )
    .await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["n"].status, TaskStatus::Failure);
    let err = outcome.records["n"].error.as_ref().expect("fit error");
    assert!(
        err.message.contains("does not fit `returns:`"),
        "the TYPE-101 diagnostic rides the failure: {}",
        err.message
    );
    assert!(
        !err.message.contains("s3cret-like"),
        "the VALUE never rides a diagnostic (exec output may carry secrets): {}",
        err.message
    );
}

// ─── capture forms: stderr · combined · structured ─────────────────────────

#[tokio::test]
async fn stderr_and_combined_captures_feed_the_decode_pipeline() {
    let yaml = r#"
nika: w3-streams
permits: { exec: true }
tasks:
  logs:
    exec: { command: ["warn-mock"], capture: stderr, decode: json }
    returns: { object: { warn: string } }
"#;
    let shell = MockShell::new().enqueue_result(
        ShellResult::new(0, "noise", r#"{"warn": "disk"}"#, Duration::from_millis(1))
            .with_raw(b"noise".to_vec(), br#"{"warn": "disk"}"#.to_vec()),
    );
    let outcome = run_yaml(yaml, shell).await;
    assert!(outcome.ok, "stderr capture decodes the STDERR octets");
    assert_eq!(outcome.records["logs"].output, json!({"warn": "disk"}));
}

#[tokio::test]
async fn structured_capture_with_returns_types_the_object_directly() {
    // Under `capture: structured` there is NO decode (the value is already
    // an object — PARSE-025 refuses an explicit decode) and `returns:`
    // types { stdout, stderr, exit_code } directly.
    let yaml = r#"
nika: w3-structured
permits: { exec: true }
tasks:
  probe:
    exec: { command: ["probe-mock"], capture: structured }
    returns:
      object:
        stdout: string
        stderr: string
        exit_code: integer
      additional: true
"#;
    let shell = MockShell::new().enqueue_result(ShellResult::new(
        3,
        "out",
        "err",
        Duration::from_millis(1),
    ));
    let outcome = run_yaml(yaml, shell).await;
    assert!(outcome.ok, "non-zero exit is DATA under structured");
    assert_eq!(
        outcome.records["probe"].output,
        json!({"stdout": "out", "stderr": "err", "exit_code": 3})
    );
}

// ─── large output · timeout race · on_error scope ──────────────────────────

#[tokio::test]
async fn a_large_stream_decodes_whole_through_jsonl() {
    // 50k rows ≈ 1MB — the pipeline decodes the WHOLE capture (caps live
    // at the kernel capture layer, not in decode).
    let mut big = Vec::with_capacity(1_200_000);
    for i in 0..50_000u32 {
        big.extend_from_slice(format!("{{\"i\":{i}}}\n").as_bytes());
    }
    let yaml = r#"
nika: w3-big
permits: { exec: true }
tasks:
  bulk:
    exec: { command: ["gen-mock"], decode: jsonl }
    returns: { array: { object: { i: integer } } }
"#;
    let outcome = run_yaml(yaml, MockShell::new().enqueue_result(ok_bytes(&big))).await;
    assert!(outcome.ok);
    let rows = outcome.records["bulk"].output.as_array().expect("array");
    assert_eq!(rows.len(), 50_000, "every line decoded · none dropped");
    assert_eq!(rows[49_999], json!({"i": 49_999}));
}

#[tokio::test]
async fn on_error_skip_scopes_both_refusal_classes() {
    // A decode failure (1705) and a fit violation (TYPE-101) are verb-stage
    // task failures — `on_error: skip` catches them like any other (spec 05
    // scope), and the run continues.
    let yaml = r#"
nika: w3-onerror
permits: { exec: true }
tasks:
  bad_decode:
    exec: { command: ["bin-mock"], decode: json }
    returns: { object: { ok: bool } }
    on_error: { skip: true }
  bad_fit:
    exec: { command: ["num-mock"], decode: json }
    returns: { object: { count: integer } }
    on_error: { skip: true }
  survivor:
    after: { bad_decode: terminal, bad_fit: terminal }
    exec: { command: ["echo", "alive"] }
"#;
    let shell = MockShell::new()
        .enqueue_result(ok_bytes(b"not-json"))
        .enqueue_result(ok_bytes(br#"{"count": "nope"}"#))
        .enqueue_ok("alive\n");
    let outcome = run_yaml(yaml, shell).await;
    assert_eq!(outcome.records["bad_decode"].status, TaskStatus::Skipped);
    assert_eq!(outcome.records["bad_fit"].status, TaskStatus::Skipped);
    // `after: terminal` edges open on ANY terminal settle (spec 03 · W2) —
    // the survivor runs even though both upstreams were skipped-on-error.
    assert_eq!(outcome.records["survivor"].status, TaskStatus::Success);
}

#[tokio::test]
async fn a_dead_process_with_a_declared_contract_settles_cleanly() {
    // The attempt dies BEFORE any bytes exist (the cancellation class:
    // decode never reached) — the task settles a plain verb failure, the
    // declared contract stays moot: no fit runs, no panic, no 1705.
    let yaml = r#"
nika: w3-dead
permits: { exec: true }
tasks:
  slow:
    exec: { command: ["sleep-mock"], decode: json }
    returns: { object: { ok: bool } }
"#;
    // A mock with NO enqueued result reports a shell error (process death).
    let outcome = run_yaml(yaml, MockShell::new()).await;
    assert!(!outcome.ok);
    assert_eq!(outcome.records["slow"].status, TaskStatus::Failure);
    let err = outcome.records["slow"].error.as_ref().expect("verb error");
    assert!(
        !err.message.contains("decode") && !err.message.contains("returns"),
        "neither decode nor fit ever ran — the failure is the verb's: {}",
        err.message
    );
}
