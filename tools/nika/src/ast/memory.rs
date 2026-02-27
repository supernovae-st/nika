//! Memory configuration for workflow (v0.6)
//!
//! The `memory:` block in a workflow allows loading files at workflow start.
//! Files are loaded into the DataStore and accessible via `{{memory.files.alias}}` bindings.
//!
//! # Example
//!
//! ```yaml
//! memory:
//!   files:
//!     brand: ./context/brand.md        # Markdown → string
//!     persona: ./context/persona.json  # JSON → parsed object
//!     examples: ./context/*.md         # Glob → array of strings
//!   session: .nika/sessions/prev.json  # Session restore
//! ```

use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Memory configuration for workflow (v0.6)
///
/// Defines files to load at workflow start and optional session restoration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct MemoryConfig {
    /// Files to load at workflow start
    ///
    /// Key is the alias, value is the file path (supports glob patterns).
    /// - Single files: loaded as string (markdown, txt) or parsed (json, yaml)
    /// - Glob patterns: loaded as array of strings
    #[serde(default)]
    pub files: FxHashMap<String, String>,

    /// Session file to restore
    ///
    /// Path to a JSON file containing previous session data.
    /// Accessible via `{{memory.session.key}}` bindings.
    pub session: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_memory_config_default() {
        let config = MemoryConfig::default();
        assert!(config.files.is_empty());
        assert!(config.session.is_none());
    }

    #[test]
    fn test_memory_config_deserialize_empty() {
        let yaml = "";
        let config: MemoryConfig = serde_yaml::from_str(yaml).unwrap_or_default();
        assert!(config.files.is_empty());
    }

    #[test]
    fn test_memory_config_deserialize_files() {
        let yaml = r#"
files:
  brand: ./context/brand.md
  persona: ./context/persona.json
"#;
        let config: MemoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert_eq!(
            config.files.get("brand"),
            Some(&"./context/brand.md".to_string())
        );
        assert_eq!(
            config.files.get("persona"),
            Some(&"./context/persona.json".to_string())
        );
    }

    #[test]
    fn test_memory_config_deserialize_session() {
        let yaml = r#"
session: .nika/sessions/prev.json
"#;
        let config: MemoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.session, Some(".nika/sessions/prev.json".to_string()));
    }

    #[test]
    fn test_memory_config_deserialize_full() {
        let yaml = r#"
files:
  brand: ./context/brand.md
  examples: ./context/*.md
session: .nika/sessions/prev.json
"#;
        let config: MemoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert!(config.files.contains_key("brand"));
        assert!(config.files.contains_key("examples"));
        assert!(config.session.is_some());
    }

    #[test]
    fn test_memory_config_glob_pattern() {
        let yaml = r#"
files:
  examples: ./context/*.md
"#;
        let config: MemoryConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.files.get("examples").unwrap().contains('*'));
    }
}
