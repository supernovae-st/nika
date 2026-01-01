//! DAG execution with tokio (v0.1)
//!
//! Simplified runner using TaskExecutor for execution
//! and TaskContext::from_use_block for context building.

use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::task::JoinSet;
use tracing::{info, instrument};

use crate::context::TaskContext;
use crate::dag::DagAnalyzer;
use crate::datastore::{DataStore, TaskResult};
use crate::error::NikaError;
use crate::event::{EventKind, EventLog};
use crate::executor::TaskExecutor;
use crate::output_policy::OutputFormat;
use crate::validator;
use crate::workflow::{Task, Workflow};

/// DAG workflow runner with event sourcing
pub struct Runner {
    workflow: Workflow,
    dag: DagAnalyzer,
    datastore: DataStore,
    executor: TaskExecutor,
    event_log: EventLog,
}

impl Runner {
    pub fn new(workflow: Workflow) -> Self {
        let dag = DagAnalyzer::from_workflow(&workflow);
        let datastore = DataStore::new();
        let event_log = EventLog::new();
        let executor = TaskExecutor::new(&workflow.provider, workflow.model.as_deref());

        Self {
            workflow,
            dag,
            datastore,
            executor,
            event_log,
        }
    }

    /// Get the event log for inspection/export
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
                let deps = self.dag.get_dependencies(&task.id);
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
        let final_tasks = self.dag.get_final_tasks();

        // Return first successful final task output
        for task_id in final_tasks {
            if let Some(result) = self.datastore.get(&task_id) {
                if result.is_success() {
                    return Some(result.output_str());
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
        validator::validate_use_blocks(&self.workflow, &self.dag)?;

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
                let task_id = task.id.clone(); // Clone once, reuse in spawn
                let datastore = self.datastore.clone();
                let executor = self.executor.clone();
                let event_log = self.event_log.clone();

                // EMIT: TaskScheduled
                let deps = self.dag.get_dependencies(&task.id);
                self.event_log.emit(EventKind::TaskScheduled {
                    task_id: task_id.clone(),
                    dependencies: deps.into_iter().cloned().collect(),
                });

                println!("  {} {} {}", "[⟳]".yellow(), &task_id, "running...".dimmed());

                join_set.spawn(async move {
                    let start = Instant::now();

                    // Build context from use: block
                    let context = match TaskContext::from_use_block(
                        task.use_block.as_ref(),
                        &datastore,
                    ) {
                        Ok(ctx) => ctx,
                        Err(e) => {
                            let duration = start.elapsed();
                            // EMIT: TaskFailed (context build failed)
                            event_log.emit(EventKind::TaskFailed {
                                task_id: task_id.clone(),
                                error: e.to_string(),
                                duration_ms: duration.as_millis() as u64,
                            });
                            return (task_id, TaskResult::failed(e.to_string(), duration));
                        }
                    };

                    // EMIT: InputsResolved (the original request!)
                    event_log.emit(EventKind::InputsResolved {
                        task_id: task_id.clone(),
                        inputs: context.to_value(),
                    });

                    // EMIT: TaskStarted
                    event_log.emit(EventKind::TaskStarted {
                        task_id: task_id.clone(),
                    });

                    // Execute via TaskExecutor
                    let result = executor.execute(&task.action, &context).await;
                    let duration = start.elapsed();

                    // Convert result to TaskResult with output policy
                    let task_result = match result {
                        Ok(output) => {
                            let tr = make_task_result(output, task.output.as_ref(), duration);
                            // EMIT: TaskCompleted or TaskFailed (based on result)
                            if tr.is_success() {
                                event_log.emit(EventKind::TaskCompleted {
                                    task_id: task_id.clone(),
                                    output: tr.output.clone(),
                                    duration_ms: duration.as_millis() as u64,
                                });
                            } else {
                                event_log.emit(EventKind::TaskFailed {
                                    task_id: task_id.clone(),
                                    error: tr.error().unwrap_or("Unknown error").to_string(),
                                    duration_ms: duration.as_millis() as u64,
                                });
                            }
                            tr
                        }
                        Err(e) => {
                            // EMIT: TaskFailed
                            event_log.emit(EventKind::TaskFailed {
                                task_id: task_id.clone(),
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

                        println!("  {} {} {} {}", status, task_id, symbol_colored, duration_str);

                        if let Some(err) = task_result.error() {
                            println!("      {} {}", "Error:".red(), err);
                        }

                        self.datastore.insert(&task_id, task_result);
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
