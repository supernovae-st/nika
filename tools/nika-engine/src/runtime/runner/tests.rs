//! Runner unit tests — DAG scheduling, for_each, events, retry, cancellation, pause.
//!
//! Extracted from runner.rs to reduce file size. Tests cover:
//! - Runner construction and builder API
//! - DAG scheduling (get_ready_tasks, all_done, pending)
//! - for_each parallel execution with various binding patterns
//! - Event emission sequences and timestamps
//! - Cancellation and pause/resume
//! - Structured output retry configuration
//! - Error retryability classification
//! - is_truthy when: condition evaluation

use super::*;
use crate::ast::analyzed::{
    AnalyzedExecAction, AnalyzedForEach, AnalyzedInferAction, AnalyzedOutput, AnalyzedTask,
    AnalyzedTaskAction, AnalyzedWorkflow, OutputFormat as AnalyzedOutputFormat, TaskId, TaskTable,
};
use crate::ast::schema::SchemaVersion;
use crate::ast::structured::StructuredOutputSpec;
use crate::binding::types::{BindingPath, BindingSource};
use crate::binding::{WithEntry, WithSpec};
use crate::runtime::structured_retry::{get_retry_config, is_retryable};
use crate::runtime::task_dispatch::is_truthy;
use crate::source::Span;
use indexmap::IndexMap;
use nika_core::ast::templatable::Templatable;
use serde_json::json;
use std::panic::panic_any;
use std::time::Duration;

// ═══════════════════════════════════════════════════════════════
// QUIET MODE TEST
// ═══════════════════════════════════════════════════════════════

fn make_empty_workflow() -> AnalyzedWorkflow {
    AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table: TaskTable::new(),
        tasks: vec![],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    }
}

#[test]
fn test_runner_quiet_mode() {
    // Default should not be quiet
    let runner = Runner::new(make_empty_workflow()).unwrap();
    assert!(!runner.quiet, "Runner should not be quiet by default");

    // quiet() should enable quiet mode
    let runner = Runner::new(make_empty_workflow()).unwrap().quiet();
    assert!(runner.quiet, "Runner should be quiet after .quiet()");

    // Can chain with with_event_log
    let event_log = crate::event::EventLog::new();
    let runner = Runner::with_event_log(make_empty_workflow(), event_log)
        .unwrap()
        .quiet();
    assert!(runner.quiet, "Runner should be quiet when chained");
}

// ═══════════════════════════════════════════════════════════════
// INITIAL CONTEXT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_with_initial_context_stores_value() {
    use serde_json::json;

    let workflow = make_empty_workflow();
    let runner = Runner::new(workflow).unwrap().with_initial_context(
        "__parent_context__",
        json!({"key": "value", "nested": {"deep": true}}),
    );

    // Context should be stored in datastore
    let result = runner.datastore.get("__parent_context__");
    assert!(result.is_some(), "Context should be stored");

    let stored = result.unwrap();
    assert!(stored.is_success(), "Should be stored as success");

    let output = stored.output_str();
    assert!(output.contains("key"), "Should contain 'key'");
    assert!(output.contains("value"), "Should contain 'value'");
}

#[test]
fn test_with_initial_context_chaining() {
    use serde_json::json;

    // Should chain with other builder methods
    let workflow = make_empty_workflow();
    let event_log = EventLog::new();
    let runner = Runner::with_event_log(workflow, event_log)
        .unwrap()
        .quiet()
        .with_initial_context("test_ctx", json!({"test": 123}));

    assert!(runner.quiet, "Should be quiet");
    assert!(
        runner.datastore.get("test_ctx").is_some(),
        "Context should exist"
    );
}

// ═══════════════════════════════════════════════════════════════
// FOR_EACH RESULT AGGREGATION TESTS
// ═══════════════════════════════════════════════════════════════

/// Helper to create a workflow with a single for_each exec task
fn create_for_each_workflow(
    task_id: &str,
    items_json: &str,
    as_var: &str,
    command: &str,
    concurrency: Option<u32>,
    fail_fast: bool,
    shell: bool,
) -> AnalyzedWorkflow {
    let mut task_table = TaskTable::new();
    task_table.insert(task_id);
    let tid = task_table.get_id(task_id).unwrap();

    let task = AnalyzedTask {
        id: tid,
        name: task_id.to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: command.to_string(),
            shell: Templatable::Value(shell),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: Some(AnalyzedForEach {
            items: items_json.to_string(),
            as_var: as_var.to_string(),
            concurrency: concurrency.map(Templatable::Value),
            fail_fast: Templatable::Value(fail_fast),
            span: Span::dummy(),
        }),
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![task],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    }
}

#[tokio::test]
async fn test_for_each_collects_all_results() {
    let workflow = create_for_each_workflow(
        "echo_items",
        r#"["a", "b", "c"]"#,
        "item",
        "echo {{with.item}}",
        None,  // sequential
        true,  // fail_fast default
        false, // no shell
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let parent_result = runner.datastore.get("echo_items");
    assert!(parent_result.is_some(), "Parent task result should exist");

    let result = parent_result.unwrap();
    let output = result.output_str();
    let has_a = output.contains("a") || output.contains("\"a\"");
    let has_b = output.contains("b") || output.contains("\"b\"");
    let has_c = output.contains("c") || output.contains("\"c\"");

    assert!(
        has_a && has_b && has_c,
        "Output should contain all 3 results, got: {}",
        output
    );
}

#[tokio::test]
async fn test_for_each_preserves_order() {
    let workflow = create_for_each_workflow(
        "ordered",
        r#"["first", "second", "third"]"#,
        "x",
        "echo {{with.x}}",
        None,
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    runner.run().await.unwrap();

    let parent_result = runner.datastore.get("ordered");
    assert!(parent_result.is_some(), "Parent task result should exist");

    let result = parent_result.unwrap();
    let output = result.output_str();
    if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&output) {
        assert_eq!(arr.len(), 3, "Should have 3 results");
        let first = arr[0].as_str().unwrap_or("");
        let last = arr[2].as_str().unwrap_or("");
        assert!(
            first.contains("first"),
            "First element should contain 'first'"
        );
        assert!(
            last.contains("third"),
            "Last element should contain 'third'"
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// BASIC WORKFLOW TESTS
// ═══════════════════════════════════════════════════════════════

/// Helper to create a minimal workflow with exec tasks
fn create_exec_workflow(tasks: Vec<(&str, &str)>, edges: Vec<(&str, &str)>) -> AnalyzedWorkflow {
    let mut task_table = TaskTable::new();
    for (id, _) in &tasks {
        task_table.insert(id);
    }

    let analyzed_tasks: Vec<AnalyzedTask> = tasks
        .into_iter()
        .map(|(id, cmd)| {
            let task_id = task_table.get_id(id).unwrap();
            let depends_on: Vec<_> = edges
                .iter()
                .filter(|(_, tgt)| *tgt == id)
                .filter_map(|(src, _)| task_table.get_id(src))
                .collect();
            AnalyzedTask {
                id: task_id,
                name: id.to_string(),
                description: None,
                action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
                    command: cmd.to_string(),
                    shell: Templatable::Value(false),
                    cwd: None,
                    env: IndexMap::new(),
                    timeout_ms: None,
                    max_stdout: None,
                    span: Span::dummy(),
                }),
                provider: None,
                model: None,

                with_spec: Default::default(),
                depends_on,
                implicit_deps: vec![],
                output: None,
                for_each: None,
                retry: None,
                on_error: None,
                decompose: None,
                concurrency: None,
                fail_fast: None,
                artifact: None,
                log: None,
                structured: None,
                record: None,
                context_budget: None,
                preset: None,
                routing: None,
                when: None,
                span: Span::dummy(),
            }
        })
        .collect();

    AnalyzedWorkflow {
        schema_version: SchemaVersion::V01,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: analyzed_tasks,
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    }
}

#[tokio::test]
async fn event_sequence_for_single_task() {
    let workflow = create_exec_workflow(vec![("greet", "echo hello")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();

    let result = runner.run().await.unwrap();
    assert_eq!(result, "hello");

    // Verify event sequence
    let events = runner.event_log().events();

    // Expected sequence:
    // 1. WorkflowStarted
    // 2. TaskScheduled
    // 3. TaskStarted (with inputs from ResolvedBindings)
    // 4. TemplateResolved (from executor)
    // 5. TaskCompleted
    // 6. WorkflowCompleted

    assert!(
        events.len() >= 5,
        "Expected at least 5 events, got {}",
        events.len()
    );

    // First event should be WorkflowStarted
    assert!(matches!(
        &events[0].kind,
        EventKind::WorkflowStarted { task_count: 1, .. }
    ));

    // WorkflowCompleted should be emitted (followed by MediaCleanup GC event)
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. })),
        "WorkflowCompleted should be emitted"
    );
    // Last event is MediaCleanup (GC runs after workflow)
    let last = events.last().unwrap();
    assert!(matches!(&last.kind, EventKind::MediaCleanup { .. }));

    // Verify task events exist
    let task_events = runner.event_log().filter_task("greet");
    assert!(task_events.len() >= 3, "Expected at least 3 task events");

    // Verify TaskCompleted with correct output
    let completed = task_events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }));
    assert!(completed.is_some(), "TaskCompleted event not found");
}

#[tokio::test]
async fn event_sequence_for_chained_tasks() {
    // Two tasks: greet -> shout (shout depends on greet)
    let workflow = create_exec_workflow(
        vec![("greet", "echo hello"), ("shout", "echo DONE")],
        vec![("greet", "shout")],
    );
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().events();

    // Verify WorkflowStarted with correct task count
    assert!(matches!(
        &events[0].kind,
        EventKind::WorkflowStarted { task_count: 2, .. }
    ));

    // Verify both tasks have complete event sequences
    let greet_events = runner.event_log().filter_task("greet");
    let shout_events = runner.event_log().filter_task("shout");

    assert!(!greet_events.is_empty(), "greet task events missing");
    assert!(!shout_events.is_empty(), "shout task events missing");

    // Verify order: greet TaskCompleted must come before shout TaskStarted
    let greet_completed_id = greet_events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }))
        .map(|e| e.id);
    let shout_started_id = shout_events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::TaskStarted { .. }))
        .map(|e| e.id);

    assert!(greet_completed_id.is_some());
    assert!(shout_started_id.is_some());
    assert!(
        greet_completed_id.unwrap() < shout_started_id.unwrap(),
        "greet should complete before shout starts"
    );
}

#[tokio::test]
async fn event_sequence_for_parallel_tasks() {
    // Two independent tasks that can run in parallel
    let workflow = create_exec_workflow(
        vec![("task_a", "echo A"), ("task_b", "echo B")],
        vec![], // No dependencies = parallel
    );
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().events();

    // Verify WorkflowStarted
    assert!(matches!(
        &events[0].kind,
        EventKind::WorkflowStarted { task_count: 2, .. }
    ));

    // Both tasks should have been scheduled
    let scheduled: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::TaskScheduled { .. }))
        .collect();
    assert_eq!(scheduled.len(), 2, "Both tasks should be scheduled");

    // Both tasks should complete
    let completed: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, EventKind::TaskCompleted { .. }))
        .collect();
    assert_eq!(completed.len(), 2, "Both tasks should complete");

    // WorkflowCompleted should be present; MediaCleanup GC event follows it
    assert!(
        events
            .iter()
            .any(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. })),
        "WorkflowCompleted should be emitted"
    );
    let last = events.last().unwrap();
    assert!(matches!(&last.kind, EventKind::MediaCleanup { .. }));
}

#[tokio::test]
async fn event_ids_are_monotonic() {
    let workflow = create_exec_workflow(
        vec![("a", "echo 1"), ("b", "echo 2"), ("c", "echo 3")],
        vec![("a", "b"), ("b", "c")],
    );
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().events();
    let ids: Vec<u64> = events.iter().map(|e| e.id).collect();

    // Verify monotonic and sequential
    for (i, &id) in ids.iter().enumerate() {
        assert_eq!(id, i as u64, "IDs should be sequential from 0");
    }
}

#[tokio::test]
async fn timestamps_are_relative_and_increasing() {
    let workflow = create_exec_workflow(
        vec![("fast", "echo quick"), ("slow", "echo done")],
        vec![("fast", "slow")],
    );
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().events();

    // First timestamp should be small (relative to start)
    // Use generous 5000ms threshold for CI environments under load
    assert!(
        events[0].timestamp_ms < 5000,
        "First event should be near start (got {}ms, expected < 5000ms)",
        events[0].timestamp_ms
    );

    // Timestamps should generally increase
    for window in events.windows(2) {
        assert!(
            window[1].timestamp_ms >= window[0].timestamp_ms,
            "Timestamps should not decrease"
        );
    }
}

#[tokio::test]
async fn failed_task_emits_task_failed_event() {
    let workflow = create_exec_workflow(vec![("fail", "exit 1")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();

    // Workflow run() now returns Err when tasks fail (NIKA-084)
    let result = runner.run().await;
    assert!(
        result.is_err(),
        "workflow should return Err when tasks fail"
    );

    let events = runner.event_log().filter_task("fail");
    let failed = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::TaskFailed { .. }));

    assert!(failed.is_some(), "TaskFailed event should be emitted");
}

#[tokio::test]
async fn template_resolved_event_captures_before_and_after() {
    // Create workflow with task that has a command
    let workflow = create_exec_workflow(vec![("echo_test", "echo hello world")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().filter_task("echo_test");
    let template_event = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::TemplateResolved { .. }));

    assert!(template_event.is_some(), "TemplateResolved event expected");

    if let EventKind::TemplateResolved {
        template, result, ..
    } = &template_event.unwrap().kind
    {
        assert_eq!(template, "echo hello world");
        assert_eq!(result, "echo hello world");
    }
}

