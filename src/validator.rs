//! Unified Validator
//!
//! Orchestrates all 5 validation layers into a single pipeline.
//! Loads rules from YAML files for portable validation.

use crate::custom_nodes::CustomNodeLoader;
use crate::errors::ValidationResult;
use crate::rules::{NodeTypes, ParadigmMatrix};
use crate::validators::{validate_edges, validate_graph, validate_nodes, validate_paradigms};
use crate::workflow::Workflow;
use anyhow::{Context, Result};
use std::path::Path;

/// The unified validator that runs all 5 layers
pub struct Validator {
    node_types: NodeTypes,
    paradigm_matrix: ParadigmMatrix,
}

impl Validator {
    /// Create validator with embedded rules (for tests)
    pub fn new(node_types: NodeTypes, paradigm_matrix: ParadigmMatrix) -> Self {
        Self {
            node_types,
            paradigm_matrix,
        }
    }

    /// Load validator from YAML rule files
    pub fn from_files(node_types_path: &Path, paradigm_matrix_path: &Path) -> Result<Self> {
        let node_types_yaml = std::fs::read_to_string(node_types_path)
            .with_context(|| format!("Failed to read node types from {:?}", node_types_path))?;

        let paradigm_matrix_yaml =
            std::fs::read_to_string(paradigm_matrix_path).with_context(|| {
                format!(
                    "Failed to read paradigm matrix from {:?}",
                    paradigm_matrix_path
                )
            })?;

        let node_types = NodeTypes::from_yaml(&node_types_yaml)
            .with_context(|| "Failed to parse node-types.yaml")?;

        let paradigm_matrix = ParadigmMatrix::from_yaml(&paradigm_matrix_yaml)
            .with_context(|| "Failed to parse paradigm-matrix.yaml")?;

        Ok(Self::new(node_types, paradigm_matrix))
    }

    /// Load validator from spec/validation/ directory
    pub fn from_spec_dir(spec_dir: &Path) -> Result<Self> {
        let node_types_path = spec_dir.join("node-types.yaml");
        let paradigm_matrix_path = spec_dir.join("paradigm-matrix.yaml");
        Self::from_files(&node_types_path, &paradigm_matrix_path)
    }

    /// Load and merge custom nodes from project directory
    /// Scans .nika/nodes/*.node.yaml and adds them to the lookup table
    pub fn with_custom_nodes(mut self, project_dir: &Path) -> Result<Self> {
        let loader = CustomNodeLoader::new(project_dir);
        let custom_lookup = loader
            .get_paradigm_lookup()
            .with_context(|| format!("Failed to load custom nodes from {:?}", project_dir))?;

        if !custom_lookup.is_empty() {
            self.node_types.merge_custom_nodes(custom_lookup);
        }

        Ok(self)
    }

    /// Validate a workflow through all 5 layers
    pub fn validate(&self, workflow: &Workflow, file_path: &str) -> ValidationResult {
        let mut result = ValidationResult::new(file_path);
        result.node_count = workflow.nodes.len();
        result.edge_count = workflow.edges.len();

        // Layer 1: Schema (handled by serde during parsing)
        // Parsing errors are caught before we get here

        // Layer 2: Nodes
        for error in validate_nodes(workflow, &self.node_types) {
            result.add_error(error);
        }

        // Layer 3: Edges
        for error in validate_edges(workflow) {
            result.add_error(error);
        }

        // Layer 4: Paradigms (THE KEY RULE!)
        for error in validate_paradigms(workflow, &self.node_types, &self.paradigm_matrix) {
            result.add_error(error);
        }

        // Layer 5: Graph (warnings)
        for warning in validate_graph(workflow) {
            result.add_error(warning); // add_error handles severity
        }

        result
    }

    /// Validate a workflow file from path
    pub fn validate_file(&self, path: &Path) -> Result<ValidationResult> {
        let yaml = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read workflow file {:?}", path))?;

        let workflow: Workflow = serde_yaml::from_str(&yaml)
            .with_context(|| format!("Failed to parse workflow YAML from {:?}", path))?;

        let file_path = path.to_string_lossy().to_string();
        Ok(self.validate(&workflow, &file_path))
    }

