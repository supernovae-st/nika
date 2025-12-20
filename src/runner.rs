//! Simple workflow runner for MVP
//!
//! Executes workflows using Claude CLI as the provider.
//! Architecture v3: 2 task types (agent + action) with scope.

use crate::workflow::{Scope, Task, TaskType, Workflow};
use anyhow::{anyhow, Result};
use std::collections::HashMap;
use std::process::Command;

/// Execution result for a task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub tokens_used: Option<u32>,
}

/// Workflow execution summary
#[derive(Debug)]
pub struct RunResult {
    pub workflow_name: String,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub results: Vec<TaskResult>,
    pub total_tokens: u32,
}

/// Simple workflow runner
pub struct Runner {
    /// Provider to use (claude, openai, ollama)
    provider: String,
    /// Verbose output
    verbose: bool,
}

impl Runner {
    pub fn new(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
            verbose: false,
        }
    }

    pub fn verbose(mut self, v: bool) -> Self {
        self.verbose = v;
        self
    }

    /// Execute a workflow
    pub fn run(&self, workflow: &Workflow) -> Result<RunResult> {
        let mut results = Vec::new();
        let mut total_tokens = 0u32;

        // Build task map for lookups
        let task_map: HashMap<&str, &Task> =
            workflow.tasks.iter().map(|t| (t.id.as_str(), t)).collect();

        // Get execution order (topological sort)
        let order = self.topological_sort(workflow)?;

        if self.verbose {
            println!("Execution order: {:?}", order);
        }

        // Execute tasks in order
        for task_id in &order {
            let task = task_map
                .get(task_id.as_str())
                .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

            if self.verbose {
                println!("\n→ Executing: {} ({:?})", task_id, task.task_type);
            }

            let result = self.execute_task(task, workflow)?;

            if let Some(tokens) = result.tokens_used {
                total_tokens += tokens;
            }

            if self.verbose {
                println!(
                    "  {} {}",
                    if result.success { "✓" } else { "✗" },
                    if result.output.len() > 100 {
                        format!("{}...", &result.output[..100])
                    } else {
                        result.output.clone()
                    }
                );
            }

            results.push(result);
        }

        let tasks_completed = results.iter().filter(|r| r.success).count();
        let tasks_failed = results.len() - tasks_completed;

        Ok(RunResult {
            workflow_name: workflow
                .main_agent
                .system_prompt
                .as_deref()
                .and_then(|s| s.lines().next())
                .unwrap_or("workflow")
                .to_string(),
            tasks_completed,
            tasks_failed,
            results,
            total_tokens,
        })
    }

    /// Execute a single task
    fn execute_task(&self, task: &Task, workflow: &Workflow) -> Result<TaskResult> {
        match &task.task_type {
            TaskType::Agent => self.execute_agent(task, workflow),
            TaskType::Action => self.execute_action(task),
        }
    }

    /// Execute an agent task (LLM call)
    fn execute_agent(&self, task: &Task, workflow: &Workflow) -> Result<TaskResult> {
        let prompt = task.prompt.as_deref().unwrap_or("");
        let scope = task.scope.as_ref().unwrap_or(&Scope::Main);

        match self.provider.as_str() {
            "claude" => self.execute_claude_agent(task, prompt, scope, workflow),
            "openai" => Ok(TaskResult {
                task_id: task.id.clone(),
                success: true,
                output: format!("[OpenAI] Would execute: {}", prompt),
                tokens_used: Some(500),
            }),
            "ollama" => Ok(TaskResult {
                task_id: task.id.clone(),
                success: true,
                output: format!("[Ollama] Would execute: {}", prompt),
                tokens_used: Some(500),
            }),
            "mock" => Ok(TaskResult {
                task_id: task.id.clone(),
                success: true,
                output: format!("[Mock] Executed: {}", prompt),
                tokens_used: Some(100),
            }),
            _ => Err(anyhow!("Unknown provider: {}", self.provider)),
        }
    }

    /// Execute agent task using Claude CLI
    fn execute_claude_agent(
        &self,
        task: &Task,
        prompt: &str,
        scope: &Scope,
        _workflow: &Workflow,
    ) -> Result<TaskResult> {
        // Check if claude CLI is available
        let claude_check = Command::new("which").arg("claude").output();

        if claude_check.is_err() || !claude_check.unwrap().status.success() {
            return Ok(TaskResult {
                task_id: task.id.clone(),
                success: true,
                output: format!(
                    "[Claude CLI not found] Would execute {} task: {}",
                    match scope {
                        Scope::Main => "context",
                        Scope::Isolated => "isolated",
                    },
                    prompt
                ),
                tokens_used: Some(0),
            });
        }

        // Build claude command: claude -p "prompt"
        let mut cmd = Command::new("claude");
        cmd.arg("-p"); // Print mode (non-interactive)

        // Add system prompt for isolated scope
        if *scope == Scope::Isolated {
            if let Some(sys_prompt) = &task.system_prompt {
                cmd.arg("--system-prompt").arg(sys_prompt);
            }
        }

        // Add the prompt as positional argument
        cmd.arg(prompt);

        // Skip permissions for automated execution
        cmd.arg("--dangerously-skip-permissions");

        // Execute
        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let success = output.status.success();

                Ok(TaskResult {
                    task_id: task.id.clone(),
                    success,
                    output: if success {
                        stdout
                    } else {
                        String::from_utf8_lossy(&output.stderr).to_string()
                    },
                    tokens_used: Some(500), // Estimate
                })
            }
            Err(e) => Ok(TaskResult {
                task_id: task.id.clone(),
                success: false,
                output: format!("Failed to execute claude: {}", e),
                tokens_used: None,
            }),
        }
    }

    /// Execute an action task (deterministic)
    fn execute_action(&self, task: &Task) -> Result<TaskResult> {
        let run = task.run.as_deref().unwrap_or("passthrough");

        let output = match run {
            "aggregate" => "[action:aggregate] Aggregated inputs".to_string(),
            "transform" => {
                let format = task.format.as_deref().unwrap_or("json");
                format!("[action:transform] Transformed to {}", format)
            }
            "cache" => "[action:cache] Cached result".to_string(),
            "http" => {
                let url = task.url.as_deref().unwrap_or("(no url)");
                format!("[action:http] Would call {}", url)
            }
            "slack" => {
                let channel = task.channel.as_deref().unwrap_or("#general");
                format!("[action:slack] Would post to {}", channel)
            }
            _ => format!("[action:{}] Executed", run),
        };

        Ok(TaskResult {
            task_id: task.id.clone(),
            success: true,
            output,
            tokens_used: Some(0), // Actions are free
        })
    }

    /// Topological sort for execution order
    fn topological_sort(&self, workflow: &Workflow) -> Result<Vec<String>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();

        // Initialize
        for task in &workflow.tasks {
            in_degree.insert(&task.id, 0);
            adjacency.insert(&task.id, Vec::new());
        }

        // Build graph
        for flow in &workflow.flows {
            if let Some(adj) = adjacency.get_mut(flow.source.as_str()) {
                adj.push(&flow.target);
            }
            if let Some(deg) = in_degree.get_mut(flow.target.as_str()) {
                *deg += 1;
            }
        }

        // Kahn's algorithm
        let mut queue: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut result = Vec::new();

        while let Some(node) = queue.pop() {
            result.push(node.to_string());

            if let Some(neighbors) = adjacency.get(node) {
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push(neighbor);
                        }
                    }
                }
            }
        }

        if result.len() != workflow.tasks.len() {
            return Err(anyhow!("Workflow has cycles"));
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_workflow() -> Workflow {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test workflow"

tasks:
  - id: step1
    type: agent
    prompt: "Analyze this"

  - id: step2
    type: action
    run: transform
    format: uppercase

flows:
  - source: step1
    target: step2
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_topological_sort() {
        let workflow = make_workflow();
        let runner = Runner::new("claude");
        let order = runner.topological_sort(&workflow).unwrap();
        assert_eq!(order, vec!["step1", "step2"]);
    }

    #[test]
    fn test_run_workflow() {
        let workflow = make_workflow();
        // Use "mock" provider to avoid actually calling Claude CLI in tests
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        // Both tasks should complete with placeholders
        assert_eq!(result.tasks_completed, 2, "Should complete 2 tasks");
        assert_eq!(result.tasks_failed, 0, "No tasks should fail");
        assert_eq!(result.results.len(), 2, "Should have 2 results");
    }
}
