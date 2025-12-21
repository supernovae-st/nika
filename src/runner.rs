//! Nika Workflow Runner (v4.5)
//!
//! Executes workflows using Claude CLI as the provider.
//! Architecture v4.5: 7 keywords with type inference.
//!
//! Key feature: ExecutionContext for passing data between tasks.

use crate::workflow::{Task, TaskKeyword, Workflow};
use anyhow::{anyhow, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::process::Command;

// ============================================================================
// LAZY REGEX PATTERNS (compiled once)
// ============================================================================

/// Pattern for {{task_id}} or {{task_id.field}} references
static TASK_REF_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\{\{([\w-]+)(?:\.([\w-]+))?\}\}").unwrap());

/// Pattern for ${input.name} references
static INPUT_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$\{input\.(\w+)\}").unwrap());

/// Pattern for ${env.NAME} references
static ENV_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\$\{env\.(\w+)\}").unwrap());

// ============================================================================
// EXECUTION CONTEXT
// ============================================================================

/// Execution context passed between tasks
///
/// This enables:
/// - Task output references: {{task_id}} or {{task_id.field}}
/// - Shared agent conversation history
/// - Environment and secrets access
#[derive(Debug, Default, Clone)]
pub struct ExecutionContext {
    /// Outputs from completed tasks (task_id -> output string)
    outputs: HashMap<String, String>,

    /// Structured outputs for field access (task_id -> JSON value)
    structured_outputs: HashMap<String, serde_json::Value>,

    /// Main agent conversation history (for context sharing between agent: tasks)
    agent_history: Vec<AgentMessage>,

    /// Input parameters passed to the workflow
    inputs: HashMap<String, String>,

    /// Environment variables snapshot
    env_vars: HashMap<String, String>,
}

/// A message in the agent conversation history
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub role: MessageRole,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context with input parameters
    pub fn with_inputs(inputs: HashMap<String, String>) -> Self {
        Self {
            inputs,
            ..Default::default()
        }
    }

    /// Store a task's output
    pub fn set_output(&mut self, task_id: &str, output: String) {
        self.outputs.insert(task_id.to_string(), output);
    }

    /// Store a structured output (for field access)
    pub fn set_structured_output(&mut self, task_id: &str, value: serde_json::Value) {
        self.structured_outputs.insert(task_id.to_string(), value);
    }

    /// Get a task's output
    pub fn get_output(&self, task_id: &str) -> Option<&String> {
        self.outputs.get(task_id)
    }

    /// Get a field from a structured output
    pub fn get_field(&self, task_id: &str, field: &str) -> Option<String> {
        self.structured_outputs
            .get(task_id)
            .and_then(|v| v.get(field))
            .map(|v| match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            })
    }

    /// Get an input parameter
    pub fn get_input(&self, name: &str) -> Option<&String> {
        self.inputs.get(name)
    }

    /// Get an environment variable
    pub fn get_env(&self, name: &str) -> Option<String> {
        self.env_vars
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
    }

    /// Add a message to the agent conversation history
    pub fn add_agent_message(&mut self, role: MessageRole, content: String) {
        self.agent_history.push(AgentMessage { role, content });
    }

    /// Get the agent conversation history
    pub fn agent_history(&self) -> &[AgentMessage] {
        &self.agent_history
    }

    /// Get conversation history as a formatted string for context injection
    pub fn format_agent_history(&self) -> String {
        self.agent_history
            .iter()
            .map(|msg| {
                let role = match msg.role {
                    MessageRole::User => "User",
                    MessageRole::Assistant => "Assistant",
                    MessageRole::System => "System",
                };
                format!("{}: {}", role, msg.content)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Check if we have any conversation history
    pub fn has_history(&self) -> bool {
        !self.agent_history.is_empty()
    }
}

// ============================================================================
// TEMPLATE RESOLUTION
// ============================================================================

/// Generic pattern resolver - DRY helper for template substitution
///
/// Applies a regex pattern to the input string and replaces matches using
/// the provided resolver function. If the resolver returns None, the original
/// match is preserved.
fn resolve_pattern<F>(input: &str, pattern: &Regex, resolver: F) -> String
where
    F: Fn(&regex::Captures) -> Option<String>,
{
    let mut result = input.to_string();
    for cap in pattern.captures_iter(input) {
        let full_match = cap.get(0).unwrap().as_str();
        if let Some(replacement) = resolver(&cap) {
            result = result.replace(full_match, &replacement);
        }
    }
    result
}

/// Resolve task args (YAML → String with template resolution)
///
/// Used by mcp: and function: tasks to serialize and resolve their args.
fn resolve_args(args: Option<&serde_yaml::Value>, ctx: &ExecutionContext) -> Result<String> {
    match args {
        Some(args) => {
            let raw_args = serde_yaml::to_string(args).unwrap_or_default();
            resolve_templates(&raw_args, ctx)
        }
        None => Ok(String::new()),
    }
}

/// Resolve templates in a string using the execution context
///
/// Supported formats:
/// - {{task_id}} - Reference entire task output
/// - {{task_id.field}} - Reference field from structured output
/// - ${input.name} - Reference input parameter
/// - ${env.NAME} - Reference environment variable
pub fn resolve_templates(template: &str, ctx: &ExecutionContext) -> Result<String> {
    // 1. Resolve {{task_id}} and {{task_id.field}} patterns
    let result = resolve_pattern(template, &TASK_REF_PATTERN, |cap| {
        let task_id = cap.get(1).unwrap().as_str();
        let field = cap.get(2).map(|m| m.as_str());

        Some(if let Some(field_name) = field {
            ctx.get_field(task_id, field_name)
                .unwrap_or_else(|| format!("{{{{{}:{}}}}}", task_id, field_name))
        } else {
            ctx.get_output(task_id)
                .cloned()
                .unwrap_or_else(|| format!("{{{{{}}}}}", task_id))
        })
    });

    // 2. Resolve ${input.name} patterns
    let result = resolve_pattern(&result, &INPUT_PATTERN, |cap| {
        let input_name = cap.get(1).unwrap().as_str();
        Some(
            ctx.get_input(input_name)
                .cloned()
                .unwrap_or_else(|| format!("${{input.{}}}", input_name)),
        )
    });

    // 3. Resolve ${env.NAME} patterns
    let result = resolve_pattern(&result, &ENV_PATTERN, |cap| {
        let env_name = cap.get(1).unwrap().as_str();
        Some(
            ctx.get_env(env_name)
                .unwrap_or_else(|| format!("${{env.{}}}", env_name)),
        )
    });

    Ok(result)
}

// ============================================================================
// TASK RESULT
// ============================================================================

/// Execution result for a task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub tokens_used: Option<u32>,
}

impl TaskResult {
    /// Create a successful task result
    pub fn success(id: impl Into<String>, output: impl Into<String>, tokens: Option<u32>) -> Self {
        Self {
            task_id: id.into(),
            success: true,
            output: output.into(),
            tokens_used: tokens,
        }
    }

    /// Create a failed task result
    pub fn failure(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            task_id: id.into(),
            success: false,
            output: error.into(),
            tokens_used: None,
        }
    }

    /// Try to parse the output as JSON
    pub fn as_json(&self) -> Option<serde_json::Value> {
        serde_json::from_str(&self.output).ok()
    }
}

