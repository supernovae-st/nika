// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Context configuration for workflow
//!
//! The `context:` block in a workflow allows loading files at workflow start.
//! Files are loaded into the RunContext and accessible via `{{context.files.alias}}` bindings.
//!
//! # Example
//!
//! ```yaml
//! context:
//!   files:
//!     brand: ./context/brand.md        # Markdown → string
//!     persona: ./context/persona.json  # JSON → parsed object
//!     examples: ./context/*.md         # Glob → array of strings
//!   session: .nika/sessions/prev.json  # Session restore
//! ```

use rustc_hash::FxHashMap;
use serde::Deserialize;

/// Context configuration for workflow
///
/// Defines files to load at workflow start and optional session restoration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct ContextConfig {
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
    /// Accessible via `{{context.session.key}}` bindings.
    pub session: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_context_config_default() {
        let config = ContextConfig::default();
        assert!(config.files.is_empty());
        assert!(config.session.is_none());
    }

    #[test]
    fn test_context_config_deserialize_empty() {
        let yaml = "";
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap_or_default();
        assert!(config.files.is_empty());
    }

    #[test]
    fn test_context_config_deserialize_files() {
        let yaml = r#"
files:
  brand: ./context/brand.md
  persona: ./context/persona.json
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
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
    fn test_context_config_deserialize_session() {
        let yaml = r#"
session: .nika/sessions/prev.json
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.session, Some(".nika/sessions/prev.json".to_string()));
    }

    #[test]
    fn test_context_config_deserialize_full() {
        let yaml = r#"
files:
  brand: ./context/brand.md
  examples: ./context/*.md
session: .nika/sessions/prev.json
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.files.len(), 2);
        assert!(config.files.contains_key("brand"));
        assert!(config.files.contains_key("examples"));
        assert!(config.session.is_some());
    }

    #[test]
    fn test_context_config_glob_pattern() {
        let yaml = r#"
files:
  examples: ./context/*.md
"#;
        let config: ContextConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.files.get("examples").unwrap().contains('*'));
    }
}
