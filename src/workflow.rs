//! Nika Workflow Types (v4.5)
//!
//! Core types for .nika.yaml workflow files.
//! Architecture v4.5: 7 keywords with type inference (agent, subagent, shell, http, mcp, function, llm).

use serde::Deserialize;

// ============================================================================
// WORKFLOW ROOT
// ============================================================================

/// Root workflow structure
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub agent: Agent,
    #[serde(default)]
    pub tasks: Vec<Task>,
    #[serde(default)]
    pub flows: Vec<Flow>,
}

// ============================================================================
// AGENT CONFIG
// ============================================================================

/// Agent configuration - the invisible orchestrator
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Agent {
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

/// A workflow task - type inferred from keyword (v4.5)
///
/// Exactly ONE of the 7 keywords must be present:
/// - Agent: agent, subagent
/// - Tool: shell, http, mcp, function, llm
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    /// Unique identifier (required)
    pub id: String,

    // ========== 7 KEYWORDS (v4.5) - exactly one required ==========

    /// agent: Main Agent works (shared context)
    #[serde(default)]
    pub agent: Option<String>,

    /// subagent: Subagent (isolated 200K context)
    #[serde(default)]
    pub subagent: Option<String>,

    /// shell: Execute shell command
    #[serde(default)]
    pub shell: Option<String>,

    /// http: HTTP request URL
    #[serde(default)]
    pub http: Option<String>,

    /// mcp: MCP server::tool
    #[serde(default)]
    pub mcp: Option<String>,

    /// function: path::functionName
    #[serde(default)]
    pub function: Option<String>,

    /// llm: One-shot LLM (stateless)
    #[serde(default)]
    pub llm: Option<String>,

    // ========== Agent-specific fields ==========

    /// Override model for this task
    #[serde(default)]
    pub model: Option<String>,

    /// Task-specific system prompt
    #[serde(default)]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub system_prompt_file: Option<String>,

    /// Tool access for agent (agent inherits from config, subagent is independent)
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,

    /// Skills to inject
    #[serde(default)]
    pub skills: Option<Vec<String>>,

    /// Max turns for subagent
    #[serde(default)]
    pub max_turns: Option<u32>,

    // ========== Tool-specific fields ==========

    /// HTTP method (for http:)
    #[serde(default)]
    pub method: Option<String>,

    /// HTTP headers (for http:)
    #[serde(default)]
    pub headers: Option<serde_yaml::Value>,

    /// HTTP body (for http:)
    #[serde(default)]
    pub body: Option<serde_yaml::Value>,

    /// Args for function/mcp
    #[serde(default)]
    pub args: Option<serde_yaml::Value>,

    /// Working directory (for shell:)
    #[serde(default)]
    pub cwd: Option<String>,

    // ========== Config block ==========
    #[serde(default)]
    pub config: Option<TaskConfig>,
}

/// Task keyword type (v4.5) - inferred from which keyword is present
///
/// 7 variants = 1 byte with repr(u8), Copy is zero-cost
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TaskKeyword {
    /// agent: Main Agent (shared context)
    Agent = 0,
    /// subagent: Subagent (isolated 200K context)
    Subagent = 1,
    /// shell: Execute shell command
    Shell = 2,
    /// http: HTTP request
    Http = 3,
    /// mcp: MCP server::tool
    Mcp = 4,
    /// function: path::fn
    Function = 5,
    /// llm: One-shot LLM (stateless)
    Llm = 6,
}

impl TaskKeyword {
    /// Get the category for this keyword
    pub fn category(self) -> TaskCategory {
        TaskCategory::from(self)
    }

    /// Check if this is an isolated context (subagent)
    pub fn is_isolated(self) -> bool {
        matches!(self, TaskKeyword::Subagent)
    }

    /// Check if this is a tool keyword (not agent/subagent)
    pub fn is_tool(self) -> bool {
        !matches!(self, TaskKeyword::Agent | TaskKeyword::Subagent)
    }
}

impl std::fmt::Display for TaskKeyword {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskKeyword::Agent => write!(f, "agent"),
            TaskKeyword::Subagent => write!(f, "subagent"),
            TaskKeyword::Shell => write!(f, "shell"),
            TaskKeyword::Http => write!(f, "http"),
            TaskKeyword::Mcp => write!(f, "mcp"),
            TaskKeyword::Function => write!(f, "function"),
            TaskKeyword::Llm => write!(f, "llm"),
        }
    }
}

