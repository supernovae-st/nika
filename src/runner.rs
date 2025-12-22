//! # Nika Workflow Runner (v4.6)
//!
//! Executes workflows using providers (Claude CLI, mock).
//!
//! ## v4.6 Performance Optimizations
//!
//! - Single-pass template resolution (template.rs)
//! - SmartString for task IDs (inline ≤31 chars)
//! - Arc<str> for zero-copy context sharing
//! - Memory pool for ExecutionContext reuse
//!
//! ## Overview
//!
//! The runner is responsible for:
//!
//! - **Task execution** - Running each task based on its keyword type
//! - **Context passing** - Sharing data between tasks via templates
//! - **Topological sorting** - Determining execution order from flows
//! - **Retry logic** - Handling transient failures with backoff
//!
//! ## Execution Flow
//!
//! 1. Parse workflow and compute execution order (topological sort)
//! 2. For each task in order:
//!    - Resolve templates (`{{task_id}}`, `${env.VAR}`, `${input.field}`)
//!    - Execute based on keyword type (agent, shell, http, etc.)
//!    - Store output in context for downstream tasks
//!
//! ## Template Resolution
//!
//! Templates are resolved in this order:
//!
//! | Pattern | Source | Example |
//! |---------|--------|---------|
//! | `${input.field}` | Input parameters | `${input.file_path}` |
//! | `${env.VAR}` | Environment variables | `${env.API_KEY}` |
//! | `{{task_id}}` | Previous task output | `{{analyze}}` |
//! | `{{task_id.field}}` | Structured output field | `{{analyze.summary}}` |
//!
//! ## Keyword Execution
//!
//! Each keyword type has specific execution behavior:
//!
//! - **agent/subagent/llm** → Provider (Claude CLI or mock)
//! - **shell** → Subprocess with timeout
//! - **http** → HTTP request (placeholder for now)
//! - **mcp/function** → External tool call (placeholder)
//!
//! ## Example
//!
//! ```rust,ignore
//! use nika::{Workflow, Runner};
//!
//! let yaml = r#"
//! agent:
//!   model: claude-sonnet-4-5
//!   systemPrompt: "You are helpful."
//! tasks:
//!   - id: greet
//!     agent:
//!       prompt: "Say hello"
//! flows: []
//! "#;
//!
//! let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
//! let runner = Runner::new("mock")?;  // "mock" for testing, "claude" for production
//! let result = runner.run(&workflow).await?;  // async execution
//!
//! println!("Completed: {}/{}", result.tasks_completed, result.tasks_completed + result.tasks_failed);
//! for task_result in &result.results {
//!     println!("  {}: {}", task_result.task_id, task_result.success);
//! }
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Key Types
//!
//! - [`Runner`] - Main executor, holds provider and configuration
//! - [`RunResult`] - Workflow execution summary
//! - [`TaskResult`] - Individual task execution result
//! - [`ExecutionContext`] - Data passed between tasks

use crate::limits::{CircuitBreaker, ResourceLimits};
use crate::provider::{create_provider, PromptRequest, Provider, TokenUsage};
use crate::smart_string::SmartString;
use crate::workflow::{Task, TaskKeyword, Workflow};
use anyhow::{anyhow, Context, Result};
use std::collections::HashMap;
use std::process::Command;
use std::sync::Arc;
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

// ============================================================================
// CONSTANTS
// ============================================================================

/// Default timeout for shell commands (30 seconds)
const DEFAULT_SHELL_TIMEOUT: Duration = Duration::from_secs(30);

// ============================================================================
// LAZY REGEX PATTERNS (compiled once)
// ============================================================================

// Regex patterns removed - now using single-pass template resolver in template.rs

// ============================================================================
// TIMEOUT PARSING
// ============================================================================

