//! DAG execution with tokio (v0.1)
//!
//! Simplified runner using TaskExecutor for execution
//! and UseBindings::from_use_wiring for bindings resolution.

use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::task::JoinSet;
use tracing::{info, instrument};

use crate::datastore::{DataStore, TaskResult};
use crate::error::NikaError;
use crate::event_log::{EventKind, EventLog};
use crate::flow_graph::FlowGraph;
use crate::output_policy::OutputFormat;
use crate::task_executor::TaskExecutor;
use crate::use_bindings::UseBindings;
use crate::validator;
use crate::workflow::{Task, Workflow};

/// DAG workflow runner with event sourcing
pub struct Runner {
    workflow: Workflow,
    flow_graph: FlowGraph,
    datastore: DataStore,
    executor: TaskExecutor,
    event_log: EventLog,
}

impl Runner {
    pub fn new(workflow: Workflow) -> Self {
        let flow_graph = FlowGraph::from_workflow(&workflow);
        let datastore = DataStore::new();
        let event_log = EventLog::new();
        let executor = TaskExecutor::new(&workflow.provider, workflow.model.as_deref(), event_log.clone());

        Self {
            workflow,
            flow_graph,
            datastore,
            executor,
            event_log,
        }
    }

    /// Get the event log for inspection/export
    #[allow(dead_code)] // Used in tests and future export
    pub fn event_log(&self) -> &EventLog {
        &self.event_log
    }

    /// Get tasks that are ready to run (all dependencies satisfied)
    fn get_ready_tasks(&self) -> Vec<Arc<Task>> {
        self.workflow
            .tasks
            .iter()
            .filter(|task| {
                // Skip if already done
                if self.datastore.contains(&task.id) {
                    return false;
                }

                // Check all dependencies are done AND successful
                let deps = self.flow_graph.get_dependencies(&task.id);
                deps.iter().all(|dep| self.datastore.is_success(dep))
            })
            .cloned() // Clone the Arc, not the Task
            .collect()
    }

    /// Check if all tasks are done
    fn all_done(&self) -> bool {
        self.workflow
            .tasks
            .iter()
            .all(|t| self.datastore.contains(&t.id))
    }

    /// Get the final output (from tasks with no successors)
    fn get_final_output(&self) -> Option<String> {
        let final_tasks = self.flow_graph.get_final_tasks();

        // Return first successful final task output
        for task_id in final_tasks {
            if let Some(result) = self.datastore.get(&task_id) {
                if result.is_success() {
                    return Some(result.output_str().into_owned());
                }
            }
        }
        None
    }

