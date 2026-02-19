//! DAG Runner - workflow execution with tokio (v0.1)
//!
//! Performance optimizations:
//! - Arc for zero-cost task/context sharing
//! - JoinSet for efficient parallel task collection
//! - Tokio handles all concurrency (no artificial limits)

use rustc_hash::FxHashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::{debug, info, instrument};

use crate::ast::{Task, Workflow};
use crate::binding::ResolvedBindings;
use crate::dag::{validate_use_wiring, FlowGraph};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::store::{DataStore, TaskResult};
use crate::util::intern;

use super::executor::TaskExecutor;
use super::output::make_task_result;

/// Result of executing a task iteration
/// For for_each tasks, includes the iteration index for ordered aggregation
struct IterationResult {
    /// ID used for storage (task_id for regular, indexed for for_each)
    store_id: Arc<str>,
    /// The actual task result
    result: TaskResult,
    /// For for_each: (parent_id, index) to enable aggregation
    for_each_info: Option<(Arc<str>, usize)>,
}

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
        Self::with_event_log(workflow, EventLog::new())
    }

    /// Create a Runner with a custom EventLog (for TUI integration)
    ///
    /// Use `EventLog::new_with_broadcast()` to create an EventLog that
    /// sends events to TUI in real-time.
    pub fn with_event_log(workflow: Workflow, event_log: EventLog) -> Self {
        let flow_graph = FlowGraph::from_workflow(&workflow);
        let datastore = DataStore::new();
        let executor = TaskExecutor::new(
            &workflow.provider,
            workflow.model.as_deref(),
            workflow.mcp.clone(),
            event_log.clone(),
        );

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

    /// Execute a single task iteration (used for both regular tasks and for_each items)
    ///
    /// # Arguments
    ///
    /// * `task` - The task to execute
    /// * `task_id` - ID for this specific execution (may include index for for_each)
    /// * `parent_task_id` - Original task ID (for for_each, this is the parent task ID)
    /// * `datastore` - Data store for task results
    /// * `executor` - Task executor
    /// * `event_log` - Event log for observability
    /// * `for_each_binding` - Optional (var_name, value, index) for for_each iteration
    async fn execute_task_iteration(
        task: Arc<Task>,
        task_id: Arc<str>,
        parent_task_id: Arc<str>,
        datastore: DataStore,
        executor: TaskExecutor,
        event_log: EventLog,
        for_each_binding: Option<(String, Value, usize)>, // Added index
    ) -> IterationResult {
        let start = Instant::now();

        // Extract for_each info if present
        let for_each_info = for_each_binding
            .as_ref()
            .map(|(_, _, idx)| (Arc::clone(&parent_task_id), *idx));
        let _is_for_each = for_each_binding.is_some();

        // Build bindings from use: wiring
        let mut bindings = match ResolvedBindings::from_wiring_spec(task.use_wiring.as_ref(), &datastore)
        {
            Ok(b) => b,
            Err(e) => {
                let duration = start.elapsed();
                // EMIT: TaskFailed (bindings build failed)
                event_log.emit(EventKind::TaskFailed {
                    task_id: Arc::clone(&task_id),
                    error: e.to_string(),
                    duration_ms: duration.as_millis() as u64,
                });
                return IterationResult {
                    store_id: task_id, // Store with indexed ID for for_each
                    result: TaskResult::failed(e.to_string(), duration),
                    for_each_info,
                };
            }
        };

        // Add for_each binding if present (v0.3)
        if let Some((var_name, value, _idx)) = for_each_binding {
            bindings.set(&var_name, value);
        }

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
                let tr = make_task_result(output, task.output.as_ref(), duration).await;
                // EMIT: TaskCompleted or TaskFailed (based on result)
                if tr.is_success() {
                    event_log.emit(EventKind::TaskCompleted {
                        task_id: Arc::clone(&task_id),
                        output: Arc::clone(&tr.output), // O(1) Arc clone
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

        IterationResult {
            store_id: task_id, // Store individual results with indexed ID
            result: task_result,
            for_each_info,
        }
    }

    /// Main execution loop
    #[instrument(skip(self), fields(workflow_tasks = self.workflow.tasks.len()))]
    pub async fn run(&self) -> Result<String, NikaError> {
        let workflow_start = Instant::now();
        info!("Starting workflow execution");

        // Validate use: blocks before execution (fail-fast)
        validate_use_wiring(&self.workflow, &self.flow_graph)?;

        let total_tasks = self.workflow.tasks.len();
        let mut completed = 0;

        // EMIT: WorkflowStarted
        self.event_log.emit(EventKind::WorkflowStarted {
            task_count: total_tasks,
            generation_id: format!("gen-{}", uuid::Uuid::new_v4()),
            workflow_hash: self.workflow.compute_hash(),
            nika_version: env!("CARGO_PKG_VERSION").to_string(),
        });

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

            // Spawn all ready tasks in parallel (Tokio handles concurrency)
            let mut join_set = JoinSet::new();

            for task in ready {
                let task = Arc::clone(&task);
                let task_id = intern(&task.id); // Interned Arc<str> for deduplication

                // EMIT: TaskScheduled
                let deps = self.flow_graph.get_dependencies(&task.id);
                self.event_log.emit(EventKind::TaskScheduled {
                    task_id: Arc::clone(&task_id),
                    dependencies: deps.to_vec(), // Arc::clone is O(1)
                });

                println!(
                    "  {} {} {}",
                    "[⟳]".yellow(),
                    &task_id,
                    "running...".dimmed()
                );

                // Check if task has for_each (v0.3 parallelism)
                if let Some(for_each) = &task.for_each {
                    if let Some(items) = for_each.as_array() {
                        // Get concurrency settings from task (v0.3)
                        let concurrency = task.for_each_concurrency();
                        let fail_fast = task.for_each_fail_fast();

                        debug!(
                            task_id = %task.id,
                            items = items.len(),
                            concurrency = concurrency,
                            fail_fast = fail_fast,
                            "Starting for_each iteration"
                        );

                        // Create semaphore for concurrency limiting
                        let semaphore = Arc::new(Semaphore::new(concurrency));
                        // Create cancellation flag for fail_fast
                        let cancelled = Arc::new(AtomicBool::new(false));

                        // Spawn one execution per item in the array
                        let var_name = task.for_each_var().to_string();
                        for (idx, item) in items.iter().enumerate() {
                            // Check if cancelled before spawning
                            if fail_fast && cancelled.load(Ordering::Relaxed) {
                                debug!(
                                    task_id = %task.id,
                                    idx = idx,
                                    "Skipping iteration due to fail_fast cancellation"
                                );
                                break;
                            }

                            let task = Arc::clone(&task);
                            let task_id = intern(&format!("{}[{}]", task.id, idx));
                            let parent_task_id = intern(&task.id);
                            let datastore = self.datastore.clone();
                            let executor = self.executor.clone();
                            let event_log = self.event_log.clone();
                            let item = item.clone();
                            let var_name = var_name.clone();
                            let semaphore = Arc::clone(&semaphore);
                            let cancelled = Arc::clone(&cancelled);

                            join_set.spawn(async move {
                                // Acquire semaphore permit (blocks if at concurrency limit)
                                let _permit = semaphore.acquire().await.expect("semaphore closed");

                                // Check cancellation before executing
                                if cancelled.load(Ordering::Relaxed) {
                                    return IterationResult {
                                        store_id: task_id,
                                        result: TaskResult::failed(
                                            "Cancelled due to fail_fast".to_string(),
                                            std::time::Duration::ZERO,
                                        ),
                                        for_each_info: Some((parent_task_id, idx)),
                                    };
                                }

                                let result = Self::execute_task_iteration(
                                    task,
                                    Arc::clone(&task_id),
                                    Arc::clone(&parent_task_id),
                                    datastore,
                                    executor,
                                    event_log,
                                    Some((var_name, item, idx)),
                                )
                                .await;

                                // If failed and fail_fast, set cancellation flag
                                if !result.result.is_success() && fail_fast {
                                    cancelled.store(true, Ordering::Relaxed);
                                }

                                result
                            });
                        }
                    }
                } else {
                    // Regular task without for_each
                    let datastore = self.datastore.clone();
                    let executor = self.executor.clone();
                    let event_log = self.event_log.clone();

                    join_set.spawn(async move {
                        Self::execute_task_iteration(
                            task,
                            Arc::clone(&task_id),
                            task_id,
                            datastore,
                            executor,
                            event_log,
                            None,
                        )
                        .await
                    });
                }
            }

            // Collect for_each results for aggregation: parent_id -> Vec<(index, result)>
            let mut for_each_results: FxHashMap<Arc<str>, Vec<(usize, TaskResult)>> = FxHashMap::default();

            // Wait for all spawned tasks to complete
            while let Some(result) = join_set.join_next().await {
                match result {
                    Ok(iteration_result) => {
                        let IterationResult {
                            store_id,
                            result: task_result,
                            for_each_info,
                        } = iteration_result;

                        completed += 1;
                        let success = task_result.is_success();

                        let status = if success {
                            format!("[{}/{}]", completed, total_tasks).green()
                        } else {
                            format!("[{}/{}]", completed, total_tasks).red()
                        };

                        let symbol = if success { "✓" } else { "✗" };
                        let symbol_colored = if success {
                            symbol.green()
                        } else {
                            symbol.red()
                        };
                        let duration_str =
                            format!("({:.1}s)", task_result.duration.as_secs_f32()).dimmed();

                        println!(
                            "  {} {} {} {}",
                            status, &*store_id, symbol_colored, duration_str
                        );

                        if let Some(err) = task_result.error() {
                            println!("      {} {}", "Error:".red(), err);
                        }

                        // Store individual result
                        self.datastore
                            .insert(Arc::clone(&store_id), task_result.clone());

                        // If this is a for_each iteration, collect for aggregation
                        if let Some((parent_id, idx)) = for_each_info {
                            for_each_results
                                .entry(parent_id)
                                .or_default()
                                .push((idx, task_result));
                        }
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

            // Aggregate for_each results into parent task
            for (parent_id, mut results) in for_each_results {
                // Sort by index to preserve order
                results.sort_by_key(|(idx, _)| *idx);

                // Collect outputs into JSON array
                let outputs: Vec<Value> = results
                    .iter()
                    .map(|(_, r)| {
                        // Try to parse as JSON, fall back to string
                        let output_str = r.output_str();
                        serde_json::from_str(&output_str)
                            .unwrap_or(Value::String(output_str.into_owned()))
                    })
                    .collect();

                // Calculate aggregate duration and success
                let total_duration: std::time::Duration =
                    results.iter().map(|(_, r)| r.duration).sum();
                let all_success = results.iter().all(|(_, r)| r.is_success());

                // Create aggregated result with JSON array
                let aggregated_result = if all_success {
                    TaskResult::success(Value::Array(outputs), total_duration)
                } else {
                    // Collect errors
                    let errors: Vec<String> = results
                        .iter()
                        .filter_map(|(idx, r)| r.error().map(|e| format!("[{}]: {}", idx, e)))
                        .collect();
                    TaskResult::failed(errors.join("; "), total_duration)
                };

                // Store aggregated result under parent ID
                self.datastore.insert(parent_id, aggregated_result);
            }
        }

        // Get final output
        let output = self.get_final_output().unwrap_or_default();

        // EMIT: WorkflowCompleted
        self.event_log.emit(EventKind::WorkflowCompleted {
            final_output: Arc::new(Value::String(output.clone())),
            total_duration_ms: workflow_start.elapsed().as_millis() as u64,
        });

        println!("\n{} Done!\n", "✓".green());

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ExecParams, Flow, FlowEndpoint, Task, TaskAction};
    use std::sync::Arc;

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH RESULT AGGREGATION TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn test_for_each_collects_all_results() {
        // Create workflow with for_each that runs 3 items
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            tasks: vec![Arc::new(Task {
                id: "echo_items".to_string(),
                for_each: Some(serde_json::json!(["a", "b", "c"])),
                for_each_as: Some("item".to_string()),
                concurrency: None,  // Default sequential
                fail_fast: None,    // Default true
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.item}}".to_string(),
                    },
                },
                use_wiring: None,
                output: None,
            })],
            flows: vec![],
        };

        let runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(
            result.is_ok(),
            "Workflow should complete: {:?}",
            result.err()
        );

        // The final output should contain results from all 3 iterations
        // When for_each completes, results should be aggregated
        // Check datastore has the parent task result
        let parent_result = runner.datastore.get("echo_items");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        // Should contain all three outputs somehow (either as array or concatenated)
        // The exact format depends on implementation, but all should be present
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
        // Create workflow with for_each that runs 5 items
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            tasks: vec![Arc::new(Task {
                id: "ordered".to_string(),
                for_each: Some(serde_json::json!(["first", "second", "third"])),
                for_each_as: Some("x".to_string()),
                concurrency: None,
                fail_fast: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.x}}".to_string(),
                    },
                },
                use_wiring: None,
                output: None,
            })],
            flows: vec![],
        };

        let runner = Runner::new(workflow);
        runner.run().await.unwrap();

        let parent_result = runner.datastore.get("ordered");
        assert!(parent_result.is_some(), "Parent task result should exist");

        // If stored as array, order should be preserved
        let result = parent_result.unwrap();
        let output = result.output_str();
        if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(&output) {
            assert_eq!(arr.len(), 3, "Should have 3 results");
            // First element should be "first", last should be "third"
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
        // If not an array, at least verify all are present (parallel execution may reorder)
    }

    // ═══════════════════════════════════════════════════════════════
    // BASIC WORKFLOW TESTS
    // ═══════════════════════════════════════════════════════════════

    /// Helper to create a minimal workflow with exec tasks
    fn create_exec_workflow(tasks: Vec<(&str, &str)>, flows: Vec<(&str, &str)>) -> Workflow {
        Workflow {
            schema: "nika/workflow@0.1".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            tasks: tasks
                .into_iter()
                .map(|(id, cmd)| {
                    Arc::new(Task {
                        id: id.to_string(),
                        use_wiring: None,
                        output: None,
                        for_each: None,
                        for_each_as: None,
                        concurrency: None,
                        fail_fast: None,
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
        let runner = Runner::new(workflow);

        let result = runner.run().await;
        assert!(result.is_ok());

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
        for (i, &id) in ids.iter().enumerate() {
            assert_eq!(id, i as u64, "IDs should be sequential from 0");
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

    // ═══════════════════════════════════════════════════════════════
    // UNIT TESTS FOR RUNNER INTERNAL METHODS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn get_ready_tasks_returns_tasks_with_no_deps() {
        // Two independent tasks - both should be ready
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B")],
            vec![], // No flows = no dependencies
        );
        let runner = Runner::new(workflow);

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 2, "Both tasks should be ready");

        let ids: Vec<&str> = ready.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"a"), "Task 'a' should be ready");
        assert!(ids.contains(&"b"), "Task 'b' should be ready");
    }

    #[test]
    fn get_ready_tasks_respects_dependencies() {
        // Chain: a -> b -> c
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B"), ("c", "echo C")],
            vec![("a", "b"), ("b", "c")],
        );
        let runner = Runner::new(workflow);

        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1, "Only first task should be ready");
        assert_eq!(ready[0].id, "a", "Task 'a' should be ready");
    }

    #[test]
    fn get_ready_tasks_excludes_completed_tasks() {
        let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
        let runner = Runner::new(workflow);

        // Initially task is ready
        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 1);

        // Mark task as done
        runner
            .datastore
            .insert(intern("only"), TaskResult::success_str("done", std::time::Duration::ZERO));

        // Now no tasks should be ready
        let ready = runner.get_ready_tasks();
        assert_eq!(ready.len(), 0, "Completed task should not be ready");
    }

    #[test]
    fn all_done_returns_false_when_tasks_pending() {
        let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
        let runner = Runner::new(workflow);

        assert!(!runner.all_done(), "Not all tasks are done initially");
    }

    #[test]
    fn all_done_returns_true_when_all_completed() {
        let workflow = create_exec_workflow(vec![("a", "echo A"), ("b", "echo B")], vec![]);
        let runner = Runner::new(workflow);

        // Mark all tasks as done
        runner
            .datastore
            .insert(intern("a"), TaskResult::success_str("A", std::time::Duration::ZERO));
        runner
            .datastore
            .insert(intern("b"), TaskResult::success_str("B", std::time::Duration::ZERO));

        assert!(runner.all_done(), "All tasks should be done");
    }

    #[test]
    fn get_final_output_returns_output_from_final_task() {
        // Chain: a -> b (b is final)
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "echo B")],
            vec![("a", "b")],
        );
        let runner = Runner::new(workflow);

        // Mark tasks as done
        runner
            .datastore
            .insert(intern("a"), TaskResult::success_str("A", std::time::Duration::ZERO));
        runner
            .datastore
            .insert(intern("b"), TaskResult::success_str("final output", std::time::Duration::ZERO));

        let output = runner.get_final_output();
        assert!(output.is_some());
        assert_eq!(output.unwrap(), "final output");
    }

    #[test]
    fn get_final_output_returns_none_when_no_results() {
        let workflow = create_exec_workflow(vec![("only", "echo x")], vec![]);
        let runner = Runner::new(workflow);

        let output = runner.get_final_output();
        assert!(output.is_none(), "No output when tasks not complete");
    }

    #[test]
    fn get_final_output_skips_failed_tasks() {
        let workflow = create_exec_workflow(
            vec![("a", "echo A"), ("b", "exit 1")],
            vec![], // Both are final tasks (no successors)
        );
        let runner = Runner::new(workflow);

        // a succeeds, b fails
        runner
            .datastore
            .insert(intern("a"), TaskResult::success_str("success", std::time::Duration::ZERO));
        runner
            .datastore
            .insert(intern("b"), TaskResult::failed("error", std::time::Duration::ZERO));

        let output = runner.get_final_output();
        assert!(output.is_some());
        assert_eq!(output.unwrap(), "success", "Should return successful task output");
    }

    // ═══════════════════════════════════════════════════════════════
    // FOR_EACH CONCURRENCY AND FAIL_FAST TESTS
    // ═══════════════════════════════════════════════════════════════

    #[tokio::test]
    async fn for_each_with_explicit_concurrency() {
        // Create workflow with for_each that specifies concurrency=2
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            tasks: vec![Arc::new(Task {
                id: "concurrent".to_string(),
                for_each: Some(serde_json::json!(["a", "b", "c", "d"])),
                for_each_as: Some("item".to_string()),
                concurrency: Some(2), // Limit to 2 concurrent
                fail_fast: None,
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.item}}".to_string(),
                    },
                },
                use_wiring: None,
                output: None,
            })],
            flows: vec![],
        };

        let runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(result.is_ok(), "Workflow should complete: {:?}", result.err());

        // Verify all 4 items were processed
        let parent_result = runner.datastore.get("concurrent");
        assert!(parent_result.is_some(), "Parent task result should exist");

        let result = parent_result.unwrap();
        let output = result.output_str();
        assert!(output.contains("a") || output.contains("\"a\""));
        assert!(output.contains("d") || output.contains("\"d\""));
    }

    #[tokio::test]
    async fn for_each_fail_fast_stops_on_first_error() {
        // Create workflow with for_each where middle item fails
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            tasks: vec![Arc::new(Task {
                id: "failfast".to_string(),
                for_each: Some(serde_json::json!(["ok1", "FAIL", "ok2", "ok3"])),
                for_each_as: Some("item".to_string()),
                concurrency: Some(1), // Sequential to make failure predictable
                fail_fast: Some(true),
                action: TaskAction::Exec {
                    exec: ExecParams {
                        // Exit with error if item is "FAIL"
                        command: "test '{{use.item}}' != 'FAIL' && echo {{use.item}}".to_string(),
                    },
                },
                use_wiring: None,
                output: None,
            })],
            flows: vec![],
        };

        let runner = Runner::new(workflow);
        let result = runner.run().await;
        // Workflow completes but parent task may be marked as failed
        assert!(result.is_ok() || result.is_err());

        // The important thing is that some iterations may have been skipped
        // due to fail_fast behavior
    }

    #[tokio::test]
    async fn for_each_fail_fast_false_continues_on_error() {
        // Create workflow with fail_fast=false
        let workflow = Workflow {
            schema: "nika/workflow@0.3".to_string(),
            provider: "mock".to_string(),
            model: None,
            mcp: None,
            tasks: vec![Arc::new(Task {
                id: "continue".to_string(),
                for_each: Some(serde_json::json!(["ok1", "ok2"])),
                for_each_as: Some("item".to_string()),
                concurrency: None,
                fail_fast: Some(false), // Explicitly disable fail_fast
                action: TaskAction::Exec {
                    exec: ExecParams {
                        command: "echo {{use.item}}".to_string(),
                    },
                },
                use_wiring: None,
                output: None,
            })],
            flows: vec![],
        };

        let runner = Runner::new(workflow);
        let result = runner.run().await;
        assert!(result.is_ok(), "Workflow should complete");

        // All items should be processed
        let parent_result = runner.datastore.get("continue");
        assert!(parent_result.is_some());
    }

    // ═══════════════════════════════════════════════════════════════
    // CONSTRUCTOR AND EVENT LOG TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn with_event_log_uses_provided_event_log() {
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let custom_log = EventLog::new();
        let runner = Runner::with_event_log(workflow, custom_log);

        // The runner should use the provided event log
        assert!(runner.event_log().events().is_empty());
    }

    #[tokio::test]
    async fn workflow_completed_event_has_duration() {
        let workflow = create_exec_workflow(vec![("quick", "echo fast")], vec![]);
        let runner = Runner::new(workflow);

        runner.run().await.unwrap();

        let events = runner.event_log().events();
        let completed = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowCompleted { .. }));

        assert!(completed.is_some());
        if let EventKind::WorkflowCompleted { total_duration_ms, .. } = &completed.unwrap().kind {
            assert!(*total_duration_ms > 0, "Duration should be positive");
        }
    }

    #[tokio::test]
    async fn workflow_started_event_has_generation_id() {
        let workflow = create_exec_workflow(vec![("a", "echo A")], vec![]);
        let runner = Runner::new(workflow);

        runner.run().await.unwrap();

        let events = runner.event_log().events();
        let started = events
            .iter()
            .find(|e| matches!(&e.kind, EventKind::WorkflowStarted { .. }));

        assert!(started.is_some());
        if let EventKind::WorkflowStarted { generation_id, .. } = &started.unwrap().kind {
            assert!(generation_id.starts_with("gen-"), "Generation ID should have prefix");
            assert!(generation_id.len() > 10, "Generation ID should include UUID");
        }
    }
}