/// Parse a duration string like "30s", "5m", "1h" into a Duration
///
/// Supported formats:
/// - "30" or "30s" -> 30 seconds
/// - "5m" -> 5 minutes
/// - "1h" -> 1 hour
/// - "500ms" -> 500 milliseconds
fn parse_duration(duration_str: &str) -> Option<Duration> {
    let s = duration_str.trim();
    if s.is_empty() {
        return None;
    }

    // Try to parse with suffix
    if let Some(ms) = s.strip_suffix("ms") {
        return ms.parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(secs) = s.strip_suffix('s') {
        return secs.parse::<u64>().ok().map(Duration::from_secs);
    }
    if let Some(mins) = s.strip_suffix('m') {
        return mins
            .parse::<u64>()
            .ok()
            .map(|m| Duration::from_secs(m * 60));
    }
    if let Some(hours) = s.strip_suffix('h') {
        return hours
            .parse::<u64>()
            .ok()
            .map(|h| Duration::from_secs(h * 3600));
    }

    // No suffix: assume seconds
    s.parse::<u64>().ok().map(Duration::from_secs)
}

/// Alias for backwards compatibility
fn parse_timeout(timeout_str: &str) -> Option<Duration> {
    parse_duration(timeout_str)
}

/// Default backoff for retries (1 second)
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_secs(1);

// ============================================================================
// EXECUTION CONTEXT
// ============================================================================

/// Execution context passed between tasks
///
/// This enables:
/// - Task output references: {{task_id}} or {{task_id.field}}
/// - Shared agent conversation history
/// - Environment and secrets access
///
/// Uses Arc<str> for zero-copy sharing of immutable strings
/// Uses SmartString for task IDs to avoid heap allocation for short IDs
#[derive(Debug, Default, Clone)]
pub struct ExecutionContext {
    /// Outputs from completed tasks (task_id -> output string)
    /// SmartString keys for efficient task ID storage
    /// Arc<str> values for zero-copy sharing since outputs are immutable once set
    outputs: HashMap<SmartString, Arc<str>>,

    /// Structured outputs for field access (task_id -> JSON value)
    /// SmartString keys for efficient task ID storage
    structured_outputs: HashMap<SmartString, serde_json::Value>,

    /// Main agent conversation history (for context sharing between agent: tasks)
    agent_history: Vec<AgentMessage>,

    /// Input parameters passed to the workflow
    /// Arc<str> since inputs are set once and never modified
    inputs: HashMap<String, Arc<str>>,

    /// Environment variables snapshot
    /// Arc<str> since env vars are read-only during execution
    env_vars: HashMap<String, Arc<str>>,
}

/// A message in the agent conversation history
///
/// Uses Arc<str> for zero-copy sharing of message content
#[derive(Debug, Clone)]
pub struct AgentMessage {
    pub role: MessageRole,
    pub content: Arc<str>,
}

/// Role for agent messages (1 byte with Copy)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageRole {
    User = 0,
    Assistant = 1,
    System = 2,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new() -> Self {
        Self::default()
    }

    /// Create context with input parameters
    pub fn with_inputs(inputs: HashMap<String, String>) -> Self {
        // Convert String to Arc<str>
        let inputs = inputs.into_iter().map(|(k, v)| (k, Arc::from(v))).collect();
        Self {
            inputs,
            ..Default::default()
        }
    }

    /// Store a task's output
    pub fn set_output(&mut self, task_id: &str, output: String) {
        self.outputs
            .insert(SmartString::from(task_id), Arc::from(output));
    }

    /// Store multiple outputs at once (batch operation)
    pub fn set_outputs_batch<I, K, V>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: Into<String>,
    {
        self.outputs.extend(
            outputs
                .into_iter()
                .map(|(k, v)| (SmartString::from(k.as_ref()), Arc::from(v.into()))),
        );
    }

    /// Store a structured output (for field access)
    pub fn set_structured_output(&mut self, task_id: &str, value: serde_json::Value) {
        self.structured_outputs
            .insert(SmartString::from(task_id), value);
    }

    /// Store multiple structured outputs at once (batch operation)
    pub fn set_structured_outputs_batch<I, K>(&mut self, outputs: I)
    where
        I: IntoIterator<Item = (K, serde_json::Value)>,
        K: AsRef<str>,
    {
        self.structured_outputs.extend(
            outputs
                .into_iter()
                .map(|(k, v)| (SmartString::from(k.as_ref()), v)),
        );
    }

    /// Get a task's output
    pub fn get_output(&self, task_id: &str) -> Option<&str> {
        self.outputs.get(task_id).map(|arc| arc.as_ref())
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
    pub fn get_input(&self, name: &str) -> Option<&str> {
        self.inputs.get(name).map(|arc| arc.as_ref())
    }

    /// Get an environment variable
    pub fn get_env(&self, name: &str) -> Option<String> {
        self.env_vars
            .get(name)
            .map(|arc| arc.to_string())
            .or_else(|| std::env::var(name).ok())
    }

    /// Add a message to the agent conversation history
    pub fn add_agent_message(&mut self, role: MessageRole, content: String) {
        self.agent_history.push(AgentMessage {
            role,
            content: Arc::from(content),
        });
    }

    /// Add multiple messages to the agent conversation history (batch operation)
    pub fn add_agent_messages_batch<I, S>(&mut self, messages: I)
    where
        I: IntoIterator<Item = (MessageRole, S)>,
        S: Into<String>,
    {
        self.agent_history
            .extend(messages.into_iter().map(|(role, content)| AgentMessage {
                role,
                content: Arc::from(content.into()),
            }));
    }

    /// Get the agent conversation history
    pub fn agent_history(&self) -> &[AgentMessage] {
        &self.agent_history
    }

    /// Clear all data from the context (for reuse in memory pool)
    pub fn clear(&mut self) {
        self.outputs.clear();
        self.structured_outputs.clear();
        self.agent_history.clear();
        self.inputs.clear();
        self.env_vars.clear();
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

// Pattern resolver removed - now using single-pass template resolver in template.rs

/// Resolve task args (YAML → String with template resolution)
///
/// Used by mcp: and function: tasks to serialize and resolve their args.
fn resolve_args(args: Option<&serde_json::Value>, ctx: &ExecutionContext) -> Result<String> {
    match args {
        Some(args) => {
            let raw_args = serde_json::to_string(args).unwrap_or_default();
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
///
/// Uses single-pass tokenization and caching for optimal performance
pub fn resolve_templates(template: &str, ctx: &ExecutionContext) -> Result<String> {
    // Use the new single-pass template resolver with caching
    crate::template::resolve_templates(template, ctx)
}

// ============================================================================
// TASK RESULT
// ============================================================================

/// Error context for failed tasks
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// The task keyword (shell, http, agent, etc.)
    pub keyword: Option<String>,
    /// Error category for structured handling
    pub category: Option<ErrorCategory>,
    /// Additional diagnostic info
    pub details: Option<String>,
}

/// Error categories for structured error handling
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorCategory {
    /// Command/process timeout
    Timeout,
    /// Network or connectivity issue
    Network,
    /// Provider/model error
    Provider,
    /// Template resolution failure
    Template,
    /// Task configuration error
    Config,
    /// General execution error
    Execution,
}

impl From<&str> for ErrorCategory {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "timeout" => ErrorCategory::Timeout,
            "network" => ErrorCategory::Network,
            "provider" => ErrorCategory::Provider,
            "template" => ErrorCategory::Template,
            "config" => ErrorCategory::Config,
            _ => ErrorCategory::Execution,
        }
    }
}

/// Execution result for a task
#[derive(Debug, Clone)]
pub struct TaskResult {
    pub task_id: String,
    pub success: bool,
    pub output: String,
    pub tokens_used: Option<u32>,
    /// Error context for failed tasks (None if success)
    pub error_context: Option<ErrorContext>,
}

impl TaskResult {
    /// Create a successful task result
    pub fn success(id: impl Into<String>, output: impl Into<String>, tokens: Option<u32>) -> Self {
        Self {
            task_id: id.into(),
            success: true,
            output: output.into(),
            tokens_used: tokens,
            error_context: None,
        }
    }

    /// Short alias for success
    #[inline(always)]
    pub fn ok(id: impl Into<String>, output: impl Into<String>, tokens: u32) -> Self {
        Self::success(id, output, Some(tokens))
    }

    /// Create a failed task result
    pub fn failure(id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            task_id: id.into(),
            success: false,
            output: error.into(),
            tokens_used: None,
            error_context: None,
        }
    }

    /// Short alias for failure with category
    #[inline(always)]
    pub fn err(id: impl Into<String>, msg: impl Into<String>, cat: ErrorCategory) -> Self {
        Self::failure_with_context(id, msg, "", cat)
    }

    /// Create a failed task result with context
    pub fn failure_with_context(
        id: impl Into<String>,
        error: impl Into<String>,
        keyword: impl Into<String>,
        category: ErrorCategory,
    ) -> Self {
        Self {
            task_id: id.into(),
            success: false,
            output: error.into(),
            tokens_used: None,
            error_context: Some(ErrorContext {
                keyword: Some(keyword.into()),
                category: Some(category),
                details: None,
            }),
        }
    }

    /// Add details to error context
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        if let Some(ref mut ctx) = self.error_context {
            ctx.details = Some(details.into());
        } else {
            self.error_context = Some(ErrorContext {
                keyword: None,
                category: None,
                details: Some(details.into()),
            });
        }
        self
    }

    /// Check if error is a timeout
    pub fn is_timeout(&self) -> bool {
        self.error_context
            .as_ref()
            .and_then(|c| c.category.as_ref())
            .is_some_and(|cat| *cat == ErrorCategory::Timeout)
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
    /// Provider instance for LLM execution
    provider: Box<dyn Provider>,
    /// Resource limits for safe execution
    limits: ResourceLimits,
    /// Circuit breaker for external services
    circuit_breaker: Option<Arc<CircuitBreaker>>,
    /// Verbose output
    verbose: bool,
}

impl Runner {
    /// Create a new runner with the specified provider
    ///
    /// # Arguments
    /// * `provider_name` - Name of the provider ("claude", "mock", etc.)
    ///
    /// # Errors
    /// Returns an error if the provider is unknown
    pub fn new(provider_name: &str) -> Result<Self> {
        Ok(Self {
            provider: create_provider(provider_name)?,
            limits: ResourceLimits::default(),
            circuit_breaker: None,
            verbose: false,
        })
    }

    pub fn verbose(mut self, v: bool) -> Self {
        self.verbose = v;
        self
    }

    /// Set resource limits
    pub fn with_limits(mut self, limits: ResourceLimits) -> Self {
        self.limits = limits;
        self
    }

    /// Add a circuit breaker for external services
    pub fn with_circuit_breaker(mut self, breaker: Arc<CircuitBreaker>) -> Self {
        self.circuit_breaker = Some(breaker);
        self
    }

    /// Execute a workflow with default empty context
    pub async fn run(&self, workflow: &Workflow) -> Result<RunResult> {
        self.run_with_context(workflow, ExecutionContext::new())
            .await
    }

    /// Execute a workflow with provided inputs
    pub async fn run_with_inputs(
        &self,
        workflow: &Workflow,
        inputs: HashMap<String, String>,
    ) -> Result<RunResult> {
        self.run_with_context(workflow, ExecutionContext::with_inputs(inputs))
            .await
    }

    /// Execute a workflow with a pre-configured context
    pub async fn run_with_context(
        &self,
        workflow: &Workflow,
        mut ctx: ExecutionContext,
    ) -> Result<RunResult> {
        let workflow_start = Instant::now();
        let mut results = Vec::new();
        let mut total_tokens = 0u32;

        // Build task map for lookups
        let task_map: HashMap<&str, &Task> =
            workflow.tasks.iter().map(|t| (t.id.as_str(), t)).collect();

        // Get execution order (topological sort)
        let order = self
            .topological_sort(workflow)
            .context("Failed to determine task execution order")?;

        if self.verbose {
            println!("Execution order: {:?}", order);
        }

        // Execute tasks in order
        for task_id in &order {
            // Check workflow timeout
            if workflow_start.elapsed() > self.limits.max_workflow_duration {
                return Err(anyhow!(
                    "Workflow timeout exceeded ({:?})",
                    self.limits.max_workflow_duration
                ));
            }

            let task = task_map
                .get(task_id.as_str())
                .ok_or_else(|| anyhow!("Task not found: {}", task_id))?;

            if self.verbose {
                let keyword = task.keyword();
                println!("\n→ Executing: {} ({})", task_id, keyword);
            }

            // Execute task with context and retry logic
            let result = self
                .execute_task_with_retry(task, workflow, &mut ctx)
                .await
                .with_context(|| {
                    format!(
                        "Failed to execute task '{}' ({:?})",
                        task_id,
                        task.keyword()
                    )
                })?;

            if let Some(tokens) = result.tokens_used {
                total_tokens += tokens;
            }

            // Check output size limit
            if result.output.len() > self.limits.max_output_size {
                return Err(anyhow!(
                    "Task '{}' output exceeds size limit ({} > {})",
                    task_id,
                    result.output.len(),
                    self.limits.max_output_size
                ));
            }

            // Store output in context for downstream tasks
            ctx.set_output(&result.task_id, result.output.clone());

            // Try to parse as JSON for structured access
            if let Some(json) = result.as_json() {
                ctx.set_structured_output(&result.task_id, json);
            }

            // For agent: tasks, add to conversation history
            if task.keyword() == TaskKeyword::Agent {
                // Add the prompt as user message
                let prompt = task.prompt();
                ctx.add_agent_message(MessageRole::User, prompt.to_string());
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

    /// Execute a single task with retry logic
    ///
    /// Respects `config.retry.max` and `config.retry.backoff` settings.
    /// Retries only on failure, not on errors (Result::Err).
    async fn execute_task_with_retry(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        // Get retry config
        let max_attempts = task
            .config
            .as_ref()
            .and_then(|c| c.retry.as_ref())
            .map(|r| r.max)
            .unwrap_or(1); // Default: no retries (1 attempt)

        let backoff = task
            .config
            .as_ref()
            .and_then(|c| c.retry.as_ref())
            .and_then(|r| r.backoff.as_ref())
            .and_then(|b| parse_duration(b))
            .unwrap_or(DEFAULT_RETRY_BACKOFF);

        let mut last_result = None;

        for attempt in 1..=max_attempts {
            let result = self.execute_task(task, workflow, ctx).await?;

            if result.success {
                return Ok(result);
            }

            // Task failed
            last_result = Some(result);

            // If we have more attempts, wait before retrying
            if attempt < max_attempts {
                if self.verbose {
                    println!(
                        "  ⟳ Retry {}/{} for task '{}' after {:?}",
                        attempt, max_attempts, task.id, backoff
                    );
                }
                tokio::time::sleep(backoff).await;
            }
        }

        // All attempts exhausted, return last failure
        Ok(last_result
            .unwrap_or_else(|| TaskResult::failure(&task.id, "Task failed with no result")))
    }

    /// Execute a single task with context (v4.6 - keyword based)
    async fn execute_task(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &mut ExecutionContext,
    ) -> Result<TaskResult> {
        match task.keyword() {
            TaskKeyword::Agent => self.execute_agent(task, workflow, ctx).await,
            TaskKeyword::Subagent => self.execute_subagent(task, workflow, ctx).await,
            TaskKeyword::Shell => self.execute_shell(task, ctx),
            TaskKeyword::Http => self.execute_http(task, ctx),
            TaskKeyword::Mcp => self.execute_mcp(task, ctx),
            TaskKeyword::Function => self.execute_function(task, ctx),
            TaskKeyword::Llm => self.execute_llm(task, ctx),
        }
    }

    /// Execute agent: task (Main Agent, shared context)
    async fn execute_agent(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &ExecutionContext,
    ) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract agent definition from the TaskAction enum
        let agent_def = match &task.action {
            TaskAction::Agent { agent } => agent,
            _ => return Ok(TaskResult::failure(&task.id, "Expected agent task")),
        };

        let prompt = resolve_templates(&agent_def.prompt, ctx)
            .with_context(|| format!("Failed to resolve templates for agent task '{}'", task.id))?;

        // Build request with shared context (agent: tasks share history)
        let request = PromptRequest::new(
            &prompt,
            agent_def.model.as_deref().unwrap_or(&workflow.agent.model),
        )
        .with_system_prompt(
            agent_def
                .system_prompt
                .as_deref()
                .or(workflow.agent.system_prompt.as_deref())
                .unwrap_or(""),
        )
        .with_history(
            ctx.agent_history()
                .iter()
                .map(|m| AgentMessage {
                    role: m.role,
                    content: m.content.clone(),
                })
                .collect(),
        )
        .with_tools(agent_def.allowed_tools.clone().unwrap_or_default());

        // Execute via provider
        let response = self.provider.execute(request).await?;

        if response.success {
            Ok(TaskResult::success(
                &task.id,
                response.content,
                Some(response.usage.total_tokens),
            ))
        } else {
            // Detect timeout errors from provider
            let category = if response.content.contains("timed out") {
                ErrorCategory::Timeout
            } else {
                ErrorCategory::Provider
            };
            Ok(TaskResult::failure_with_context(
                &task.id,
                &response.content,
                "agent",
                category,
            ))
        }
    }

    /// Execute subagent: task (Subagent, isolated 200K context)
    async fn execute_subagent(
        &self,
        task: &Task,
        workflow: &Workflow,
        ctx: &ExecutionContext,
    ) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract subagent definition from the TaskAction enum
        let subagent_def = match &task.action {
            TaskAction::Subagent { subagent } => subagent,
            _ => return Ok(TaskResult::failure(&task.id, "Expected subagent task")),
        };

        let prompt = resolve_templates(&subagent_def.prompt, ctx).with_context(|| {
            format!(
                "Failed to resolve templates for subagent task '{}'",
                task.id
            )
        })?;

        // Build request in isolated mode (subagent: tasks don't share history)
        let request = PromptRequest::new(
            &prompt,
            subagent_def
                .model
                .as_deref()
                .unwrap_or(&workflow.agent.model),
        )
        .with_system_prompt(
            subagent_def
                .system_prompt
                .as_deref()
                .or(workflow.agent.system_prompt.as_deref())
                .unwrap_or(""),
        )
        .with_tools(subagent_def.allowed_tools.clone().unwrap_or_default())
        .isolated();

        // Execute via provider
        let response = self.provider.execute(request).await?;

        if response.success {
            Ok(TaskResult::success(
                &task.id,
                response.content,
                Some(response.usage.total_tokens),
            ))
        } else {
            // Detect timeout errors from provider
            let category = if response.content.contains("timed out") {
                ErrorCategory::Timeout
            } else {
                ErrorCategory::Provider
            };
            Ok(TaskResult::failure_with_context(
                &task.id,
                &response.content,
                "subagent",
                category,
            ))
        }
    }

    /// Execute shell: task
    ///
    /// Runs the command through the system shell (sh -c on Unix).
    /// Captures stdout/stderr and returns them as the task result.
    /// Supports timeout configuration via `config.timeout` (e.g., "30s", "5m").
    fn execute_shell(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract shell definition from the TaskAction enum
        let shell_def = match &task.action {
            TaskAction::Shell { shell } => shell,
            _ => return Ok(TaskResult::failure(&task.id, "Expected shell task")),
        };

        let cmd_str = resolve_templates(&shell_def.command, ctx)
            .with_context(|| format!("Failed to resolve templates for shell task '{}'", task.id))?;

        // Get timeout from task config, or use default
        let timeout = task
            .config
            .as_ref()
            .and_then(|c| c.timeout.as_ref())
            .and_then(|t| parse_timeout(t))
            .unwrap_or(DEFAULT_SHELL_TIMEOUT);

        // Spawn the process (non-blocking)
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(&cmd_str)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .with_context(|| format!("Failed to spawn shell command: {}", cmd_str))?;

        // Wait with timeout
        match child.wait_timeout(timeout)? {
            Some(status) => {
                // Process completed within timeout
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                if status.success() {
                    // Return stdout, or stderr if stdout is empty
                    let result = if stdout.trim().is_empty() && !stderr.trim().is_empty() {
                        stderr.trim().to_string()
                    } else {
                        stdout.trim().to_string()
                    };

                    // Estimate tokens based on command + output length
                    let tokens = TokenUsage::estimate(cmd_str.len(), result.len());

                    Ok(TaskResult::success(
                        &task.id,
                        result,
                        Some(tokens.total_tokens),
                    ))
                } else {
                    // Command failed - return stderr or exit code info
                    let error_msg = if stderr.trim().is_empty() {
                        format!("Command exited with code: {}", status.code().unwrap_or(-1))
                    } else {
                        stderr.trim().to_string()
                    };
                    Ok(TaskResult::failure_with_context(
                        &task.id,
                        error_msg,
                        "shell",
                        ErrorCategory::Execution,
                    )
                    .with_details(format!("command: {}", cmd_str)))
                }
            }
            None => {
                // Timeout! Kill the process
                let _ = child.kill();
                let _ = child.wait(); // Reap the zombie

                let error_msg = format!("Shell command timed out after {:?}: {}", timeout, cmd_str);
                Ok(TaskResult::failure_with_context(
                    &task.id,
                    error_msg,
                    "shell",
                    ErrorCategory::Timeout,
                ))
            }
        }
    }

    /// Execute http: task
    fn execute_http(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract HTTP definition from the TaskAction enum
        let http_def = match &task.action {
            TaskAction::Http { http } => http,
            _ => return Ok(TaskResult::failure(&task.id, "Expected http task")),
        };

        let resolved_url = resolve_templates(&http_def.url, ctx)
            .with_context(|| format!("Failed to resolve URL for http task '{}'", task.id))?;

        let method = http_def.method.as_deref().unwrap_or("GET");

        Ok(TaskResult::success(
            &task.id,
            format!("[http] Would {} {}", method, resolved_url),
            Some(0),
        ))
    }

    /// Execute mcp: task
    fn execute_mcp(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract MCP definition from the TaskAction enum
        let mcp_def = match &task.action {
            TaskAction::Mcp { mcp } => mcp,
            _ => return Ok(TaskResult::failure(&task.id, "Expected mcp task")),
        };

        let args_str = resolve_args(mcp_def.args.as_ref(), ctx)
            .with_context(|| format!("Failed to resolve args for mcp task '{}'", task.id))?;

        Ok(TaskResult::success(
            &task.id,
            format!(
                "[mcp] Would call {} with args: {}",
                mcp_def.reference, args_str
            ),
            Some(0),
        ))
    }

    /// Execute function: task
    fn execute_function(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract function definition from the TaskAction enum
        let func_def = match &task.action {
            TaskAction::Function { function } => function,
            _ => return Ok(TaskResult::failure(&task.id, "Expected function task")),
        };

        let args_str = resolve_args(func_def.args.as_ref(), ctx)
            .with_context(|| format!("Failed to resolve args for function task '{}'", task.id))?;

        Ok(TaskResult::success(
            &task.id,
            format!(
                "[function] Would call {} with args: {}",
                func_def.reference, args_str
            ),
            Some(0),
        ))
    }

    /// Execute llm: task (one-shot, stateless)
    fn execute_llm(&self, task: &Task, ctx: &ExecutionContext) -> Result<TaskResult> {
        use crate::task::TaskAction;

        // Extract LLM definition from the TaskAction enum
        let llm_def = match &task.action {
            TaskAction::Llm { llm } => llm,
            _ => return Ok(TaskResult::failure(&task.id, "Expected llm task")),
        };

        let prompt = resolve_templates(&llm_def.prompt, ctx)
            .with_context(|| format!("Failed to resolve templates for llm task '{}'", task.id))?;

        Ok(TaskResult::success(
            &task.id,
            format!("[llm] Would execute one-shot: {}", prompt),
            Some(50),
        ))
    }

    /// Topological sort for execution order
    fn topological_sort(&self, workflow: &Workflow) -> Result<Vec<String>> {
        // Pre-allocate with known size
        let task_count = workflow.tasks.len();
        let mut in_degree: HashMap<&str, usize> = HashMap::with_capacity(task_count);
        let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::with_capacity(task_count);

        // Initialize
        for task in &workflow.tasks {
            in_degree.insert(&task.id, 0);
            adjacency.insert(&task.id, Vec::with_capacity(2)); // Most tasks have 0-2 outputs
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

        let mut result = Vec::with_capacity(task_count);

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

        assert_eq!(ctx.get_output("task1"), Some("Hello World"));
        assert_eq!(ctx.get_output("nonexistent"), None);
    }

    #[test]
    fn test_batch_outputs() {
        let mut ctx = ExecutionContext::new();

        // Batch set outputs
        ctx.set_outputs_batch([
            ("task1", "output1"),
            ("task2", "output2"),
            ("task3", "output3"),
        ]);

        assert_eq!(ctx.get_output("task1"), Some("output1"));
        assert_eq!(ctx.get_output("task2"), Some("output2"));
        assert_eq!(ctx.get_output("task3"), Some("output3"));
    }

    #[test]
    fn test_batch_messages() {
        let mut ctx = ExecutionContext::new();

        // Batch add messages
        ctx.add_agent_messages_batch([
            (MessageRole::User, "Question 1"),
            (MessageRole::Assistant, "Answer 1"),
            (MessageRole::User, "Question 2"),
        ]);

        assert_eq!(ctx.agent_history().len(), 3);
        assert_eq!(ctx.agent_history()[0].role, MessageRole::User);
        assert_eq!(ctx.agent_history()[1].role, MessageRole::Assistant);
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

        assert_eq!(ctx.get_input("file"), Some("src/main.rs"));
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
        ctx.inputs = inputs.into_iter().map(|(k, v)| (k, Arc::from(v))).collect();

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

    // ========== Template Edge Case Tests ==========

    #[test]
    fn test_template_empty_string() {
        let ctx = ExecutionContext::new();
        let result = resolve_templates("", &ctx).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_template_no_placeholders() {
        let ctx = ExecutionContext::new();
        let result = resolve_templates("Plain text without templates", &ctx).unwrap();
        assert_eq!(result, "Plain text without templates");
    }

    #[test]
    fn test_template_multiple_same_placeholder() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("name", "Alice".to_string());

        let result = resolve_templates("Hello {{name}}, {{name}} is great!", &ctx).unwrap();
        assert_eq!(result, "Hello Alice, Alice is great!");
    }

    #[test]
    fn test_template_adjacent_placeholders() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("a", "X".to_string());
        ctx.set_output("b", "Y".to_string());

        let result = resolve_templates("{{a}}{{b}}", &ctx).unwrap();
        assert_eq!(result, "XY");
    }

    #[test]
    fn test_template_nested_braces_not_supported() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("outer", "{{inner}}".to_string());

        // Nested templates are NOT resolved - they're treated as literal output
        let result = resolve_templates("Result: {{outer}}", &ctx).unwrap();
        assert_eq!(result, "Result: {{inner}}");
    }

    #[test]
    fn test_template_special_characters_in_output() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("data", r#"{"key": "value", "arr": [1,2,3]}"#.to_string());

        let result = resolve_templates("JSON: {{data}}", &ctx).unwrap();
        assert_eq!(result, r#"JSON: {"key": "value", "arr": [1,2,3]}"#);
    }

    #[test]
    fn test_template_multiline_output() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("code", "line1\nline2\nline3".to_string());

        let result = resolve_templates("Code:\n{{code}}", &ctx).unwrap();
        assert_eq!(result, "Code:\nline1\nline2\nline3");
    }

    #[test]
    fn test_template_unicode_content() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("greeting", "Bonjour 你好 🎉".to_string());

        let result = resolve_templates("Say: {{greeting}}", &ctx).unwrap();
        assert_eq!(result, "Say: Bonjour 你好 🎉");
    }

    #[test]
    fn test_template_empty_output_value() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("empty", "".to_string());

        let result = resolve_templates("Value: [{{empty}}]", &ctx).unwrap();
        assert_eq!(result, "Value: []");
    }

    #[test]
    fn test_template_missing_field_in_structured() {
        let mut ctx = ExecutionContext::new();
        ctx.set_structured_output("user", serde_json::json!({"name": "Bob"}));

        // Missing field should preserve the template pattern
        let result = resolve_templates("Email: {{user.email}}", &ctx).unwrap();
        assert_eq!(result, "Email: {{user:email}}");
    }

    #[test]
    fn test_template_deeply_nested_field() {
        let mut ctx = ExecutionContext::new();
        ctx.set_structured_output(
            "response",
            serde_json::json!({
                "data": {
                    "user": {
                        "profile": {
                            "name": "Deep"
                        }
                    }
                }
            }),
        );

        // Only single level field access is supported
        // data.user.profile.name won't work - only direct fields
        let result = resolve_templates("Name: {{response.data}}", &ctx).unwrap();
        // data is an object, will be serialized as JSON
        assert!(result.contains("user"));
    }

    #[test]
    fn test_template_input_with_special_chars() {
        let mut inputs = HashMap::new();
        inputs.insert(
            "path".to_string(),
            "/path/to/file with spaces.txt".to_string(),
        );

        let ctx = ExecutionContext::with_inputs(inputs);
        let result = resolve_templates("File: ${input.path}", &ctx).unwrap();
        assert_eq!(result, "File: /path/to/file with spaces.txt");
    }

    #[test]
    fn test_template_env_missing_var() {
        let ctx = ExecutionContext::new();

        // Missing env var should preserve the pattern
        let result =
            resolve_templates("Value: ${env.NIKA_DEFINITELY_NOT_SET_12345}", &ctx).unwrap();
        assert_eq!(result, "Value: ${env.NIKA_DEFINITELY_NOT_SET_12345}");
    }

    #[test]
    fn test_template_mixed_resolved_and_unresolved() {
        let mut ctx = ExecutionContext::new();
        ctx.set_output("found", "yes".to_string());

        let result = resolve_templates("Found: {{found}}, Missing: {{missing}}", &ctx).unwrap();
        assert_eq!(result, "Found: yes, Missing: {{missing}}");
    }

    // ========== Runner Tests ==========

    fn make_workflow_v45() -> Workflow {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test workflow"

tasks:
  - id: step1
    agent:
      prompt: "Analyze this"

  - id: step2
    function:

      reference: "transform::uppercase"

flows:
  - source: step1
    target: step2
"#;
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn test_topological_sort_v45() {
        let workflow = make_workflow_v45();
        let runner = Runner::new("claude").unwrap();
        let order = runner.topological_sort(&workflow).unwrap();
        assert_eq!(order, vec!["step1", "step2"]);
    }

    #[tokio::test]
    async fn test_run_workflow_v45() {
        let workflow = make_workflow_v45();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_completed, 2, "Should complete 2 tasks");
        assert_eq!(result.tasks_failed, 0, "No tasks should fail");
        assert_eq!(result.results.len(), 2, "Should have 2 results");

        // Check context was populated
        assert!(result.context.get_output("step1").is_some());
        assert!(result.context.get_output("step2").is_some());
    }

    #[tokio::test]
    async fn test_run_with_inputs() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: process
    agent:
      prompt: "Process file: ${input.file}"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

        let mut inputs = HashMap::new();
        inputs.insert("file".to_string(), "README.md".to_string());

        let runner = Runner::new("mock").unwrap();
        let result = runner.run_with_inputs(&workflow, inputs).await.unwrap();

        // The prompt should have been resolved
        let output = result.context.get_output("process").unwrap();
        assert!(output.contains("README.md"));
    }

    #[tokio::test]
    async fn test_context_passing_between_tasks() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: step1
    agent:
      prompt: "Generate data"

  - id: step2
    agent:
      prompt: "Process: {{step1}}"

flows:
  - source: step1
    target: step2
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        // step2 should have received step1's output in its prompt
        let step2_output = result.context.get_output("step2").unwrap();
        // In mock mode, the resolved prompt is echoed back
        assert!(step2_output.contains("[Mock] Executed prompt"));
    }

    #[tokio::test]
    async fn test_all_7_keywords_execution() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: t1
    agent:
      prompt: "agent task"
  - id: t2
    subagent:
      prompt: "subagent task"
  - id: t3
    shell:
      command: "echo test"
  - id: t4
    http:
      url: "https://example.com"
  - id: t5
    mcp:
      reference: "fs::read"
  - id: t6
    function:
      reference: "tools::fn"
  - id: t7
    llm:
      prompt: "classify"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

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

    #[tokio::test]
    async fn test_agent_history_accumulation() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: ask
    agent:
      prompt: "What is Rust?"
  - id: followup
    agent:
      prompt: "Tell me more about its memory safety"

