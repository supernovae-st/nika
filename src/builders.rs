//! Builder patterns for ergonomic workflow construction (v4.6)
//!
//! Provides fluent APIs for building complex workflows programmatically.

use crate::types::{ShellCommand, TaskId, Url, WorkflowName};
use crate::workflow::{Agent, ExecutionMode, Flow, Task, TaskConfig, Workflow};
use std::collections::HashMap;

// ============================================================================
// WORKFLOW BUILDER
// ============================================================================

/// Fluent builder for constructing workflows
pub struct WorkflowBuilder {
    name: Option<WorkflowName>,
    agent: Option<Agent>,
    tasks: Vec<Task>,
    flows: Vec<Flow>,
    inputs: HashMap<String, String>,
    metadata: HashMap<String, String>,
}

impl WorkflowBuilder {
    /// Create a new workflow builder
    pub fn new() -> Self {
        Self {
            name: None,
            agent: None,
            tasks: Vec::new(),
            flows: Vec::new(),
            inputs: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    /// Set workflow name
    pub fn name(mut self, name: impl TryInto<WorkflowName>) -> Result<Self, BuilderError> {
        self.name = Some(
            name.try_into()
                .map_err(|_| BuilderError::InvalidName("Invalid workflow name".into()))?
        );
        Ok(self)
    }

    /// Configure the main agent
    pub fn agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);
        self
    }

    /// Configure using AgentBuilder
    pub fn with_agent<F>(mut self, f: F) -> Result<Self, BuilderError>
    where
        F: FnOnce(AgentBuilder) -> Result<AgentBuilder, BuilderError>,
    {
        let builder = f(AgentBuilder::new())?;
        self.agent = Some(builder.build()?);
        Ok(self)
    }

    /// Add a task
    pub fn task(mut self, task: Task) -> Self {
        self.tasks.push(task);
        self
    }

    /// Add a task using TaskBuilder
    pub fn with_task<F>(mut self, id: &str, f: F) -> Result<Self, BuilderError>
    where
        F: FnOnce(TaskBuilder) -> Result<TaskBuilder, BuilderError>,
    {
        let builder = f(TaskBuilder::new(id)?)?;
        self.tasks.push(builder.build()?);
        Ok(self)
    }

    /// Add a flow
    pub fn flow(mut self, source: &str, target: &str) -> Self {
        self.flows.push(Flow {
            source: source.to_string(),
            target: target.to_string(),
            condition: None,
        });
        self
    }

    /// Add a conditional flow
    pub fn conditional_flow(mut self, source: &str, target: &str, condition: &str) -> Self {
        self.flows.push(Flow {
            source: source.to_string(),
            target: target.to_string(),
            condition: Some(condition.to_string()),
        });
        self
    }

    /// Add input parameter
    pub fn input(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.inputs.insert(key.into(), value.into());
        self
    }

    /// Add metadata
    pub fn meta(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Build the workflow
    pub fn build(self) -> Result<Workflow, BuilderError> {
        let agent = self.agent.ok_or(BuilderError::MissingAgent)?;

        if self.tasks.is_empty() {
            return Err(BuilderError::NoTasks);
        }

        Ok(Workflow {
            agent,
            tasks: self.tasks,
            flows: self.flows,
        })
    }
}

impl Default for WorkflowBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// AGENT BUILDER
// ============================================================================

/// Builder for Agent configuration
pub struct AgentBuilder {
    model: Option<String>,
    system_prompt: Option<String>,
    mode: Option<ExecutionMode>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

impl AgentBuilder {
    pub fn new() -> Self {
        Self {
            model: None,
            system_prompt: None,
            mode: None,
            temperature: None,
            max_tokens: None,
        }
    }

    pub fn model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    pub fn mode(mut self, mode: ExecutionMode) -> Self {
        self.mode = Some(mode);
        self
    }

    pub fn temperature(mut self, temp: f32) -> Self {
        self.temperature = Some(temp);
        self
    }

    pub fn max_tokens(mut self, tokens: u32) -> Self {
        self.max_tokens = Some(tokens);
        self
    }

    pub fn build(self) -> Result<Agent, BuilderError> {
        let model = self
            .model
            .ok_or(BuilderError::MissingField("model".into()))?;

        Ok(Agent {
            model,
            system_prompt: self.system_prompt,
            system_prompt_file: None, // Add missing field
            mode: self.mode.unwrap_or(ExecutionMode::Strict),
            max_turns: None, // Add missing field
            max_budget_usd: None, // Add missing field
            allowed_tools: None, // Add missing field
            disallowed_tools: None, // Add missing field
            output: None, // Add missing field
        })
    }
}

impl Default for AgentBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// TASK BUILDER
// ============================================================================

/// Builder for individual tasks
pub struct TaskBuilder {
    id: String,
    agent: Option<String>,
    subagent: Option<String>,
    shell: Option<String>,
    http: Option<String>,
    mcp: Option<String>,
    function: Option<String>,
    llm: Option<String>,
    method: Option<String>,
    args: Option<serde_json::Value>,
    config: Option<TaskConfig>,
    allowed_tools: Option<Vec<String>>,
}

impl TaskBuilder {
    /// Create new task builder
    pub fn new(id: &str) -> Result<Self, BuilderError> {
        // Validate ID
        TaskId::new(id).map_err(|e| BuilderError::InvalidId(e.to_string()))?;

        Ok(Self {
            id: id.to_string(),
            agent: None,
            subagent: None,
            shell: None,
            http: None,
            mcp: None,
            function: None,
            llm: None,
            method: None,
            args: None,
            config: None,
            allowed_tools: None,
        })
    }

