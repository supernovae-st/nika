//! Include specification for DAG fusion (v0.9)
//!
//! The `include:` block merges tasks from external workflows into the main DAG.
//! Included tasks share the same DataStore as the parent workflow.
//!
//! # Example
//!
//! ```yaml
//! include:
//!   - path: ./lib/seo-tasks.nika.yaml
//!     prefix: seo_
//!   - path: ./lib/common.nika.yaml
//! ```

use serde::Deserialize;

/// Include specification for DAG fusion (v0.9)
///
/// Merges tasks from an external workflow into the main DAG at parse time.
#[derive(Debug, Clone, Deserialize)]
pub struct IncludeSpec {
    /// Path to the workflow file to include (relative to parent workflow)
    pub path: String,

    /// Optional prefix for all task IDs from this include
    ///
    /// Prevents ID collisions when including multiple workflows.
    /// Example: prefix "seo_" transforms task "analyze" to "seo_analyze"
    #[serde(default)]
    pub prefix: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_include_spec_parse_minimal() {
        let yaml = r#"
path: ./lib/tasks.nika.yaml
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.path, "./lib/tasks.nika.yaml");
        assert!(spec.prefix.is_none());
    }

    #[test]
    fn test_include_spec_parse_with_prefix() {
        let yaml = r#"
path: ./lib/seo-tasks.nika.yaml
prefix: seo_
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.path, "./lib/seo-tasks.nika.yaml");
        assert_eq!(spec.prefix, Some("seo_".to_string()));
    }

    #[test]
    fn test_include_spec_parse_array() {
        let yaml = r#"
- path: ./lib/seo.nika.yaml
  prefix: seo_
- path: ./lib/common.nika.yaml
"#;
        let specs: Vec<IncludeSpec> = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].prefix, Some("seo_".to_string()));
        assert!(specs[1].prefix.is_none());
    }

    #[test]
    fn test_include_spec_empty_prefix() {
        let yaml = r#"
path: ./lib/tasks.nika.yaml
prefix: ""
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.prefix, Some(String::new()));
    }
}
