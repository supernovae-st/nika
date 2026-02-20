//! TUI Event Pipeline Tests
//!
//! Verifies that events flow correctly from Runner to TUI state.
//! Tests the broadcast channel integration and event sequence.
//!
//! Run: `cargo test --test tui_event_pipeline_test --features tui`

#![cfg(feature = "tui")]

use std::time::Duration;

use tokio::sync::broadcast;
use tokio::time::timeout;

use nika::ast::Workflow;
use nika::event::{Event, EventKind, EventLog};
use nika::runtime::Runner;

// ═══════════════════════════════════════════════════════════════════════════
// HELPER FUNCTIONS
// ═══════════════════════════════════════════════════════════════════════════

fn parse_workflow(yaml: &str) -> Result<Workflow, nika::error::NikaError> {
    serde_yaml::from_str(yaml).map_err(|e| nika::error::NikaError::ParseError {
        details: e.to_string(),
    })
}

async fn collect_events(mut rx: broadcast::Receiver<Event>, timeout_ms: u64) -> Vec<Event> {
    let mut events = Vec::new();
    let deadline = Duration::from_millis(timeout_ms);

    loop {
        match timeout(deadline, rx.recv()).await {
            Ok(Ok(event)) => {
                let is_terminal = matches!(
                    &event.kind,
                    EventKind::WorkflowCompleted { .. } | EventKind::WorkflowFailed { .. }
                );
                events.push(event);
                if is_terminal {
                    break;
                }
            }
            Ok(Err(_)) => break, // Channel closed
            Err(_) => break,     // Timeout
        }
    }

    events
}

// ═══════════════════════════════════════════════════════════════════════════
// BASIC EVENT FLOW TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_exec_workflow_emits_events() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: step1
    exec: "echo hello"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Verify event sequence
    let event_types: Vec<&str> = events
        .iter()
        .map(|e| match &e.kind {
            EventKind::WorkflowStarted { .. } => "WorkflowStarted",
            EventKind::TaskScheduled { .. } => "TaskScheduled",
            EventKind::TaskStarted { .. } => "TaskStarted",
            EventKind::TaskCompleted { .. } => "TaskCompleted",
            EventKind::WorkflowCompleted { .. } => "WorkflowCompleted",
            _ => "Other",
        })
        .collect();

    assert!(
        event_types.contains(&"WorkflowStarted"),
        "Should have WorkflowStarted event"
    );
    assert!(
        event_types.contains(&"TaskScheduled"),
        "Should have TaskScheduled event"
    );
    assert!(
        event_types.contains(&"TaskStarted"),
        "Should have TaskStarted event"
    );
    assert!(
        event_types.contains(&"TaskCompleted"),
        "Should have TaskCompleted event"
    );
    assert!(
        event_types.contains(&"WorkflowCompleted"),
        "Should have WorkflowCompleted event"
    );
}

#[tokio::test]
async fn test_multiple_tasks_emit_sequential_events() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: step1
    exec: "echo first"
  - id: step2
    exec: "echo second"
flows:
  - source: step1
    target: step2
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find task events
    let task_started: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::TaskStarted { task_id, .. } => Some(task_id.as_ref()),
            _ => None,
        })
        .collect();

    assert_eq!(task_started.len(), 2);
    assert_eq!(task_started[0], "step1");
    assert_eq!(task_started[1], "step2");
}

#[tokio::test]
async fn test_parallel_tasks_emit_events() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: step1
    exec: "echo first"
  - id: step2
    exec: "echo second"
  - id: step3
    exec: "echo final"
