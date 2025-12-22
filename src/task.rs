//! # Task v6.0 - Clean Nested Format 4
//!
//! No legacy. No backwards compatibility. No compromises.
//!
//! ## Design Philosophy
//! - **No legacy support** - We're v0, break things to make them better
//! - **No backward compatibility** - Clean migrations, not compatibility layers
//! - **No aliases** - One way to do things
//! - **Start fresh** - Delete and rewrite is better than patching
//!
//! ## Format 4 - The Only Way
//! ```yaml
//! tasks:
//!   - id: api-call
//!     http:
//!       url: "https://api.example.com"
//!       method: POST
//!       headers:
//!         Authorization: Bearer token
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ============================================================================
// CLEAN TASK STRUCTURE
// ============================================================================

/// A workflow task - clean and simple
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Task {
    /// Unique task identifier
    pub id: String,

    /// The action - exactly one keyword
    #[serde(flatten)]
    pub action: TaskAction,

    /// Optional config block
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<TaskConfig>,
}

// ============================================================================
// TASK ACTIONS - ONE FOR EACH KEYWORD
// ============================================================================

/// The 7 task actions - clean enum, no complexity
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum TaskAction {
    /// agent: Main Agent with shared context
    Agent { agent: AgentDef },

    /// subagent: Isolated context (200K sandbox)
    Subagent { subagent: SubagentDef },

    /// shell: Execute shell command
    Shell { shell: ShellDef },

    /// http: Make HTTP request
    Http { http: HttpDef },

    /// mcp: MCP server::tool call
    Mcp { mcp: McpDef },

    /// function: Call path::functionName
    Function { function: FunctionDef },

    /// llm: One-shot stateless LLM call
    Llm { llm: LlmDef },
}

// ============================================================================
// ACTION DEFINITIONS - NESTED STRUCTURE
// ============================================================================

/// Agent definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentDef {
    /// The prompt (required)
    pub prompt: String,

    /// Override model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// Override system prompt
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Allowed tools (restricted from agent pool)
    #[serde(rename = "allowedTools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,

    /// Skills to inject
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,
}

/// Subagent definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SubagentDef {
    /// The prompt (required)
    pub prompt: String,

    /// Model for this subagent
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,

    /// System prompt
    #[serde(rename = "systemPrompt", skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Independent tool access
    #[serde(rename = "allowedTools", skip_serializing_if = "Option::is_none")]
    pub allowed_tools: Option<Vec<String>>,

    /// Skills to inject
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skills: Option<Vec<String>>,

    /// Max turns
    #[serde(rename = "maxTurns", skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
}

/// Shell definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShellDef {
    /// The command (required)
    pub command: String,

    /// Working directory
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,

    /// Environment variables
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<HashMap<String, String>>,
}

/// HTTP definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpDef {
    /// URL (required)
    pub url: String,

    /// HTTP method (default: GET)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>,

    /// Headers
    #[serde(skip_serializing_if = "Option::is_none")]
    pub headers: Option<HashMap<String, String>>,

    /// Body
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<serde_json::Value>,
}

/// MCP definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct McpDef {
    /// server::tool reference (required)
    pub reference: String,

    /// Arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// Function definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FunctionDef {
    /// path::name reference (required)
    pub reference: String,

    /// Arguments
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

/// LLM definition - nested structure
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LlmDef {
    /// The prompt (required)
    pub prompt: String,

    /// Model (default: haiku)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

// ============================================================================
// TASK CONFIG
// ============================================================================

/// Task configuration - same for all tasks
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry: Option<RetryConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_error: Option<String>,
}

/// Retry configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetryConfig {
    pub max: u32,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub backoff: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_delay: Option<String>,
}

// ============================================================================
// TASK METHODS
// ============================================================================

impl Task {
    /// Get the keyword type for this task
    pub fn keyword(&self) -> TaskKeyword {
        match &self.action {
            TaskAction::Agent { .. } => TaskKeyword::Agent,
            TaskAction::Subagent { .. } => TaskKeyword::Subagent,
            TaskAction::Shell { .. } => TaskKeyword::Shell,
            TaskAction::Http { .. } => TaskKeyword::Http,
            TaskAction::Mcp { .. } => TaskKeyword::Mcp,
            TaskAction::Function { .. } => TaskKeyword::Function,
            TaskAction::Llm { .. } => TaskKeyword::Llm,
        }
    }

    /// Get the category for connection validation
    pub fn category(&self) -> TaskCategory {
        self.keyword().into()
    }

    /// Alias for category (compatibility)
    pub fn connection_key(&self) -> TaskCategory {
        self.category()
    }

    /// Always returns 1 (enum guarantees single keyword)
    pub fn keyword_count(&self) -> usize {
        1
    }

    /// Get the main content/prompt for display
    pub fn prompt(&self) -> &str {
        match &self.action {
            TaskAction::Agent { agent } => &agent.prompt,
            TaskAction::Subagent { subagent } => &subagent.prompt,
            TaskAction::Shell { shell } => &shell.command,
            TaskAction::Http { http } => &http.url,
            TaskAction::Mcp { mcp } => &mcp.reference,
            TaskAction::Function { function } => &function.reference,
            TaskAction::Llm { llm } => &llm.prompt,
        }
    }

    /// Check if isolated (subagent)
    pub fn is_isolated(&self) -> bool {
        matches!(self.action, TaskAction::Subagent { .. })
    }

    /// Check if tool (not agent/subagent)
    pub fn is_tool(&self) -> bool {
        !matches!(
            self.action,
            TaskAction::Agent { .. } | TaskAction::Subagent { .. }
        )
    }
}

// ============================================================================
// ENUMS
// ============================================================================

/// Task keyword type - 7 values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TaskKeyword {
    Agent = 0,
    Subagent = 1,
    Shell = 2,
    Http = 3,
    Mcp = 4,
    Function = 5,
    Llm = 6,
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

/// Task category for connection matrix
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskCategory {
    Context,  // agent:
    Isolated, // subagent:
    Tool,     // all others
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

// ============================================================================
// TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_agent_nested() {
        let yaml = r#"
id: greet
agent:
  prompt: "Say hello in French"
  model: claude-opus
  allowedTools: [Read, Write]
"#;
        let task: Task = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(task.id, "greet");
        assert_eq!(task.keyword(), TaskKeyword::Agent);
        assert_eq!(task.prompt(), "Say hello in French");
    }

    #[test]
    fn test_parse_http_nested() {
        let yaml = r#"
id: webhook
http:
  url: "https://api.example.com/webhook"
  method: POST
  headers:
    Authorization: "Bearer token"
"#;
        let task: Task = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(task.id, "webhook");
        assert_eq!(task.keyword(), TaskKeyword::Http);
        assert_eq!(task.prompt(), "https://api.example.com/webhook");
    }

    #[test]
    fn test_parse_shell_nested() {
        let yaml = r#"
id: build
shell:
  command: "npm run build"
  cwd: "./app"
  env:
    NODE_ENV: production
"#;
        let task: Task = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(task.id, "build");
        assert_eq!(task.keyword(), TaskKeyword::Shell);
        assert_eq!(task.prompt(), "npm run build");
    }
}