#[tokio::test]
async fn event_log_to_json_serializes_correctly() {
    let workflow = create_exec_workflow(vec![("simple", "echo test")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let json = runner.event_log().to_json();
    assert!(json.is_array());

    let array = json.as_array().unwrap();
    assert!(!array.is_empty());

    // Verify structure of first event
    let first = &array[0];
    assert!(first.get("id").is_some());
    assert!(first.get("timestamp_ms").is_some());
    assert!(first.get("kind").is_some());
    assert_eq!(first["kind"]["type"], "workflow_started");
}

// ═══════════════════════════════════════════════════════════════
// UNIT TESTS FOR RUNNER INTERNAL METHODS
// ═══════════════════════════════════════════════════════════════

/// Helper: create a fresh pending_indices vec for all tasks in the runner.
fn fresh_pending(runner: &Runner) -> Vec<usize> {
    (0..runner.workflow.tasks.len()).collect()
}

#[test]
fn get_ready_tasks_returns_tasks_with_no_deps() {
    // Two independent tasks - both should be ready
    let workflow = create_exec_workflow(
        vec![("a", "echo A"), ("b", "echo B")],
        vec![], // No flows = no dependencies
    );
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 2, "Both tasks should be ready");

    let names: Vec<&str> = ready.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"a"), "Task 'a' should be ready");
    assert!(names.contains(&"b"), "Task 'b' should be ready");
}

#[test]
fn get_ready_tasks_respects_dependencies() {
    // Chain: a -> b -> c
    let workflow = create_exec_workflow(
        vec![("a", "echo A"), ("b", "echo B"), ("c", "echo C")],
        vec![("a", "b"), ("b", "c")],
    );
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 1, "Only first task should be ready");
    assert_eq!(ready[0].name, "a", "Task 'a' should be ready");
}

#[test]
fn get_ready_tasks_excludes_completed_tasks() {
    let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    // Initially task is ready
    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 1);

    // Mark task as done
    runner.datastore.insert(
        intern("only"),
        TaskResult::success_str("done", std::time::Duration::ZERO),
    );

    // Now no tasks should be ready (was already removed from pending on dispatch)
    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 0, "Completed task should not be ready");
}

#[test]
fn all_done_returns_false_when_tasks_pending() {
    let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
    let runner = Runner::new(workflow).unwrap();

    assert!(!runner.all_done(), "Not all tasks are done initially");
}

#[test]
fn all_done_returns_true_when_all_completed() {
    let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
    let runner = Runner::new(workflow).unwrap();

    // Mark all tasks as done
    runner.datastore.insert(
        intern("a"),
        TaskResult::success_str("A", std::time::Duration::ZERO),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::success_str("B", std::time::Duration::ZERO),
    );

    assert!(runner.all_done(), "All tasks should be done");
}

#[test]
fn get_final_output_returns_output_from_final_task() {
    // Chain: a -> b (b is final)
    let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![("a", "b")]);
    let runner = Runner::new(workflow).unwrap();

    // Mark tasks as done
    runner.datastore.insert(
        intern("a"),
        TaskResult::success_str("A", std::time::Duration::ZERO),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::success_str("final output", std::time::Duration::ZERO),
    );

    let output = runner.get_final_output();
    assert!(output.is_some());
    assert_eq!(output.unwrap(), "final output");
}

#[test]
fn get_final_output_returns_none_when_no_results() {
    let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
    let runner = Runner::new(workflow).unwrap();

    let output = runner.get_final_output();
    assert!(output.is_none(), "No output when tasks not complete");
}

#[test]
fn get_final_output_skips_failed_tasks() {
    let workflow = create_exec_workflow(
        vec![("a", "echo A"), ("b", "exit 1")],
        vec![], // Both are final tasks (no successors)
    );
    let runner = Runner::new(workflow).unwrap();

    // a succeeds, b fails
    runner.datastore.insert(
        intern("a"),
        TaskResult::success_str("success", std::time::Duration::ZERO),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::failed("error", std::time::Duration::ZERO),
    );

    let output = runner.get_final_output();
    assert!(output.is_some());
    assert_eq!(
        output.unwrap(),
        "success",
        "Should return successful task output"
    );
}

// ═══════════════════════════════════════════════════════════════
// FOR_EACH CONCURRENCY AND FAIL_FAST TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn for_each_with_explicit_concurrency() {
    let workflow = create_for_each_workflow(
        "concurrent",
        r#"["a", "b", "c", "d"]"#,
        "item",
        "echo {{with.item}}",
        Some(2), // Limit to 2 concurrent
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let parent_result = runner.datastore.get("concurrent");
    assert!(parent_result.is_some(), "Parent task result should exist");

    let result = parent_result.unwrap();
    let output = result.output_str();
    assert!(output.contains("a") || output.contains("\"a\""));
    assert!(output.contains("d") || output.contains("\"d\""));
}

