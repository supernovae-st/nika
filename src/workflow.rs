//! Nika Workflow Types (v3)
//!
//! Core types for .nika.yaml workflow files.
//! Architecture v3: 2 task types (agent + action) with scope attribute.

use serde::Deserialize;

// ============================================================================
// WORKFLOW ROOT
// ============================================================================

/// Root workflow structure
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub main_agent: MainAgent,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub flows: Vec<Flow>,
}

// ============================================================================
// MAIN AGENT
// ============================================================================

/// Main Agent configuration - the invisible orchestrator
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainAgent {
    /// LLM model (required)
    pub model: String,

    /// System prompt (one of these required)
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub system_prompt_file: Option<String>,

    /// Execution mode
    #[serde(default)]
    pub mode: ExecutionMode,

    /// Resource limits
    #[serde(default)]
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub max_budget_usd: Option<f32>,

    /// Tool access control
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disallowed_tools: Option<Vec<String>>,

    /// Output task ID
    #[serde(default)]
    pub output: Option<String>,
}

/// Execution mode
#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionMode {
    /// Follow DAG exactly, deterministic
    #[default]
    Strict,
    /// Main Agent can deviate, skip, retry autonomously
    Agentic,
}

// ============================================================================
// TASK
// ============================================================================

/// A workflow task - either agent or action
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique identifier (required)
    pub id: String,

    /// Task type: agent or action (required)
    #[serde(rename = "type")]
    pub task_type: TaskType,

    // ========== Agent-specific fields ==========
    /// Scope: main (default) or isolated (agent only)
    #[serde(default)]
    pub scope: Option<Scope>,

    /// LLM prompt (required for agent)
    #[serde(default)]
    pub prompt: Option<String>,

    /// Override model for this task
    #[serde(default)]
    pub model: Option<String>,

    /// Task-specific system prompt
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub system_prompt_file: Option<String>,

    /// Tool access for this agent
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    // ========== Action-specific fields ==========
    /// Tool/package to run (required for action)
    #[serde(default)]
    pub run: Option<String>,

    // ========== Common action parameters (flat) ==========
    /// File path (for Read/Write actions)
    #[serde(default)]
    pub file: Option<String>,

    /// URL (for http action)
    #[serde(default)]
    pub url: Option<String>,

    /// HTTP method
    #[serde(default)]
    pub method: Option<String>,

    /// Command (for Bash action)
    #[serde(default)]
    pub command: Option<String>,

    /// Format (for transform/aggregate)
    #[serde(default)]
    pub format: Option<String>,

    /// Channel (for slack/notifications)
    #[serde(default)]
    pub channel: Option<String>,

    /// Message (for notifications)
    #[serde(default)]
    pub message: Option<String>,

    // ========== Config block ==========
    #[serde(default)]
    pub config: Option<TaskConfig>,
}

/// Task type enum (v3)
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskType {
    /// LLM reasoning task
    Agent,
    /// Deterministic function execution
    Action,
}

/// Scope for agent tasks
#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Scope {
    /// Visible to Main Agent, enriches shared context
    #[default]
    Main,
    /// Separate 200K context, passthrough
    Isolated,
}

/// Task configuration
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskConfig {
    #[serde(default)]
    pub retry: Option<RetryConfig>,
    #[serde(default)]
    pub timeout: Option<String>,
    #[serde(default)]
    pub on_error: Option<String>,
}

/// Retry configuration
#[derive(Debug, Deserialize)]
pub struct RetryConfig {
    pub max: u32,
    #[serde(default)]
    pub backoff: Option<String>,
}

// ============================================================================
// FLOW
// ============================================================================

/// A flow connecting two tasks (v3 - no handles)
#[derive(Debug, Deserialize)]
pub struct Flow {
    /// Source task ID
    pub source: String,
    /// Target task ID
    pub target: String,
    /// Optional condition expression
    #[serde(default)]
    pub condition: Option<String>,
}

// ============================================================================
// CONNECTION KEY (for validation)
// ============================================================================

/// Connection key for matrix lookup
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectionKey {
    AgentMain,
    AgentIsolated,
    Action,
}

impl std::fmt::Display for ConnectionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConnectionKey::AgentMain => write!(f, "agent(main)"),
            ConnectionKey::AgentIsolated => write!(f, "agent(isolated)"),
            ConnectionKey::Action => write!(f, "action"),
        }
    }
}

impl Task {
    /// Get the connection key for this task
    pub fn connection_key(&self) -> ConnectionKey {
        match self.task_type {
            TaskType::Agent => {
                let scope = self.scope.clone().unwrap_or_default();
                match scope {
                    Scope::Main => ConnectionKey::AgentMain,
                    Scope::Isolated => ConnectionKey::AgentIsolated,
                }
            }
            TaskType::Action => ConnectionKey::Action,
        }
    }
}

// ============================================================================
// WORKFLOW HELPERS
// ============================================================================

impl Workflow {
    /// Get a task by its ID
    pub fn get_task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|t| t.id == id)
    }

    /// Iterator over all task IDs
    pub fn task_ids(&self) -> impl Iterator<Item = &str> {
        self.tasks.iter().map(|t| t.id.as_str())
    }
}

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hello_world() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: greet
    type: agent
    prompt: "Say hello in French."

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.main_agent.model, "claude-sonnet-4-5");
        assert_eq!(workflow.tasks.len(), 1);
        assert_eq!(workflow.tasks[0].task_type, TaskType::Agent);
    }

    #[test]
    fn test_parse_agent_isolated() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: researcher
    type: agent
    scope: isolated
    prompt: "Research deeply."

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].scope, Some(Scope::Isolated));
        assert_eq!(
            workflow.tasks[0].connection_key(),
            ConnectionKey::AgentIsolated
        );
    }

    #[test]
    fn test_parse_action() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: read-file
    type: action
    run: Read
    file: "source.txt"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].task_type, TaskType::Action);
        assert_eq!(workflow.tasks[0].run, Some("Read".to_string()));
        assert_eq!(workflow.tasks[0].file, Some("source.txt".to_string()));
        assert_eq!(workflow.tasks[0].connection_key(), ConnectionKey::Action);
    }

    #[test]
    fn test_parse_flow_no_handles() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: a
    type: agent
    prompt: "A"
  - id: b
    type: agent
    prompt: "B"

flows:
  - source: a
    target: b
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.flows.len(), 1);
        assert_eq!(workflow.flows[0].source, "a");
        assert_eq!(workflow.flows[0].target, "b");
    }

    #[test]
    fn test_parse_conditional_flow() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: validate
    type: agent
    prompt: "Validate"
  - id: publish
    type: action
    run: http
    url: "https://api.example.com"

flows:
  - source: validate
    target: publish
    condition: "output.score >= 8"
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            workflow.flows[0].condition,
            Some("output.score >= 8".to_string())
        );
    }

    #[test]
    fn test_execution_mode() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  mode: agentic

tasks: []
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.main_agent.mode, ExecutionMode::Agentic);
    }

    #[test]
    fn test_connection_key_default_scope() {
        let task = Task {
            id: "test".to_string(),
            task_type: TaskType::Agent,
            scope: None, // Should default to Main
            prompt: Some("test".to_string()),
            model: None,
            system_prompt: None,
            system_prompt_file: None,
            allowed_tools: None,
            run: None,
            file: None,
            url: None,
            method: None,
            command: None,
            format: None,
            channel: None,
            message: None,
            config: None,
        };
        assert_eq!(task.connection_key(), ConnectionKey::AgentMain);
    }
}
