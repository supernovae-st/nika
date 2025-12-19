//! Rule loading from YAML files
//!
//! Loads validation rules from spec/validation/*.yaml files.
//! These rules are the single source of truth for validation logic.

use serde::Deserialize;
use std::collections::HashMap;

/// Paradigm definition (from paradigm-matrix.yaml)
#[derive(Debug, Clone, Deserialize)]
pub struct ParadigmDef {
    pub symbol: String,
    pub description: String,
    pub color: String,
    pub border: String,
    pub sdk_mapping: String,
    pub token_cost: String,
}

/// Complete paradigm matrix (from paradigm-matrix.yaml)
#[derive(Debug, Deserialize)]
pub struct ParadigmMatrix {
    pub version: String,
    pub description: String,
    pub paradigms: HashMap<String, ParadigmDef>,
    pub connections: HashMap<String, HashMap<String, bool>>,
}

impl ParadigmMatrix {
    /// Load from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Check if a connection between paradigms is allowed
    pub fn is_connection_allowed(&self, source: &str, target: &str) -> bool {
        self.connections
            .get(source)
            .and_then(|targets| targets.get(target))
            .copied()
            .unwrap_or(false)
    }

    /// Get paradigm symbol (e.g., "🧠" for context)
    pub fn get_symbol(&self, paradigm: &str) -> Option<&str> {
        self.paradigms.get(paradigm).map(|p| p.symbol.as_str())
    }
}

/// Visual node types that are Studio-only (not part of execution standard)
/// These nodes are allowed in workflows but produce warnings
/// When Studio layer is built, this could move to a config file
pub const VISUAL_NODE_TYPES: &[&str] = &[
    "startNode",   // Entry point marker (visual only)
    "commentNode", // Comments (visual only)
    "groupNode",   // Grouping (visual only)
];

/// Node types registry (from node-types.yaml)
#[derive(Debug, Deserialize)]
pub struct NodeTypes {
    pub version: String,
    pub description: String,
    /// Fast lookup: node_type -> paradigm
    pub lookup: HashMap<String, String>,
}

impl NodeTypes {
    /// Load from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Get the paradigm for a node type
    pub fn get_paradigm(&self, node_type: &str) -> Option<&str> {
        self.lookup.get(node_type).map(|s| s.as_str())
    }

    /// Check if a node type is valid (execution node)
    pub fn is_valid_type(&self, node_type: &str) -> bool {
        self.lookup.contains_key(node_type)
    }

    /// Check if a node type is a visual-only Studio node
    pub fn is_visual_type(&self, node_type: &str) -> bool {
        VISUAL_NODE_TYPES.contains(&node_type)
    }

    /// Check if a node type is known (either execution or visual)
    pub fn is_known_type(&self, node_type: &str) -> bool {
        self.is_valid_type(node_type) || self.is_visual_type(node_type)
    }

    /// Find similar node types (for "did you mean?" suggestions)
    pub fn find_similar(&self, node_type: &str, max_results: usize) -> Vec<&str> {
        let lower = node_type.to_lowercase();
        self.lookup
            .keys()
            .filter(|k| k.to_lowercase().contains(&lower) || lower.contains(&k.to_lowercase()))
            .take(max_results)
            .map(|s| s.as_str())
            .collect()
    }

