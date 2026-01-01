//! DAG execution with tokio (v0.1)
//!
//! Simplified runner using TaskExecutor for execution
//! and TaskContext::from_use_block for context building.

use std::sync::Arc;
use std::time::Instant;

use colored::Colorize;
use serde_json::Value;
use tokio::task::JoinSet;

use crate::context::TaskContext;
use crate::dag::DagAnalyzer;
use crate::datastore::{DataStore, TaskResult};
use crate::error::NikaError;
use crate::executor::TaskExecutor;
use crate::output_policy::OutputFormat;
use crate::workflow::{Task, Workflow};

/// DAG workflow runner
pub struct Runner {
    workflow: Workflow,
    dag: DagAnalyzer,
    datastore: DataStore,
    executor: TaskExecutor,
}

impl Runner {
    pub fn new(workflow: Workflow) -> Self {
        let dag = DagAnalyzer::from_workflow(&workflow);
        let datastore = DataStore::new();
        let executor = TaskExecutor::new(&workflow.provider, workflow.model.as_deref());

        Self {
            workflow,
            dag,
            datastore,
            executor,
        }
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
    pub async fn run(&self) -> Result<String, NikaError> {
        let total_tasks = self.workflow.tasks.len();
        let mut completed = 0;

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
                            return (task_id, TaskResult::failed(e.to_string(), duration));
                        }
                    };

                    // Execute via TaskExecutor
                    let result = executor.execute(&task.action, &context).await;
                    let duration = start.elapsed();

                    // Convert result to TaskResult with output policy
                    let task_result = match result {
                        Ok(output) => make_task_result(output, task.output.as_ref(), duration),
                        Err(e) => TaskResult::failed(e.to_string(), duration),
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
                        return Err(NikaError::Execution(format!("Task panicked: {}", e)));
                    }
                }
            }
        }

        // Get final output
        let output = self.get_final_output().unwrap_or_default();

        println!("\n{} Done!\n", "✓".green());

        Ok(output)
    }
}

/// Convert execution output to TaskResult, parsing as JSON if output format is json
fn make_task_result(
    output: String,
    policy: Option<&crate::output_policy::OutputPolicy>,
    duration: std::time::Duration,
) -> TaskResult {
    if let Some(policy) = policy {
        if policy.format == OutputFormat::Json {
            if let Ok(json_value) = serde_json::from_str::<Value>(&output) {
                return TaskResult::success(json_value, duration);
            }
        }
    }
    TaskResult::success_str(output, duration)
}