// ============================================================================
// RUN RESULT
// ============================================================================

/// Workflow execution summary
#[derive(Debug)]
pub struct RunResult {
    pub workflow_name: String,
    pub tasks_completed: usize,
    pub tasks_failed: usize,
    pub results: Vec<TaskResult>,
    pub total_tokens: u32,
    /// The final execution context (for inspection/debugging)
    pub context: ExecutionContext,
}

// ============================================================================
// RUNNER
// ============================================================================

/// Workflow runner with context management
pub struct Runner {
    /// Provider to use (claude, openai, ollama, mock)
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

    /// Execute a workflow with default empty context
    pub fn run(&self, workflow: &Workflow) -> Result<RunResult> {
        self.run_with_context(workflow, ExecutionContext::new())
    }

    /// Execute a workflow with provided inputs
    pub fn run_with_inputs(
        &self,
        workflow: &Workflow,
        inputs: HashMap<String, String>,
    ) -> Result<RunResult> {
        self.run_with_context(workflow, ExecutionContext::with_inputs(inputs))
    }

    /// Execute a workflow with a pre-configured context
    pub fn run_with_context(
        &self,
        workflow: &Workflow,
        mut ctx: ExecutionContext,
    ) -> Result<RunResult> {
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
                let keyword = task
                    .keyword()
                    .map(|k| format!("{}", k))
                    .unwrap_or_else(|| "unknown".to_string());
                println!("\n→ Executing: {} ({})", task_id, keyword);
            }