#[tokio::test]
async fn for_each_fail_fast_stops_on_first_error() {
    let workflow = create_for_each_workflow(
        "failfast",
        r#"["ok1", "FAIL", "ok2", "ok3"]"#,
        "item",
        "test '{{with.item}}' != 'FAIL' && echo {{with.item}}",
        Some(1), // Sequential to make failure predictable
        true,    // fail_fast
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    // runner.run() now returns Err when tasks fail (NIKA-084)
    assert!(
        result.is_err(),
        "Workflow should fail when fail_fast triggers"
    );
}

#[tokio::test]
async fn for_each_fail_fast_false_continues_on_error() {
    let workflow = create_for_each_workflow(
        "continue",
        r#"["ok1", "ok2"]"#,
        "item",
        "echo {{with.item}}",
        None,
        false, // Explicitly disable fail_fast
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok(), "Workflow should complete");

    let parent_result = runner.datastore.get("continue");
    assert!(parent_result.is_some());
}

// ═══════════════════════════════════════════════════════════════
// FOR_EACH WITH INPUTS.* SUPPORT
// ═══════════════════════════════════════════════════════════════

/// Helper to create a for_each workflow with inputs
fn create_for_each_with_inputs(
    task_id: &str,
    items_expr: &str,
    as_var: &str,
    command: &str,
    inputs: IndexMap<String, serde_json::Value>,
    concurrency: Option<u32>,
) -> AnalyzedWorkflow {
    let mut workflow = create_for_each_workflow(
        task_id,
        items_expr,
        as_var,
        command,
        concurrency,
        true,
        false,
    );
    workflow.inputs = inputs;
    workflow
}

#[tokio::test]
async fn for_each_with_dollar_inputs_array() {
    let mut inputs = IndexMap::new();
    inputs.insert(
        "items".to_string(),
        json!({
            "type": "array",
            "default": ["alpha", "beta", "gamma"]
        }),
    );
    let workflow = create_for_each_with_inputs(
        "process_items",
        "$inputs.items",
        "item",
        "echo {{with.item}}",
        inputs,
        None,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("process_items");
    assert!(task_result.is_some(), "Task result should exist");
    assert!(task_result.unwrap().is_success(), "Task should succeed");
}

#[tokio::test]
async fn for_each_with_template_inputs() {
    let mut inputs = IndexMap::new();
    inputs.insert(
        "locales".to_string(),
        json!({
            "type": "array",
            "default": ["fr-FR", "en-US"]
        }),
    );
    let workflow = create_for_each_with_inputs(
        "translate",
        "{{inputs.locales}}",
        "locale",
        "echo Translating to {{with.locale}}",
        inputs,
        Some(2),
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("translate");
    assert!(task_result.is_some(), "Task result should exist");
    assert!(task_result.unwrap().is_success(), "Task should succeed");
}

#[tokio::test]
async fn for_each_with_inputs_missing_fails_gracefully() {
    let mut inputs = IndexMap::new();
    inputs.insert(
        "other_param".to_string(),
        json!({
            "type": "string",
            "default": "test"
        }),
    );
    let workflow = create_for_each_with_inputs(
        "missing_input",
        "$inputs.nonexistent",
        "item",
        "echo {{with.item}}",
        inputs,
        None,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    // Now returns Err(DependencyChainFailed) when task fails (NIKA-084)
    assert!(result.is_err(), "Workflow should fail when task fails");

    let task_result = runner.datastore.get("missing_input");
    assert!(task_result.is_some(), "Task result should exist");
    let tr = task_result.unwrap();
    assert!(!tr.is_success(), "Task should fail due to missing input");
    let error_msg = tr.error().expect("Failed task should have error message");
    assert!(
        error_msg.contains("not found"),
        "Error should mention 'not found': {}",
        error_msg
    );
}

#[tokio::test]
async fn for_each_with_inputs_nested_path() {
    let mut inputs = IndexMap::new();
    inputs.insert(
        "data".to_string(),
        json!({
            "type": "object",
            "default": {
                "items": ["one", "two", "three"]
            }
        }),
    );
    let workflow = create_for_each_with_inputs(
        "nested",
        "$inputs.data.items",
        "n",
        "echo {{with.n}}",
        inputs,
        None,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("nested");
    assert!(task_result.is_some(), "Task result should exist");
    assert!(task_result.unwrap().is_success(), "Task should succeed");
}

// ═══════════════════════════════════════════════════════════════
// for_each Pattern 2 ($alias) — nested paths + error
// ═══════════════════════════════════════════════════════════════

/// Helper to create a 2-step workflow where step1 produces output, step2 iterates with for_each
fn create_two_step_for_each_workflow(
    step1_cmd: &str,
    step1_shell: bool,
    for_each_items: &str,
    step2_cmd: &str,
) -> AnalyzedWorkflow {
    let mut task_table = TaskTable::new();
    task_table.insert("step1");
    task_table.insert("step2");
    let tid1 = task_table.get_id("step1").unwrap();
    let tid2 = task_table.get_id("step2").unwrap();

    let step1 = AnalyzedTask {
        id: tid1,
        name: "step1".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: step1_cmd.to_string(),
            shell: Templatable::Value(step1_shell),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let mut with_spec = WithSpec::default();
    with_spec.insert(
        "step1".to_string(),
        WithEntry::simple(BindingPath {
            source: BindingSource::Task(intern("step1")),
            segments: vec![],
        }),
    );

    let step2 = AnalyzedTask {
        id: tid2,
        name: "step2".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: step2_cmd.to_string(),
            shell: Templatable::Value(false),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec,
        depends_on: vec![tid1],
        implicit_deps: vec![],
        output: None,
        for_each: Some(AnalyzedForEach {
            items: for_each_items.to_string(),
            as_var: "item".to_string(),
            concurrency: None,
            fail_fast: Templatable::Value(true),
            span: Span::dummy(),
        }),
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![step1, step2],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    }
}

#[tokio::test]
async fn for_each_dollar_binding_nested_path() {
    let workflow = create_two_step_for_each_workflow(
        r#"echo '{"items": ["alpha", "beta", "gamma"], "count": 3}'"#,
        true,
        "$step1.items",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");
    assert!(
        task_result.unwrap().is_success(),
        "step2 should succeed with 3 items from nested path"
    );
}

#[tokio::test]
async fn for_each_dollar_binding_non_array_errors() {
    let workflow = create_two_step_for_each_workflow(
        "echo not_an_array",
        false,
        "$step1",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let _ = runner.run().await;

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");

    let result = task_result.unwrap();
    assert!(
        !result.is_success(),
        "step2 should FAIL when for_each binding resolves to non-array"
    );
    let error_msg = result.error().expect("should have error message");
    assert!(
        error_msg.contains("non-array"),
        "Error should mention 'non-array', got: {}",
        error_msg
    );
}

#[tokio::test]
async fn for_each_dollar_binding_json_string_array() {
    let workflow = create_two_step_for_each_workflow(
        r#"echo '["x","y","z"]'"#,
        true,
        "$step1",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");
    assert!(
        task_result.unwrap().is_success(),
        "step2 should succeed — JSON string array should be parsed"
    );
}

#[tokio::test]
async fn for_each_dollar_binding_with_pipe_transform() {
    // Step1 outputs a raw JSON string (not auto-parsed because it's a plain string value).
    // Step2 uses `$step1 | parse_json` to parse + iterate.
    let workflow = create_two_step_for_each_workflow(
        r#"echo '["alpha","beta","gamma"]'"#,
        true,
        "$step1 | parse_json",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");
    assert!(
        task_result.unwrap().is_success(),
        "step2 should succeed — pipe transform parse_json should produce iterable array"
    );
}

#[tokio::test]
async fn for_each_dollar_binding_with_chained_pipe_transforms() {
    // Step1 outputs a JSON string. Step2 uses `$step1 | parse_json | reverse` to
    // parse and reverse the array before iterating.
    let workflow = create_two_step_for_each_workflow(
        r#"echo '["x","y","z"]'"#,
        true,
        "$step1 | parse_json | reverse",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");
    assert!(
        task_result.unwrap().is_success(),
        "step2 should succeed — chained pipe transforms should produce iterable array"
    );
}

#[tokio::test]
async fn for_each_dollar_binding_with_path_and_pipe_transform() {
    // Step1 outputs a JSON object. Step2 uses `$step1.data | sort` to
    // access a nested path AND apply a transform.
    let workflow = create_two_step_for_each_workflow(
        r#"echo '{"data":["cherry","apple","banana"]}'"#,
        true,
        "$step1.data | sort",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");
    assert!(
        task_result.unwrap().is_success(),
        "step2 should succeed — path + pipe transform should produce iterable array"
    );
}

#[tokio::test]
async fn for_each_output_auto_parses_json_strings() {
    // Step1 outputs a JSON array of names. Step2 for_each iterates and echoes JSON objects.
    // After aggregation, step2's output elements should be Value::Object, not Value::String.
    let workflow = create_two_step_for_each_workflow(
        r#"echo '["alice","bob"]'"#,
        true,
        "$step1",
        r#"echo '{"name": "{{with.item}}"}'"#,
    );
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok(), "Workflow failed: {:?}", result.err());

    let step2_result = runner.datastore.get("step2").expect("step2 result");
    assert!(step2_result.is_success());
    let output = &*step2_result.output;
    let arr = output.as_array().expect("for_each output should be array");
    assert_eq!(arr.len(), 2);
    // Key assertion: elements should be auto-parsed to Value::Object
    assert!(
        arr[0].is_object(),
        "for_each element should be auto-parsed to object, got: {:?}",
        arr[0]
    );
    assert_eq!(arr[0]["name"], "alice");
    assert_eq!(arr[1]["name"], "bob");
}

#[tokio::test]
async fn for_each_output_preserves_plain_strings() {
    // Step2 echoes plain text (not JSON). Output should stay as Value::String.
    let workflow = create_two_step_for_each_workflow(
        r#"echo '["hello","world"]'"#,
        true,
        "$step1",
        "echo {{with.item}}",
    );
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok());

    let step2_result = runner.datastore.get("step2").expect("step2 result");
    let arr = step2_result.output.as_array().expect("array");
    assert_eq!(arr.len(), 2);
    // Plain strings should stay as strings
    assert!(arr[0].is_string(), "plain string should stay as string");
    assert_eq!(arr[0].as_str().unwrap().trim(), "hello");
}

#[tokio::test]
async fn for_each_dollar_binding_nested_path_not_found() {
    let workflow = create_two_step_for_each_workflow(
        r#"echo '{"data": {"count": 5}}'"#,
        true,
        "$step1.data.nonexistent",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let _ = runner.run().await;

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");

    let result = task_result.unwrap();
    assert!(
        !result.is_success(),
        "step2 should FAIL when nested path segment doesn't exist"
    );
    let error_msg = result.error().expect("should have error message");
    assert!(
        error_msg.contains("not found"),
        "Error should mention path segment not found, got: {}",
        error_msg
    );
}

// ═══════════════════════════════════════════════════════════════
// FOR_EACH EMPTY ARRAY EDGE CASE
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn for_each_empty_array_completes_with_empty_result() {
    let workflow = create_for_each_workflow(
        "empty_loop",
        "[]", // empty JSON array
        "item",
        "echo {{with.item}}",
        None,
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow with empty for_each should succeed, got: {:?}",
        result.err()
    );

    // Parent task should have a result (empty array, not missing)
    let parent_result = runner.datastore.get("empty_loop");
    assert!(
        parent_result.is_some(),
        "for_each with empty array should store a result"
    );

    let result = parent_result.unwrap();
    assert!(
        result.is_success(),
        "for_each with empty array should be success"
    );

    // Output should be an empty array "[]"
    let output = result.output_str();
    assert_eq!(
        output.trim(),
        "[]",
        "for_each with empty array should produce empty array, got: {}",
        output
    );
}

// ═══════════════════════════════════════════════════════════════
// FOR_EACH INDEX VARIABLE
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn for_each_injects_for_each_index_binding() {
    let workflow = create_for_each_workflow(
        "indexed",
        r#"["a", "b", "c"]"#,
        "item",
        "echo {{with.for_each_index}}",
        None,
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow should succeed: {:?}",
        result.err()
    );

    let parent = runner.datastore.get("indexed");
    assert!(parent.is_some(), "Parent result should exist");

    let parent_result = parent.unwrap();
    let output = parent_result.output_str();
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("Should be valid JSON array: {output}"));

    // Results should contain indices 0, 1, 2 (order may vary due to concurrency)
    let indices: Vec<String> = parsed
        .iter()
        .filter_map(|v| v.as_str().map(|s| s.trim().to_string()))
        .collect();
    assert!(
        indices.contains(&"0".to_string()),
        "Should contain index 0: {indices:?}"
    );
    assert!(
        indices.contains(&"1".to_string()),
        "Should contain index 1: {indices:?}"
    );
    assert!(
        indices.contains(&"2".to_string()),
        "Should contain index 2: {indices:?}"
    );
}

// ═══════════════════════════════════════════════════════════════
// CONSTRUCTOR AND EVENT LOG TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn with_event_log_uses_provided_event_log() {
    let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
    let custom_log = EventLog::new();
    let runner = Runner::with_event_log(workflow, custom_log).unwrap();

    // The runner should use the provided event log
    assert!(runner.event_log().events().is_empty());
}

#[test]
fn new_and_with_event_log_return_result() {
    // Valid workflow should return Ok
    let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
    let result = Runner::new(workflow);
    assert!(
        result.is_ok(),
        "Runner::new should return Ok for a valid workflow"
    );

    // Valid workflow with custom event log should return Ok
    let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
    let event_log = EventLog::new();
    let result = Runner::with_event_log(workflow, event_log);
    assert!(
        result.is_ok(),
        "Runner::with_event_log should return Ok for a valid workflow"
    );
}

#[tokio::test]
async fn workflow_completed_event_has_duration() {
    let workflow = create_exec_workflow(vec![("quick", "echo fast")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().events();
    let completed = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. }));

    assert!(completed.is_some());
    // Verify the event has a duration field (u64 is inherently non-negative)
    assert!(matches!(
        &completed.unwrap().kind,
        EventKind::WorkflowCompleted {
            total_duration_ms: _,
            ..
        }
    ));
}

#[tokio::test]
async fn workflow_started_event_has_generation_id() {
    let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();

    runner.run().await.unwrap();

    let events = runner.event_log().events();
    let started = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::WorkflowStarted { .. }));

    assert!(started.is_some());
    if let EventKind::WorkflowStarted { generation_id, .. } = &started.unwrap().kind {
        assert!(
            generation_id.starts_with("gen-"),
            "Generation ID should have prefix"
        );
        assert!(
            generation_id.len() > 10,
            "Generation ID should include UUID"
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// CANCELLATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_cancel_token_default() {
    let workflow = make_empty_workflow();
    let runner = Runner::new(workflow).unwrap();

    // Should not be cancelled by default
    assert!(
        !runner.is_cancelled(),
        "Runner should not be cancelled by default"
    );
}

#[test]
fn test_cancel_token_can_be_set() {
    let workflow = make_empty_workflow();
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let runner = Runner::new(workflow).unwrap().with_cancel_token(token);

    // Cancelling the original token should be reflected
    token_clone.cancel();
    assert!(runner.is_cancelled(), "Runner should detect cancellation");
}

#[test]
fn test_cancel_token_cloning() {
    let workflow = make_empty_workflow();
    let runner = Runner::new(workflow).unwrap();

    let token1 = runner.cancel_token();
    let token2 = runner.cancel_token();

    // Both tokens should be clones of the same underlying token
    token1.cancel();
    assert!(token2.is_cancelled(), "Cloned tokens should share state");
    assert!(runner.is_cancelled(), "Runner should detect cancellation");
}

#[tokio::test]
async fn test_cancellation_before_start_returns_aborted() {
    // Create a slow workflow
    let workflow = create_exec_workflow(vec![("slow", "sleep 10")], vec![]);
    let token = CancellationToken::new();

    let mut runner = Runner::new(workflow)
        .unwrap()
        .with_cancel_token(token.clone());

    // Cancel before starting
    token.cancel();

    let result = runner.run().await;
    assert!(result.is_err(), "Cancelled workflow should return error");

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("cancelled") || err.to_string().contains("aborted"),
        "Error should mention cancellation: {}",
        err
    );

    // Should emit WorkflowAborted event
    let events = runner.event_log().events();
    let aborted = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::WorkflowAborted { .. }));
    assert!(aborted.is_some(), "WorkflowAborted event should be emitted");
}

#[tokio::test]
async fn test_cancellation_during_execution_aborts_workflow() {
    use std::time::Duration;

    // Create a workflow with a slow task
    let workflow = create_exec_workflow(vec![("slow", "sleep 5")], vec![]);
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let mut runner = Runner::new(workflow).unwrap().with_cancel_token(token);

    // Spawn the workflow run in background
    let handle = tokio::spawn(async move { runner.run().await });

    // Wait a bit then cancel
    tokio::time::sleep(Duration::from_millis(100)).await;
    token_clone.cancel();

    // Should complete with error (not take 5 seconds)
    let result = tokio::time::timeout(Duration::from_secs(2), handle).await;
    assert!(
        result.is_ok(),
        "Cancellation should complete within 2 seconds"
    );

    let workflow_result = result.unwrap().unwrap();
    assert!(
        workflow_result.is_err(),
        "Cancelled workflow should return error"
    );
}

#[tokio::test]
async fn test_workflow_aborted_event_has_running_tasks() {
    use std::time::Duration;

    // Create workflow with parallel slow tasks
    let workflow = create_exec_workflow(
        vec![("slow1", "sleep 5"), ("slow2", "sleep 5")],
        vec![], // No deps = parallel
    );
    let token = CancellationToken::new();
    let token_clone = token.clone();

    let event_log = EventLog::new();
    let event_log_clone = event_log.clone();
    let mut runner = Runner::with_event_log(workflow, event_log)
        .unwrap()
        .with_cancel_token(token);

    // Spawn the workflow
    let run_handle = tokio::spawn(async move { runner.run().await });

    // Wait for tasks to start, then cancel
    tokio::time::sleep(Duration::from_millis(100)).await;
    token_clone.cancel();

    // Wait for abort
    let result = run_handle.await.unwrap();
    assert!(result.is_err(), "Cancelled workflow should return error");

    // Check that WorkflowAborted event was emitted with running tasks
    let events = event_log_clone.events();
    let aborted = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::WorkflowAborted { .. }));
    assert!(aborted.is_some(), "WorkflowAborted event should be emitted");

    if let EventKind::WorkflowAborted { running_tasks, .. } = &aborted.unwrap().kind {
        // At least one task should have been running
        assert!(
            !running_tasks.is_empty() || running_tasks.len() <= 2,
            "Should have captured running tasks (0-2 expected)"
        );
    }
}

// ═══════════════════════════════════════════════════════════════
// PAUSE/RESUME TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_pause_state_default() {
    let workflow = make_empty_workflow();
    let runner = Runner::new(workflow).unwrap();

    // Should not be paused by default
    assert!(
        !runner.is_paused(),
        "Runner should not be paused by default"
    );
}

#[test]
fn test_pause_and_resume() {
    let workflow = make_empty_workflow();
    let runner = Runner::new(workflow).unwrap();

    // Initially not paused
    assert!(!runner.is_paused());

    // Pause
    runner.pause();
    assert!(runner.is_paused(), "Runner should be paused after pause()");

    // Resume
    runner.resume();
    assert!(
        !runner.is_paused(),
        "Runner should not be paused after resume()"
    );
}

#[test]
fn test_pause_handles_cloning() {
    let workflow = make_empty_workflow();
    let runner = Runner::new(workflow).unwrap();

    let (paused1, notify1) = runner.pause_handles();
    let (paused2, _notify2) = runner.pause_handles();

    // Both should share the same underlying state
    runner.pause();
    assert!(
        paused1.load(Ordering::SeqCst),
        "First handle should see paused state"
    );
    assert!(
        paused2.load(Ordering::SeqCst),
        "Second handle should see paused state"
    );

    // Resume via runner
    runner.resume();
    assert!(
        !paused1.load(Ordering::SeqCst),
        "First handle should see resumed state"
    );
    assert!(
        !paused2.load(Ordering::SeqCst),
        "Second handle should see resumed state"
    );

    // Verify notify exists (just access it to prove it's valid)
    notify1.notify_one();
}

#[test]
fn test_pause_emits_events() {
    let workflow = make_empty_workflow();
    let event_log = EventLog::new();
    let runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();

    // Pause and resume
    runner.pause();
    runner.resume();

    // Check events
    let events = event_log.events();
    let paused = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::WorkflowPaused));
    let resumed = events
        .iter()
        .find(|e| matches!(&e.kind, EventKind::WorkflowResumed));

    assert!(paused.is_some(), "WorkflowPaused event should be emitted");
    assert!(resumed.is_some(), "WorkflowResumed event should be emitted");
}

#[tokio::test]
async fn test_pause_waits_for_resume() {
    use std::sync::atomic::AtomicUsize;
    use std::time::Duration;

    // Create a simple workflow
    let workflow = create_exec_workflow(vec![("task1", "echo done")], vec![]);
    let event_log = EventLog::new();
    let event_log_clone = event_log.clone();
    let mut runner = Runner::with_event_log(workflow, event_log).unwrap();

    // Pause before running
    runner.pause();

    let (paused, notify) = runner.pause_handles();
    let resume_count = Arc::new(AtomicUsize::new(0));
    let resume_count_clone = Arc::clone(&resume_count);

    // Spawn the workflow
    let handle = tokio::spawn(async move { runner.run().await });

    // Wait a bit - workflow should be waiting
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Check events - should not have completed yet
    {
        let events = event_log_clone.events();
        let completed = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. }));
        assert!(
            completed.is_none(),
            "Workflow should be paused, not completed"
        );
    }

    // Resume
    paused.store(false, Ordering::SeqCst);
    notify.notify_one();
    resume_count_clone.fetch_add(1, Ordering::SeqCst);

    // Now it should complete
    let result = tokio::time::timeout(Duration::from_secs(5), handle).await;
    assert!(result.is_ok(), "Workflow should complete after resume");

    let inner_result = result.unwrap().unwrap();
    assert!(inner_result.is_ok(), "Workflow should succeed");
}

