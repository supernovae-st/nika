// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Golden regression tests for all 5 Nika verbs (S12-F11, AMEND-1).
//!
//! Purpose: safety net for Sessions 13/14 verb extraction. Each verb has a
//! minimal workflow fixture executed end-to-end through `Runner::run` with
//! `provider: mock`. Tests snapshot the normalized workflow lifecycle event
//! sequence using `insta`, so any regression in a verb extraction commit
//! will fail these tests immediately.
//!
//! **Why only the lifecycle?** Timestamps, durations, task_ids, and per-verb
//! event payloads are non-deterministic. Snapshotting just the high-level
//! lifecycle (WorkflowStarted → Task* → WorkflowCompleted) gives us
//! structural coverage without brittle snapshots.
//!
//! **Why lib tests and not integration tests?** The sacred invariant
//! `cargo test --workspace --lib` forbids integration tests in `tests/`
//! (they trigger macOS Keychain popups in production code paths). Placing
//! these tests inside `nika-engine/src/runtime/runner/` keeps them safely
//! under `--lib`.
//!
//! **Fetch caveat:** the `fetch:` verb requires a network server, which the
//! lib test suite deliberately avoids. The fetch placeholder verifies the
//! lifecycle for a minimal two-task DAG that mirrors the fetch call-site
//! shape. The real `fetch:` code path is covered by the engine's existing
//! wiremock-backed tests.

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

/// Parse + run a fixture, returning the event log and the run result.
async fn run_fixture(yaml: &str) -> (EventLog, Result<String, String>) {
    let workflow = parse_analyzed(yaml).expect("golden fixture must parse");
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .expect("golden fixture must build a Runner")
        .quiet();
    let result = runner.run().await.map_err(|e| e.to_string());
    (event_log, result)
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
    let (event_log, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "exec golden must succeed: {result:?}");

    let lifecycle = workflow_lifecycle(normalize_events(&event_log));
    insta::assert_yaml_snapshot!("golden_exec_hello_lifecycle", lifecycle);
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
    let (event_log, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "infer golden must succeed: {result:?}");

    let lifecycle = workflow_lifecycle(normalize_events(&event_log));
    insta::assert_yaml_snapshot!("golden_infer_mock_lifecycle", lifecycle);
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
    let (event_log, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "invoke golden must succeed: {result:?}");

    let lifecycle = workflow_lifecycle(normalize_events(&event_log));
    insta::assert_yaml_snapshot!("golden_invoke_log_lifecycle", lifecycle);
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
    let (event_log, result) = run_fixture(yaml).await;
    assert!(result.is_ok(), "agent golden must succeed: {result:?}");

    let lifecycle = workflow_lifecycle(normalize_events(&event_log));
    insta::assert_yaml_snapshot!("golden_agent_mock_lifecycle", lifecycle);
}

// ═══════════════════════════════════════════════════════════════════════════
// Golden: fetch verb (placeholder — see module doc fetch caveat)
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn golden_fetch_placeholder() {
    // The `fetch:` verb requires a network server. This placeholder golden
    // verifies the workflow lifecycle for a minimal two-task DAG that
    // mirrors the `fetch:` call-site shape without actually making a
    // request. The real `fetch:` code path is covered by the engine's
    // existing wiremock-backed integration tests.
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
    let (event_log, result) = run_fixture(yaml).await;
    assert!(
        result.is_ok(),
        "fetch placeholder golden must succeed: {result:?}"
    );

    let lifecycle = workflow_lifecycle(normalize_events(&event_log));
    insta::assert_yaml_snapshot!("golden_fetch_placeholder_lifecycle", lifecycle);
}