    /// Main execution loop
    #[instrument(skip(self), fields(workflow_tasks = self.workflow.tasks.len()))]
    pub async fn run(&self) -> Result<String, NikaError> {
        let workflow_start = Instant::now();
        info!("Starting workflow execution");

        // Validate use: blocks before execution (fail-fast)
        validator::validate_use_wiring(&self.workflow, &self.flow_graph)?;

        let total_tasks = self.workflow.tasks.len();
        let mut completed = 0;

        // EMIT: WorkflowStarted
        self.event_log.emit(EventKind::WorkflowStarted { task_count: total_tasks });

        println!(
            "{} Running workflow with {} tasks...\n",
            "→".cyan(),
            total_tasks
        );

        loop {
            let ready = self.get_ready_tasks();

            // Check for completion or deadlock
            if ready.is_empty() {
                if self.all_done() {
                    break;
                }
                // EMIT: WorkflowFailed (deadlock)
                self.event_log.emit(EventKind::WorkflowFailed {
                    error: "Deadlock: no tasks ready but workflow not complete".to_string(),
                    failed_task: None,
                });
                return Err(NikaError::Execution(
                    "Deadlock: no tasks ready but workflow not complete".to_string(),
                ));
            }

            // Spawn all ready tasks in parallel
            let mut join_set = JoinSet::new();

            for task in ready {
                let task = Arc::clone(&task);
                let task_id: Arc<str> = Arc::from(task.id.as_str()); // Arc<str> for zero-cost cloning
                let datastore = self.datastore.clone();
                let executor = self.executor.clone();
                let event_log = self.event_log.clone();

                // EMIT: TaskScheduled
                let deps = self.flow_graph.get_dependencies(&task.id);
                self.event_log.emit(EventKind::TaskScheduled {
                    task_id: Arc::clone(&task_id),
                    dependencies: deps.iter().cloned().collect(), // Arc::clone is O(1)
                });

                println!("  {} {} {}", "[⟳]".yellow(), &task_id, "running...".dimmed());

                join_set.spawn(async move {
                    let start = Instant::now();

                    // Build bindings from use: wiring
                    let bindings = match UseBindings::from_use_wiring(
                        task.use_wiring.as_ref(),
                        &datastore,
                    ) {
                        Ok(b) => b,
                        Err(e) => {
                            let duration = start.elapsed();
                            // EMIT: TaskFailed (bindings build failed)
                            event_log.emit(EventKind::TaskFailed {
                                task_id: Arc::clone(&task_id),
                                error: e.to_string(),
                                duration_ms: duration.as_millis() as u64,
                            });
                            return (task_id, TaskResult::failed(e.to_string(), duration));
                        }
                    };

                    // EMIT: TaskStarted (with resolved inputs from use: wiring)
                    event_log.emit(EventKind::TaskStarted {
                        task_id: Arc::clone(&task_id),
                        inputs: bindings.to_value(),
                    });

                    // Execute via TaskExecutor
                    let result = executor.execute(&task_id, &task.action, &bindings).await;
                    let duration = start.elapsed();

                    // Convert result to TaskResult with output policy
                    let task_result = match result {
                        Ok(output) => {
                            let tr = make_task_result(output, task.output.as_ref(), duration);
                            // EMIT: TaskCompleted or TaskFailed (based on result)
                            if tr.is_success() {
                                event_log.emit(EventKind::TaskCompleted {
                                    task_id: Arc::clone(&task_id),
                                    output: tr.output.clone(),
                                    duration_ms: duration.as_millis() as u64,
                                });
                            } else {
                                event_log.emit(EventKind::TaskFailed {
                                    task_id: Arc::clone(&task_id),
                                    error: tr.error().unwrap_or("Unknown error").to_string(),
                                    duration_ms: duration.as_millis() as u64,
                                });
                            }
                            tr
                        }
                        Err(e) => {
                            // EMIT: TaskFailed
                            event_log.emit(EventKind::TaskFailed {
                                task_id: Arc::clone(&task_id),
                                error: e.to_string(),
                                duration_ms: duration.as_millis() as u64,
                            });
                            TaskResult::failed(e.to_string(), duration)
                        }
                    };

                    (task_id, task_result)
                });
            }

            // Wait for all spawned tasks to complete
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok((task_id, task_result)) => {
                        completed += 1;
                        let success = task_result.is_success();

                        let status = if success {
                            format!("[{}/{}]", completed, total_tasks).green()
                        } else {
                            format!("[{}/{}]", completed, total_tasks).red()
                        };

                        let symbol = if success { "✓" } else { "✗" };
                        let symbol_colored = if success { symbol.green() } else { symbol.red() };
                        let duration_str =
                            format!("({:.1}s)", task_result.duration.as_secs_f32()).dimmed();

                        println!("  {} {} {} {}", status, &*task_id, symbol_colored, duration_str);

                        if let Some(err) = task_result.error() {
                            println!("      {} {}", "Error:".red(), err);
                        }

                        self.datastore.insert(task_id, task_result);
                    }
                    Err(e) => {
                        // EMIT: WorkflowFailed (task panic)
                        self.event_log.emit(EventKind::WorkflowFailed {
                            error: format!("Task panicked: {}", e),
                            failed_task: None,
                        });
                        return Err(NikaError::Execution(format!("Task panicked: {}", e)));
                    }
                }
            }
        }

        // Get final output
        let output = self.get_final_output().unwrap_or_default();

        // EMIT: WorkflowCompleted
        self.event_log.emit(EventKind::WorkflowCompleted {
            final_output: Value::String(output.clone()),
            total_duration_ms: workflow_start.elapsed().as_millis() as u64,
        });

        println!("\n{} Done!\n", "✓".green());

        Ok(output)
    }
}

/// Convert execution output to TaskResult, parsing as JSON if output format is json
/// Also validates against schema if declared.
fn make_task_result(
    output: String,
    policy: Option<&crate::output_policy::OutputPolicy>,
    duration: std::time::Duration,
) -> TaskResult {
    if let Some(policy) = policy {
        if policy.format == OutputFormat::Json {
            // Parse as JSON
            let json_value = match serde_json::from_str::<Value>(&output) {
                Ok(v) => v,
                Err(e) => {
                    return TaskResult::failed(
                        format!("NIKA-060: Invalid JSON output: {}", e),
                        duration,
                    );
                }
            };

            // Validate against schema if declared
            if let Some(schema_path) = &policy.schema {
                if let Err(e) = validate_schema(&json_value, schema_path) {
                    return TaskResult::failed(e.to_string(), duration);
                }
            }

            return TaskResult::success(json_value, duration);
        }
    }
    TaskResult::success_str(output, duration)
}