    /// Merge custom node paradigms into the lookup table
    pub fn merge_custom_nodes(&mut self, custom: HashMap<String, String>) {
        self.lookup.extend(custom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real YAML from spec/validation/paradigm-matrix.yaml (subset for testing)
    const PARADIGM_MATRIX_YAML: &str = r#"
version: "1.0"
description: "Connection rules between Nika paradigms"

paradigms:
  context:
    symbol: "🧠"
    description: "LLM-powered nodes"
    color: "violet"
    border: "solid"
    sdk_mapping: "query()"
    token_cost: "500+"
  isolated:
    symbol: "🤖"
    description: "Separate context window"
    color: "amber"
    border: "dashed"
    sdk_mapping: "agents param"
    token_cost: "8000+"
  data:
    symbol: "⚡"
    description: "Deterministic operations"
    color: "cyan"
    border: "thin"
    sdk_mapping: "tool definition"
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

    // Real YAML from spec/validation/node-types.yaml (subset for testing)
    const NODE_TYPES_YAML: &str = r#"
version: "1.0"
description: "All 54 Nika node types"

lookup:
  context: context
  isolated: isolated
  data: data
  nika/router: data
  nika/transform: data
  nika/summarize: isolated
  nika/analyze: isolated
"#;

    #[test]
    fn test_load_paradigm_matrix() {
        let matrix =
            ParadigmMatrix::from_yaml(PARADIGM_MATRIX_YAML).expect("Should parse paradigm matrix");

        assert_eq!(matrix.version, "1.0");
        assert_eq!(matrix.paradigms.len(), 3);
    }

    #[test]
    fn test_paradigm_symbols() {
        let matrix = ParadigmMatrix::from_yaml(PARADIGM_MATRIX_YAML).unwrap();

        assert_eq!(matrix.get_symbol("context"), Some("🧠"));
        assert_eq!(matrix.get_symbol("isolated"), Some("🤖"));
        assert_eq!(matrix.get_symbol("data"), Some("⚡"));
        assert_eq!(matrix.get_symbol("unknown"), None);
    }

    #[test]
    fn test_connection_rules() {
        let matrix = ParadigmMatrix::from_yaml(PARADIGM_MATRIX_YAML).unwrap();

        // Valid connections
        assert!(matrix.is_connection_allowed("context", "context")); // 🧠 → 🧠 ✅
        assert!(matrix.is_connection_allowed("context", "isolated")); // 🧠 → 🤖 ✅
        assert!(matrix.is_connection_allowed("data", "context")); // ⚡ → 🧠 ✅
        assert!(matrix.is_connection_allowed("isolated", "data")); // 🤖 → ⚡ ✅

        // Invalid connections (THE KEY RULES!)
        assert!(!matrix.is_connection_allowed("isolated", "context")); // 🤖 → 🧠 ❌
        assert!(!matrix.is_connection_allowed("isolated", "isolated")); // 🤖 → 🤖 ❌
    }

    #[test]
    fn test_load_node_types() {
        let types = NodeTypes::from_yaml(NODE_TYPES_YAML).expect("Should parse node types");

        assert_eq!(types.version, "1.0");
        assert!(types.lookup.len() >= 7);
    }

    #[test]
    fn test_get_paradigm() {
        let types = NodeTypes::from_yaml(NODE_TYPES_YAML).unwrap();

        assert_eq!(types.get_paradigm("context"), Some("context"));
        assert_eq!(types.get_paradigm("isolated"), Some("isolated"));
        assert_eq!(types.get_paradigm("data"), Some("data"));
        assert_eq!(types.get_paradigm("unknownNode"), None);
    }

    #[test]
    fn test_is_valid_type() {
        let types = NodeTypes::from_yaml(NODE_TYPES_YAML).unwrap();

        assert!(types.is_valid_type("context"));
        assert!(types.is_valid_type("nika/transform"));
        assert!(!types.is_valid_type("madeUpNode"));
    }

    #[test]
    fn test_find_similar() {
        let types = NodeTypes::from_yaml(NODE_TYPES_YAML).unwrap();

        // "cont" should find "context"
        let similar = types.find_similar("cont", 5);
        assert!(similar.contains(&"context"));

        // "transform" should find "nika/transform"
        let similar = types.find_similar("transform", 5);
        assert!(similar.contains(&"nika/transform"));
    }

    #[test]
    fn test_merge_custom_nodes() {
        let mut types = NodeTypes::from_yaml(NODE_TYPES_YAML).unwrap();

        let custom_lookup: HashMap<String, String> = [
            ("slackNode".to_string(), "data".to_string()),
            ("gptNode".to_string(), "isolated".to_string()),
        ]
        .iter()
        .cloned()
        .collect();

        types.merge_custom_nodes(custom_lookup);

        assert_eq!(types.get_paradigm("slackNode"), Some("data"));
        assert_eq!(types.get_paradigm("gptNode"), Some("isolated"));
        assert_eq!(types.get_paradigm("context"), Some("context"));
    }
}