flows:
  - source: [step1, step2]
    target: step3
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Count task completions
    let completions: Vec<&str> = events
        .iter()
        .filter_map(|e| match &e.kind {
            EventKind::TaskCompleted { task_id, .. } => Some(task_id.as_ref()),
            _ => None,
        })
        .collect();

    assert_eq!(completions.len(), 3);
    assert!(completions.contains(&"step1"));
    assert!(completions.contains(&"step2"));
    assert!(completions.contains(&"step3"));
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK OUTPUT VERIFICATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_exec_output_captured_in_event() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: echo_test
    exec: "echo 'hello world'"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find TaskCompleted event
    let completed = events.iter().find_map(|e| match &e.kind {
        EventKind::TaskCompleted {
            task_id, output, ..
        } if task_id.as_ref() == "echo_test" => Some(output),
        _ => None,
    });

    assert!(
        completed.is_some(),
        "Should have TaskCompleted for echo_test"
    );
    let output = completed.unwrap();

    // Output should contain "hello world"
    let output_str = output.to_string();
    assert!(
        output_str.contains("hello") || output_str.contains("world"),
        "Output should contain echo result: {}",
        output_str
    );
}

#[tokio::test]
async fn test_json_output_captured() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: json_output
    exec: |
      echo '{"key": "value", "number": 42}'
    output:
      format: json
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    let completed = events.iter().find_map(|e| match &e.kind {
        EventKind::TaskCompleted {
            task_id, output, ..
        } if task_id.as_ref() == "json_output" => Some(output),
        _ => None,
    });

    assert!(completed.is_some());
    let output = completed.unwrap();

    // Should be parsed JSON
    if let Some(obj) = output.as_object() {
        assert_eq!(obj.get("key").and_then(|v| v.as_str()), Some("value"));
        assert_eq!(obj.get("number").and_then(|v| v.as_i64()), Some(42));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK INPUT VERIFICATION TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_task_inputs_captured_in_event() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: producer
    exec: |
      echo '{"value": 42}'
    output:
      format: json

  - id: consumer
    use:
      data: producer
    exec: "echo consumed"
flows:
  - source: producer
    target: consumer
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find TaskStarted for consumer
    let started = events.iter().find_map(|e| match &e.kind {
        EventKind::TaskStarted {
            task_id, inputs, ..
        } if task_id.as_ref() == "consumer" => Some(inputs),
        _ => None,
    });

    assert!(started.is_some(), "Should have TaskStarted for consumer");
    let inputs = started.unwrap();

    // Inputs should contain resolved 'data' binding
    if let Some(data) = inputs.get("data") {
        assert_eq!(data.get("value").and_then(|v| v.as_i64()), Some(42));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// FAILURE EVENT TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_failed_task_emits_failure_event() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: failing
    exec: "exit 1"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Should have TaskFailed
    let failed = events.iter().any(|e| {
        matches!(
            &e.kind,
            EventKind::TaskFailed { task_id, .. } if task_id.as_ref() == "failing"
        )
    });
    assert!(failed, "Should have TaskFailed event");

    // Workflow still completes (WorkflowFailed is only for deadlocks/panics)
    // The workflow completes even when tasks fail - they're marked as failed
    let workflow_completed = events
        .iter()
        .any(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. }));
    assert!(
        workflow_completed,
        "Workflow should complete even with failed tasks"
    );
}

#[tokio::test]
async fn test_failed_task_includes_error_message() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: bad_command
    exec: "this_command_does_not_exist_12345"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find error message
    let error_msg = events.iter().find_map(|e| match &e.kind {
        EventKind::TaskFailed { error, .. } => Some(error.clone()),
        _ => None,
    });

    assert!(error_msg.is_some(), "Should have error message");
    let msg = error_msg.unwrap();
    assert!(
        !msg.is_empty(),
        "Error message should not be empty: {}",
        msg
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// EVENT TIMING TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_event_timestamps_are_monotonic() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: step1
    exec: "echo 1"
  - id: step2
    exec: "echo 2"
  - id: step3
    exec: "echo 3"
flows:
  - source: step1
    target: step2
  - source: step2
    target: step3
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Verify timestamps are monotonically increasing
    let timestamps: Vec<u64> = events.iter().map(|e| e.timestamp_ms).collect();
    for i in 1..timestamps.len() {
        assert!(
            timestamps[i] >= timestamps[i - 1],
            "Timestamps should be monotonic: {:?}",
            timestamps
        );
    }
}

