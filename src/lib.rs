//! # Nika CLI Library (v4.6)
//!
//! Workflow validation and execution for the Nika specification.
//!
//! ## Overview
//!
//! Nika is a YAML-based workflow orchestration system designed for AI agent pipelines.
//! This library provides:
//!
//! - **Parsing**: Load and deserialize `.nika.yaml` workflow files
//! - **Validation**: 5-layer validation pipeline with error codes
//! - **Execution**: Run workflows with provider abstraction (Claude, mock)
//! - **TUI**: Terminal interface for workflow visualization
//!
//! ## Architecture v4.6
//!
//! The v4.6 architecture uses **7 keywords** with type inference and performance optimizations:
//!
//! | Keyword | Category | Description |
//! |---------|----------|-------------|
//! | `agent:` | Context | Main agent (shared context) |
//! | `subagent:` | Isolated | Subagent (isolated 200K context) |
//! | `shell:` | Tool | Execute shell command |
//! | `http:` | Tool | HTTP request |
//! | `mcp:` | Tool | MCP server::tool call |
//! | `function:` | Tool | path::functionName call |
//! | `llm:` | Tool | One-shot stateless LLM call |
//!
//! ### Connection Matrix
//!
//! Only 2 connections are blocked (everything else is allowed):
//!
//! - `subagent: → agent:` - BLOCKED (needs bridge via tool)
//! - `subagent: → subagent:` - BLOCKED (cannot spawn from sub)
//!
//! The **bridge pattern** routes data through a tool:
//! `subagent: → function: → agent:` ✓
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use nika::{Workflow, Validator, Runner};
//!
//! // Parse workflow
//! let yaml = std::fs::read_to_string("workflow.nika.yaml")?;
//! let workflow: Workflow = serde_yaml::from_str(&yaml)?;
//!
//! // Validate
//! let validator = Validator::new();
//! let result = validator.validate(&workflow, "workflow.nika.yaml");
//! if !result.is_valid() {
//!     for error in &result.errors {
//!         eprintln!("{}", error);
//!     }
//!     return Err(anyhow::anyhow!("Validation failed"));
//! }
//!
//! // Execute (async - uses "mock" provider for testing, "claude" for production)
//! let runner = Runner::new("mock")?;
//! let run_result = runner.run(&workflow).await?;
//! println!("Completed {} tasks", run_result.tasks_completed);
//! # Ok::<(), anyhow::Error>(())
//! ```
//!
//! ## Modules
//!
//! - [`workflow`] - Core types: Workflow, Task, Flow, TaskKeyword
//! - [`validator`] - 5-layer validation with error codes (NIKA-001 to NIKA-040)
//! - [`runner`] - Workflow execution with context passing
//! - [`provider`] - Provider abstraction (Claude CLI, mock)
//! - [`tui`] - Terminal UI with Ratatui
//! - [`init`] - Project initialization (scaffold new workflow)
//!
//! ## Error Codes
//!
//! All errors use standardized codes for easy troubleshooting:
//!
//! | Range | Layer | Example |
//! |-------|-------|---------|
//! | NIKA-001..009 | Schema | Missing model |
//! | NIKA-010..019 | Task | Invalid keyword |
//! | NIKA-015..019 | Tool Access | Tool not in pool |
//! | NIKA-020..029 | Flow | Missing source/target |
//! | NIKA-030..039 | Connection | Blocked connection |
//! | NIKA-040..049 | Graph | Cycle detected (warning) |

pub mod builders;
pub mod context_pool;
pub mod error;
pub mod init;
pub mod limits;
pub mod provider;
pub mod runner;
pub mod smart_string;
pub mod task;
pub mod template;
pub mod tui;
pub mod types;
pub mod validator;
pub mod workflow;

// Re-export main types from runner module (v4.6 architecture)
pub use init::{init_project, InitResult};
pub use runner::{
    // Agent core
    AgentConfig,
    AgentCore,
    AgentError,
    // Context types
    AgentMessage,
    AgentOutput,
    ContextReader,
    ContextWriter,
    ErrorCategory,
    ErrorContext,
    ExecutionContext,
    GlobalContext,
    IsolatedAgentRunner,
    LocalContext,
    MessageRole,
    RunResult,
    // Workflow runner
    Runner,
    // Runners and results
    SharedAgentRunner,
    SubagentResult,
    TaskResult,
};
pub use validator::{ValidationError, ValidationResult, Validator};
// Re-export error types
pub use error::{FixSuggestion, NikaError};
// Re-export Task types from task
pub use task::{Task, TaskAction, TaskCategory, TaskConfig, TaskKeyword};
// Re-export workflow types
pub use workflow::{Agent, ExecutionMode, Flow, Workflow};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_hello_world_v6() {
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
        assert_eq!(workflow.tasks[0].prompt(), "Say hello in French.");
    }

    #[test]
    fn test_validate_hello_world_v6() {
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
        let result = Validator::new().validate(&workflow, "test.nika.yaml");
        assert!(result.is_valid());
    }

    #[test]
    fn test_translation_pipeline_v6() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a translation orchestrator."
  mode: strict

tasks:
  - id: read-source
    mcp:
      reference: "filesystem::read_file"

  - id: translate-fr
    subagent:
      prompt: "Translate to French."

  - id: translate-es
    subagent:
      prompt: "Translate to Spanish."

  - id: collect
    function:
      reference: "aggregate::merge"

  - id: validate
    agent:
      prompt: "Review translations."

flows:
  - source: read-source
    target: translate-fr
  - source: read-source
    target: translate-es
  - source: translate-fr
    target: collect
  - source: translate-es
    target: collect
  - source: collect
    target: validate
"#;
        let workflow: Workflow = serde_yaml::from_str(yaml).unwrap();
        let result = Validator::new().validate(&workflow, "translation.nika.yaml");
        assert!(
            result.is_valid(),
            "Translation pipeline should be valid: {:?}",
            result.errors
        );
        assert_eq!(workflow.tasks.len(), 5);
        assert_eq!(workflow.flows.len(), 5);
    }
}