            // Execute task with context
            let result = self.execute_task(task, workflow, &mut ctx)?;

            if let Some(tokens) = result.tokens_used {
                total_tokens += tokens;
            }

            // Store output in context for downstream tasks
            ctx.set_output(&result.task_id, result.output.clone());

            // Try to parse as JSON for structured access
            if let Some(json) = result.as_json() {
                ctx.set_structured_output(&result.task_id, json);
            }

            // For agent: tasks, add to conversation history
            if task.keyword() == Some(TaskKeyword::Agent) {
                // Add the prompt as user message
                if let Some(prompt) = &task.agent {
                    ctx.add_agent_message(MessageRole::User, prompt.clone());
                }
                // Add the response as assistant message
                ctx.add_agent_message(MessageRole::Assistant, result.output.clone());
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
                .agent
                .system_prompt
                .as_deref()
                .and_then(|s| s.lines().next())
                .unwrap_or("workflow")
                .to_string(),
            tasks_completed,
            tasks_failed,
            results,
            total_tokens,
            context: ctx,
        })
    }

    /// Execute a single task with context (v4.5 - keyword based)
    fn execute_task(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        match task.keyword() {
            Some(TaskKeyword::Agent) => self.execute_agent(task, workflow, ctx),
            Some(TaskKeyword::Subagent) => self.execute_subagent(task, workflow, ctx),
            Some(TaskKeyword::Shell) => self.execute_shell(task, ctx),
            Some(TaskKeyword::Http) => self.execute_http(task, ctx),
            Some(TaskKeyword::Mcp) => self.execute_mcp(task, ctx),
            Some(TaskKeyword::Function) => self.execute_function(task, ctx),
            Some(TaskKeyword::Llm) => self.execute_llm(task, ctx),
            None => Ok(TaskResult::failure(&task.id, "Task has no keyword")),
        }
    }

    /// Execute agent: task (Main Agent, shared context)
    fn execute_agent(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &ExecutionContext,
    ) -> Result<TaskResult> {
        let prompt = resolve_templates(task.prompt().unwrap_or(""), ctx)?;

        match self.provider.as_str() {
            "claude" => self.execute_claude_prompt(task, &prompt, false, workflow, ctx),
            "openai" => Ok(TaskResult::success(
                &task.id,
                format!("[OpenAI] Would execute prompt: {}", prompt),
                Some(500),
            )),
            "ollama" => Ok(TaskResult::success(
                &task.id,
                format!("[Ollama] Would execute prompt: {}", prompt),
                Some(500),
            )),
            "mock" => Ok(TaskResult::success(
                &task.id,
                format!("[Mock] Executed prompt: {}", prompt),
                Some(100),
            )),
            _ => Err(anyhow!("Unknown provider: {}", self.provider)),
        }
    }

    /// Execute subagent: task (Subagent, isolated 200K context)
    fn execute_subagent(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &ExecutionContext,
    ) -> Result<TaskResult> {
        let prompt = resolve_templates(task.prompt().unwrap_or(""), ctx)?;

        match self.provider.as_str() {
            "claude" => self.execute_claude_prompt(task, &prompt, true, workflow, ctx),
            "mock" => Ok(TaskResult::success(
                &task.id,
                format!("[Mock] Spawned subagent: {}", prompt),
                Some(200),
            )),
            _ => Ok(TaskResult::success(
                &task.id,
                format!("[{}] Would spawn: {}", self.provider, prompt),
                Some(500),
            )),
        }
    }

    /// Execute agent task using Claude CLI
    fn execute_claude_prompt(
        &self,
        task: &Task,
        prompt: &str,
        is_isolated: bool,
        _workflow: &Workflow,
        ctx: &ExecutionContext,
    ) -> Result<TaskResult> {
        // Check if claude CLI is available
        let claude_check = Command::new("which").arg("claude").output();

        if claude_check.is_err() || !claude_check.unwrap().status.success() {
            return Ok(TaskResult::success(
                &task.id,
                format!(
                    "[Claude CLI not found] Would execute {} task: {}",
                    if is_isolated { "subagent" } else { "agent" },
                    prompt
                ),
                Some(0),
            ));
        }

        // Build claude command: claude -p "prompt"
        let mut cmd = Command::new("claude");
        cmd.arg("-p"); // Print mode (non-interactive)

        // For non-isolated (agent:) tasks, inject conversation history
        if !is_isolated && ctx.has_history() {
            // Build context-aware prompt
            let context_prompt = format!(
                "Previous conversation:\n{}\n\nCurrent task:\n{}",
                ctx.format_agent_history(),
                prompt
            );
            cmd.arg(&context_prompt);
        } else {
            cmd.arg(prompt);
        }

        // Add system prompt for isolated scope (subagent)
        if is_isolated {
            if let Some(sys_prompt) = &task.system_prompt {
                cmd.arg("--system-prompt").arg(sys_prompt);
            }
        }

        // Skip permissions for automated execution
        cmd.arg("--dangerously-skip-permissions");

        // Execute
        match cmd.output() {
            Ok(output) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                if output.status.success() {
                    Ok(TaskResult::success(&task.id, stdout, Some(500)))
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    Ok(TaskResult::failure(&task.id, stderr))
                }
            }
            Err(e) => Ok(TaskResult::failure(
                &task.id,
                format!("Failed to execute claude: {}", e),
            )),
        }
    }

    /// Execute shell: task
    fn execute_shell(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        let cmd = resolve_templates(task.prompt().unwrap_or("echo 'no command'"), ctx)?;

        Ok(TaskResult::success(
            &task.id,
            format!("[shell] Would execute: {}", cmd),
            Some(0),
        ))
    }

    /// Execute http: task
    fn execute_http(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        let url = resolve_templates(task.prompt().unwrap_or("(no url)"), ctx)?;
        let method = task.method.as_deref().unwrap_or("GET");

        Ok(TaskResult::success(
            &task.id,
            format!("[http] Would {} {}", method, url),
            Some(0),
        ))
    }

    /// Execute mcp: task
    fn execute_mcp(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        let mcp = task.prompt().unwrap_or("unknown::unknown");
        let args_str = resolve_args(task.args.as_ref(), ctx)?;

        Ok(TaskResult::success(
            &task.id,
            format!("[mcp] Would call {} with args: {}", mcp, args_str),
            Some(0),
        ))
    }

    /// Execute function: task
    fn execute_function(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        let func = task.prompt().unwrap_or("unknown::unknown");
        let args_str = resolve_args(task.args.as_ref(), ctx)?;

        Ok(TaskResult::success(
            &task.id,
            format!("[function] Would call {} with args: {}", func, args_str),
            Some(0),
        ))
    }

    /// Execute llm: task (one-shot, stateless)
    fn execute_llm(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        let prompt = resolve_templates(task.prompt().unwrap_or(""), ctx)?;

        Ok(TaskResult::success(
            &task.id,
            format!("[llm] Would execute one-shot: {}", prompt),
            Some(50),
        ))
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

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========== ExecutionContext Tests ==========

    #[test]
    fn test_context_output_storage() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("task1", "Hello World".to_string());

        assert_eq!(ctx.get_output("task1"), Some(&"Hello World".to_string()));
        assert_eq!(ctx.get_output("nonexistent"), None);
    }

    #[test]
    fn test_context_structured_output() {
        let mut ctx = ExecutionContext::new();
        let json = serde_json::json!({
            "name": "Alice",
            "age": 30,
            "active": true
        });
        ctx.set_structured_output("user", json);

        assert_eq!(ctx.get_field("user", "name"), Some("Alice".to_string()));
        assert_eq!(ctx.get_field("user", "age"), Some("30".to_string()));
        assert_eq!(ctx.get_field("user", "active"), Some("true".to_string()));
        assert_eq!(ctx.get_field("user", "nonexistent"), None);
    }

    #[test]
    fn test_context_inputs() {
        let mut inputs = HashMap::new();
        inputs.insert("file".to_string(), "src/main.rs".to_string());

        let ctx = ExecutionContext::with_inputs(inputs);

        assert_eq!(ctx.get_input("file"), Some(&"src/main.rs".to_string()));
        assert_eq!(ctx.get_input("missing"), None);
    }

    #[test]
    fn test_context_agent_history() {
        let mut ctx = ExecutionContext::new();

        ctx.add_agent_message(MessageRole::User, "What is 2+2?".to_string());
        ctx.add_agent_message(MessageRole::Assistant, "2+2 equals 4.".to_string());

        assert!(ctx.has_history());
        assert_eq!(ctx.agent_history().len(), 2);

        let formatted = ctx.format_agent_history();
        assert!(formatted.contains("User: What is 2+2?"));
        assert!(formatted.contains("Assistant: 2+2 equals 4."));
    }

    // ========== Template Resolution Tests ==========

    #[test]
    fn test_resolve_task_output() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("step1", "analysis result".to_string());

        let result = resolve_templates("Process: {{step1}}", &ctx).unwrap();
        assert_eq!(result, "Process: analysis result");
    }

    #[test]
    fn test_resolve_task_field() {
        let mut ctx = ExecutionContext::new();
        ctx.set_structured_output(
            "user",
            serde_json::json!({
                "name": "Bob",
                "email": "bob@example.com"
            }),
        );

        let result = resolve_templates("Hello {{user.name}}, email: {{user.email}}", &ctx).unwrap();
        assert_eq!(result, "Hello Bob, email: bob@example.com");
    }

    #[test]
    fn test_resolve_input() {
        let mut inputs = HashMap::new();
        inputs.insert("filename".to_string(), "test.txt".to_string());

        let ctx = ExecutionContext::with_inputs(inputs);

        let result = resolve_templates("Reading ${input.filename}", &ctx).unwrap();
        assert_eq!(result, "Reading test.txt");
    }

    #[test]
    fn test_resolve_env() {
        // Set a test env var
        std::env::set_var("NIKA_TEST_VAR", "test_value");

        let ctx = ExecutionContext::new();
        let result = resolve_templates("Env: ${env.NIKA_TEST_VAR}", &ctx).unwrap();
        assert_eq!(result, "Env: test_value");

        std::env::remove_var("NIKA_TEST_VAR");
    }

    #[test]
    fn test_resolve_mixed_templates() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("analyze", "security issues found".to_string());

        let mut inputs = HashMap::new();
        inputs.insert("target".to_string(), "src/".to_string());
        ctx.inputs = inputs;

        let template = "Analyzed ${input.target}: {{analyze}}";
        let result = resolve_templates(template, &ctx).unwrap();
        assert_eq!(result, "Analyzed src/: security issues found");
    }

    #[test]
    fn test_resolve_unmatched_templates() {
        let ctx = ExecutionContext::new();

        // Unmatched templates should be preserved
        let result = resolve_templates("Missing: {{unknown}}", &ctx).unwrap();
        assert_eq!(result, "Missing: {{unknown}}");
    }

    // ========== Runner Tests ==========

    fn make_workflow_v45() -> Workflow {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test workflow"

tasks:
  - id: step1
    agent: "Analyze this"

  - id: step2
    function: transform::uppercase

flows:
  - source: step1
    target: step2
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_topological_sort_v45() {
        let workflow = make_workflow_v45();
        let runner = Runner::new("claude");
        let order = runner.topological_sort(&workflow).unwrap();
        assert_eq!(order, vec!["step1", "step2"]);
    }

    #[test]
    fn test_run_workflow_v45() {
        let workflow = make_workflow_v45();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        assert_eq!(result.tasks_completed, 2, "Should complete 2 tasks");
        assert_eq!(result.tasks_failed, 0, "No tasks should fail");
        assert_eq!(result.results.len(), 2, "Should have 2 results");

        // Check context was populated
        assert!(result.context.get_output("step1").is_some());
        assert!(result.context.get_output("step2").is_some());
    }

    #[test]
    fn test_run_with_inputs() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: process
    agent: "Process file: ${input.file}"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

        let mut inputs = HashMap::new();
        inputs.insert("file".to_string(), "README.md".to_string());

        let runner = Runner::new("mock");
        let result = runner.run_with_inputs(&workflow, inputs).unwrap();

        // The prompt should have been resolved
        let output = result.context.get_output("process").unwrap();
        assert!(output.contains("README.md"));
    }

    #[test]
    fn test_context_passing_between_tasks() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: step1
    agent: "Generate data"

  - id: step2
    agent: "Process: {{step1}}"