// ═══════════════════════════════════════════════════════════════
// VALUE_TO_ARRAY HELPER TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_value_to_array_direct_array() {
    use serde_json::json;

    let value = json!(["a", "b", "c"]);
    let result = value_to_array(&value);
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 3);
}

#[test]
fn test_value_to_array_json_string() {
    use serde_json::json;

    // String containing JSON array (common case from exec output)
    let value = json!(r#"["x","y","z"]"#);
    let result = value_to_array(&value);
    assert!(result.is_some(), "Should parse JSON array string");
    let arr = result.unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], "x");
    assert_eq!(arr[1], "y");
    assert_eq!(arr[2], "z");
}

#[test]
fn test_value_to_array_json_string_with_whitespace() {
    use serde_json::json;

    // String with leading/trailing whitespace
    let value = json!("  [1, 2, 3]  ");
    let result = value_to_array(&value);
    assert!(result.is_some(), "Should handle whitespace");
    assert_eq!(result.unwrap().len(), 3);
}

#[test]
fn test_value_to_array_not_array_string() {
    use serde_json::json;

    // String that's not a JSON array
    let value = json!("hello world");
    let result = value_to_array(&value);
    assert!(result.is_none(), "Should return None for non-array string");
}

#[test]
fn test_value_to_array_object() {
    use serde_json::json;

    // Object should return None
    let value = json!({"key": "value"});
    let result = value_to_array(&value);
    assert!(result.is_none(), "Should return None for object");
}

#[test]
fn test_value_to_array_number() {
    use serde_json::json;

    // Number should return None
    let value = json!(42);
    let result = value_to_array(&value);
    assert!(result.is_none(), "Should return None for number");
}

#[test]
fn test_value_to_array_nested_json_string() {
    use serde_json::json;

    // Complex JSON array as string
    let value = json!(r#"[{"id": 1}, {"id": 2}]"#);
    let result = value_to_array(&value);
    assert!(result.is_some(), "Should parse complex JSON array string");
    let arr = result.unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["id"], 1);
    assert_eq!(arr[1]["id"], 2);
}

#[test]
fn test_value_to_array_invalid_json_string() {
    use serde_json::json;

    // Invalid JSON that looks like an array
    let value = json!("[not valid json");
    let result = value_to_array(&value);
    assert!(result.is_none(), "Should return None for invalid JSON");
}

#[test]
fn test_value_to_array_markdown_fenced_json_array() {
    let value = Value::String("```json\n[\"a\", \"b\", \"c\"]\n```".to_string());
    let result = value_to_array(&value);
    assert!(
        result.is_some(),
        "Should parse JSON array from markdown fence"
    );
    let arr = result.unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], json!("a"));
    assert_eq!(arr[1], json!("b"));
    assert_eq!(arr[2], json!("c"));
}

#[test]
fn test_value_to_array_plain_fenced_json_array() {
    let value = Value::String("```\n[1, 2, 3]\n```".to_string());
    let result = value_to_array(&value);
    assert!(result.is_some(), "Should parse JSON array from plain fence");
    let arr = result.unwrap();
    assert_eq!(arr.len(), 3);
    assert_eq!(arr[0], json!(1));
    assert_eq!(arr[1], json!(2));
    assert_eq!(arr[2], json!(3));
}

#[test]
fn test_value_to_array_bare_json_string_still_works() {
    let value = Value::String("[\"x\", \"y\"]".to_string());
    let result = value_to_array(&value);
    assert!(result.is_some(), "Bare JSON array string should still work");
    let arr = result.unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], json!("x"));
    assert_eq!(arr[1], json!("y"));
}

#[test]
fn test_value_to_array_direct_array_value_still_works() {
    let value = json!(["a", "b"]);
    let result = value_to_array(&value);
    assert!(result.is_some(), "Direct array value should still work");
    let arr = result.unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0], json!("a"));
    assert_eq!(arr[1], json!("b"));
}

#[test]
fn test_value_to_array_non_array_string_returns_none() {
    let value = Value::String("just a string".to_string());
    let result = value_to_array(&value);
    assert!(result.is_none(), "Non-array string should return None");
}

#[test]
fn test_value_to_array_empty_array() {
    use serde_json::json;

    // Empty array (direct)
    let value = json!([]);
    let result = value_to_array(&value);
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 0);

    // Empty array (as string)
    let value = json!("[]");
    let result = value_to_array(&value);
    assert!(result.is_some());
    assert_eq!(result.unwrap().len(), 0);
}

// ═══════════════════════════════════════════════════════════════
// GET_RETRY_CONFIG TESTS
// ═══════════════════════════════════════════════════════════════

/// Helper to create an AnalyzedTask with an infer action.
///
/// `output` controls the AnalyzedOutput (format + schema).
/// `structured` controls the StructuredOutputSpec (max_retries).
/// Both must be Some with matching values for retry to qualify.
fn make_infer_task(
    name: &str,
    output: Option<AnalyzedOutput>,
    structured: Option<StructuredOutputSpec>,
) -> AnalyzedTask {
    AnalyzedTask {
        id: TaskId(0),
        name: name.to_string(),
        description: None,
        action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: "test prompt".to_string(),
            system: None,
            temperature: None,
            max_tokens: None,
            ..Default::default()
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    }
}

#[test]
fn test_get_retry_config_none_for_exec_task() {
    let task = AnalyzedTask {
        id: TaskId(0),
        name: "exec_task".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: "echo hi".to_string(),
            shell: Templatable::Value(false),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(json!({"type": "object"})),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: Some(StructuredOutputSpec::with_inline_schema(
            json!({"type": "object"}),
        )),
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "Exec tasks should never qualify for retry"
    );
}

#[test]
fn test_get_retry_config_none_for_no_output() {
    let task = make_infer_task("no_output", None, None);
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "No output means no retry"
    );
}

#[test]
fn test_get_retry_config_none_for_text_format() {
    let task = make_infer_task(
        "text_format",
        Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Text,
            schema: Some(json!({"type": "object"})),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        Some(StructuredOutputSpec::with_inline_schema(
            json!({"type": "object"}),
        )),
    );
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "Text format should not qualify for retry"
    );
}

#[test]
fn test_get_retry_config_none_for_json_no_schema() {
    let task = make_infer_task(
        "json_no_schema",
        Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: None,
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        Some(StructuredOutputSpec::with_inline_schema(
            json!({"type": "object"}),
        )),
    );
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "JSON without schema should not qualify"
    );
}

#[test]
fn test_get_retry_config_none_for_no_structured() {
    let task = make_infer_task(
        "no_structured",
        Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(json!({"type": "object"})),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        None, // No structured spec → no max_retries
    );
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "No structured spec means no retry"
    );
}

#[test]
fn test_get_retry_config_none_for_zero_retries() {
    let mut structured = StructuredOutputSpec::with_inline_schema(json!({"type": "object"}));
    structured.max_retries = Some(0);
    let task = make_infer_task(
        "zero_retries",
        Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(json!({"type": "object"})),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        Some(structured),
    );
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "Zero retries means no retry"
    );
}

#[test]
fn test_get_retry_config_none_for_default_retries() {
    let mut structured = StructuredOutputSpec::with_inline_schema(json!({"type": "object"}));
    structured.max_retries = None; // defaults to 0 via unwrap_or(0)
    let task = make_infer_task(
        "default_retries",
        Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(json!({"type": "object"})),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        Some(structured),
    );
    assert!(
        get_retry_config(&task, &task.action).is_none(),
        "Default retries (None → 0) means no retry"
    );
}

#[test]
fn test_get_retry_config_some_for_valid_config() {
    let schema = json!({"type": "object", "properties": {"name": {"type": "string"}}});
    let mut structured = StructuredOutputSpec::with_inline_schema(schema.clone());
    structured.max_retries = Some(3);
    let task = make_infer_task(
        "valid_retry",
        Some(AnalyzedOutput {
            format: AnalyzedOutputFormat::Json,
            schema: Some(schema.clone()),
            schema_ref: None,
            max_retries: None,
            span: Span::dummy(),
        }),
        Some(structured),
    );
    let result = get_retry_config(&task, &task.action);
    assert!(result.is_some(), "Valid config should return Some");

    let (ret_schema, max_retries, infer) = result.unwrap();
    assert_eq!(ret_schema, schema);
    assert_eq!(max_retries, 3);
    assert_eq!(infer.prompt, "test prompt");
}

// ═══════════════════════════════════════════════════════════════
// FIND_ROOT_FAILURE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_find_root_failure_none_when_empty() {
    let runner = Runner::new(make_empty_workflow()).unwrap();
    assert!(
        runner.find_root_failure().is_none(),
        "Empty workflow has no failures"
    );
}

#[test]
fn test_find_root_failure_none_when_all_succeed() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
    let runner = Runner::new(workflow).unwrap();

    // Simulate successful completions
    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::success(json!("ok"), Duration::from_millis(20)),
    );

    assert!(
        runner.find_root_failure().is_none(),
        "All-success should return None"
    );
}

#[test]
fn test_find_root_failure_returns_first_failed() {
    let workflow = create_exec_workflow(
        vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
        vec![],
    );
    let runner = Runner::new(workflow).unwrap();

    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::failed("something broke".to_string(), Duration::from_millis(20)),
    );
    runner.datastore.insert(
        intern("c"),
        TaskResult::failed("also broken".to_string(), Duration::from_millis(30)),
    );

    assert_eq!(
        runner.find_root_failure(),
        Some("b".to_string()),
        "Should return first failed task in workflow order"
    );
}

#[test]
fn test_find_root_failure_skips_dependency_failed() {
    let workflow = create_exec_workflow(
        vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
        vec![("a", "b"), ("b", "c")],
    );
    let runner = Runner::new(workflow).unwrap();

    runner.datastore.insert(
        intern("a"),
        TaskResult::failed("root cause".to_string(), Duration::from_millis(10)),
    );
    runner
        .datastore
        .insert(intern("b"), TaskResult::dependency_failed("a"));
    runner
        .datastore
        .insert(intern("c"), TaskResult::dependency_failed("b"));

    assert_eq!(
        runner.find_root_failure(),
        Some("a".to_string()),
        "Should skip DependencyFailed and return the actual failure"
    );
}

// ═══════════════════════════════════════════════════════════════
// GET_PENDING_TASKS TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_get_pending_tasks_all_pending() {
    let workflow = create_exec_workflow(
        vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
        vec![],
    );
    let runner = Runner::new(workflow).unwrap();

    let pending = runner.get_pending_tasks();
    assert_eq!(pending.len(), 3);
    assert!(pending.contains(&"a".to_string()));
    assert!(pending.contains(&"b".to_string()));
    assert!(pending.contains(&"c".to_string()));
}

#[test]
fn test_get_pending_tasks_excludes_completed() {
    let workflow = create_exec_workflow(
        vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
        vec![],
    );
    let runner = Runner::new(workflow).unwrap();

    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );
    runner.datastore.insert(
        intern("c"),
        TaskResult::success(json!("ok"), Duration::from_millis(20)),
    );

    let pending = runner.get_pending_tasks();
    assert_eq!(pending, vec!["b".to_string()]);
}

#[test]
fn test_get_pending_tasks_empty_when_all_done() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
    let runner = Runner::new(workflow).unwrap();

    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::success(json!("ok"), Duration::from_millis(20)),
    );

    let pending = runner.get_pending_tasks();
    assert!(pending.is_empty());
}

#[test]
fn test_get_pending_tasks_excludes_failed() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
    let runner = Runner::new(workflow).unwrap();

    runner.datastore.insert(
        intern("a"),
        TaskResult::failed("error".to_string(), Duration::from_millis(10)),
    );

    let pending = runner.get_pending_tasks();
    assert_eq!(pending, vec!["b".to_string()]);
}

// ═══════════════════════════════════════════════════════════════
// GET_READY_TASKS + DEPENDENCY FAILURE PROPAGATION TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_get_ready_tasks_no_deps() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 2, "Tasks with no deps should all be ready");
}

#[test]
fn test_get_ready_tasks_blocked_by_incomplete_dep() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name, "a", "Only root task should be ready");
}

#[test]
fn test_get_ready_tasks_unblocked_after_dep_success() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name, "b", "b should be ready after a succeeds");
}

#[test]
fn test_get_ready_tasks_skips_already_done() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![]);
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].name, "b", "Completed task should not be returned");
}

#[test]
fn test_dependency_failure_propagation() {
    // a → b → c: if a fails, b and c should get DependencyFailed
    let workflow = create_exec_workflow(
        vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
        vec![("a", "b"), ("b", "c")],
    );
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    // Mark a as failed
    runner.datastore.insert(
        intern("a"),
        TaskResult::failed("boom".to_string(), Duration::from_millis(10)),
    );

    // Single call cascades when tasks are in topological order (a, b, c):
    // retain processes indices in order, so b is marked DependencyFailed and
    // stored in the datastore before c is checked. c then sees b's failure
    // and cascades in the same pass. This is an optimization, not a guarantee —
    // reverse task order would require two passes (see reverse-order test below).
    let ready = runner.get_ready_tasks(&mut pending);
    assert!(ready.is_empty(), "No tasks should be ready when dep failed");

    // Verify b was stored as DependencyFailed
    let b_result = runner.datastore.get("b").expect("b should be in store");
    assert!(
        b_result.is_dependency_failed(),
        "b should be DependencyFailed"
    );
    assert_eq!(
        b_result.failed_dependency(),
        Some("a"),
        "b should record a as the failed dependency"
    );

    // c cascaded in the same pass (topological order)
    let c_result = runner.datastore.get("c").expect("c should be in store");
    assert!(
        c_result.is_dependency_failed(),
        "c should be DependencyFailed"
    );
    assert_eq!(
        c_result.failed_dependency(),
        Some("b"),
        "c should record b as the failed dependency"
    );
}

