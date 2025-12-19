//! Core workflow types for Nika
//!
//! These types represent the structure of a .nika.yaml workflow file.
//! They are designed to match the YAML structure exactly for easy parsing.

use serde::Deserialize;
use std::collections::HashMap;

/// Main workflow structure
/// Represents the root of a .nika.yaml file
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workflow {
    pub main_agent: MainAgent,
    #[serde(default)]
    pub nodes: Vec<Node>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

/// Main Agent configuration
/// The invisible orchestrator (the "chicken" 🐔)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MainAgent {
    pub model: String,
    pub system_prompt: String,
    #[serde(default)]
    pub allowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub disallowed_tools: Option<Vec<String>>,
    #[serde(default)]
    pub max_turns: Option<u32>,
}

/// A workflow node
/// Building block with id, type, and optional data
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub id: String,
    #[serde(rename = "type")]
    pub node_type: String,
    #[serde(default)]
    pub data: Option<HashMap<String, serde_yaml::Value>>,
}

/// An edge connecting two nodes
#[derive(Debug, Deserialize)]
pub struct Edge {
    pub source: String,
    pub target: String,
    #[serde(rename = "sourceHandle")]
    #[serde(default)]
    pub source_handle: Option<String>,
    #[serde(rename = "targetHandle")]
    #[serde(default)]
    pub target_handle: Option<String>,
}

impl Workflow {
    /// Get a node by its ID
    pub fn get_node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// Iterator over all node IDs
    pub fn node_ids(&self) -> impl Iterator<Item = &str> {
        self.nodes.iter().map(|n| n.id.as_str())
    }
}