/// Task category (v4.5) - unified connection key for validation
///
/// Replaces the old ConnectionKey enum. Used for connection matrix validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskCategory {
    /// LLM reasoning with shared context (agent:)
    Context,
    /// LLM reasoning with isolated context (subagent:)
    Isolated,
    /// Deterministic execution (shell, http, mcp, function, llm)
    Tool,
}

impl From<TaskKeyword> for TaskCategory {
    fn from(keyword: TaskKeyword) -> Self {
        match keyword {
            TaskKeyword::Agent => TaskCategory::Context,
            TaskKeyword::Subagent => TaskCategory::Isolated,
            _ => TaskCategory::Tool,
        }
    }
}

impl std::fmt::Display for TaskCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskCategory::Context => write!(f, "agent:"),
            TaskCategory::Isolated => write!(f, "subagent:"),
            TaskCategory::Tool => write!(f, "tool"),
        }
    }
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

/// A flow connecting two tasks (v4.3)
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

impl Task {
    /// Infer the keyword type from which field is Some (v4.5)
    ///
    /// Priority order (spec section 7):
    /// shell > http > mcp > function > llm > subagent > agent
    pub fn keyword(&self) -> Option<TaskKeyword> {
        if self.shell.is_some() {
            Some(TaskKeyword::Shell)
        } else if self.http.is_some() {
            Some(TaskKeyword::Http)
        } else if self.mcp.is_some() {
            Some(TaskKeyword::Mcp)
        } else if self.function.is_some() {
            Some(TaskKeyword::Function)
        } else if self.llm.is_some() {
            Some(TaskKeyword::Llm)
        } else if self.subagent.is_some() {
            Some(TaskKeyword::Subagent)
        } else if self.agent.is_some() {
            Some(TaskKeyword::Agent)
        } else {
            None
        }
    }

    /// Count how many keywords are set (should be exactly 1)
    pub fn keyword_count(&self) -> usize {
        [
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
        .count()
    }

    /// Get the connection key for this task (v4.5)
    ///
    /// Returns the TaskCategory which is used for connection matrix validation.
    pub fn connection_key(&self) -> TaskCategory {
        self.keyword().map(TaskCategory::from).unwrap_or(TaskCategory::Tool)
    }

    /// Get the prompt/value for this task's keyword
    ///
    /// Returns the string value of whichever keyword is set:
    /// - agent/subagent/llm: the prompt text
    /// - shell: the command
    /// - http: the URL
    /// - mcp/function: the server::tool or path::fn reference
    pub fn prompt(&self) -> Option<&str> {
        self.keyword().and_then(|kw| match kw {
            TaskKeyword::Agent => self.agent.as_deref(),
            TaskKeyword::Subagent => self.subagent.as_deref(),
            TaskKeyword::Shell => self.shell.as_deref(),
            TaskKeyword::Http => self.http.as_deref(),
            TaskKeyword::Mcp => self.mcp.as_deref(),
            TaskKeyword::Function => self.function.as_deref(),
            TaskKeyword::Llm => self.llm.as_deref(),
        })
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
// TESTS (v4.5 - keyword syntax)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hello_world_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: greet
    agent: "Say hello in French."

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.agent.model, "claude-sonnet-4-5");
        assert_eq!(workflow.tasks.len(), 1);
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Agent));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Context);
    }

    #[test]
    fn test_parse_subagent_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: researcher
    subagent: "Research deeply."
    model: claude-opus-4
    allowedTools: [Read, Grep]
    maxTurns: 20

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Subagent));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Isolated);
        assert_eq!(workflow.tasks[0].subagent, Some("Research deeply.".to_string()));
        assert_eq!(workflow.tasks[0].max_turns, Some(20));
    }

    #[test]
    fn test_parse_shell_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: test
    shell: "npm test --coverage"
    cwd: "./app"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Shell));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Tool);
        assert_eq!(workflow.tasks[0].shell, Some("npm test --coverage".to_string()));
        assert_eq!(workflow.tasks[0].cwd, Some("./app".to_string()));
    }

    #[test]
    fn test_parse_http_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: webhook
    http: "https://api.slack.com/webhook"
    method: POST

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Http));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Tool);
        assert_eq!(workflow.tasks[0].http, Some("https://api.slack.com/webhook".to_string()));
    }

    #[test]
    fn test_parse_mcp_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: read-file
    mcp: "filesystem::read_file"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Mcp));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Tool);
    }

    #[test]
    fn test_parse_function_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: transform
    function: "./tools/transform.js::processData"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Function));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Tool);
    }

    #[test]
    fn test_parse_llm_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: classify
    llm: "Classify as: bug | feature | question"
    model: claude-haiku

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword(), Some(TaskKeyword::Llm));
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Tool);
    }

    #[test]
    fn test_parse_flow_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: a
    agent: "A"
  - id: b
    agent: "B"

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
    fn test_parse_conditional_flow_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: validate
    agent: "Validate"
  - id: publish
    http: "https://api.example.com"
    method: POST

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
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
  mode: agentic

