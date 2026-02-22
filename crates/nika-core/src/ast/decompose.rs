//! Decompose Module - Runtime DAG expansion via MCP traversal (v0.5)
//!
//! The `decompose:` modifier enables dynamic task expansion based on
//! semantic graph traversal. Instead of static `for_each` arrays,
//! decompose queries NovaNet to discover iteration items at runtime.
//!
//! # Example
//!
//! ```yaml
//! tasks:
//!   - id: generate_all
//!     decompose:
//!       strategy: semantic
//!       traverse: HAS_CHILD
//!       source: $entity
//!     infer: "Generate for {{use.item}}"
//! ```

use serde::{Deserialize, Serialize};

/// Decomposition strategy for runtime DAG expansion
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DecomposeStrategy {
    /// Use novanet_traverse with arc to discover items
    #[default]
    Semantic,
    /// Use literal array from source binding
    Static,
    /// Recursive decomposition (nested traversal)
    Nested,
}

/// Specification for runtime decomposition (v0.5)
///
/// Decompose expands a task at runtime into multiple iterations
/// based on graph traversal or static arrays.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecomposeSpec {
    /// Strategy for discovering iteration items
    #[serde(default)]
    pub strategy: DecomposeStrategy,
    /// Arc name to traverse (e.g., "HAS_CHILD", "HAS_NATIVE")
    pub traverse: String,
    /// Source binding expression (e.g., "$entity", "{{use.entity_key}}")
    pub source: String,
    /// MCP server to use for traversal (defaults to "novanet")
    #[serde(default)]
    pub mcp_server: Option<String>,
    /// Maximum items to expand (optional limit)
    #[serde(default)]
    pub max_items: Option<usize>,
    /// Maximum recursion depth for nested strategy (default: 3)
    #[serde(default)]
    pub max_depth: Option<usize>,
}

impl DecomposeSpec {
    /// Get the MCP server name (defaults to "novanet")
    pub fn mcp_server(&self) -> &str {
        self.mcp_server.as_deref().unwrap_or("novanet")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decompose_strategy_default_is_semantic() {
        let strategy = DecomposeStrategy::default();
        assert_eq!(strategy, DecomposeStrategy::Semantic);
    }

    #[test]
    fn test_decompose_spec_parses_minimal() {
        let yaml = r#"
traverse: HAS_CHILD
source: $entity
"#;
        let spec: DecomposeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.strategy, DecomposeStrategy::Semantic);
        assert_eq!(spec.traverse, "HAS_CHILD");
        assert_eq!(spec.source, "$entity");
        assert_eq!(spec.mcp_server(), "novanet");
        assert!(spec.max_items.is_none());
    }

    #[test]
    fn test_decompose_spec_parses_full() {
        let yaml = r#"
strategy: nested
traverse: HAS_NATIVE
source: "{{use.entity_key}}"
mcp_server: custom_mcp
max_items: 10
"#;
        let spec: DecomposeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.strategy, DecomposeStrategy::Nested);
        assert_eq!(spec.traverse, "HAS_NATIVE");
        assert_eq!(spec.source, "{{use.entity_key}}");
        assert_eq!(spec.mcp_server(), "custom_mcp");
        assert_eq!(spec.max_items, Some(10));
    }

    #[test]
    fn test_decompose_spec_static_strategy() {
        let yaml = r#"
strategy: static
traverse: DUMMY
source: $locales
"#;
        let spec: DecomposeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.strategy, DecomposeStrategy::Static);
    }

    #[test]
    fn test_decompose_spec_serializes() {
        let spec = DecomposeSpec {
            strategy: DecomposeStrategy::Semantic,
            traverse: "HAS_CHILD".to_string(),
            source: "$entity".to_string(),
            mcp_server: None,
            max_items: Some(5),
            max_depth: None,
        };
        let yaml = serde_yaml::to_string(&spec).unwrap();
        assert!(yaml.contains("traverse: HAS_CHILD"));
        assert!(yaml.contains("source: $entity"));
        assert!(yaml.contains("max_items: 5"));
    }
}