    /// Get reference to node types
    pub fn node_types(&self) -> &NodeTypes {
        &self.node_types
    }

    /// Get reference to paradigm matrix
    pub fn paradigm_matrix(&self) -> &ParadigmMatrix {
        &self.paradigm_matrix
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::ValidationError;

    const PARADIGM_MATRIX_YAML: &str = r#"
version: "1.0"
description: "Test matrix"
paradigms:
  context:
    symbol: "🧠"
    description: "Context"
    color: "violet"
    border: "solid"
    sdk_mapping: "query()"
    token_cost: "500+"
  isolated:
    symbol: "🤖"
    description: "Isolated"
    color: "amber"
    border: "dashed"
    sdk_mapping: "agents"
    token_cost: "8000+"
  data:
    symbol: "⚡"
    description: "Data"
    color: "cyan"
    border: "thin"
    sdk_mapping: "@tool"
    token_cost: "0"
connections:
  context:
    context: true
    data: true
    isolated: true
  data:
    context: true
    data: true
    isolated: true
  isolated:
    context: false
    data: true
    isolated: false
"#;

    const NODE_TYPES_YAML: &str = r#"
version: "1.0"
description: "Test types"
lookup:
  context: context
  isolated: isolated
  data: data
  nika/transform: data
  nika/summarize: isolated
"#;

    fn make_validator() -> Validator {
        let node_types = NodeTypes::from_yaml(NODE_TYPES_YAML).unwrap();
        let paradigm_matrix = ParadigmMatrix::from_yaml(PARADIGM_MATRIX_YAML).unwrap();
        Validator::new(node_types, paradigm_matrix)
    }

    #[test]
    fn test_validate_valid_workflow() {
        let validator = make_validator();
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: transform1
    type: nika/transform
edges:
  - source: prompt1
    target: transform1
"#,
        )
        .unwrap();

        let result = validator.validate(&workflow, "test.nika.yaml");
        assert!(
            result.is_valid(),
            "Expected valid workflow: {:?}",
            result.errors
        );
        assert_eq!(result.node_count, 2);
        assert_eq!(result.edge_count, 1);
    }

    #[test]
    fn test_validate_invalid_paradigm_connection() {
        let validator = make_validator();
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: prompt1
    type: context
edges:
  - source: expert1
    target: prompt1
"#,
        )
        .unwrap();

        let result = validator.validate(&workflow, "test.nika.yaml");
        assert!(!result.is_valid(), "Expected invalid workflow");
        assert!(
            result
                .errors
                .iter()
                .any(|e| matches!(e, ValidationError::InvalidParadigmConnection { .. })),
            "Expected paradigm connection error"
        );
    }

    #[test]
    fn test_validate_bridge_pattern_valid() {
        let validator = make_validator();
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: expert1
    type: isolated
  - id: bridge
    type: nika/transform
  - id: prompt1
    type: context
edges:
  - source: expert1
    target: bridge
  - source: bridge
    target: prompt1
"#,
        )
        .unwrap();

        let result = validator.validate(&workflow, "test.nika.yaml");
        assert!(
            result.is_valid(),
            "Bridge pattern should be valid: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_multiple_errors() {
        let validator = make_validator();
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: 123invalid
    type: madeUpNode
  - id: prompt1
    type: context
edges:
  - source: nonexistent
    target: prompt1
"#,
        )
        .unwrap();

        let result = validator.validate(&workflow, "test.nika.yaml");
        assert!(!result.is_valid());
        // Should have: invalid ID format, unknown node type, missing edge source
        assert!(
            result.errors.len() >= 3,
            "Expected multiple errors: {:?}",
            result.errors
        );
    }

    #[test]
    fn test_validate_with_warnings() {
        let validator = make_validator();
        let workflow: Workflow = serde_yaml::from_str(
            r#"
mainAgent:
  model: "claude-sonnet-4-5"
  systemPrompt: "Test"
nodes:
  - id: prompt1
    type: context
  - id: orphan
    type: nika/transform
edges: []
"#,
        )
        .unwrap();

        let result = validator.validate(&workflow, "test.nika.yaml");
        // Both nodes are orphans (no edges), but workflow is technically "valid" (no errors)
        assert!(result.has_warnings());
    }
}