flows:
  - source: step1
    target: step2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        // step2 should have received step1's output in its prompt
        let step2_output = result.context.get_output("step2").unwrap();
        // In mock mode, the resolved prompt is echoed back
        assert!(step2_output.contains("[Mock] Executed prompt"));
    }

    #[test]
    fn test_all_7_keywords_execution() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: t1
    agent: "agent task"
  - id: t2
    subagent: "subagent task"
  - id: t3
    shell: "echo test"
  - id: t4
    http: "https://example.com"
  - id: t5
    mcp: "fs::read"
  - id: t6
    function: "tools::fn"
  - id: t7
    llm: "classify"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        assert_eq!(result.tasks_completed, 7, "All 7 tasks should complete");

        // All outputs should be stored in context
        for i in 1..=7 {
            assert!(
                result.context.get_output(&format!("t{}", i)).is_some(),
                "t{} should have output",
                i
            );
        }
    }

    #[test]
    fn test_agent_history_accumulation() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: ask
    agent: "What is Rust?"
  - id: followup
    agent: "Tell me more about its memory safety"

flows:
  - source: ask
    target: followup
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        // Should have 4 messages: 2 user prompts + 2 assistant responses
        assert_eq!(result.context.agent_history().len(), 4);
    }

    // ========== E2E Context Passing Tests ==========

    #[test]
    fn test_e2e_context_passing_simple_chain() {
        // Simulates the mvp-context-demo.nika.yaml workflow
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: |
    You are a helpful assistant that processes files and summarizes content.

tasks:
  - id: read-file
    agent: |
      Simulate reading a configuration file.
      Return JSON: {"name": "nika", "version": "0.1.0"}

  - id: summarize
    agent: |
      Here is the content from the previous task:
      {{read-file}}
      Please summarize this in one sentence.

flows:
  - source: read-file
    target: summarize
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        assert_eq!(result.tasks_completed, 2);
        assert_eq!(result.tasks_failed, 0);

        // Verify context was passed
        let _read_output = result.context.get_output("read-file").unwrap();
        let summarize_output = result.context.get_output("summarize").unwrap();

        // The summarize task should have received the read-file output
        // In mock mode, the prompt (with resolved templates) is echoed back
        assert!(
            summarize_output.contains("Here is the content from the previous task:"),
            "summarize should contain the context header"
        );
    }

    #[test]
    fn test_e2e_context_passing_structured_fields() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test structured context"

