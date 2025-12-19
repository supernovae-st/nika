//! Custom node template loading
//!
//! Reads .nika/nodes/*.node.yaml files and extracts paradigm information
//! for validation.

use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Metadata section of custom node
#[derive(Debug, Clone, Deserialize)]
pub struct CustomNodeMetadata {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
}

/// Custom node template definition
#[derive(Debug, Clone, Deserialize)]
pub struct CustomNode {
    pub name: String,
    pub description: String,
    pub extends: String,
    pub version: String,
    pub metadata: CustomNodeMetadata,
}

/// CORE primitives that custom nodes can extend
pub const CORE_PRIMITIVES: &[&str] = &["context", "isolated", "data"];

/// Error types for custom node loading and validation
#[derive(Debug, thiserror::Error)]
pub enum CustomNodeError {
    #[error("Custom node '{node_name}' extends '{extends}' which is not a CORE primitive. Must extend one of: {valid_options:?}")]
    InvalidExtends {
        node_name: String,
        extends: String,
        valid_options: Vec<String>,
    },

    #[error("IO error reading {0}: {1}")]
    IoError(PathBuf, String),

    #[error("YAML parse error in {0}: {1}")]
    ParseError(PathBuf, String),

    #[error("Glob pattern error: {0}")]
    GlobError(String),
}

impl CustomNode {
    /// Parse from YAML string
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    /// Get paradigm derived from extends field
    /// No need for metadata.paradigm - it's derived automatically
    pub fn get_paradigm(&self) -> Option<&'static str> {
        match self.extends.as_str() {
            "context" => Some("context"),
            "isolated" => Some("isolated"),
            "data" => Some("data"),
            _ => None,
        }
    }

    /// Validate that extends field is a CORE primitive
    pub fn validate_extends(&self) -> Result<(), CustomNodeError> {
        if CORE_PRIMITIVES.contains(&self.extends.as_str()) {
            Ok(())
        } else {
            Err(CustomNodeError::InvalidExtends {
                node_name: self.name.clone(),
                extends: self.extends.clone(),
                valid_options: CORE_PRIMITIVES.iter().map(|s| s.to_string()).collect(),
            })
        }
    }
}

/// Loads custom node definitions from .nika/nodes/ directory
pub struct CustomNodeLoader {
    base_path: PathBuf,
}

impl CustomNodeLoader {
    pub fn new(base_path: impl AsRef<Path>) -> Self {
        Self {
            base_path: base_path.as_ref().to_path_buf(),
        }
    }

    /// Scan for .node.yaml files and parse them
    pub fn scan(&self) -> Result<Vec<CustomNode>, CustomNodeError> {
        let pattern = self
            .base_path
            .join(".nika/nodes/*.node.yaml")
            .to_string_lossy()
            .to_string();

        let mut nodes = Vec::new();

        // Check if the directory exists; if not, return empty list
        let nodes_dir = self.base_path.join(".nika/nodes");
        if !nodes_dir.exists() {
            return Ok(nodes);
        }

        let entries =
            glob::glob(&pattern).map_err(|e| CustomNodeError::GlobError(e.to_string()))?;

        for entry in entries {
            match entry {
                Ok(path) => {
                    let content = std::fs::read_to_string(&path)
                        .map_err(|e| CustomNodeError::IoError(path.clone(), e.to_string()))?;
                    let node = CustomNode::from_yaml(&content)
                        .map_err(|e| CustomNodeError::ParseError(path.clone(), e.to_string()))?;
                    node.validate_extends()?;
                    nodes.push(node);
                }
                Err(e) => {
                    return Err(CustomNodeError::GlobError(e.to_string()));
                }
            }
        }

        Ok(nodes)
    }

    /// Get paradigm lookup table from custom nodes
    pub fn get_paradigm_lookup(&self) -> Result<HashMap<String, String>, CustomNodeError> {
        let nodes = self.scan()?;
        Ok(nodes
            .into_iter()
            .filter_map(|n| n.get_paradigm().map(|p| (n.name, p.to_string())))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_CUSTOM_NODE: &str = r#"
name: slackNode
description: Send message to Slack
extends: data
version: 1.0.0
metadata:
  category: custom
"#;

    #[test]
    fn test_parse_custom_node() {
        let node =
            CustomNode::from_yaml(VALID_CUSTOM_NODE).expect("Should parse valid custom node");

        assert_eq!(node.name, "slackNode");
        assert_eq!(node.extends, "data");
        assert_eq!(node.get_paradigm(), Some("data"));
    }

    #[test]
    fn test_validate_extends_core_only() {
        let node = CustomNode::from_yaml(VALID_CUSTOM_NODE).unwrap();
        assert!(node.validate_extends().is_ok());

        // Invalid: extends non-CORE node
        let invalid = CustomNode::from_yaml(
            r#"
name: badNode
description: Invalid
extends: nika/router
version: 1.0.0
metadata: {}
"#,
        )
        .unwrap();

        let err = invalid.validate_extends().unwrap_err();
        assert!(err.to_string().contains("CORE primitive"));
    }

    #[test]
    fn test_load_custom_nodes_from_dir() {
        // Create temp directory with test files
        let temp_dir = tempfile::tempdir().unwrap();
        let nodes_dir = temp_dir.path().join(".nika/nodes");
        std::fs::create_dir_all(&nodes_dir).unwrap();

        std::fs::write(nodes_dir.join("slack.node.yaml"), VALID_CUSTOM_NODE).unwrap();

        let loader = CustomNodeLoader::new(temp_dir.path());
        let nodes = loader.scan().unwrap();

        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].name, "slackNode");
    }

    #[test]
    fn test_get_paradigm_lookup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let nodes_dir = temp_dir.path().join(".nika/nodes");
        std::fs::create_dir_all(&nodes_dir).unwrap();

        std::fs::write(nodes_dir.join("slack.node.yaml"), VALID_CUSTOM_NODE).unwrap();

        std::fs::write(
            nodes_dir.join("gpt.node.yaml"),
            r#"
name: gptNode
description: Call GPT model
extends: isolated
version: 1.0.0
metadata:
  category: custom
"#,
        )
        .unwrap();

        let loader = CustomNodeLoader::new(temp_dir.path());
        let lookup = loader.get_paradigm_lookup().unwrap();

        assert_eq!(lookup.get("slackNode"), Some(&"data".to_string()));
        assert_eq!(lookup.get("gptNode"), Some(&"isolated".to_string()));
    }
}
