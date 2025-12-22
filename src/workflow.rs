//! # Nika Workflow Types (v4.6)
//!
//! Core types for `.nika.yaml` workflow files with TaskAction enum.
//!
//! ## Overview
//!
//! This module defines the data structures for Nika workflow files:
//!
//! - [`Workflow`] - Root structure containing agent config, tasks, and flows
//! - [`Agent`] - Agent configuration (model, system prompt, limits)
//! - [`Task`] - Individual workflow task with TaskAction enum (v4.6)
//! - [`Flow`] - Connection between tasks with optional conditions
//!
//! ## The 7 Keywords (v4.6)
//!
//! Each task must have exactly **one** keyword that determines its type:
//!
//! ```yaml
//! # Agent keywords (LLM reasoning)
//! - id: analyze
//!   agent:
//!     prompt: "Analyze the code"         # Main Agent (shared context)
//!
//! - id: research
//!   subagent:
//!     prompt: "Research deeply"          # Subagent (isolated 200K context)
//!
//! # Tool keywords (deterministic execution)
//! - id: run-tests
//!   shell:
//!     command: "npm test"                # Execute shell command
//!
//! - id: webhook
//!   http:
//!     url: "https://api.example.com"     # HTTP request
//!
//! - id: read-file
//!   mcp:
//!     reference: "filesystem::read_file" # MCP server::tool
//!
//! - id: transform
//!   function:
//!     reference: "utils::processData"    # path::functionName
//!
//! - id: classify
//!   llm:
//!     prompt: "Classify: bug | feature"  # One-shot stateless LLM
//! ```
//!
//! ## Task Categories
//!
//! Keywords are grouped into categories for connection validation:
//!
//! - **Context** (`agent:`) - Shared main agent context
//! - **Isolated** (`subagent:`) - Sandboxed 200K context
//! - **Tool** (all others) - Deterministic execution
//!
//! ## Example
//!
//! ```rust
//! use nika::{Workflow, TaskKeyword, TaskCategory};
//!
//! let yaml = r#"
//! agent:
//!   model: claude-sonnet-4-5
//!   systemPrompt: "You are a helpful assistant."
//! tasks:
//!   - id: greet
//!     agent:
//!       prompt: "Say hello"
//! flows: []
//! "#;
//!
//! let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
//! let task = &workflow.tasks[0];
//!
//! assert_eq!(task.keyword(), TaskKeyword::Agent);
//! assert_eq!(task.category(), TaskCategory::Context);
//! ```

use serde::Deserialize;

// Import the new Task structure from task
pub use crate::task::{
    Task, TaskAction, TaskKeyword, TaskCategory, TaskConfig,
    RetryConfig
};

// ============================================================================
// WORKFLOW ROOT
// ============================================================================

/// Root workflow structure
#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
// TASK - Now imported from task module
// ============================================================================

// Task, TaskKeyword, TaskCategory, TaskConfig, and RetryConfig are imported from task above

// ============================================================================
// FLOW
// ============================================================================

/// A flow connecting two tasks (v4.6)
#[derive(Debug, Clone, Deserialize)]
pub struct Flow {
    /// Source task ID
    pub source: String,
    /// Target task ID
    pub target: String,
    /// Optional condition expression
    #[serde(default)]
    pub condition: Option<String>,
}

// Task methods are now implemented in task.rs with cleaner pattern matching

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
// TESTS (v4.6 - TaskAction enum)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // WORKFLOW PARSING - v4.6 nested format
    // ==========================================================================

    #[test]
    fn test_parse_workflow_v46() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a helpful assistant."

tasks:
  - id: greet
    agent:
      prompt: "Say hello in French."

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.agent.model, "claude-sonnet-4-5");
        assert_eq!(workflow.tasks.len(), 1);
        assert_eq!(workflow.tasks[0].keyword(), TaskKeyword::Agent);
        assert_eq!(workflow.tasks[0].category(), TaskCategory::Context);
        assert_eq!(workflow.tasks[0].prompt(), "Say hello in French.");
    }

    #[test]
    fn test_parse_all_7_keywords_v46() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: t1
    agent:
      prompt: "Agent task"
  - id: t2
    subagent:
      prompt: "Subagent task"
  - id: t3
    shell:
      command: "npm test"
  - id: t4
    http:
      url: "https://api.example.com"
  - id: t5
    mcp:
      reference: "server::tool"
  - id: t6
    function:
      reference: "path::fn"
  - id: t7
    llm:
      prompt: "Classify"

flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks.len(), 7);
        assert_eq!(workflow.tasks[0].keyword(), TaskKeyword::Agent);
        assert_eq!(workflow.tasks[1].keyword(), TaskKeyword::Subagent);
        assert_eq!(workflow.tasks[2].keyword(), TaskKeyword::Shell);
        assert_eq!(workflow.tasks[3].keyword(), TaskKeyword::Http);
        assert_eq!(workflow.tasks[4].keyword(), TaskKeyword::Mcp);
        assert_eq!(workflow.tasks[5].keyword(), TaskKeyword::Function);
        assert_eq!(workflow.tasks[6].keyword(), TaskKeyword::Llm);
    }

    #[test]
    fn test_task_categories_v46() {
        assert_eq!(TaskCategory::from(TaskKeyword::Agent), TaskCategory::Context);
        assert_eq!(TaskCategory::from(TaskKeyword::Subagent), TaskCategory::Isolated);
        assert_eq!(TaskCategory::from(TaskKeyword::Shell), TaskCategory::Tool);
        assert_eq!(TaskCategory::from(TaskKeyword::Http), TaskCategory::Tool);
        assert_eq!(TaskCategory::from(TaskKeyword::Mcp), TaskCategory::Tool);
        assert_eq!(TaskCategory::from(TaskKeyword::Function), TaskCategory::Tool);
        assert_eq!(TaskCategory::from(TaskKeyword::Llm), TaskCategory::Tool);
    }

    #[test]
    fn test_bridge_pattern_v46() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"

tasks:
  - id: worker
    subagent:
      prompt: "Work in isolation"
  - id: bridge
    function:
      reference: "aggregate::collect"
  - id: router
    agent:
      prompt: "Route the results"

flows:
  - source: worker
    target: bridge
  - source: bridge
    target: router
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.tasks[0].category(), TaskCategory::Isolated);
        assert_eq!(workflow.tasks[1].category(), TaskCategory::Tool);
        assert_eq!(workflow.tasks[2].category(), TaskCategory::Context);
    }

    // ==========================================================================
    // WORKFLOW HELPERS
    // ==========================================================================

    #[test]
    fn test_workflow_get_task() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: task_a
    agent:
      prompt: "A"
  - id: task_b
    shell:
      command: "echo B"
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert!(workflow.get_task("task_a").is_some());
        assert!(workflow.get_task("task_b").is_some());
        assert!(workflow.get_task("nonexistent").is_none());
    }

    #[test]
    fn test_workflow_task_ids() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - id: first
    agent:
      prompt: "First"
  - id: second
    agent:
      prompt: "Second"
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let ids: Vec<&str> = workflow.task_ids().collect();
        assert_eq!(ids, vec!["first", "second"]);
    }

    // ==========================================================================
    // DISPLAY TRAITS
    // ==========================================================================

    #[test]
    fn test_keyword_display() {
        assert_eq!(format!("{}", TaskKeyword::Agent), "agent");
        assert_eq!(format!("{}", TaskKeyword::Subagent), "subagent");
        assert_eq!(format!("{}", TaskKeyword::Shell), "shell");
        assert_eq!(format!("{}", TaskKeyword::Http), "http");
        assert_eq!(format!("{}", TaskKeyword::Mcp), "mcp");
        assert_eq!(format!("{}", TaskKeyword::Function), "function");
        assert_eq!(format!("{}", TaskKeyword::Llm), "llm");
    }

    #[test]
    fn test_category_display() {
        assert_eq!(format!("{}", TaskCategory::Context), "agent:");
        assert_eq!(format!("{}", TaskCategory::Isolated), "subagent:");
        assert_eq!(format!("{}", TaskCategory::Tool), "tool");
    }

    // ==========================================================================
    // AGENT CONFIG
    // ==========================================================================

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
    fn test_default_mode_is_strict() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks: []
flows: []
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(workflow.agent.mode, ExecutionMode::Strict);
    }

    // ==========================================================================
    // ERROR CASES
    // ==========================================================================

    #[test]
    fn test_parse_missing_agent() {
        let yaml = r#"
tasks: []
flows: []
"#;
        let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_model() {
        let yaml = r#"
agent:
  systemPrompt: "Test"
tasks: []
flows: []
"#;
        let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_task_missing_id() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "Test"
tasks:
  - agent:
      prompt: "No ID"
flows: []
"#;
        let result: Result<Workflow, _> = serde_yaml::from_str(yaml);
        assert!(result.is_err());
    }
}