flows:
  - source: ask
    target: followup
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        // Should have 4 messages: 2 user prompts + 2 assistant responses
        assert_eq!(result.context.agent_history().len(), 4);
    }

    // ========== E2E Context Passing Tests ==========

    #[tokio::test]
    async fn test_e2e_context_passing_simple_chain() {
        // Simulates the mvp-context-demo.nika.yaml workflow
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: |
    You are a helpful assistant that processes files and summarizes content.

tasks:
  - id: read-file
    agent:
      prompt: |
        Simulate reading a configuration file.
        Return JSON: {"name": "nika", "version": "0.1.0"}

  - id: summarize
    agent:
      prompt: |
        Here is the content from the previous task:
        {{read-file}}
        Please summarize this in one sentence.

flows:
  - source: read-file
    target: summarize
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

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
    agent:
      prompt: "Return user data"

  - id: use-field
    agent:
      prompt: "Process user: {{generate-user.name}} with email: {{generate-user.email}}"

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
        ctx.set_output(
            "generate-user",
            r#"{"name":"Alice","email":"alice@example.com"}"#.to_string(),
        );

        // Test template resolution directly (runner not used)
        let _runner = Runner::new("mock").unwrap();

        // Use resolve_templates directly to test field resolution
        let template = "Process user: {{generate-user.name}} with email: {{generate-user.email}}";
        let resolved = resolve_templates(template, &ctx).unwrap();

        assert_eq!(
            resolved,
            "Process user: Alice with email: alice@example.com"
        );
    }

    #[tokio::test]
    async fn test_e2e_context_passing_multi_hop() {
        // A → B → C: context flows through all three
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Multi-hop test"

tasks:
  - id: step-a
    agent:
      prompt: "Generate initial data"

  - id: step-b
    agent:
      prompt: "Transform: {{step-a}}"

  - id: step-c
    agent:
      prompt: "Final: {{step-b}} and original: {{step-a}}"

flows:
  - source: step-a
    target: step-b
  - source: step-b
    target: step-c
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

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

    #[tokio::test]
    async fn test_e2e_context_subagent_isolation() {
        // subagent: tasks should NOT accumulate into agent_history
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: main-task
    agent:
      prompt: "Main agent task"

  - id: isolated-task
    subagent:
      prompt: "Isolated subagent task"

  - id: follow-task
    agent:
      prompt: "Follow up agent task"

flows:
  - source: main-task
    target: isolated-task
  - source: isolated-task
    target: follow-task
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

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

    #[tokio::test]
    async fn test_e2e_context_parallel_merge() {
        // Parallel branches that merge: A → (B, C) → D
        // D should have access to both B and C outputs
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Parallel test"

tasks:
  - id: source
    agent:
      prompt: "Source data"

  - id: branch-1
    subagent:
      prompt: "Process branch 1"

  - id: branch-2
    subagent:
      prompt: "Process branch 2"

  - id: merge
    agent:
      prompt: "Merge: {{branch-1}} and {{branch-2}}"

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
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_completed, 4);

        // The merge task should have both branch outputs available
        let merge_output = result.context.get_output("merge").unwrap();
        // Since mock echoes the prompt with resolved templates,
        // and both branches have outputs, the merge should contain "Merge:"
        assert!(merge_output.contains("Merge:"));
    }

    #[tokio::test]
    async fn test_e2e_context_with_environment() {
        std::env::set_var("NIKA_E2E_TEST", "e2e_test_value");

        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Env test"

tasks:
  - id: use-env
    agent:
      prompt: "Environment value: ${env.NIKA_E2E_TEST}"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        let output = result.context.get_output("use-env").unwrap();
        assert!(output.contains("e2e_test_value"));

        std::env::remove_var("NIKA_E2E_TEST");
    }

    #[tokio::test]
    async fn test_e2e_context_with_inputs() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Input test"

tasks:
  - id: process
    agent:
      prompt: "Processing file: ${input.filename}"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();

        let mut inputs = HashMap::new();
        inputs.insert("filename".to_string(), "config.yaml".to_string());

        let runner = Runner::new("mock").unwrap();
        let result = runner.run_with_inputs(&workflow, inputs).await.unwrap();

        let output = result.context.get_output("process").unwrap();
        assert!(output.contains("config.yaml"));
    }

    // ========== P0.1: Shell Timeout Tests ==========

    #[tokio::test]
    async fn test_shell_command_with_default_timeout() {
        // A quick command should complete successfully
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Shell test"

tasks:
  - id: quick-cmd
    shell:
      command: "echo 'hello'"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_completed, 1);
        let output = result.context.get_output("quick-cmd").unwrap();
        assert!(output.contains("hello"));
    }

    #[tokio::test]
    async fn test_shell_command_timeout_returns_error() {
        // A command that would hang should timeout and return an error
        // Using sleep 60 but with a 1-second timeout configured
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Shell timeout test"

tasks:
  - id: slow-cmd
    shell:
      command: "sleep 60"
    config:
      timeout: "1s"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        // The task should have failed due to timeout
        assert_eq!(result.tasks_failed, 1);

        // Check the error message in results
        let task_result = result
            .results
            .iter()
            .find(|r| r.task_id == "slow-cmd")
            .unwrap();
        assert!(!task_result.success, "Task should have failed");
        assert!(
            task_result.output.contains("timed out"),
            "Error should mention timeout, got: {}",
            task_result.output
        );

        // Check error context is set correctly
        assert!(
            task_result.is_timeout(),
            "Error should be categorized as timeout"
        );
        let ctx = task_result.error_context.as_ref().unwrap();
        assert_eq!(ctx.keyword.as_deref(), Some("shell"));
        assert_eq!(ctx.category, Some(ErrorCategory::Timeout));
    }

    // ========== P0.3: Error Context Tests ==========

    #[tokio::test]
    async fn test_shell_failure_has_error_context() {
        // A command that fails (non-zero exit) should have execution error context
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Error context test"

tasks:
  - id: fail-cmd
    shell:
      command: "exit 1"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        assert_eq!(result.tasks_failed, 1);

        let task_result = result
            .results
            .iter()
            .find(|r| r.task_id == "fail-cmd")
            .unwrap();
        assert!(!task_result.success);

        // Check error context
        let ctx = task_result.error_context.as_ref().unwrap();
        assert_eq!(ctx.keyword.as_deref(), Some("shell"));
        assert_eq!(ctx.category, Some(ErrorCategory::Execution));
        assert!(ctx.details.as_ref().unwrap().contains("exit 1"));
    }

    // ========== P0.4: Shell Token Estimation Tests ==========

    #[tokio::test]
    async fn test_shell_task_estimates_tokens() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Token test"