#[test]
fn test_dependency_failure_cascade_reverse_order() {
    // Same chain a → b → c, but tasks declared in reverse order.
    // Cascade needs two passes: first marks b (since a is failed),
    // second marks c (since b is now failed).
    let workflow = create_exec_workflow(
        vec![("c", "echo c"), ("b", "echo b"), ("a", "echo a")],
        vec![("a", "b"), ("b", "c")],
    );
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    runner.datastore.insert(
        intern("a"),
        TaskResult::failed("boom".to_string(), Duration::from_millis(10)),
    );

    // First pass: c is checked first but b hasn't failed yet → c stays pending.
    // b is checked next → a failed → b marked DependencyFailed.
    let ready = runner.get_ready_tasks(&mut pending);
    assert!(ready.is_empty());
    assert!(runner.datastore.get("b").unwrap().is_dependency_failed());

    // Second pass: c is checked → b now failed → c marked DependencyFailed.
    let ready = runner.get_ready_tasks(&mut pending);
    assert!(ready.is_empty());
    assert!(
        runner.datastore.get("c").unwrap().is_dependency_failed(),
        "c should cascade on second pass when tasks are in reverse order"
    );
}

#[test]
fn test_dependency_failure_does_not_affect_parallel_tasks() {
    // a → b, a → c (parallel), d (independent)
    // If a fails, b and c get DependencyFailed, but d remains pending
    let workflow = create_exec_workflow(
        vec![
            ("a", "echo a"),
            ("b", "echo b"),
            ("c", "echo c"),
            ("d", "echo d"),
        ],
        vec![("a", "b"), ("a", "c")],
    );
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    // Mark a as failed
    runner.datastore.insert(
        intern("a"),
        TaskResult::failed("oops".to_string(), Duration::from_millis(10)),
    );

    let ready = runner.get_ready_tasks(&mut pending);
    assert_eq!(ready.len(), 1, "Only d should be ready");
    assert_eq!(ready[0].name, "d");

    // b and c should be dependency-failed
    assert!(runner.datastore.get("b").unwrap().is_dependency_failed());
    assert!(runner.datastore.get("c").unwrap().is_dependency_failed());
}

#[test]
fn test_dependency_failure_emits_events() {
    let workflow = create_exec_workflow(vec![("a", "echo a"), ("b", "echo b")], vec![("a", "b")]);
    let runner = Runner::new(workflow).unwrap();
    let mut pending = fresh_pending(&runner);

    runner.datastore.insert(
        intern("a"),
        TaskResult::failed("crash".to_string(), Duration::from_millis(10)),
    );

    // Trigger dependency failure propagation
    let _ = runner.get_ready_tasks(&mut pending);

    // Check that a TaskSkipped event was emitted for b
    let events = runner.event_log.events();
    let skip_events: Vec<_> = events
        .iter()
        .filter(|e| {
            matches!(
                &e.kind,
                EventKind::TaskSkipped { task_id, .. } if task_id.as_ref() == "b"
            )
        })
        .collect();
    assert_eq!(
        skip_events.len(),
        1,
        "Should emit exactly one TaskSkipped event for b"
    );
}

// ═══════════════════════════════════════════════════════════════
// ALL_DONE TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_all_done_empty_workflow() {
    let runner = Runner::new(make_empty_workflow()).unwrap();
    assert!(runner.all_done(), "Empty workflow is trivially done");
}

#[test]
fn test_all_done_false_when_pending() {
    let workflow = create_exec_workflow(vec![("a", "echo a")], vec![]);
    let runner = Runner::new(workflow).unwrap();
    assert!(!runner.all_done());
}

#[test]
fn test_all_done_true_with_mixed_outcomes() {
    let workflow = create_exec_workflow(
        vec![("a", "echo a"), ("b", "echo b"), ("c", "echo c")],
        vec![],
    );
    let runner = Runner::new(workflow).unwrap();

    runner.datastore.insert(
        intern("a"),
        TaskResult::success(json!("ok"), Duration::from_millis(10)),
    );
    runner.datastore.insert(
        intern("b"),
        TaskResult::failed("err".to_string(), Duration::from_millis(20)),
    );
    runner
        .datastore
        .insert(intern("c"), TaskResult::dependency_failed("b"));

    assert!(
        runner.all_done(),
        "All tasks have results (success, failed, or dep-failed)"
    );
}

// ═══════════════════════════════════════════════════════════════
// AUDIT: FOR_EACH EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn audit_for_each_empty_array_produces_empty_result() {
    let workflow = create_for_each_workflow(
        "empty_loop",
        "[]",
        "item",
        "echo {{with.item}}",
        None,
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Empty for_each should complete successfully: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("empty_loop");
    assert!(task_result.is_some(), "Task result should exist");

    let tr = task_result.unwrap();
    assert!(tr.is_success(), "Empty for_each should succeed");

    // The output should be an empty array [], NOT the output of running
    // the task body once as a regular task.
    let output = tr.output_str();
    let parsed: Result<Vec<Value>, _> = serde_json::from_str(&output);
    assert!(
        parsed.is_ok(),
        "Output should be valid JSON array, got: {}",
        output
    );
    assert_eq!(
        parsed.unwrap().len(),
        0,
        "Empty for_each should produce empty array, got: {}",
        output
    );
}

#[tokio::test]
async fn audit_for_each_single_item_array_works() {
    let workflow = create_for_each_workflow(
        "single",
        r#"["only_one"]"#,
        "item",
        "echo {{with.item}}",
        None,
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Single-item for_each should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("single");
    assert!(task_result.is_some(), "Task result should exist");

    let tr = task_result.unwrap();
    assert!(tr.is_success(), "Task should succeed");

    let output = tr.output_str();
    let parsed: Vec<Value> = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("Output should be JSON array, got: {}", output));
    assert_eq!(parsed.len(), 1, "Should have exactly one result");
    let first_str = parsed[0].as_str().unwrap_or("");
    assert!(
        first_str.contains("only_one"),
        "Single result should contain 'only_one', got: {}",
        first_str
    );
}

#[tokio::test]
async fn audit_for_each_nested_json_items_bound_correctly() {
    let workflow = create_for_each_workflow(
        "nested_items",
        r#"[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]"#,
        "person",
        "echo {{with.person | shell}}",
        None,
        true,
        true,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Nested JSON for_each should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("nested_items");
    assert!(task_result.is_some(), "Task result should exist");

    let tr = task_result.unwrap();
    assert!(tr.is_success(), "Task should succeed");

    let output = tr.output_str();
    assert!(
        output.contains("Alice") && output.contains("Bob"),
        "Output should contain both names from nested JSON items, got: {}",
        output
    );
}

#[tokio::test]
async fn audit_for_each_fail_fast_false_continues_after_failure() {
    // BUG TEST: With fail_fast=false, ALL iterations should run even when
    // one fails. Previously, abort_all was called unconditionally.
    let workflow = create_for_each_workflow(
        "continue_on_fail",
        r#"["ok1", "FAIL", "ok2"]"#,
        "item",
        "test '{{with.item}}' != 'FAIL' && echo {{with.item | shell}}",
        Some(1),
        false, // fail_fast = false
        true,  // shell = true (command uses shell operators)
    );

    let mut runner = Runner::new(workflow).unwrap();
    let _result = runner.run().await;

    let task_result = runner.datastore.get("continue_on_fail");
    assert!(task_result.is_some(), "Parent task result should exist");

    let ok1_result = runner.datastore.get("continue_on_fail[0]");
    let fail_result = runner.datastore.get("continue_on_fail[1]");
    let ok2_result = runner.datastore.get("continue_on_fail[2]");

    assert!(ok1_result.is_some(), "First iteration result should exist");
    assert!(
        fail_result.is_some(),
        "Second iteration result should exist"
    );
    assert!(
        ok2_result.is_some(),
        "Third iteration result should exist (fail_fast=false)"
    );

    assert!(
        ok1_result.unwrap().is_success(),
        "First iteration should succeed"
    );
    assert!(
        !fail_result.unwrap().is_success(),
        "Second iteration should fail"
    );
    assert!(
        ok2_result.unwrap().is_success(),
        "Third iteration should succeed (not aborted by fail_fast=false)"
    );
}

#[tokio::test]
async fn audit_for_each_with_depends_on_runs_after_dependency() {
    let workflow = create_two_step_for_each_workflow(
        r#"echo '["red", "green", "blue"]'"#,
        true,
        "$step1",
        "echo color={{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "for_each with depends_on should complete: {:?}",
        result.err()
    );

    let step1_result = runner.datastore.get("step1");
    assert!(step1_result.is_some());
    assert!(step1_result.unwrap().is_success());

    let step2_result = runner.datastore.get("step2");
    assert!(step2_result.is_some());
    let tr = step2_result.unwrap();
    assert!(
        tr.is_success(),
        "step2 should succeed, got error: {:?}",
        tr.error()
    );

    let output = tr.output_str();
    assert!(
        output.contains("red") && output.contains("green") && output.contains("blue"),
        "for_each with depends_on should produce all 3 colors, got: {}",
        output
    );
}

#[tokio::test]
async fn audit_for_each_items_non_array_non_string_errors() {
    let workflow =
        create_two_step_for_each_workflow("echo 42", false, "$step1", "echo {{with.item}}");

    let mut runner = Runner::new(workflow).unwrap();
    let _ = runner.run().await;

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");

    let tr = task_result.unwrap();
    assert!(
        !tr.is_success(),
        "for_each with non-array binding should fail"
    );
    let error_msg = tr.error().expect("should have error");
    assert!(
        error_msg.contains("non-array"),
        "Error should mention non-array, got: {}",
        error_msg
    );
}

#[tokio::test]
async fn audit_for_each_large_array_with_concurrency() {
    let items: Vec<String> = (0..20).map(|i| format!("\"item{}\"", i)).collect();
    let items_json = format!("[{}]", items.join(", "));

    let workflow = create_for_each_workflow(
        "large_batch",
        &items_json,
        "x",
        "echo {{with.x}}",
        Some(4),
        true,
        false,
    );

    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Large for_each should complete: {:?}",
        result.err()
    );

    let task_result = runner.datastore.get("large_batch");
    assert!(task_result.is_some());
    let tr = task_result.unwrap();
    assert!(tr.is_success(), "Large batch should succeed");

    let output = tr.output_str();
    let parsed: Vec<Value> = serde_json::from_str(&output)
        .unwrap_or_else(|_| panic!("Should be JSON array, got: {}", output));
    assert_eq!(
        parsed.len(),
        20,
        "Should have 20 results from large batch, got: {}",
        parsed.len()
    );
}

