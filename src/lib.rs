//! Nika CLI Library (v3)
//!
//! Workflow validation and execution for the Nika specification.
//! Architecture v3: 2 task types (agent + action) with scope attribute.

pub mod init;
pub mod runner;
pub mod tui;
pub mod validator;
pub mod workflow;

// Re-export main types
pub use init::{init_project, InitResult};
pub use runner::{RunResult, Runner, TaskResult};
pub use validator::{ValidationError, ValidationResult, Validator};
pub use workflow::{
    ConnectionKey, ExecutionMode, Flow, MainAgent, Scope, Task, TaskConfig, TaskType, Workflow,
};

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
        assert_eq!(workflow.tasks[0].prompt, Some("Say hello in French.".into()));
    }

    #[test]
    fn test_validate_hello_world() {
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
        let result = Validator::new().validate(&workflow, "test.nika.yaml");
        assert!(result.is_valid());
    }

    #[test]
    fn test_translation_pipeline() {
        let yaml = r#"
mainAgent:
  model: claude-sonnet-4-5
  systemPrompt: "You are a translation orchestrator."
  mode: strict

tasks:
  - id: read-source
    type: action
    run: Read
    file: "content/source.md"

  - id: translate-fr
    type: agent
    scope: isolated
    prompt: "Translate to French."

  - id: translate-es
    type: agent
    scope: isolated
    prompt: "Translate to Spanish."

  - id: collect
    type: action
    run: aggregate
    format: json

  - id: validate
    type: agent
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