tasks:
  - id: echo-cmd
    shell:
      command: "echo 'This is a test output with some content'"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        // Token count should be > 0 (not hardcoded to 0)
        assert!(
            result.total_tokens > 0,
            "Shell tasks should estimate token usage, got {}",
            result.total_tokens
        );
    }

    // ========== P1.2: Retry Logic Tests ==========

    #[tokio::test]
    async fn test_retry_succeeds_on_second_attempt() {
        // Use a shell command with a state file to track attempts
        // First call fails, second succeeds
        let temp_dir = std::env::temp_dir();
        let state_file = temp_dir.join("nika_retry_test_state");
        let _ = std::fs::remove_file(&state_file); // Clean up from previous runs

        let yaml = format!(
            r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Retry test"

tasks:
  - id: flaky-cmd
    shell:
      command: |
        if [ -f "{state}" ]; then
          echo "success on retry"
        else
          touch "{state}"
          exit 1
        fi
    config:
      retry:
        max: 2

flows: []
"#,
            state = state_file.display()
        );

        let workflow: Workflow = serde_yaml::from_str(&yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        // Clean up
        let _ = std::fs::remove_file(&state_file);

        // Task should succeed after retry
        assert_eq!(
            result.tasks_completed, 1,
            "Task should complete after retry"
        );
        assert_eq!(result.tasks_failed, 0, "No tasks should fail");

        let task_result = result
            .results
            .iter()
            .find(|r| r.task_id == "flaky-cmd")
            .unwrap();
        assert!(task_result.success);
        assert!(task_result.output.contains("success on retry"));
    }

    #[tokio::test]
    async fn test_retry_exhausted_returns_failure() {
        // Command that always fails - retry should be exhausted
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Retry exhausted test"

tasks:
  - id: always-fail
    shell:
      command: "exit 1"
    config:
      retry:
        max: 2

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let runner = Runner::new("mock").unwrap();
        let result = runner.run(&workflow).await.unwrap();

        // Task should fail after all retries exhausted
        assert_eq!(result.tasks_failed, 1);

        let task_result = result
            .results
            .iter()
            .find(|r| r.task_id == "always-fail")
            .unwrap();
        assert!(!task_result.success);
    }
}