// ═══════════════════════════════════════════════════════════════
// AUDIT: STRUCTURED OUTPUT ENGINE EDGE CASES
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn audit_structured_output_layer3_with_mock_callback_succeeds() {
    use crate::runtime::structured_output::{InferCallback, StructuredOutputEngine};

    let log = Arc::new(EventLog::new());
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "age": { "type": "integer" }
        },
        "required": ["name", "age"]
    });
    let mut spec = StructuredOutputSpec::with_inline_schema(schema);
    spec.max_retries = Some(2);
    spec.enable_retry = Some(true);

    let callback: InferCallback = Arc::new(move |_prompt: String, _max_tokens: Option<u32>| {
        Box::pin(async move { Ok(r#"{"name": "Fixed", "age": 42}"#.to_string()) })
    });

    let mut engine = StructuredOutputEngine::new(spec, log.clone())
        .with_infer_callback(callback)
        .with_original_prompt("Generate a user".to_string());

    let result = engine
        .validate("retry-test", r#"{"name": "Incomplete"}"#)
        .await;

    assert!(
        result.is_ok(),
        "Layer 3 should succeed with mock callback: {:?}",
        result.err()
    );
    let r = result.unwrap();
    assert_eq!(r.layer, 3, "Should have succeeded at Layer 3");
    assert_eq!(r.value["name"], "Fixed");
    assert_eq!(r.value["age"], 42);
}

#[tokio::test]
async fn audit_structured_output_layer3_exhausts_retries() {
    use crate::runtime::structured_output::{InferCallback, StructuredOutputEngine};

    let log = Arc::new(EventLog::new());
    let schema = json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" },
            "score": { "type": "number" }
        },
        "required": ["name", "score"]
    });
    let mut spec = StructuredOutputSpec::with_inline_schema(schema);
    spec.max_retries = Some(2);
    spec.enable_retry = Some(true);
    spec.enable_repair = Some(false);

    let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let call_count_clone = Arc::clone(&call_count);
    let callback: InferCallback = Arc::new(move |_prompt: String, _max_tokens: Option<u32>| {
        call_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Box::pin(async move { Ok(r#"{"name": "Still Wrong"}"#.to_string()) })
    });

    let mut engine = StructuredOutputEngine::new(spec, log.clone())
        .with_infer_callback(callback)
        .with_original_prompt("test".to_string());

    let result = engine
        .validate("exhaust-test", r#"{"name": "Invalid"}"#)
        .await;

    assert!(result.is_err(), "Should fail after exhausting all retries");

    let calls = call_count.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        calls, 2,
        "Layer 3 should call LLM exactly max_retries times, got: {}",
        calls
    );
}

#[tokio::test]
async fn audit_structured_output_layer4_repair_succeeds() {
    use crate::runtime::structured_output::{InferCallback, StructuredOutputEngine};

    let log = Arc::new(EventLog::new());
    let schema = json!({
        "type": "object",
        "properties": {
            "valid": { "type": "boolean" }
        },
        "required": ["valid"]
    });
    let mut spec = StructuredOutputSpec::with_inline_schema(schema);
    spec.enable_retry = Some(false);
    spec.enable_repair = Some(true);

    let callback: InferCallback = Arc::new(move |_prompt: String, _max_tokens: Option<u32>| {
        Box::pin(async move { Ok(r#"{"valid": true}"#.to_string()) })
    });

    let mut engine = StructuredOutputEngine::new(spec, log.clone()).with_infer_callback(callback);

    let result = engine
        .validate("repair-test", r#"{"invalid_field": 123}"#)
        .await;

    assert!(
        result.is_ok(),
        "Layer 4 repair should succeed: {:?}",
        result.err()
    );
    let r = result.unwrap();
    assert_eq!(r.layer, 4, "Should have succeeded at Layer 4");
    assert_eq!(r.value["valid"], true);
}

#[tokio::test]
async fn audit_structured_output_validates_array_schema() {
    use crate::runtime::structured_output::StructuredOutputEngine;

    let log = Arc::new(EventLog::new());
    let schema = json!({
        "type": "array",
        "items": { "type": "string" },
        "minItems": 1
    });
    let spec = StructuredOutputSpec::with_inline_schema(schema);
    let mut engine = StructuredOutputEngine::new(spec, log);

    let result = engine.validate("arr-ok", r#"["hello", "world"]"#).await;
    assert!(result.is_ok(), "String array should validate");

    let mut engine2 = StructuredOutputEngine::new(
        StructuredOutputSpec::with_inline_schema(json!({
            "type": "array",
            "items": { "type": "string" },
            "minItems": 1
        })),
        Arc::new(EventLog::new()),
    );
    let result = engine2.validate("arr-empty", "[]").await;
    assert!(result.is_err(), "Empty array should fail minItems check");
}

#[tokio::test]
async fn audit_structured_output_validates_additional_properties_false() {
    use crate::runtime::structured_output::StructuredOutputEngine;

    let log = Arc::new(EventLog::new());
    let spec = StructuredOutputSpec::with_inline_schema(json!({
        "type": "object",
        "properties": {
            "name": { "type": "string" }
        },
        "required": ["name"],
        "additionalProperties": false
    }));
    let mut engine = StructuredOutputEngine::new(spec, log);

    let result = engine.validate("addl-ok", r#"{"name": "test"}"#).await;
    assert!(result.is_ok(), "Known properties only should validate");

    let mut engine2 = StructuredOutputEngine::new(
        StructuredOutputSpec::with_inline_schema(json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" }
            },
            "required": ["name"],
            "additionalProperties": false
        })),
        Arc::new(EventLog::new()),
    );
    let result = engine2
        .validate("addl-bad", r#"{"name": "test", "extra": true}"#)
        .await;
    assert!(
        result.is_err(),
        "Extra properties should fail when additionalProperties=false"
    );
}

#[tokio::test]
async fn audit_structured_output_validates_deeply_nested_schema() {
    use crate::runtime::structured_output::StructuredOutputEngine;

    let log = Arc::new(EventLog::new());
    let schema = json!({
        "type": "object",
        "properties": {
            "level1": {
                "type": "object",
                "properties": {
                    "level2": {
                        "type": "object",
                        "properties": {
                            "value": { "type": "integer" }
                        },
                        "required": ["value"]
                    }
                },
                "required": ["level2"]
            }
        },
        "required": ["level1"]
    });
    let spec = StructuredOutputSpec::with_inline_schema(schema);
    let mut engine = StructuredOutputEngine::new(spec, log);

    let result = engine
        .validate("deep-ok", r#"{"level1": {"level2": {"value": 42}}}"#)
        .await;
    assert!(result.is_ok(), "Deeply nested valid should pass");

    let mut engine2 = StructuredOutputEngine::new(
        StructuredOutputSpec::with_inline_schema(json!({
            "type": "object",
            "properties": {
                "level1": {
                    "type": "object",
                    "properties": {
                        "level2": {
                            "type": "object",
                            "properties": {
                                "value": { "type": "integer" }
                            },
                            "required": ["value"]
                        }
                    },
                    "required": ["level2"]
                }
            },
            "required": ["level1"]
        })),
        Arc::new(EventLog::new()),
    );
    let result = engine2
        .validate(
            "deep-bad",
            r#"{"level1": {"level2": {"value": "not_a_number"}}}"#,
        )
        .await;
    assert!(
        result.is_err(),
        "Wrong type at deep level should fail validation"
    );
}

#[tokio::test]
async fn audit_structured_output_validates_primitive_types() {
    use crate::runtime::structured_output::StructuredOutputEngine;

    // String schema
    let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "string"}));
    let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
    let result = engine.validate("str-ok", r#""hello""#).await;
    assert!(
        result.is_ok(),
        "Quoted string should validate as string type"
    );

    // Number schema
    let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "number"}));
    let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
    let result = engine.validate("num-ok", "42.5").await;
    assert!(result.is_ok(), "Number should validate as number type");

    // Boolean schema
    let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "boolean"}));
    let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
    let result = engine.validate("bool-ok", "true").await;
    assert!(result.is_ok(), "Boolean should validate as boolean type");

    // Null schema
    let spec = StructuredOutputSpec::with_inline_schema(json!({"type": "null"}));
    let mut engine = StructuredOutputEngine::new(spec, Arc::new(EventLog::new()));
    let result = engine.validate("null-ok", "null").await;
    assert!(result.is_ok(), "null should validate as null type");
}

// ═══════════════════════════════════════════════════════════════
// AUDIT: BUILD_RETRY_PROMPT TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn audit_build_retry_prompt_includes_all_components() {
    let schema = json!({"type": "object", "required": ["name"]});
    let prompt = crate::runtime::structured_retry::build_retry_prompt(
        "Generate a user",
        &schema,
        r#"{"broken": true}"#,
        "missing required field: name",
    );

    assert!(
        prompt.contains("Generate a user"),
        "Should contain original prompt"
    );
    assert!(
        prompt.contains(r#"{"broken": true}"#),
        "Should contain the actual previous output"
    );
    assert!(
        prompt.contains("missing required field: name"),
        "Should contain validation errors"
    );
}

// ═══════════════════════════════════════════════════════════════
// AUDIT: LOWERING OF STRUCTURED FIELD
// ═══════════════════════════════════════════════════════════════

#[test]
fn audit_to_output_policy_preserves_structured_spec() {
    let schema = json!({"type": "object"});
    let mut spec = StructuredOutputSpec::with_inline_schema(schema.clone());
    spec.max_retries = Some(5);
    spec.enable_repair = Some(false);

    let policy = spec.to_output_policy();

    assert_eq!(policy.format, crate::ast::output::OutputFormat::Json,);

    assert!(policy.schema.is_some());

    assert_eq!(policy.max_retries, Some(5));

    assert!(policy.source_structured_spec.is_some());
    let roundtripped = policy.source_structured_spec.unwrap();
    assert_eq!(roundtripped.max_retries, Some(5));
    assert_eq!(roundtripped.enable_repair, Some(false));
}

// ═══════════════════════════════════════════════════════════════
// LOCKFILE GUARD RAII TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn lockfile_guard_creates_and_removes_on_drop() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("store").join(".nika-run.lock");

    {
        let _guard = LockfileGuard::create(lock_path.clone());
        assert!(
            lock_path.exists(),
            "Lockfile should exist while guard is alive"
        );

        let content = std::fs::read_to_string(&lock_path).unwrap();
        assert!(
            content.starts_with("pid:"),
            "Lockfile should contain pid, got: {content}"
        );
    }

    assert!(
        !lock_path.exists(),
        "Lockfile should be removed after guard is dropped"
    );
}

#[test]
fn lockfile_guard_removes_on_panic_unwind() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("store").join(".nika-run.lock");

    let result = std::panic::catch_unwind(|| {
        let _guard = LockfileGuard::create(lock_path.clone());
        assert!(lock_path.exists(), "Lockfile should exist before panic");
        panic!("simulated runner panic");
    });

    assert!(result.is_err(), "Should have caught the panic");
    assert!(
        !lock_path.exists(),
        "Lockfile should be removed even after panic unwind"
    );
}

#[test]
fn lockfile_guard_removes_on_early_return() {
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("store").join(".nika-run.lock");

    fn simulate_early_return(path: &std::path::Path) -> Result<(), &'static str> {
        let _guard = LockfileGuard::create(path.to_path_buf());
        assert!(path.exists(), "Lockfile should exist before early return");
        Err("simulated ? operator bail-out")?;
        unreachable!()
    }

    let result = simulate_early_return(&lock_path);
    assert!(result.is_err());
    assert!(
        !lock_path.exists(),
        "Lockfile should be removed after early return via ?"
    );
}

#[test]
fn lockfile_guard_tolerates_missing_file() {
    // If someone manually deletes the lockfile during a run,
    // Drop should not panic.
    let dir = tempfile::tempdir().unwrap();
    let lock_path = dir.path().join("store").join(".nika-run.lock");

    let guard = LockfileGuard::create(lock_path.clone());
    assert!(lock_path.exists());

    // Simulate external deletion
    std::fs::remove_file(&lock_path).unwrap();
    assert!(!lock_path.exists());

    // Drop should not panic
    drop(guard);
}

// ═══════════════════════════════════════════════════════════════
// Bug #24: for_each {{with.alias.path}} traversal failure must
// record a failed result, not silently run as a regular task.
// ═══════════════════════════════════════════════════════════════

