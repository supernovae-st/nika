// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Golden regression tests for all 5 Nika verbs (S12-F11, AMEND-1).
//!
//! Purpose: safety net for Sessions 13/14 verb extraction. Each verb has a
//! minimal workflow fixture executed end-to-end through `Runner::run` with
//! `provider: mock`. Tests snapshot TWO things per verb:
//!
//! 1. **Normalized workflow lifecycle** — the sequence of high-signal events
//!    (WorkflowStarted → TaskScheduled → TaskStarted → TaskCompleted →
//!    WorkflowCompleted). Captures structural workflow behaviour.
//!
//! 2. **Task output** — the exact string output recorded in the
//!    `RunContext` datastore for each task. Captures observable behavioural
//!    correctness: if a verb extraction commit alters the output shape
//!    (wrong stdout, wrong JSON structure, missing fields), the snapshot
//!    diff catches it.
//!
//! Snapshotting both is critical for AMEND-1 of the Session 12 rework:
//! lifecycle-only snapshots cannot catch output-shape regressions.
//!
//! Non-deterministic fields (timestamps, durations, task_ids in events)
//! are stripped before snapshotting.
//!
//! **Why lib tests and not integration tests?** Sacred invariant
//! `cargo test --workspace --lib` forbids integration tests in `tests/`
//! (they trigger macOS Keychain popups). Placing these tests inside
//! `nika-engine/src/runtime/runner/` keeps them safely under `--lib`.
//!
//! **Fetch caveat:** the `fetch:` verb requires a network server, which the
//! lib test suite deliberately avoids. The fetch placeholder exercises the
//! lifecycle for a minimal two-task DAG that mirrors the fetch call-site
//! shape. The real `fetch:` code path is covered by the engine's existing
//! wiremock-backed tests (`tests_wiremock.rs`).

use crate::ast::parse_analyzed;
use crate::event::{EventKind, EventLog};
use crate::runtime::runner::Runner;

/// Normalize an event stream to a deterministic string snapshot.
///
/// Captures only event kind + task_id (for task events) — stripping
/// timestamps, durations, outputs, and other non-deterministic payload.
/// Unknown/noisy events collapse to `_`.
fn normalize_events(event_log: &EventLog) -> Vec<String> {
    event_log
        .events()
        .iter()
        .map(|e| match &e.kind {
            EventKind::WorkflowStarted { .. } => "WorkflowStarted".to_string(),
            EventKind::WorkflowCompleted { .. } => "WorkflowCompleted".to_string(),
            EventKind::WorkflowFailed { .. } => "WorkflowFailed".to_string(),
            EventKind::TaskScheduled { task_id, .. } => {
                format!("TaskScheduled({task_id})")
            }
            EventKind::TaskStarted { task_id, .. } => {
                format!("TaskStarted({task_id})")
            }
            EventKind::TaskCompleted { task_id, .. } => {
                format!("TaskCompleted({task_id})")
            }
            EventKind::TaskFailed { task_id, .. } => {
                format!("TaskFailed({task_id})")
            }
            EventKind::TaskSkipped { task_id, .. } => {
                format!("TaskSkipped({task_id})")
            }
            _ => "_".to_string(),
        })
        .collect()
}

/// Drop the `_` placeholders so the snapshot focuses on the lifecycle.
fn workflow_lifecycle(events: Vec<String>) -> Vec<String> {
    events.into_iter().filter(|e| e != "_").collect()
}

/// Parse + run a fixture, returning the runner (for datastore access)
/// and the run result. The runner is kept alive so tests can query the
/// recorded task outputs via `runner.datastore()`.
async fn run_fixture(yaml: &str) -> (Runner, Result<String, String>) {
    let workflow = parse_analyzed(yaml).expect("golden fixture must parse");
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log)
        .expect("golden fixture must build a Runner")
        .quiet();
    let result = runner.run().await.map_err(|e| e.to_string());
    (runner, result)
}

/// Build the combined golden snapshot for a single-task workflow: both
/// lifecycle and task output in one structure, so a single insta snapshot
/// captures both invariants.
fn golden_snapshot(runner: &Runner, task_id: &str) -> serde_json::Value {
    let lifecycle = workflow_lifecycle(normalize_events(runner.event_log()));
    let output = runner
        .datastore()
        .get(task_id)
        .map(|r| r.output_str().to_string())
        .unwrap_or_else(|| "<missing>".to_string());
    serde_json::json!({
        "lifecycle": lifecycle,
        "output": output,
    })
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden: exec verb
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_exec_hello() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: golden-exec-hello
provider: mock
tasks:
  - id: greet
    exec: "echo hello golden"
"#;
    let (runner, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "exec golden must succeed: {result:?}");

    insta::assert_yaml_snapshot!("golden_exec_hello", golden_snapshot(&runner, "greet"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden: infer verb (provider: mock → deterministic)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_infer_mock() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: golden-infer-mock
provider: mock
tasks:
  - id: summarize
    infer:
      prompt: "Summarize this text in one sentence"
"#;
    let (runner, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "infer golden must succeed: {result:?}");

    insta::assert_yaml_snapshot!("golden_infer_mock", golden_snapshot(&runner, "summarize"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden: invoke verb (builtin tool — nika:log is deterministic)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_invoke_builtin_log() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: golden-invoke-log
provider: mock
tasks:
  - id: log_message
    invoke:
      tool: "nika:log"
      params:
        level: "info"
        message: "golden invoke fixture"
"#;
    let (runner, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "invoke golden must succeed: {result:?}");

    insta::assert_yaml_snapshot!("golden_invoke_log", golden_snapshot(&runner, "log_message"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden: agent verb (provider: mock, bounded turns)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_agent_mock() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: golden-agent-mock
provider: mock
tasks:
  - id: research
    agent:
      prompt: "Find three key points about Rust ownership"
      max_turns: 2
"#;
    let (runner, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "agent golden must succeed: {result:?}");

    insta::assert_yaml_snapshot!("golden_agent_mock", golden_snapshot(&runner, "research"));
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden: fetch verb (placeholder — see module doc fetch caveat)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_fetch_placeholder() {
    let yaml = r#"
schema: "nika/workflow@0.12"
workflow: golden-fetch-placeholder
provider: mock
tasks:
  - id: stub
    exec: "echo fetch-placeholder"
  - id: consume
    depends_on: [stub]
    exec: "echo consumed"
"#;
    let (runner, result) = run_fixture(yaml).await;
    assert!(
        result.is_ok(),
        "fetch placeholder golden must succeed: {result:?}"
    );

    // Two tasks — snapshot both outputs as a single structure.
    let combined = serde_json::json!({
        "stub": golden_snapshot(&runner, "stub"),
        "consume": golden_snapshot(&runner, "consume"),
    });
    insta::assert_yaml_snapshot!("golden_fetch_placeholder", combined);
}