tasks: []
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.agent.mode, ExecutionMode::Agentic);
    }

    #[test]
    fn test_keyword_count() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: valid
    agent: "Just one keyword"
  - id: none
    # No keyword - invalid

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].keyword_count(), 1);
        assert_eq!(workflow.tasks[1].keyword_count(), 0);
    }

    #[test]
    fn test_keyword_category() {
        // Context keywords
        assert_eq!(TaskKeyword::Agent.category(), TaskCategory::Context);
        assert_eq!(TaskKeyword::Subagent.category(), TaskCategory::Isolated);
        // Tool keywords
        assert_eq!(TaskKeyword::Shell.category(), TaskCategory::Tool);
        assert_eq!(TaskKeyword::Http.category(), TaskCategory::Tool);
        assert_eq!(TaskKeyword::Mcp.category(), TaskCategory::Tool);
        assert_eq!(TaskKeyword::Function.category(), TaskCategory::Tool);
        assert_eq!(TaskKeyword::Llm.category(), TaskCategory::Tool);
    }

    #[test]
    fn test_keyword_from_trait() {
        // Test From<TaskKeyword> for TaskCategory
        assert_eq!(TaskCategory::from(TaskKeyword::Agent), TaskCategory::Context);
        assert_eq!(TaskCategory::from(TaskKeyword::Subagent), TaskCategory::Isolated);
        assert_eq!(TaskCategory::from(TaskKeyword::Shell), TaskCategory::Tool);
    }

    #[test]
    fn test_keyword_is_tool() {
        assert!(!TaskKeyword::Agent.is_tool());
        assert!(!TaskKeyword::Subagent.is_tool());
        assert!(TaskKeyword::Shell.is_tool());
        assert!(TaskKeyword::Http.is_tool());
        assert!(TaskKeyword::Mcp.is_tool());
        assert!(TaskKeyword::Function.is_tool());
        assert!(TaskKeyword::Llm.is_tool());
    }

    #[test]
    fn test_bridge_pattern_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: worker
    subagent: "Work in isolation"
  - id: bridge
    function: aggregate::collect
  - id: router
    agent: "Route the results"

flows:
  - source: worker
    target: bridge
  - source: bridge
    target: router
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].connection_key(), TaskCategory::Isolated);
        assert_eq!(workflow.tasks[1].connection_key(), TaskCategory::Tool);
        assert_eq!(workflow.tasks[2].connection_key(), TaskCategory::Context);
    }

    #[test]
    fn test_task_prompt() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: t1
    agent: "Analyze the code"
  - id: t2
    subagent: "Research deeply"
  - id: t3
    shell: "npm test"
  - id: t4
    http: "https://api.example.com"
  - id: t5
    mcp: "filesystem::read"
  - id: t6
    function: "utils::transform"
  - id: t7
    llm: "Classify this"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].prompt(), Some("Analyze the code"));
        assert_eq!(workflow.tasks[1].prompt(), Some("Research deeply"));
        assert_eq!(workflow.tasks[2].prompt(), Some("npm test"));
        assert_eq!(workflow.tasks[3].prompt(), Some("https://api.example.com"));
        assert_eq!(workflow.tasks[4].prompt(), Some("filesystem::read"));
        assert_eq!(workflow.tasks[5].prompt(), Some("utils::transform"));
        assert_eq!(workflow.tasks[6].prompt(), Some("Classify this"));
    }

}