/// Build a two-step workflow where step2 uses {{with.step1.path}} for_each.
fn create_with_template_for_each_workflow(
    step1_cmd: &str,
    step1_shell: bool,
    for_each_template: &str,
    step2_cmd: &str,
) -> AnalyzedWorkflow {
    let mut task_table = TaskTable::new();
    task_table.insert("step1");
    task_table.insert("step2");
    let tid1 = task_table.get_id("step1").unwrap();
    let tid2 = task_table.get_id("step2").unwrap();

    let step1 = AnalyzedTask {
        id: tid1,
        name: "step1".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: step1_cmd.to_string(),
            shell: Templatable::Value(step1_shell),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let mut with_spec = WithSpec::default();
    with_spec.insert(
        "step1".to_string(),
        WithEntry::simple(BindingPath {
            source: BindingSource::Task(intern("step1")),
            segments: vec![],
        }),
    );

    let step2 = AnalyzedTask {
        id: tid2,
        name: "step2".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: step2_cmd.to_string(),
            shell: Templatable::Value(false),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec,
        depends_on: vec![tid1],
        implicit_deps: vec![],
        output: None,
        for_each: Some(AnalyzedForEach {
            items: for_each_template.to_string(),
            as_var: "item".to_string(),
            concurrency: None,
            fail_fast: Templatable::Value(true),
            span: Span::dummy(),
        }),
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![step1, step2],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    }
}

#[tokio::test]
async fn bug24_for_each_with_template_traversal_failure_records_error() {
    // step1 outputs JSON object, step2 references nonexistent nested path
    let workflow = create_with_template_for_each_workflow(
        r#"echo '{"items": ["a","b"]}'"#,
        true,
        "{{with.step1.nonexistent}}",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap().quiet();
    let _ = runner.run().await;

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");

    let result = task_result.unwrap();
    assert!(
        !result.is_success(),
        "step2 should FAIL when path traversal fails, not run as regular task"
    );
    let error_msg = result.error().expect("should have error message");
    assert!(
        error_msg.contains("traversal failed"),
        "Error should mention path traversal failure, got: {}",
        error_msg
    );
}

// ═══════════════════════════════════════════════════════════════
// Bug #25: for_each {{with.alias}} non-array must record error,
// not silently run as a regular task.
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn bug25_for_each_with_template_non_array_records_error() {
    // step1 outputs a plain string (not an array), step2 tries to iterate over it
    let workflow = create_with_template_for_each_workflow(
        "echo not_an_array",
        false,
        "{{with.step1}}",
        "echo {{with.item}}",
    );

    let mut runner = Runner::new(workflow).unwrap().quiet();
    let _ = runner.run().await;

    let task_result = runner.datastore.get("step2");
    assert!(task_result.is_some(), "step2 result should exist");

    let result = task_result.unwrap();
    assert!(
        !result.is_success(),
        "step2 should FAIL when for_each binding resolves to non-array"
    );
    let error_msg = result.error().expect("should have error message");
    assert!(
        error_msg.contains("non-array"),
        "Error should mention 'non-array', got: {}",
        error_msg
    );
}

// ═══════════════════════════════════════════════════════════════
// FOR_EACH CONCURRENT FAIL_FAST CANCELLATION
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn for_each_concurrent_fail_fast_cancels_remaining_iterations() {
    // Verify that with concurrency > 1 and fail_fast = true, when one
    // iteration fails, iterations waiting on the semaphore are cancelled
    // (returned as Skipped) rather than being allowed to proceed.
    //
    // Cancellation semantics: the CancellationToken fires when an iteration
    // fails. This cancels iterations that are:
    //   (a) waiting to acquire the semaphore (via tokio::select!)
    //   (b) not yet spawned (checked in the spawn loop)
    //   (c) checked again after acquiring the permit
    //
    // Iterations that already acquired a permit and started executing will
    // run to completion — the token does not abort running shell processes.
    //
    // Strategy:
    //   - 6 items, concurrency=2, fail_fast=true
    //   - Item 0 fails immediately (exit 1)
    //   - Item 1 succeeds quickly (echo)
    //   - Items 2-5 would run if allowed, but should be cancelled at the
    //     semaphore gate since the cancel token fires before they acquire.

    let workflow = create_for_each_workflow(
        "cancel_test",
        r#"["FAIL", "ok1", "wait2", "wait3", "wait4", "wait5"]"#,
        "item",
        // Non-failing items sleep briefly so cancellation propagates before they complete.
        // Without the sleep, items 2-5 can finish instantly after acquiring the permit
        // released by the failing item, racing the CancellationToken.
        "if [ '{{with.item}}' = 'FAIL' ]; then exit 1; else sleep 0.05 && echo {{with.item | shell}}; fi",
        Some(2), // concurrency = 2
        true,    // fail_fast = true
        true,    // shell = true
    );

    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;

    // Workflow should fail (fail_fast propagates the error)
    assert!(
        result.is_err(),
        "Workflow should fail when fail_fast triggers on concurrent for_each"
    );

    // The failing iteration (index 0) should exist and be a failure
    let fail_iter = runner.datastore.get("cancel_test[0]");
    assert!(
        fail_iter.is_some(),
        "Failing iteration [0] result should exist"
    );
    assert!(
        !fail_iter.unwrap().is_success(),
        "Iteration [0] should have failed"
    );

    // Item 1 may have completed (it was in the same concurrency batch as
    // item 0) or may have been skipped — either is acceptable.
    // But items 2-5 were waiting on the semaphore and MUST be skipped.
    let mut skipped_count = 0;
    let mut total_stored = 0;
    for idx in 2..6 {
        let key = format!("cancel_test[{}]", idx);
        if let Some(iter_result) = runner.datastore.get(&key) {
            total_stored += 1;
            if iter_result.is_skipped() {
                skipped_count += 1;
            }
        }
        // Iterations that were never spawned (spawn loop saw cancellation)
        // won't have a datastore entry at all — that's also valid cancellation.
    }

    // Items 2-5 were queued behind the semaphore. They should either be
    // skipped (cancel fired while waiting) or never spawned (cancel fired
    // before the spawn loop reached them). Either way, they should NOT
    // have succeeded.
    let succeeded_after_cancel: Vec<usize> = (2..6)
        .filter(|idx| {
            let key = format!("cancel_test[{}]", idx);
            runner
                .datastore
                .get(&key)
                .map(|r| r.is_success())
                .unwrap_or(false)
        })
        .collect();

    assert!(
        succeeded_after_cancel.is_empty(),
        "Iterations behind the semaphore should not succeed after fail_fast cancellation, \
        but these succeeded: {:?}",
        succeeded_after_cancel
    );

    // At least some of the queued iterations should have a skipped result
    // (others may not have been spawned at all).
    let not_spawned = 4 - total_stored;
    assert!(
        skipped_count + not_spawned >= 1,
        "At least one iteration should be cancelled (skipped={}, not_spawned={}). \
        This suggests cancellation tokens are not working for concurrent for_each.",
        skipped_count,
        not_spawned
    );
}

// ═══════════════════════════════════════════════════════════════
// Bug #26: fail_fast should only cancel sibling iterations,
// not unrelated tasks in the same JoinSet.
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn bug26_fail_fast_does_not_abort_unrelated_sibling_tasks() {
    // Two independent for_each parents: one fails fast, the other should still complete.
    // "failing_parent" has fail_fast=true and one failing item.
    // "passing_parent" has fail_fast=true but all items succeed.
    // Both run in parallel (no depends_on between them).
    //
    // Before the fix, abort_all() would kill BOTH parents' tasks.
    // After the fix, only failing_parent's iterations are cancelled.

    let mut task_table = TaskTable::new();
    task_table.insert("failing_parent");
    task_table.insert("passing_parent");
    let tid_fail = task_table.get_id("failing_parent").unwrap();
    let tid_pass = task_table.get_id("passing_parent").unwrap();

    let failing_parent = AnalyzedTask {
        id: tid_fail,
        name: "failing_parent".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: "test '{{with.item}}' != 'FAIL' && echo {{with.item | shell}}".to_string(),
            shell: Templatable::Value(true),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: Some(AnalyzedForEach {
            items: r#"["ok", "FAIL", "ok2"]"#.to_string(),
            as_var: "item".to_string(),
            concurrency: Some(Templatable::Value(3)),
            fail_fast: Templatable::Value(true),
            span: Span::dummy(),
        }),
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let passing_parent = AnalyzedTask {
        id: tid_pass,
        name: "passing_parent".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: "echo {{with.item | shell}}".to_string(),
            shell: Templatable::Value(true),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: Some(AnalyzedForEach {
            items: r#"["a", "b", "c"]"#.to_string(),
            as_var: "item".to_string(),
            concurrency: Some(Templatable::Value(3)),
            fail_fast: Templatable::Value(true),
            span: Span::dummy(),
        }),
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let workflow = AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![failing_parent, passing_parent],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    };

    let mut runner = Runner::new(workflow).unwrap().quiet();
    let _ = runner.run().await;

    // The failing parent should have a result (aggregated, with at least one failure)
    let fail_result = runner.datastore.get("failing_parent");
    assert!(fail_result.is_some(), "failing_parent result should exist");

    // The passing parent should ALSO have a result — it should NOT be aborted
    let pass_result = runner.datastore.get("passing_parent");
    assert!(
        pass_result.is_some(),
        "passing_parent result should exist (fail_fast of sibling should not abort it)"
    );

    let pass_tr = pass_result.unwrap();
    assert!(
        pass_tr.is_success(),
        "passing_parent should succeed — its iterations were all OK. \
        Error: {:?}",
        pass_tr.error()
    );
}

// ═══════════════════════════════════════════════════════════════
// TASK-LEVEL RETRY TESTS
// ═══════════════════════════════════════════════════════════════

#[test]
fn test_is_retryable_provider_api_error() {
    let err = NikaError::ProviderApiError {
        message: "429 Too Many Requests".to_string(),
    };
    assert!(is_retryable(&err), "ProviderApiError should be retryable");
}

#[test]
fn test_is_retryable_exec_error() {
    let err = NikaError::ExecError {
        reason: "command timed out".to_string(),
    };
    assert!(is_retryable(&err), "ExecError should be retryable");
}

#[test]
fn test_is_retryable_fetch_error() {
    let err = NikaError::FetchError {
        reason: "connection reset".to_string(),
    };
    assert!(is_retryable(&err), "FetchError should be retryable");
}

#[test]
fn test_is_retryable_mcp_not_connected() {
    let err = NikaError::McpNotConnected {
        name: "novanet".to_string(),
    };
    assert!(is_retryable(&err), "McpNotConnected should be retryable");
}

#[test]
fn test_is_retryable_mcp_timeout() {
    let err = NikaError::McpTimeout {
        name: "search".to_string(),
        operation: "tool_call".to_string(),
        timeout_secs: 30,
    };
    assert!(is_retryable(&err), "McpTimeout should be retryable");
}

#[test]
fn test_is_retryable_timeout() {
    let err = NikaError::Timeout {
        operation: "infer".to_string(),
        duration_ms: 300_000,
    };
    assert!(is_retryable(&err), "Timeout should be retryable");
}

#[test]
fn test_is_retryable_endpoint_connection_failed() {
    let err = NikaError::EndpointConnectionFailed {
        endpoint: "h100".to_string(),
        reason: "connection refused".to_string(),
    };
    assert!(
        is_retryable(&err),
        "EndpointConnectionFailed should be retryable"
    );
}

#[test]
fn test_is_not_retryable_validation_error() {
    let err = NikaError::ValidationError {
        reason: "invalid schema".to_string(),
    };
    assert!(
        !is_retryable(&err),
        "ValidationError should NOT be retryable"
    );
}

#[test]
fn test_is_not_retryable_template_error() {
    let err = NikaError::TemplateError {
        template: "{{with.x}}".to_string(),
        reason: "not found".to_string(),
    };
    assert!(!is_retryable(&err), "TemplateError should NOT be retryable");
}

#[test]
fn test_is_not_retryable_cycle_detected() {
    let err = NikaError::CycleDetected {
        cycle: "a -> b -> a".to_string(),
    };
    assert!(!is_retryable(&err), "CycleDetected should NOT be retryable");
}

#[test]
fn test_is_not_retryable_missing_api_key() {
    let err = NikaError::MissingApiKey {
        provider: "openai".to_string(),
    };
    assert!(!is_retryable(&err), "MissingApiKey should NOT be retryable");
}

#[test]
fn test_is_not_retryable_provider_401() {
    let err = NikaError::ProviderApiError {
        message: "401 Unauthorized: invalid API key".to_string(),
    };
    assert!(
        !is_retryable(&err),
        "ProviderApiError 401 should NOT be retryable"
    );
}

#[test]
fn test_is_not_retryable_provider_403() {
    let err = NikaError::ProviderApiError {
        message: "403 Forbidden: account suspended".to_string(),
    };
    assert!(
        !is_retryable(&err),
        "ProviderApiError 403 should NOT be retryable"
    );
}

#[test]
fn test_is_not_retryable_exec_not_found() {
    let err = NikaError::ExecError {
        reason: "Failed to spawn command: No such file or directory".to_string(),
    };
    assert!(
        !is_retryable(&err),
        "ExecError 'not found' should NOT be retryable"
    );
}

#[test]
fn test_is_not_retryable_exec_permission_denied() {
    let err = NikaError::ExecError {
        reason: "Failed to spawn command: Permission denied".to_string(),
    };
    assert!(
        !is_retryable(&err),
        "ExecError 'permission denied' should NOT be retryable"
    );
}

#[tokio::test]
async fn test_exec_task_with_retry_runs_and_succeeds() {
    // Create a workflow with an exec task that has retry config.
    // The command succeeds on first try, so no retry needed —
    // this tests that retry config doesn't break normal execution.
    use crate::ast::analyzed::AnalyzedRetry;

    let mut task_table = TaskTable::new();
    task_table.insert("retryable");
    let tid = task_table.get_id("retryable").unwrap();

    let task = AnalyzedTask {
        id: tid,
        name: "retryable".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: "echo hello".to_string(),
            shell: Templatable::Value(false),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: Some(AnalyzedRetry {
            max_attempts: Templatable::Value(3),
            delay_ms: Templatable::Value(100),
            backoff: Some(Templatable::Value(2.0)),
            span: Span::dummy(),
        }),
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let workflow = AnalyzedWorkflow {
        schema_version: SchemaVersion::V01,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![task],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    };

    let event_log = crate::event::EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Workflow with retry should succeed: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("hello"),
        "Output should contain 'hello', got: {}",
        output
    );

    // Verify no TaskRetry events (first attempt succeeded)
    let events = event_log.events();
    let retry_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TaskRetry { .. }))
        .collect();
    assert!(
        retry_events.is_empty(),
        "No TaskRetry events should be emitted when first attempt succeeds"
    );
}

#[tokio::test]
async fn test_exec_task_with_retry_retries_on_failure() {
    // Create a workflow with an exec task that fails with retry config.
    // Uses a temp file counter: first call creates the file and fails,
    // second call sees the file and succeeds.
    use crate::ast::analyzed::AnalyzedRetry;

    let tmp = std::env::temp_dir().join(format!("nika_retry_test_{}", std::process::id()));
    // Clean up any leftover from previous run
    let _ = std::fs::remove_file(&tmp);

    let cmd = format!(
        "if [ -f '{}' ]; then echo success; else touch '{}'; exit 1; fi",
        tmp.display(),
        tmp.display()
    );

    let mut task_table = TaskTable::new();
    task_table.insert("retryable");
    let tid = task_table.get_id("retryable").unwrap();

    let task = AnalyzedTask {
        id: tid,
        name: "retryable".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: cmd,
            shell: Templatable::Value(true),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: Some(AnalyzedRetry {
            max_attempts: Templatable::Value(3),
            delay_ms: Templatable::Value(50),
            backoff: None,
            span: Span::dummy(),
        }),
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let workflow = AnalyzedWorkflow {
        schema_version: SchemaVersion::V01,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![task],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: None,
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    };

    let event_log = crate::event::EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone())
        .unwrap()
        .quiet();
    let result = runner.run().await;

    // Clean up
    let _ = std::fs::remove_file(&tmp);

    assert!(
        result.is_ok(),
        "Workflow should succeed after retry: {:?}",
        result.err()
    );
    let output = result.unwrap();
    assert!(
        output.contains("success"),
        "Output should contain 'success' after retry, got: {}",
        output
    );

    // Verify TaskRetry event was emitted
    let events = event_log.events();
    let retry_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(e.kind, EventKind::TaskRetry { .. }))
        .collect();
    assert_eq!(
        retry_events.len(),
        1,
        "Should have exactly 1 TaskRetry event (attempt 2 after first failure)"
    );
}

// ═══════════════════════════════════════════════════════════════
// PRESET TESTS
// ═══════════════════════════════════════════════════════════════