    /// Set as agent task
    pub fn agent(mut self, prompt: impl Into<String>) -> Self {
        self.agent = Some(prompt.into());
        self
    }

    /// Set as subagent task
    pub fn subagent(mut self, prompt: impl Into<String>) -> Self {
        self.subagent = Some(prompt.into());
        self
    }

    /// Set as shell task
    pub fn shell(mut self, command: impl Into<String>) -> Result<Self, BuilderError> {
        let cmd = command.into();
        // Validate command safety
        ShellCommand::new(&cmd)
            .map_err(|e| BuilderError::UnsafeCommand(e.to_string()))?;

        self.shell = Some(cmd);
        Ok(self)
    }

    /// Set as shell task (unsafe, allows dangerous commands)
    ///
    /// # Safety
    /// Caller must ensure the command is safe to execute. This bypasses
    /// all validation and allows potentially dangerous commands like `rm -rf`.
    pub unsafe fn shell_unchecked(mut self, command: impl Into<String>) -> Self {
        self.shell = Some(command.into());
        self
    }

    /// Set as HTTP task
    pub fn http(mut self, url: impl Into<String>) -> Result<Self, BuilderError> {
        let u = url.into();
        // Validate URL
        Url::new(&u)
            .map_err(|e| BuilderError::InvalidUrl(e.to_string()))?;

        self.http = Some(u);
        Ok(self)
    }

    /// Set HTTP method
    pub fn method(mut self, method: &str) -> Self {
        self.method = Some(method.to_uppercase());
        self
    }

    /// Set as MCP task
    pub fn mcp(mut self, server_tool: impl Into<String>) -> Self {
        self.mcp = Some(server_tool.into());
        self
    }

    /// Set as function task
    pub fn function(mut self, path_func: impl Into<String>) -> Self {
        self.function = Some(path_func.into());
        self
    }

    /// Set as LLM task
    pub fn llm(mut self, prompt: impl Into<String>) -> Self {
        self.llm = Some(prompt.into());
        self
    }

    /// Set task arguments
    pub fn args(mut self, args: serde_yaml::Value) -> Self {
        // Convert serde_yaml::Value to serde_json::Value
        let json_str = serde_json::to_string(&args).unwrap_or_default();
        let json_value: serde_json::Value = serde_json::from_str(&json_str).unwrap_or(serde_json::Value::Null);
        self.args = Some(json_value);
        self
    }