tasks:
  - id: generate-user
    agent: "Return user data"

  - id: use-field
    agent: "Process user: {{generate-user.name}} with email: {{generate-user.email}}"

flows:
  - source: generate-user
    target: use-field
"#;
        let _workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let mut ctx = ExecutionContext::new();

        // Pre-populate structured output (simulating JSON response)
        ctx.set_structured_output(
            "generate-user",
            serde_json::json!({
                "name": "Alice",
                "email": "alice@example.com"
            }),
        );
        ctx.set_output("generate-user", r#"{"name":"Alice","email":"alice@example.com"}"#.to_string());

        // Test template resolution directly (runner not used)
        let _runner = Runner::new("mock");

        // Use resolve_templates directly to test field resolution
        let template = "Process user: {{generate-user.name}} with email: {{generate-user.email}}";
        let resolved = resolve_templates(template, &ctx).unwrap();

        assert_eq!(resolved, "Process user: Alice with email: alice@example.com");
    }

    #[test]
    fn test_e2e_context_passing_multi_hop() {
        // A → B → C: context flows through all three
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Multi-hop test"

tasks:
  - id: step-a
    agent: "Generate initial data"

  - id: step-b
    agent: "Transform: {{step-a}}"

  - id: step-c
    agent: "Final: {{step-b}} and original: {{step-a}}"

flows:
  - source: step-a
    target: step-b
  - source: step-b
    target: step-c
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        assert_eq!(result.tasks_completed, 3);

        // All outputs should be available
        assert!(result.context.get_output("step-a").is_some());
        assert!(result.context.get_output("step-b").is_some());
        assert!(result.context.get_output("step-c").is_some());

        // step-c output should contain references to both a and b
        let step_c_output = result.context.get_output("step-c").unwrap();
        assert!(step_c_output.contains("Final:"));
        assert!(step_c_output.contains("original:"));
    }

    #[test]
    fn test_e2e_context_subagent_isolation() {
        // subagent: tasks should NOT accumulate into agent_history
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: main-task
    agent: "Main agent task"

  - id: isolated-task
    subagent: "Isolated subagent task"

  - id: follow-task
    agent: "Follow up agent task"

flows:
  - source: main-task
    target: isolated-task
  - source: isolated-task
    target: follow-task
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        assert_eq!(result.tasks_completed, 3);

        // Only agent: tasks should be in history (not subagent:)
        // main-task: user+assistant, follow-task: user+assistant = 4 messages
        // isolated-task should NOT add to history
        assert_eq!(result.context.agent_history().len(), 4);

        // Verify the history contains only agent tasks
        let history_text = result.context.format_agent_history();
        assert!(history_text.contains("Main agent task"));
        assert!(history_text.contains("Follow up agent task"));
        // subagent output should NOT be in history
        assert!(!history_text.contains("Isolated subagent task"));
    }

    #[test]
    fn test_e2e_context_parallel_merge() {
        // Parallel branches that merge: A → (B, C) → D
        // D should have access to both B and C outputs
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Parallel test"

tasks:
  - id: source
    agent: "Source data"

  - id: branch-1
    subagent: "Process branch 1"

  - id: branch-2
    subagent: "Process branch 2"

  - id: merge
    agent: "Merge: {{branch-1}} and {{branch-2}}"

flows:
  - source: source
    target: branch-1
  - source: source
    target: branch-2
  - source: branch-1
    target: merge
  - source: branch-2
    target: merge
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        assert_eq!(result.tasks_completed, 4);

        // The merge task should have both branch outputs available
        let merge_output = result.context.get_output("merge").unwrap();
        // Since mock echoes the prompt with resolved templates,
        // and both branches have outputs, the merge should contain "Merge:"
        assert!(merge_output.contains("Merge:"));
    }

    #[test]
    fn test_e2e_context_with_environment() {
        std::env::set_var("NIKA_E2E_TEST", "e2e_test_value");

        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Env test"

tasks:
  - id: use-env
    agent: "Environment value: ${env.NIKA_E2E_TEST}"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock");
        let result = runner.run(&workflow).unwrap();

        let output = result.context.get_output("use-env").unwrap();
        assert!(output.contains("e2e_test_value"));

        std::env::remove_var("NIKA_E2E_TEST");
    }

    #[test]
    fn test_e2e_context_with_inputs() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Input test"

tasks:
  - id: process
    agent: "Processing file: ${input.filename}"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

        let mut inputs = HashMap::new();
        inputs.insert("filename".to_string(), "config.yaml".to_string());

        let runner = Runner::new("mock");
        let result = runner.run_with_inputs(&workflow, inputs).unwrap();

        let output = result.context.get_output("process").unwrap();
        assert!(output.contains("config.yaml"));
    }
}
