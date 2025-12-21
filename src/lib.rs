//! Nika CLI Library (v4.5)
//!
//! Workflow validation and execution for the Nika specification.
//! Architecture v4.5: 7 keywords with type inference (agent, subagent, shell, http, mcp, function, llm).

pub mod init;
pub mod provider;
pub mod runner;
pub mod tui;
pub mod validator;
pub mod workflow;

// Re-export main types
pub use init::{init_project, InitResult};
pub use runner::{RunResult, Runner, TaskResult};
pub use validator::{ValidationError, ValidationResult, Validator};
pub use workflow::{
    Agent, ConnectionKey, ExecutionMode, Flow, Task, TaskCategory, TaskConfig, TaskKeyword, Workflow,
};

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
        assert_eq!(workflow.tasks[0].agent, Some("Say hello in French.".into()));
    }

    #[test]
    fn test_validate_hello_world_v45() {
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
        let result = Validator::new().validate(&workflow, "test.nika.yaml");
        assert!(result.is_valid());
    }

    #[test]
    fn test_translation_pipeline_v45() {
        let yaml = r#"
agent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a translation orchestrator."
  mode: strict

tasks:
  - id: read-source
    mcp: "filesystem::read_file"

  - id: translate-fr
    subagent: "Translate to French."

  - id: translate-es
    subagent: "Translate to Spanish."

  - id: collect
    function: aggregate::merge

  - id: validate
    agent: "Review translations."

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