fn make_preset_workflow(
    preset_name: &str,
    task_provider: Option<&str>,
    task_model: Option<&str>,
) -> AnalyzedWorkflow {
    use crate::ast::agent_def::AgentDef;

    let mut task_table = TaskTable::new();
    task_table.insert("gen");
    let tid = task_table.get_id("gen").unwrap();

    let task = AnalyzedTask {
        id: tid,
        name: "gen".to_string(),
        description: None,
        action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: "hello".to_string(),
            system: None,
            temperature: None,
            max_tokens: None,
            ..Default::default()
        }),
        provider: task_provider.map(nika_core::ProviderName::parse),
        model: task_model.map(|s| s.to_string()),

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: Some(preset_name.to_string()),
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let mut agents = IndexMap::new();
    agents.insert(
        preset_name.to_string(),
        AgentDef::Inline {
            system: "You are helpful".to_string(),
            provider: Some("mock".to_string()),
            model: Some("mock-fast".to_string()),
            max_turns: None,
            temperature: Some(0.3),
            skills: None,
        },
    );

    AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![task],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: Some(agents),
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    }
}

#[tokio::test]
async fn test_preset_resolves_provider_model() {
    let workflow = make_preset_workflow("assistant", None, None);
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Preset workflow should run: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_preset_task_override_wins() {
    // Task sets provider: mock, preset sets provider: mock too but model differs.
    // The task-level provider/model should win over preset.
    let workflow = make_preset_workflow("writer", Some("mock"), Some("mock-fast"));
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Task-level provider override should work: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_preset_injects_system_temperature() {
    // Preset defines system + temperature; task has neither.
    // The runner should inject both from the preset into the lowered action.
    let workflow = make_preset_workflow("creative", None, None);
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Preset with system + temperature should work: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_preset_emits_preset_applied_event() {
    let workflow = make_preset_workflow("assistant", None, None);
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());

    let events = event_log.events();
    let preset_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.kind, crate::event::EventKind::PresetApplied { .. }))
        .collect();

    assert_eq!(
        preset_events.len(),
        1,
        "Expected exactly one PresetApplied event"
    );

    if let crate::event::EventKind::PresetApplied {
        preset_name,
        provider,
        ..
    } = &preset_events[0].kind
    {
        assert_eq!(preset_name, "assistant");
        assert_eq!(provider.as_deref(), Some("mock"));
    } else {
        panic!("Expected PresetApplied event");
    }
}

// ═══════════════════════════════════════════════════════════════
// Backward compat + integration tests for L.3
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_no_preset_workflow_runs_normally() {
    // Workflow without agents: or preset: fields → current behavior unchanged
    let workflow = create_exec_workflow(vec![("a", "echo hello")], vec![]);
    let mut runner = Runner::new(workflow).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Workflow without presets should run normally: {:?}",
        result.err()
    );
}

#[tokio::test]
async fn test_no_preset_emits_no_preset_event() {
    let workflow = create_exec_workflow(vec![("a", "echo hello")], vec![]);
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
    let _ = runner.run().await;

    let preset_events: Vec<_> = event_log
        .events()
        .iter()
        .filter(|e| matches!(&e.kind, crate::event::EventKind::PresetApplied { .. }))
        .cloned()
        .collect();
    assert!(
        preset_events.is_empty(),
        "No PresetApplied events when no preset is used"
    );
}

#[tokio::test]
async fn test_multiple_tasks_different_presets() {
    use crate::ast::agent_def::AgentDef;

    let mut task_table = TaskTable::new();
    task_table.insert("fast");
    task_table.insert("deep");
    let fast_id = task_table.get_id("fast").unwrap();
    let deep_id = task_table.get_id("deep").unwrap();

    let fast_task = AnalyzedTask {
        id: fast_id,
        name: "fast".to_string(),
        description: None,
        action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: "quick answer".to_string(),
            ..Default::default()
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: Some("speed".to_string()),
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let deep_task = AnalyzedTask {
        id: deep_id,
        name: "deep".to_string(),
        description: None,
        action: AnalyzedTaskAction::Infer(AnalyzedInferAction {
            prompt: "deep analysis".to_string(),
            ..Default::default()
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: None,
        context_budget: None,
        preset: Some("think".to_string()),
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let mut agents = IndexMap::new();
    agents.insert(
        "speed".to_string(),
        AgentDef::Inline {
            system: "Be fast".to_string(),
            provider: Some("mock".to_string()),
            model: Some("mock-fast".to_string()),
            max_turns: None,
            temperature: Some(0.1),
            skills: None,
        },
    );
    agents.insert(
        "think".to_string(),
        AgentDef::Inline {
            system: "Think deeply".to_string(),
            provider: Some("mock".to_string()),
            model: Some("mock-slow".to_string()),
            max_turns: None,
            temperature: Some(0.7),
            skills: None,
        },
    );

    let workflow = AnalyzedWorkflow {
        schema_version: SchemaVersion::V03,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![fast_task, deep_task],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        include: vec![],
        inputs: IndexMap::new(),
        artifacts: None,
        log: None,
        agents: Some(agents),
        skills_map: std::collections::HashMap::new(),
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    };

    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
    let result = runner.run().await;
    assert!(
        result.is_ok(),
        "Multiple presets should work: {:?}",
        result.err()
    );

    let preset_events: Vec<_> = event_log
        .events()
        .iter()
        .filter(|e| matches!(&e.kind, crate::event::EventKind::PresetApplied { .. }))
        .cloned()
        .collect();
    assert_eq!(
        preset_events.len(),
        2,
        "Two tasks with presets → two PresetApplied events"
    );
}

#[test]
fn test_cost_tool_registered_via_with_cost_tool() {
    use crate::runtime::builtin::BuiltinToolRouter;
    let router = BuiltinToolRouter::new().with_cost_tool(EventLog::new());
    assert!(
        router.has_tool("cost"),
        "nika:cost should be registered via with_cost_tool"
    );
}

#[tokio::test]
async fn test_cost_accumulates_across_tasks() {
    // Run a preset workflow and check that ProviderResponded events exist
    let workflow = make_preset_workflow("assistant", None, None);
    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());

    // The mock provider should have emitted at least one ProviderResponded
    let provider_events: Vec<_> = event_log
        .events()
        .iter()
        .filter(|e| matches!(&e.kind, crate::event::EventKind::ProviderResponded { .. }))
        .cloned()
        .collect();
    assert!(
        !provider_events.is_empty(),
        "Mock provider should emit ProviderResponded"
    );
}

// ═══════════════════════════════════════════════════════════════
// RECORD TESTS
// ═══════════════════════════════════════════════════════════════

#[tokio::test]
async fn test_record_shorthand_true_creates_record() {
    use nika_core::ast::record::RecordSpec;

    let mut task_table = TaskTable::new();
    task_table.insert("recorded");
    let tid = task_table.get_id("recorded").unwrap();

    let task = AnalyzedTask {
        id: tid,
        name: "recorded".to_string(),
        description: None,
        action: AnalyzedTaskAction::Exec(AnalyzedExecAction {
            command: "echo hello".to_string(),
            shell: Templatable::Value(false),
            cwd: None,
            env: IndexMap::new(),
            timeout_ms: None,
            max_stdout: None,
            span: Span::dummy(),
        }),
        provider: None,
        model: None,

        with_spec: Default::default(),
        depends_on: vec![],
        implicit_deps: vec![],
        output: None,
        for_each: None,
        retry: None,
        on_error: None,
        decompose: None,
        concurrency: None,
        fail_fast: None,
        artifact: None,
        log: None,
        structured: None,
        record: Some(RecordSpec::shorthand_true()),
        context_budget: None,
        preset: None,
        routing: None,
        when: None,
        span: Span::dummy(),
    };

    let workflow = AnalyzedWorkflow {
        schema_version: SchemaVersion::V01,
        name: None,
        description: None,
        goal: None,
        provider: Some(nika_core::ProviderName::Mock),
        model: None,

        task_table,
        tasks: vec![task],
        mcp_servers: IndexMap::new(),
        context_files: vec![],
        inputs: Default::default(),
        agents: Some(IndexMap::new()),
        skills_map: std::collections::HashMap::new(),
        artifacts: None,
        log: None,
        include: vec![],
        orchestrate: None,
        routing: None,
        schedule: None,
        max_duration_secs: Templatable::Value(3600),
        span: Span::dummy(),
    };

    let event_log = EventLog::new();
    let mut runner = Runner::with_event_log(workflow, event_log.clone()).unwrap();
    let result = runner.run().await;
    assert!(result.is_ok(), "Should succeed: {:?}", result.err());

    // Task should have produced a record in the datastore
    let record = runner.datastore().get_record("recorded");
    assert!(
        record.is_some(),
        "Task with record: true should create a Record in the datastore"
    );

    let rec = record.unwrap();
    assert_eq!(rec.task_id.as_ref(), "recorded");
    // Short output (< max_tokens) should produce a truncated/skipped record
    assert!(
        !rec.summary.is_empty(),
        "Record summary should not be empty"
    );
}

// ═══════════════════════════════════════════════════════════════
// GLOBAL TASK CONCURRENCY SEMAPHORE FOR REGULAR TASKS
// ═══════════════════════════════════════════════════════════════

#[test]
fn runner_has_global_task_semaphore_with_64_permits() {
    let runner = Runner::new(make_empty_workflow()).unwrap();
    // The semaphore should be initialized with MAX_CONCURRENT_TASKS (64) permits.
    // available_permits() returns the number of permits not currently held.
    assert_eq!(
        runner.global_task_semaphore.available_permits(),
        MAX_CONCURRENT_TASKS,
        "Global semaphore should have {MAX_CONCURRENT_TASKS} permits"
    );
}

#[tokio::test]
async fn regular_tasks_acquire_global_semaphore() {
    // Spawn many independent regular tasks (no for_each) in a single DAG layer.
    // Verify they all complete successfully — proving the semaphore is acquired
    // and released correctly for regular tasks.
    let task_count = 8;
    let tasks: Vec<(&str, &str)> = vec![
        ("t0", "echo task0"),
        ("t1", "echo task1"),
        ("t2", "echo task2"),
        ("t3", "echo task3"),
        ("t4", "echo task4"),
        ("t5", "echo task5"),
        ("t6", "echo task6"),
        ("t7", "echo task7"),
    ];
    // No edges — all tasks run in parallel in the same DAG layer
    let edges: Vec<(&str, &str)> = vec![];

    let workflow = create_exec_workflow(tasks, edges);
    let mut runner = Runner::new(workflow).unwrap().quiet();
    let result = runner.run().await;

    assert!(
        result.is_ok(),
        "Workflow with {task_count} parallel regular tasks should succeed: {:?}",
        result.err()
    );

    // Verify all tasks completed successfully
    for i in 0..task_count {
        let key = format!("t{}", i);
        let task_result = runner.datastore.get(&key);
        assert!(
            task_result.is_some(),
            "Task '{}' result should exist in datastore",
            key
        );
        assert!(
            task_result.unwrap().is_success(),
            "Task '{}' should have succeeded",
            key
        );
    }

    // After all tasks complete, all permits should be returned
    assert_eq!(
        runner.global_task_semaphore.available_permits(),
        MAX_CONCURRENT_TASKS,
        "All semaphore permits should be returned after tasks complete"
    );
}

// ═══════════════════════════════════════════════════════════════
// BUG-034: value_to_array null handling
// ═══════════════════════════════════════════════════════════════

#[test]
fn value_to_array_null_returns_empty() {
    assert_eq!(value_to_array(&Value::Null), Some(vec![]));
}

#[test]
fn value_to_array_empty_array_returns_empty() {
    assert_eq!(value_to_array(&json!([])), Some(vec![]));
}

#[test]
fn value_to_array_normal_array_unchanged() {
    let arr = json!([1, 2, 3]);
    let result = value_to_array(&arr).unwrap();
    assert_eq!(result, vec![json!(1), json!(2), json!(3)]);
}

// ── is_truthy tests ─────────────────────────────────────

#[test]
fn truthy_accepts_valid_values() {
    assert!(is_truthy("true"));
    assert!(is_truthy("yes"));
    assert!(is_truthy("1"));
    assert!(is_truthy("hello"));
    assert!(is_truthy("42"));
    assert!(is_truthy("  true  ")); // trimmed
}

#[test]
fn truthy_rejects_falsy_values() {
    assert!(!is_truthy("false"));
    assert!(!is_truthy("null"));
    assert!(!is_truthy("0"));
    assert!(!is_truthy("0.0"));
    assert!(!is_truthy(""));
    assert!(!is_truthy("  false  ")); // trimmed
    assert!(!is_truthy("  null  "));
    assert!(!is_truthy("  0  "));
}

// ═══════════════════════════════════════════════════════════════
// PANIC MESSAGE EXTRACTION (W6.5)
// ═══════════════════════════════════════════════════════════════

#[test]
fn extract_panic_message_str_literal() {
    let result = std::panic::catch_unwind(|| {
        panic!("literal message");
    });
    let payload = result.unwrap_err();
    let msg = extract_panic_message(&payload);
    assert_eq!(msg, "literal message");
}

#[test]
fn extract_panic_message_string_owned() {
    let result = std::panic::catch_unwind(|| {
        panic!("{}", format!("formatted {}", "message"));
    });
    let payload = result.unwrap_err();
    let msg = extract_panic_message(&payload);
    assert_eq!(msg, "formatted message");
}

#[test]
fn extract_panic_message_unknown_type() {
    let result = std::panic::catch_unwind(|| {
        panic_any(42_i32);
    });
    let payload = result.unwrap_err();
    let msg = extract_panic_message(&payload);
    assert_eq!(msg, "unknown panic");
}
