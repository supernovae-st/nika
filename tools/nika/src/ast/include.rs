//! Include specification for DAG fusion (v0.9)
//!
//! The `include:` block merges tasks from external workflows into the main DAG.
//! Included tasks share the same DataStore as the parent workflow.
//!
//! # Example
//!
//! ```yaml
//! include:
//!   # Filesystem path
//!   - path: ./lib/seo-tasks.nika.yaml
//!     prefix: seo_
//!   # Package reference (v0.17)
//!   - pkg: "@workflows/common"
//!     prefix: common_
//! ```

use serde::Deserialize;

/// Include specification for DAG fusion (v0.9+)
///
/// Merges tasks from an external workflow into the main DAG at parse time.
/// Supports both filesystem paths and package references (v0.17).
#[derive(Debug, Clone, Deserialize)]
pub struct IncludeSpec {
    /// Path to the workflow file to include (relative to parent workflow)
    /// Mutually exclusive with `pkg`
    #[serde(default)]
    pub path: Option<String>,

    /// Package reference to include (e.g., "@workflows/seo-audit")
    /// Mutually exclusive with `path` (v0.17)
    #[serde(default)]
    pub pkg: Option<String>,

    /// Optional prefix for all task IDs from this include
    ///
    /// Prevents ID collisions when including multiple workflows.
    /// Example: prefix "seo_" transforms task "analyze" to "seo_analyze"
    #[serde(default)]
    pub prefix: Option<String>,
}

impl IncludeSpec {
    /// Validate that exactly one of `path` or `pkg` is specified
    pub fn validate(&self) -> Result<(), String> {
        match (&self.path, &self.pkg) {
            (None, None) => Err("Include spec must have either 'path' or 'pkg'".to_string()),
            (Some(_), Some(_)) => Err("Include spec cannot have both 'path' and 'pkg'".to_string()),
            _ => Ok(()),
        }
    }
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
        assert_eq!(spec.path, Some("./lib/tasks.nika.yaml".to_string()));
        assert!(spec.pkg.is_none());
        assert!(spec.prefix.is_none());
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn test_include_spec_parse_with_prefix() {
        let yaml = r#"
path: ./lib/seo-tasks.nika.yaml
prefix: seo_
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.path, Some("./lib/seo-tasks.nika.yaml".to_string()));
        assert_eq!(spec.prefix, Some("seo_".to_string()));
        assert!(spec.validate().is_ok());
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

    // v0.17: Package reference support
    #[test]
    fn test_include_spec_parse_pkg() {
        let yaml = r#"
pkg: "@workflows/seo-audit"
prefix: seo_
"#;
        let spec: IncludeSpec = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(spec.pkg, Some("@workflows/seo-audit".to_string()));
        assert!(spec.path.is_none());
        assert_eq!(spec.prefix, Some("seo_".to_string()));
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn test_include_spec_validate_missing_both() {
        let spec = IncludeSpec {
            path: None,
            pkg: None,
            prefix: None,
        };
        assert!(spec.validate().is_err());
    }

    #[test]
    fn test_include_spec_validate_both_present() {
        let spec = IncludeSpec {
            path: Some("./lib/tasks.yaml".to_string()),
            pkg: Some("@workflows/seo".to_string()),
            prefix: None,
        };
        assert!(spec.validate().is_err());
    }
}