    /// Configure task
    pub fn config(mut self, config: TaskConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Configure using ConfigBuilder
    pub fn with_config<F>(mut self, f: F) -> Result<Self, BuilderError>
    where
        F: FnOnce(ConfigBuilder) -> ConfigBuilder,
    {
        let builder = f(ConfigBuilder::new());
        self.config = Some(builder.build());
        Ok(self)
    }

    /// Set allowed tools
    pub fn allowed_tools(mut self, tools: Vec<String>) -> Self {
        self.allowed_tools = Some(tools);
        self
    }

    /// Build the task
    pub fn build(self) -> Result<Task, BuilderError> {
        use crate::task::{TaskAction, AgentDef, SubagentDef, ShellDef, HttpDef, McpDef, FunctionDef, LlmDef};

        // Ensure exactly one keyword is set
        let keyword_count = [
            self.agent.is_some(),
            self.subagent.is_some(),
            self.shell.is_some(),
            self.http.is_some(),
            self.mcp.is_some(),
            self.function.is_some(),
            self.llm.is_some(),
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        if keyword_count == 0 {
            return Err(BuilderError::NoKeyword);
        }
        if keyword_count > 1 {
            return Err(BuilderError::MultipleKeywords);
        }

        // Build the appropriate TaskAction variant with nested structure
        let action = if let Some(prompt) = self.agent {
            TaskAction::Agent {
                agent: AgentDef {
                    prompt,
                    model: None,
                    system_prompt: None,
                    allowed_tools: self.allowed_tools,
                    skills: None,
                },
            }
        } else if let Some(prompt) = self.subagent {
            TaskAction::Subagent {
                subagent: SubagentDef {
                    prompt,
                    model: None,
                    system_prompt: None,
                    allowed_tools: self.allowed_tools,
                    skills: None,
                    max_turns: None,
                },
            }
        } else if let Some(command) = self.shell {
            TaskAction::Shell {
                shell: ShellDef {
                    command,
                    cwd: None,
                    env: None,
                },
            }
        } else if let Some(url) = self.http {
            TaskAction::Http {
                http: HttpDef {
                    url,
                    method: self.method,
                    headers: None,
                    body: None,
                },
            }
        } else if let Some(reference) = self.mcp {
            TaskAction::Mcp {
                mcp: McpDef {
                    reference,
                    args: self.args,
                },
            }
        } else if let Some(reference) = self.function {
            TaskAction::Function {
                function: FunctionDef {
                    reference,
                    args: self.args,
                },
            }
        } else if let Some(prompt) = self.llm {
            TaskAction::Llm {
                llm: LlmDef {
                    prompt,
                    model: None,
                },
            }
        } else {
            return Err(BuilderError::NoKeyword);
        };

        Ok(Task {
            id: self.id,
            action,
            config: self.config,
        })
    }
}

// ============================================================================
// CONFIG BUILDER
// ============================================================================

/// Builder for task configuration
pub struct ConfigBuilder {
    timeout: Option<String>,
    retries: Option<u32>,
    retry_delay: Option<String>,
    retry_backoff: Option<String>,
    on_error: Option<String>,
}

impl ConfigBuilder {
    pub fn new() -> Self {
        Self {
            timeout: None,
            retries: None,
            retry_delay: None,
            retry_backoff: None,
            on_error: None,
        }
    }

    pub fn timeout(mut self, timeout: &str) -> Self {
        self.timeout = Some(timeout.to_string());
        self
    }

    pub fn retries(mut self, retries: u32) -> Self {
        self.retries = Some(retries);
        self
    }

    pub fn retry_delay(mut self, delay: &str) -> Self {
        self.retry_delay = Some(delay.to_string());
        self
    }

    pub fn retry_backoff(mut self, backoff: &str) -> Self {
        self.retry_backoff = Some(backoff.to_string());
        self
    }

    pub fn build(self) -> TaskConfig {
        use crate::task::RetryConfig;

        TaskConfig {
            timeout: self.timeout,
            retry: if self.retries.is_some() {
                Some(RetryConfig {
                    max: self.retries.unwrap_or(3),
                    backoff: self.retry_backoff,
                    base_delay: self.retry_delay,
                })
            } else {
                None
            },
            on_error: self.on_error,
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// ERROR TYPE
// ============================================================================

#[derive(Debug, thiserror::Error)]
pub enum BuilderError {
    #[error("Invalid workflow name: {0}")]
    InvalidName(String),
    #[error("Invalid task ID: {0}")]
    InvalidId(String),
    #[error("Missing agent configuration")]
    MissingAgent,
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("No tasks defined")]
    NoTasks,
    #[error("Task has no keyword set")]
    NoKeyword,
    #[error("Task has multiple keywords set")]
    MultipleKeywords,
    #[error("Unsafe command: {0}")]
    UnsafeCommand(String),
    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_workflow_builder() {
        let workflow = WorkflowBuilder::new()
            .with_agent(|a| {
                Ok(a.model("claude-sonnet-4-5")
                    .system_prompt("You are helpful"))
            })
            .unwrap()
            .with_task("greet", |t| {
                Ok(t.agent("Say hello"))
            })
            .unwrap()
            .with_task("translate", |t| {
                Ok(t.subagent("Translate to French"))
            })
            .unwrap()
            .flow("greet", "translate")
            .build()
            .unwrap();

        assert_eq!(workflow.tasks.len(), 2);
        assert_eq!(workflow.flows.len(), 1);
        assert_eq!(workflow.agent.model, "claude-sonnet-4-5");
    }

    #[test]
    fn test_task_builder_validation() {
        use crate::task::{TaskAction, ShellDef};

        // Valid shell command
        let task = TaskBuilder::new("safe-cmd")
            .unwrap()
            .shell("ls -la")
            .unwrap()
            .build()
            .unwrap();
        match &task.action {
            TaskAction::Shell { shell: ShellDef { command, .. } } => {
                assert_eq!(command, "ls -la");
            }
            _ => panic!("Expected Shell action"),
        }

        // Dangerous shell command
        let result = TaskBuilder::new("danger")
            .unwrap()
            .shell("rm -rf /");
        assert!(result.is_err());

        // But can use unsafe
        let task = unsafe {
            TaskBuilder::new("danger")
                .unwrap()
                .shell_unchecked("rm -rf /")
                .build()
                .unwrap()
        };
        match &task.action {
            TaskAction::Shell { shell: ShellDef { command, .. } } => {
                assert_eq!(command, "rm -rf /");
            }
            _ => panic!("Expected Shell action"),
        }
    }

    #[test]
    fn test_multiple_keywords_error() {
        let result = TaskBuilder::new("multi")
            .unwrap()
            .agent("Do something")
            .shell("ls")
            .unwrap()
            .build();

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("multiple keywords"));
    }

    #[test]
    fn test_config_builder() {
        let task = TaskBuilder::new("configured")
            .unwrap()
            .agent("Do work")
            .with_config(|c| {
                c.timeout("30s")
                    .retries(3)
                    .retry_delay("1s")
            })
            .unwrap()
            .build()
            .unwrap();

        let config = task.config.unwrap();
        assert_eq!(config.timeout, Some("30s".into()));
        let retry = config.retry.unwrap();
        assert_eq!(retry.max, 3);
        assert_eq!(retry.base_delay, Some("1s".into()));
    }
}