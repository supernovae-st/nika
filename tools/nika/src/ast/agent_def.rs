//! Agent definition types for workflow (v0.6, v0.13)
//!
//! The `agents:` block in a workflow allows defining reusable agent configurations.
//! Agents can be defined inline or loaded from external files.
//!
//! # v0.13 Multi-format Support
//!
//! Agents and skills can be loaded from multiple formats:
//! - `.agent.yaml` / `.agent.yml` - YAML format
//! - `.skill.yaml` / `.skill.yml` - YAML format
//! - `.md` files with YAML frontmatter (Claude Code style)
//! - Folders containing the above
//!
//! # Example
//!
//! ```yaml
//! agents:
//!   researcher:
//!     from: ./agents/researcher           # v0.13: Auto-detect format (folder or file)
//!   helper:
//!     file: ./agents/helper.agent.yaml    # Explicit YAML (legacy)
//!   reviewer:
//!     from: ./agents/reviewer.md          # Markdown with frontmatter
//!   translator:
//!     system: "You are a translator..."   # Inline definition
//!     provider: claude
//!     model: claude-sonnet-4-6
//!     max_turns: 3
//! ```

use serde::Deserialize;

/// Agent definition (v0.6, v0.13)
///
/// Can be either an external file/folder reference or an inline definition.
/// v0.13 adds support for `from:` which auto-detects format.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AgentDef {
    /// External reference using `from:` (v0.13 - auto-detect format)
    From {
        /// Path to agent definition (file or folder, any supported format)
        from: String,
    },

    /// External file reference using `file:` (legacy)
    External {
        /// Path to the agent definition file (.agent.yaml)
        file: String,
    },

    /// Inline definition
    Inline {
        /// System prompt for the agent
        system: String,

        /// Provider to use (claude, openai, etc.)
        #[serde(default = "default_provider")]
        provider: String,

        /// Model to use (optional, uses provider default if not specified)
        model: Option<String>,

        /// Maximum turns for the agent (optional)
        max_turns: Option<u32>,

        /// Temperature for generation (optional)
        temperature: Option<f32>,
    },
}

fn default_provider() -> String {
    "claude".to_string()
}

impl AgentDef {
    /// Check if this is an external reference (file or from)
    pub fn is_external(&self) -> bool {
        matches!(self, AgentDef::External { .. } | AgentDef::From { .. })
    }

    /// Check if this is an inline definition
    pub fn is_inline(&self) -> bool {
        matches!(self, AgentDef::Inline { .. })
    }

    /// Check if this uses the new `from:` syntax (v0.13)
    pub fn is_from(&self) -> bool {
        matches!(self, AgentDef::From { .. })
    }

    /// Get the file/folder path if this is an external reference
    pub fn file_path(&self) -> Option<&str> {
        match self {
            AgentDef::External { file } => Some(file),
            AgentDef::From { from } => Some(from),
            AgentDef::Inline { .. } => None,
        }
    }

    /// Get the source path (alias for file_path)
    pub fn source_path(&self) -> Option<&str> {
        self.file_path()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_agent_def_from() {
        let yaml = r#"
from: ./agents/researcher
"#;
        let def: AgentDef = serde_yaml::from_str(yaml).unwrap();
        assert!(def.is_external());
        assert!(def.is_from());
        assert_eq!(def.file_path(), Some("./agents/researcher"));
    }

    #[test]
    fn test_agent_def_from_md() {
        let yaml = r#"
from: ./agents/reviewer.md
"#;
        let def: AgentDef = serde_yaml::from_str(yaml).unwrap();
        assert!(def.is_from());
        assert_eq!(def.source_path(), Some("./agents/reviewer.md"));
    }

    #[test]
    fn test_agent_def_external() {
        let yaml = r#"
file: ./agents/researcher.agent.yaml
"#;
        let def: AgentDef = serde_yaml::from_str(yaml).unwrap();
        assert!(def.is_external());
        assert!(!def.is_from());
        assert_eq!(def.file_path(), Some("./agents/researcher.agent.yaml"));
    }

    #[test]
    fn test_agent_def_inline_minimal() {
        let yaml = r#"
system: "You are a helpful assistant."
"#;
        let def: AgentDef = serde_yaml::from_str(yaml).unwrap();
        assert!(def.is_inline());
        if let AgentDef::Inline {
            system,
            provider,
            model,
            max_turns,
            temperature,
        } = def
        {
            assert_eq!(system, "You are a helpful assistant.");
            assert_eq!(provider, "claude"); // default
            assert!(model.is_none());
            assert!(max_turns.is_none());
            assert!(temperature.is_none());
        }
    }

    #[test]
    fn test_agent_def_inline_full() {
        let yaml = r#"
system: "You are a translator."
provider: openai
model: gpt-4o
max_turns: 5
temperature: 0.7
"#;
        let def: AgentDef = serde_yaml::from_str(yaml).unwrap();
        assert!(def.is_inline());
        if let AgentDef::Inline {
            system,
            provider,
            model,
            max_turns,
            temperature,
        } = def
        {
            assert_eq!(system, "You are a translator.");
            assert_eq!(provider, "openai");
            assert_eq!(model, Some("gpt-4o".to_string()));
            assert_eq!(max_turns, Some(5));
            assert_eq!(temperature, Some(0.7));
        }
    }

    #[test]
    fn test_agent_def_is_external() {
        let external = AgentDef::External {
            file: "test.yaml".to_string(),
        };
        let from = AgentDef::From {
            from: "./agents/test".to_string(),
        };
        let inline = AgentDef::Inline {
            system: "test".to_string(),
            provider: "claude".to_string(),
            model: None,
            max_turns: None,
            temperature: None,
        };

        assert!(external.is_external());
        assert!(!external.is_inline());
        assert!(!external.is_from());

        assert!(from.is_external());
        assert!(from.is_from());
        assert!(!from.is_inline());

        assert!(!inline.is_external());
        assert!(inline.is_inline());
        assert!(!inline.is_from());
    }

    #[test]
    fn test_agent_def_file_path() {
        let external = AgentDef::External {
            file: "path/to/agent.yaml".to_string(),
        };
        let from = AgentDef::From {
            from: "./agents/researcher".to_string(),
        };
        let inline = AgentDef::Inline {
            system: "test".to_string(),
            provider: "claude".to_string(),
            model: None,
            max_turns: None,
            temperature: None,
        };

        assert_eq!(external.file_path(), Some("path/to/agent.yaml"));
        assert_eq!(from.file_path(), Some("./agents/researcher"));
        assert_eq!(from.source_path(), Some("./agents/researcher"));
        assert_eq!(inline.file_path(), None);
    }
}
