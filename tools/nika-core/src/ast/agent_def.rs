// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent definition types for workflow
//!
//! The `agents:` block in a workflow allows defining reusable agent configurations.
//! Agents can be defined inline or loaded from external files.
//!
//! # Multi-format Support
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
//!     from: ./agents/researcher           # Auto-detect format (folder or file)
//!   helper:
//!     file: ./agents/helper.agent.yaml    # Explicit YAML
//!   reviewer:
//!     from: ./agents/reviewer.md          # Markdown with frontmatter
//!   translator:
//!     system: "You are a translator..."   # Inline definition
//!     provider: claude
//!     model: claude-sonnet-4-6
//!     max_turns: 3
//! ```

use serde::Deserialize;

/// Agent definition
///
/// Can be either an external file/folder reference or an inline definition.
/// Adds support for `from:` which auto-detects format.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AgentDef {
    /// External reference using `from:`
    From {
        /// Path to agent definition (file or folder, any supported format)
        from: String,
    },

    /// External file reference using `file:`
    External {
        /// Path to the agent definition file (.agent.yaml)
        file: String,
    },

    /// Inline definition
    Inline {
        /// System prompt for the agent
        system: String,

        /// Provider to use (None = inherit from workflow default)
        provider: Option<String>,

        /// Model to use (optional, uses provider default if not specified)
        model: Option<String>,

        /// Maximum turns for the agent (optional)
        max_turns: Option<u32>,

        /// Temperature for generation (optional)
        temperature: Option<f32>,

        /// Skills to load for this agent
        ///
        /// Skills can be:
        /// - Relative paths: `./skills/my-skill.md`
        /// - pkg: URIs: `pkg:@scope/name@version/path`
        ///
        /// Agent-level skills override workflow-level skills when both define
        /// the same skill path.
        #[serde(default)]
        skills: Option<Vec<String>>,
    },
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

    /// Check if this uses the new `from:` syntax
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

    /// Get the skills list if this is an inline definition
    ///
    /// Agent-level skills override workflow-level skills when both define
    /// the same skill path.
    pub fn skills(&self) -> Option<&Vec<String>> {
        match self {
            AgentDef::Inline { skills, .. } => skills.as_ref(),
            _ => None,
        }
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
            skills,
        } = def
        {
            assert_eq!(system, "You are a helpful assistant.");
            assert_eq!(provider, None); // no default — inherits from workflow
            assert!(model.is_none());
            assert!(max_turns.is_none());
            assert!(temperature.is_none());
            assert!(skills.is_none()); // no skills in minimal
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
            skills,
        } = def
        {
            assert_eq!(system, "You are a translator.");
            assert_eq!(provider, Some("openai".to_string()));
            assert_eq!(model, Some("gpt-4o".to_string()));
            assert_eq!(max_turns, Some(5));
            assert_eq!(temperature, Some(0.7));
            assert!(skills.is_none()); // no skills in this test
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
            provider: Some("claude".to_string()),
            model: None,
            max_turns: None,
            temperature: None,
            skills: None,
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
            provider: Some("claude".to_string()),
            model: None,
            max_turns: None,
            temperature: None,
            skills: None,
        };

        assert_eq!(external.file_path(), Some("path/to/agent.yaml"));
        assert_eq!(from.file_path(), Some("./agents/researcher"));
        assert_eq!(from.source_path(), Some("./agents/researcher"));
        assert_eq!(inline.file_path(), None);
    }

    // =========================================================================
    // Skills Tests
    // =========================================================================

    #[test]
    fn test_agent_def_inline_with_skills_array() {
        let yaml = r#"
            system: "You are a helpful assistant"
            provider: claude
            skills:
              - ./skills/research.md
              - ./skills/writing.md
        "#;
        let agent: AgentDef = serde_yaml::from_str(yaml).unwrap();

        assert!(agent.is_inline());
        let skills = agent.skills().expect("skills should be Some");
        assert_eq!(skills.len(), 2);
        assert_eq!(skills[0], "./skills/research.md");
        assert_eq!(skills[1], "./skills/writing.md");
    }

    #[test]
    fn test_agent_def_skills_helper_returns_some() {
        let inline = AgentDef::Inline {
            system: "test".to_string(),
            provider: Some("claude".to_string()),
            model: None,
            max_turns: None,
            temperature: None,
            skills: Some(vec!["./skill.md".to_string()]),
        };

        assert!(inline.skills().is_some());
        assert_eq!(inline.skills().unwrap().len(), 1);
    }

    #[test]
    fn test_agent_def_skills_helper_returns_none_for_no_skills() {
        let inline = AgentDef::Inline {
            system: "test".to_string(),
            provider: Some("claude".to_string()),
            model: None,
            max_turns: None,
            temperature: None,
            skills: None,
        };

        assert!(inline.skills().is_none());
    }

    #[test]
    fn test_agent_def_skills_helper_returns_none_for_external() {
        let external = AgentDef::External {
            file: "agent.yaml".to_string(),
        };

        // External agents don't have inline skills
        assert!(external.skills().is_none());
    }

    #[test]
    fn test_agent_def_empty_skills_array() {
        let yaml = r#"
            system: "You are a helpful assistant"
            provider: claude
            skills: []
        "#;
        let agent: AgentDef = serde_yaml::from_str(yaml).unwrap();

        let skills = agent.skills().expect("empty array should still be Some");
        assert!(skills.is_empty());
    }

    #[test]
    fn test_agent_def_skills_with_pkg_uri() {
        let yaml = r#"
            system: "You are a helpful assistant"
            provider: claude
            skills:
              - pkg:@supernovae/research@1.0.0/skills/deep-research.md
              - pkg:@supernovae/writing@2.0.0/skills/technical-writing.md
        "#;
        let agent: AgentDef = serde_yaml::from_str(yaml).unwrap();

        let skills = agent.skills().expect("skills should be Some");
        assert_eq!(skills.len(), 2);
        assert!(skills[0].starts_with("pkg:@"));
        assert!(skills[1].starts_with("pkg:@"));
        assert!(skills[0].contains("@1.0.0"));
        assert!(skills[1].contains("@2.0.0"));
    }

    #[test]
    fn test_agent_def_skills_with_relative_paths() {
        let yaml = r#"
            system: "You are a helpful assistant"
            provider: claude
            skills:
              - ./skills/local-skill.md
              - ../shared/common-skill.md
              - skills/nested/deep-skill.md
        "#;
        let agent: AgentDef = serde_yaml::from_str(yaml).unwrap();

        let skills = agent.skills().expect("skills should be Some");
        assert_eq!(skills.len(), 3);
        assert_eq!(skills[0], "./skills/local-skill.md");
        assert_eq!(skills[1], "../shared/common-skill.md");
        assert_eq!(skills[2], "skills/nested/deep-skill.md");
    }

    #[test]
    fn test_agent_def_skills_mixed_pkg_and_relative() {
        // Agent-level skills can mix pkg: URIs and relative paths
        // This tests the intended behavior documented in ADR:
        // - Agent-level skills override workflow-level skills
        // - Skills are loaded and injected into the agent's system prompt
        let yaml = r#"
            system: "You are a helpful assistant"
            provider: claude
            skills:
              - ./skills/project-specific.md
              - pkg:@supernovae/research@1.0.0/skills/research.md
        "#;
        let agent: AgentDef = serde_yaml::from_str(yaml).unwrap();

        let skills = agent.skills().expect("skills should be Some");
        assert_eq!(skills.len(), 2);

        // First is relative path
        assert!(skills[0].starts_with("./"));
        // Second is pkg: URI
        assert!(skills[1].starts_with("pkg:"));
    }
}