#[tokio::test]
async fn test_task_duration_matches_events() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: timed
    exec: "sleep 0.1 && echo done"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find start and completion events
    let started_ts = events.iter().find_map(|e| match &e.kind {
        EventKind::TaskStarted { task_id, .. } if task_id.as_ref() == "timed" => {
            Some(e.timestamp_ms)
        }
        _ => None,
    });

    let (completed_ts, duration_ms) = events
        .iter()
        .find_map(|e| match &e.kind {
            EventKind::TaskCompleted {
                task_id,
                duration_ms,
                ..
            } if task_id.as_ref() == "timed" => Some((e.timestamp_ms, *duration_ms)),
            _ => None,
        })
        .unwrap();

    assert!(started_ts.is_some());
    let elapsed = completed_ts - started_ts.unwrap();

    // Duration should roughly match elapsed time (within 50ms tolerance)
    assert!(
        (elapsed as i64 - duration_ms as i64).abs() < 50,
        "Duration {} should match elapsed {}: diff={}",
        duration_ms,
        elapsed,
        (elapsed as i64 - duration_ms as i64).abs()
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW METADATA TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_workflow_started_includes_metadata() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: step1
    exec: "echo hello"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find WorkflowStarted
    let started = events.iter().find_map(|e| match &e.kind {
        EventKind::WorkflowStarted {
            task_count,
            generation_id,
            nika_version,
            ..
        } => Some((task_count, generation_id, nika_version)),
        _ => None,
    });

    assert!(started.is_some());
    let (task_count, generation_id, nika_version) = started.unwrap();

    assert_eq!(*task_count, 1);
    assert!(
        !generation_id.is_empty(),
        "generation_id should not be empty"
    );
    assert!(!nika_version.is_empty(), "nika_version should not be empty");
}

#[tokio::test]
async fn test_workflow_completed_includes_final_output() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: final
    exec: "echo hello_output"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // Find WorkflowCompleted
    let completed = events.iter().find_map(|e| match &e.kind {
        EventKind::WorkflowCompleted {
            final_output,
            total_duration_ms,
        } => Some((final_output, total_duration_ms)),
        _ => None,
    });

    assert!(completed.is_some(), "Should have WorkflowCompleted event");
    let (output, duration) = completed.unwrap();

    // final_output is wrapped as Value::String
    let output_str = output.as_str().expect("final_output should be a string");
    assert!(
        output_str.contains("hello_output"),
        "Should have final output"
    );
    assert!(*duration > 0, "Duration should be > 0");
}

// ═══════════════════════════════════════════════════════════════════════════
// FOR_EACH PARALLELISM TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_for_each_emits_multiple_task_events() {
    let workflow = parse_workflow(
        r#"
schema: "nika/workflow@0.5"
provider: mock
tasks:
  - id: parallel_task
    for_each: ["a", "b", "c"]
    as: item
    concurrency: 3
    exec: "echo {{use.item}}"
"#,
    )
    .unwrap();

    let (event_log, rx) = EventLog::new_with_broadcast();
    let runner = Runner::with_event_log(workflow, event_log);
    let handle = tokio::spawn(async move { runner.run().await });

    let events = collect_events(rx, 5000).await;
    let _ = handle.await;

    // for_each creates task IDs like parallel_task[0], parallel_task[1], parallel_task[2]
    let completed_count = events
        .iter()
        .filter(|e| {
            matches!(
                &e.kind,
                EventKind::TaskCompleted { task_id, .. }
                if task_id.as_ref().starts_with("parallel_task[")
            )
        })
        .count();

    assert_eq!(
        completed_count, 3,
        "Should have 3 TaskCompleted events for parallel_task iterations"
    );
}
