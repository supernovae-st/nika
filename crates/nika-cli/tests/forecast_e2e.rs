// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>
#![allow(clippy::expect_used, clippy::panic)]

//! Forecast e2e — the honesty ladder proven end to end: REAL runs
//! through the REAL runtime (mock seams), their event streams written
//! exactly as the flight recorder writes them, then `nika explain
//! --forecast` read back through the public verb surface.
//!
//! CWD note: `explain` reads `.nika/traces/` relative to the working
//! directory (the operator contract), so each test owns a tempdir and
//! `set_current_dir`s into it. Safe under CI's nextest (process per
//! test); this file never runs under the local `--lib` law.

use std::path::Path;
use std::sync::Arc;

use nika_check::check;
use nika_event::Event;
use nika_kernel_mock::{MockClock, MockProvider, MockShell, MockToolDefinitionProvider};
use nika_providers::{ProviderRegistry, ProvidersConfig};
use nika_runtime::{DeterministicStamper, Runtime, RuntimeConfig, VecSink};
use nika_schema::{FileId, ParseMode, parse};
use nika_verb_agent::AgentVerb;
use nika_verb_exec::ExecVerb;
use nika_verb_infer::InferVerb;
use nika_verb_invoke::InvokeVerb;

/// Two tasks · two verbs — enough DAG for a run-level elapsed and a
/// per-task model bucket (`think` rides mock/echo · `probe` is exec).
const WORKFLOW: &str = r#"
nika: fc-e2e

model: mock/echo

permits: { exec: ["echo"] }

tasks:
  probe:
    exec:
      command: ["echo", "ready"]

  think:
    after:
      probe: success
    infer:
      prompt: "summarize the probe"
      max_tokens: 32
"#;

/// One REAL run through the runtime — the same chain `nika run`
/// composes, mock seams at the I/O edges only.
async fn one_run() -> Vec<Event> {
    let wf = parse(WORKFLOW, FileId::new(0), ParseMode::Strict).expect("fixture parses");
    let report = check(&wf);
    assert!(report.is_clean(), "fixture must check clean");
    let registry = Arc::new(ProviderRegistry::without_http(ProvidersConfig::default()));
    let invoke = Arc::new(InvokeVerb::new(Arc::new(
        nika_kernel_mock::MockToolExecutor::new(),
    )));
    let runtime = Runtime::new(
        ExecVerb::new(Arc::new(MockShell::new().enqueue_ok("ready\n"))),
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
    let outcome = runtime
        .run(&wf, &report, &mut stamper, &mut sink)
        .await
        .expect("clean workflow executes");
    assert!(outcome.ok, "the fixture run completes");
    sink.into_events()
}

/// Write N runs into `<dir>/.nika/traces/` exactly as the recorder
/// does (one NDJSON line per event · ISO-shaped names, newest last).
fn stage_runs(dir: &Path, events: &[Event], n: usize) {
    let traces = dir.join(".nika").join("traces");
    std::fs::create_dir_all(&traces).expect("traces dir");
    let body: String = events
        .iter()
        .map(|e| serde_json::to_string(e).expect("event serializes"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    for i in 0..n {
        let name = format!("2026-07-09T{:02}-00-00Z-e2e{i}.ndjson", 1 + i);
        std::fs::write(traces.join(name), &body).expect("trace staged");
    }
}

/// `set_current_dir` is PROCESS-global: the two tests race on it under
/// the parallel test runner (one test's `TempDir` drops under the other's
/// feet — masked by timing until the 0.103 fixture migration shifted
/// it). Every arena user holds this lock for its whole cwd section.
static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The workflow file + the cwd dance shared by both tests.
fn arena(events: &[Event], runs: usize) -> (std::sync::MutexGuard<'static, ()>, tempfile::TempDir) {
    let guard = CWD_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let dir = tempfile::tempdir().expect("arena");
    std::fs::write(dir.path().join("fc-e2e.nika.yaml"), WORKFLOW).expect("wf written");
    stage_runs(dir.path(), events, runs);
    std::env::set_current_dir(dir.path()).expect("cwd into arena");
    (guard, dir)
}

#[tokio::test]
async fn six_runs_earn_bands_and_the_json_twin_tags_the_rung() {
    let events = one_run().await;
    let _arena = arena(&events, 6);

    let out = nika_cli::verbs::explain_file::run("fc-e2e.nika.yaml", false, true);
    for needle in [
        "FORECAST",
        "based on last 6 runs",
        "(p90 ",
        "low confidence (n<10)",
        // The harness stamps no source hashes — the census must SAY so.
        "6 unverifiable",
        "estimates vary with `when` branches",
    ] {
        assert!(
            out.text.contains(needle),
            "missing `{needle}`:\n{}",
            out.text
        );
    }
    assert!(
        !out.text.contains("predict"),
        "banned vocabulary:\n{}",
        out.text
    );

    let json = nika_cli::verbs::explain_file::run("fc-e2e.nika.yaml", true, true);
    let v: serde_json::Value = serde_json::from_str(&json.text).expect("json parses");
    assert_eq!(v["forecast"]["run_duration"]["kind"], "bands");
    assert_eq!(v["forecast"]["runs"]["total"], 6);
    assert_eq!(v["forecast"]["runs"]["completed"], 6);
    assert_eq!(v["forecast"]["runs"]["unknown_hash"], 6);
}

#[tokio::test]
async fn three_runs_stay_a_range_and_arrive_unprompted() {
    let events = one_run().await;
    let _arena = arena(&events, 3);

    // n = 3 crosses the auto threshold: the section arrives WITHOUT the
    // flag, and speaks range vocabulary only — never a percentile.
    let out = nika_cli::verbs::explain_file::run("fc-e2e.nika.yaml", false, false);
    assert!(
        out.text.contains("based on last 3 runs"),
        "auto-section at n=3:\n{}",
        out.text
    );
    assert!(
        !out.text.contains("p90"),
        "no bands under n=5:\n{}",
        out.text
    );

    let json = nika_cli::verbs::explain_file::run("fc-e2e.nika.yaml", true, false);
    let v: serde_json::Value = serde_json::from_str(&json.text).expect("json parses");
    assert_eq!(v["forecast"]["run_duration"]["kind"], "range");
}