/// Validate JSON value against a JSON Schema file
fn validate_schema(value: &Value, schema_path: &str) -> Result<(), NikaError> {
    // Read schema file
    let schema_str = std::fs::read_to_string(schema_path).map_err(|e| {
        NikaError::SchemaFailed {
            details: format!("Failed to read schema '{}': {}", schema_path, e),
        }
    })?;

    // Parse schema
    let schema: Value = serde_json::from_str(&schema_str).map_err(|e| {
        NikaError::SchemaFailed {
            details: format!("Invalid JSON in schema '{}': {}", schema_path, e),
        }
    })?;

    // Compile and validate
    let compiled = jsonschema::validator_for(&schema).map_err(|e| {
        NikaError::SchemaFailed {
            details: format!("Invalid schema '{}': {}", schema_path, e),
        }
    })?;

    // Collect all validation errors
    let errors: Vec<_> = compiled.iter_errors(value).collect();
    if errors.is_empty() {
        Ok(())
    } else {
        let error_msgs: Vec<String> = errors.iter().map(|e| e.to_string()).collect();
        Err(NikaError::SchemaFailed {
            details: error_msgs.join("; "),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task_action::{ExecParams, TaskAction};
    use crate::workflow::{Flow, FlowEndpoint, Task};
    use std::sync::Arc;

    /// Helper to create a minimal workflow with exec tasks
    fn create_exec_workflow(tasks: Vec<(&str, &str)>, flows: Vec<(&str, &str)>) -> Workflow {
        Workflow {
            schema: "nika/workflow@0.1".to_string(),
            provider: "mock".to_string(),
            model: None,
            tasks: tasks
                .into_iter()
                .map(|(id, cmd)| {
                    Arc::new(Task {
                        id: id.to_string(),
                        use_wiring: None,
                        output: None,
                        action: TaskAction::Exec {
                            exec: ExecParams {
                                command: cmd.to_string(),
                            },
                        },
                    })
                })
                .collect(),
            flows: flows
                .into_iter()
                .map(|(src, tgt)| Flow {
                    source: FlowEndpoint::Single(src.to_string()),
                    target: FlowEndpoint::Single(tgt.to_string()),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn event_sequence_for_single_task() {
        let workflow = create_exec_workflow(vec![("greet", "echo hello")], vec![]);
        let runner = Runner::new(workflow);

        let result = runner.run().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "hello");

        // Verify event sequence
        let events = runner.event_log().events();

        // Expected sequence:
        // 1. WorkflowStarted
        // 2. TaskScheduled
        // 3. TaskStarted (with inputs from TaskContext)
        // 4. TemplateResolved (from executor)
        // 5. TaskCompleted
        // 6. WorkflowCompleted

        assert!(events.len() >= 5, "Expected at least 5 events, got {}", events.len());

        // First event should be WorkflowStarted
        assert!(matches!(
            &events[0].kind,
            EventKind::WorkflowStarted { task_count: 1 }
        ));

        // Last event should be WorkflowCompleted
        let last = events.last().unwrap();
        assert!(matches!(&last.kind, EventKind::WorkflowCompleted { .. }));

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
        let runner = Runner::new(workflow);

        let result = runner.run().await;
        assert!(result.is_ok());

        let events = runner.event_log().events();

        // Verify WorkflowStarted with correct task count
        assert!(matches!(
            &events[0].kind,
            EventKind::WorkflowStarted { task_count: 2 }
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
        let runner = Runner::new(workflow);

        let result = runner.run().await;
        assert!(result.is_ok());

        let events = runner.event_log().events();

        // Verify WorkflowStarted
        assert!(matches!(
            &events[0].kind,
            EventKind::WorkflowStarted { task_count: 2 }
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

        // WorkflowCompleted should be last
        let last = events.last().unwrap();
        assert!(matches!(&last.kind, EventKind::WorkflowCompleted { .. }));
    }

    #[tokio::test]
    async fn event_ids_are_monotonic() {
        let workflow = create_exec_workflow(
            vec![("a", "echo 1"), ("b", "echo 2"), ("c", "echo 3")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow);

        runner.run().await.unwrap();

        let events = runner.event_log().events();
        let ids: Vec<u64> = events.iter().map(|e| e.id).collect();

        // Verify monotonic and sequential
        for i in 0..ids.len() {
            assert_eq!(ids[i], i as u64, "IDs should be sequential from 0");
        }
    }

    #[tokio::test]
    async fn timestamps_are_relative_and_increasing() {
        let workflow = create_exec_workflow(
            vec![("fast", "echo quick"), ("slow", "sleep 0.1 && echo done")],
            vec![("fast", "slow")],
        );
        let runner = Runner::new(workflow);

        runner.run().await.unwrap();

        let events = runner.event_log().events();

        // First timestamp should be small (near 0) - use 500ms threshold for CI tolerance
        assert!(events[0].timestamp_ms < 500, "First event should be near start");

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
        let runner = Runner::new(workflow);

        let result = runner.run().await;
        // Workflow completes but task failed
        assert!(result.is_ok());

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
        let runner = Runner::new(workflow);

        runner.run().await.unwrap();

        let events = runner.event_log().filter_task("echo_test");
        let template_event = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::TemplateResolved { .. }));

        assert!(template_event.is_some(), "TemplateResolved event expected");

        if let EventKind::TemplateResolved { template, result, .. } = &template_event.unwrap().kind
        {
            assert_eq!(template, "echo hello world");
            assert_eq!(result, "echo hello world");
        }
    }

    #[tokio::test]
    async fn event_log_to_json_serializes_correctly() {
        let workflow = create_exec_workflow(vec![("simple", "echo test")], vec![]);
        let runner = Runner::new(workflow);

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
}
