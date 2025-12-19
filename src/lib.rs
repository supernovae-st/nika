//! Nika validation library
//!
//! This library provides workflow validation for the Nika specification.
//! Rules are loaded from YAML files (spec/validation/*.yaml) making them
//! portable across implementations (Rust CLI, future Web version).

pub mod auth;
pub mod custom_nodes;
pub mod errors;
pub mod publish;
pub mod rules;
pub mod validator;
pub mod validators;
pub mod workflow;

// Re-export main types
pub use errors::{Severity, ValidationError, ValidationLayer, ValidationResult};
pub use rules::{NodeTypes, ParadigmMatrix};
pub use validator::Validator;
pub use validators::{validate_edges, validate_graph, validate_nodes, validate_paradigms};
pub use workflow::{Edge, Node, Workflow};

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal valid workflow YAML
    const MINIMAL_WORKFLOW: &str = r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "You are a helpful assistant."
nodes: []
edges: []
"#;

    /// Workflow with nodes and edges
    const WORKFLOW_WITH_NODES: &str = r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "You are a helpful assistant."
nodes:
  - id: prompt1
    type: context
    data:
      prompt: "Analyze this"
  - id: transform1
    type: nika/transform
edges:
  - source: prompt1
    target: transform1
"#;

    #[test]
    fn test_parse_minimal_workflow() {
        let workflow: Workflow =
            serde_yaml::from_str(MINIMAL_WORKFLOW).expect("Should parse minimal workflow");

        assert_eq!(workflow.main_agent.model, "claude-sonnet-4-5");
        assert!(workflow.nodes.is_empty());
        assert!(workflow.edges.is_empty());
    }

    #[test]
    fn test_parse_workflow_with_nodes() {
        let workflow: Workflow =
            serde_yaml::from_str(WORKFLOW_WITH_NODES).expect("Should parse workflow with nodes");

        assert_eq!(workflow.nodes.len(), 2);
        assert_eq!(workflow.edges.len(), 1);

        // Check first node
        let node1 = &workflow.nodes[0];
        assert_eq!(node1.id, "prompt1");
        assert_eq!(node1.node_type, "context");

        // Check edge
        let edge = &workflow.edges[0];
        assert_eq!(edge.source, "prompt1");
        assert_eq!(edge.target, "transform1");
    }

    #[test]
    fn test_workflow_get_node() {
        let workflow: Workflow =
            serde_yaml::from_str(WORKFLOW_WITH_NODES).expect("Should parse workflow");

        let node = workflow.get_node("prompt1");
        assert!(node.is_some());
        assert_eq!(node.unwrap().node_type, "context");

        let missing = workflow.get_node("nonexistent");
        assert!(missing.is_none());
    }

    #[test]
    fn test_parse_node_with_data() {
        let workflow: Workflow =
            serde_yaml::from_str(WORKFLOW_WITH_NODES).expect("Should parse workflow");

        let node = workflow.get_node("prompt1").unwrap();
        assert!(node.data.is_some());

        let data = node.data.as_ref().unwrap();
        assert!(data.contains_key("prompt"));
    }

    #[test]
    fn test_parse_invalid_yaml_fails() {
        let invalid = "not: valid: yaml: here";
        let result: Result<Workflow, _> = serde_yaml::from_str(invalid);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_missing_main_agent_fails() {
        let missing_main_agent = r#"
nodes: []
edges: []
"#;
        let result: Result<Workflow, _> = serde_yaml::from_str(missing_main_agent);
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_all_node_ids() {
        let workflow: Workflow =
            serde_yaml::from_str(WORKFLOW_WITH_NODES).expect("Should parse workflow");

        let ids: Vec<&str> = workflow.node_ids().collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"prompt1"));
        assert!(ids.contains(&"transform1"));
    }
}
