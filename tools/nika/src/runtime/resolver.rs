//! Agent and Skill Resolver (v0.6)
//!
//! Resolves external agent definitions and loads skill files at workflow start.
//! This module handles the loading and resolution of:
//! - External agent definition files (.agent.yaml)
//! - Skill files (.skill.md) for prompt augmentation
//!
//! # Example
//!
//! ```yaml
//! agents:
//!   researcher:
//!     file: ./agents/researcher.agent.yaml  # Loaded from file
//!   translator:
//!     system: "You are a translator..."     # Already inline
//!
//! skills:
//!   seo: ./skills/seo-writer.skill.md       # Loaded as string content
//! ```

use crate::ast::{AgentDef, SkillDef, Workflow};
use crate::error::NikaError;
use rustc_hash::FxHashMap;
use std::path::Path;
use tokio::fs;
use tracing::debug;

/// Resolved agents (all expanded to inline definitions)
pub type ResolvedAgents = FxHashMap<String, ResolvedAgent>;

/// Resolved skills (loaded file contents)
pub type ResolvedSkills = FxHashMap<String, String>;

/// Resolved agent definition (always inline after resolution)
#[derive(Debug, Clone)]
pub struct ResolvedAgent {
    /// System prompt for the agent
    pub system: String,
    /// Provider to use (claude, openai, etc.)
    pub provider: String,
    /// Model to use (optional)
    pub model: Option<String>,
    /// Maximum turns for the agent (optional)
    pub max_turns: Option<u32>,
    /// Temperature for generation (optional)
    pub temperature: Option<f32>,
    /// Source of the definition (for debugging)
    pub source: AgentSource,
}

/// Source of agent definition
#[derive(Debug, Clone, PartialEq)]
pub enum AgentSource {
    /// Defined inline in workflow
    Inline,
    /// Loaded from external file
    External(String),
}

/// Resolved assets container
#[derive(Debug, Default)]
pub struct ResolvedAssets {
    /// Resolved agent definitions (all inline)
    pub agents: ResolvedAgents,
    /// Loaded skill contents
    pub skills: ResolvedSkills,
}

impl ResolvedAssets {
    /// Create empty resolved assets
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a resolved agent by name
    pub fn get_agent(&self, name: &str) -> Option<&ResolvedAgent> {
        self.agents.get(name)
    }

    /// Get a loaded skill content by name
    pub fn get_skill(&self, name: &str) -> Option<&str> {
        self.skills.get(name).map(String::as_str)
    }

    /// Check if assets are empty
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty() && self.skills.is_empty()
    }
}

/// Resolve all agents and skills in a workflow.
///
/// This loads external agent files and skill contents, making them available
/// for task execution.
///
/// # Arguments
/// * `workflow` - The workflow to resolve assets for
/// * `base_path` - Base directory for resolving relative paths
///
/// # Errors
/// Returns `NikaError` if any file cannot be loaded or parsed.
pub async fn resolve_assets(
    workflow: &Workflow,
    base_path: &Path,
) -> Result<ResolvedAssets, NikaError> {
    let mut assets = ResolvedAssets::new();

    // Resolve agents
    if let Some(agents) = &workflow.agents {
        for (name, def) in agents {
            let resolved = resolve_agent(name, def, base_path).await?;
            assets.agents.insert(name.clone(), resolved);
        }
    }

    // Load skills
    if let Some(skills) = &workflow.skills {
        for (name, path) in skills {
            let content = load_skill(name, path, base_path).await?;
            assets.skills.insert(name.clone(), content);
        }
    }

    debug!(
        agents = assets.agents.len(),
        skills = assets.skills.len(),
        "Resolved workflow assets"
    );

    Ok(assets)
}

/// Resolve a single agent definition.
///
/// For external definitions, loads and parses the file.
/// For inline definitions, converts directly.
async fn resolve_agent(
    name: &str,
    def: &AgentDef,
    base_path: &Path,
) -> Result<ResolvedAgent, NikaError> {
    match def {
        AgentDef::From { from } => {
            // v0.13: Use multi-format loader
            use crate::ast::loader::{load_definition, DefinitionKind};

            let source_path = base_path.join(from);
            debug!(agent = name, path = ?source_path, "Loading agent via multi-format loader");

            let loaded = load_definition(&source_path, DefinitionKind::Agent)?;

            Ok(ResolvedAgent {
                system: loaded.system,
                provider: loaded.provider.unwrap_or_else(|| "claude".to_string()),
                model: loaded.model,
                max_turns: loaded.max_turns,
                temperature: loaded.temperature,
                source: AgentSource::External(from.clone()),
            })
        }
        AgentDef::External { file } => {
            let file_path = base_path.join(file);
            debug!(agent = name, path = ?file_path, "Loading external agent definition");

            let content =
                fs::read_to_string(&file_path)
                    .await
                    .map_err(|e| NikaError::MemoryLoadError {
                        alias: name.to_string(),
                        path: file_path.display().to_string(),
                        reason: e.to_string(),
                    })?;

            // Parse the external file as an inline agent definition
            let parsed: ExternalAgentFile =
                serde_yaml::from_str(&content).map_err(|e| NikaError::ParseError {
                    details: format!("Failed to parse agent file '{}': {}", file, e),
                })?;

            Ok(ResolvedAgent {
                system: parsed.system,
                provider: parsed.provider,
                model: parsed.model,
                max_turns: parsed.max_turns,
                temperature: parsed.temperature,
                source: AgentSource::External(file.clone()),
            })
        }
        AgentDef::Inline {
            system,
            provider,
            model,
            max_turns,
            temperature,
        } => Ok(ResolvedAgent {
            system: system.clone(),
            provider: provider.clone(),
            model: model.clone(),
            max_turns: *max_turns,
            temperature: *temperature,
            source: AgentSource::Inline,
        }),
    }
}

/// External agent file structure
#[derive(Debug, serde::Deserialize)]
struct ExternalAgentFile {
    /// System prompt for the agent
    system: String,
    /// Provider to use (claude, openai, etc.)
    #[serde(default = "default_provider")]
    provider: String,
    /// Model to use (optional)
    model: Option<String>,
    /// Maximum turns for the agent (optional)
    max_turns: Option<u32>,
    /// Temperature for generation (optional)
    temperature: Option<f32>,
}

fn default_provider() -> String {
    "claude".to_string()
}

/// Load a skill file content.
async fn load_skill(name: &str, path: &SkillDef, base_path: &Path) -> Result<String, NikaError> {
    let file_path = base_path.join(path);
    debug!(skill = name, path = ?file_path, "Loading skill file");

    let content = fs::read_to_string(&file_path)
        .await
        .map_err(|e| NikaError::MemoryLoadError {
            alias: name.to_string(),
            path: file_path.display().to_string(),
            reason: e.to_string(),
        })?;

    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_resolve_assets_empty() {
        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: None,
            skills: None,
            tasks: vec![],
            flows: vec![],
        };

        let dir = tempdir().unwrap();
        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        assert!(assets.is_empty());
        assert!(assets.agents.is_empty());
        assert!(assets.skills.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_inline_agent() {
        let mut agents = FxHashMap::default();
        agents.insert(
            "test_agent".to_string(),
            AgentDef::Inline {
                system: "You are a test agent.".to_string(),
                provider: "openai".to_string(),
                model: Some("gpt-4o".to_string()),
                max_turns: Some(5),
                temperature: Some(0.7),
            },
        );

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: Some(agents),
            skills: None,
            tasks: vec![],
            flows: vec![],
        };

        let dir = tempdir().unwrap();
        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        assert_eq!(assets.agents.len(), 1);
        let agent = assets.get_agent("test_agent").unwrap();
        assert_eq!(agent.system, "You are a test agent.");
        assert_eq!(agent.provider, "openai");
        assert_eq!(agent.model, Some("gpt-4o".to_string()));
        assert_eq!(agent.max_turns, Some(5));
        assert_eq!(agent.temperature, Some(0.7));
        assert_eq!(agent.source, AgentSource::Inline);
    }

    #[tokio::test]
    async fn test_resolve_external_agent() {
        let dir = tempdir().unwrap();

        // Create external agent file
        let agent_content = r#"
system: "You are an external agent."
provider: mistral
model: mistral-large-latest
max_turns: 10
temperature: 0.5
"#;
        let agent_path = dir.path().join("agents");
        std::fs::create_dir_all(&agent_path).unwrap();
        std::fs::write(agent_path.join("external.agent.yaml"), agent_content).unwrap();

        let mut agents = FxHashMap::default();
        agents.insert(
            "ext_agent".to_string(),
            AgentDef::External {
                file: "agents/external.agent.yaml".to_string(),
            },
        );

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: Some(agents),
            skills: None,
            tasks: vec![],
            flows: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        assert_eq!(assets.agents.len(), 1);
        let agent = assets.get_agent("ext_agent").unwrap();
        assert_eq!(agent.system, "You are an external agent.");
        assert_eq!(agent.provider, "mistral");
        assert_eq!(agent.model, Some("mistral-large-latest".to_string()));
        assert_eq!(agent.max_turns, Some(10));
        assert_eq!(agent.temperature, Some(0.5));
        assert_eq!(
            agent.source,
            AgentSource::External("agents/external.agent.yaml".to_string())
        );
    }

    #[tokio::test]
    async fn test_resolve_external_agent_missing_file() {
        let dir = tempdir().unwrap();

        let mut agents = FxHashMap::default();
        agents.insert(
            "missing".to_string(),
            AgentDef::External {
                file: "nonexistent.agent.yaml".to_string(),
            },
        );

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: Some(agents),
            skills: None,
            tasks: vec![],
            flows: vec![],
        };

        let result = resolve_assets(&workflow, dir.path()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::MemoryLoadError { .. }));
    }

    #[tokio::test]
    async fn test_load_skill() {
        let dir = tempdir().unwrap();

        // Create skill file
        let skill_content = r#"# SEO Writer Skill

You are an expert SEO content writer.

## Guidelines
- Use relevant keywords naturally
- Write engaging headlines
- Structure content for readability
"#;
        let skills_path = dir.path().join("skills");
        std::fs::create_dir_all(&skills_path).unwrap();
        std::fs::write(skills_path.join("seo.skill.md"), skill_content).unwrap();

        let mut skills = FxHashMap::default();
        skills.insert("seo".to_string(), "skills/seo.skill.md".to_string());

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: None,
            skills: Some(skills),
            tasks: vec![],
            flows: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        assert_eq!(assets.skills.len(), 1);
        let skill = assets.get_skill("seo").unwrap();
        assert!(skill.contains("SEO Writer Skill"));
        assert!(skill.contains("expert SEO content writer"));
    }

    #[tokio::test]
    async fn test_load_skill_missing_file() {
        let dir = tempdir().unwrap();

        let mut skills = FxHashMap::default();
        skills.insert("missing".to_string(), "nonexistent.skill.md".to_string());

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: None,
            skills: Some(skills),
            tasks: vec![],
            flows: vec![],
        };

        let result = resolve_assets(&workflow, dir.path()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::MemoryLoadError { .. }));
    }

    #[tokio::test]
    async fn test_resolve_mixed_agents_and_skills() {
        let dir = tempdir().unwrap();

        // Create external agent file
        let agent_content = r#"
system: "You are a researcher."
"#;
        let agent_path = dir.path().join("agents");
        std::fs::create_dir_all(&agent_path).unwrap();
        std::fs::write(agent_path.join("researcher.agent.yaml"), agent_content).unwrap();

        // Create skill files
        let skill1_content = "# Skill 1\nFirst skill content.";
        let skill2_content = "# Skill 2\nSecond skill content.";
        let skills_path = dir.path().join("skills");
        std::fs::create_dir_all(&skills_path).unwrap();
        std::fs::write(skills_path.join("skill1.skill.md"), skill1_content).unwrap();
        std::fs::write(skills_path.join("skill2.skill.md"), skill2_content).unwrap();

        // Build workflow
        let mut agents = FxHashMap::default();
        agents.insert(
            "researcher".to_string(),
            AgentDef::External {
                file: "agents/researcher.agent.yaml".to_string(),
            },
        );
        agents.insert(
            "writer".to_string(),
            AgentDef::Inline {
                system: "You are a writer.".to_string(),
                provider: "claude".to_string(),
                model: None,
                max_turns: None,
                temperature: None,
            },
        );

        let mut skills = FxHashMap::default();
        skills.insert("skill1".to_string(), "skills/skill1.skill.md".to_string());
        skills.insert("skill2".to_string(), "skills/skill2.skill.md".to_string());

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: Some(agents),
            skills: Some(skills),
            tasks: vec![],
            flows: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        // Check agents
        assert_eq!(assets.agents.len(), 2);
        let researcher = assets.get_agent("researcher").unwrap();
        assert_eq!(researcher.system, "You are a researcher.");
        assert_eq!(
            researcher.source,
            AgentSource::External("agents/researcher.agent.yaml".to_string())
        );

        let writer = assets.get_agent("writer").unwrap();
        assert_eq!(writer.system, "You are a writer.");
        assert_eq!(writer.source, AgentSource::Inline);

        // Check skills
        assert_eq!(assets.skills.len(), 2);
        assert!(assets
            .get_skill("skill1")
            .unwrap()
            .contains("First skill content"));
        assert!(assets
            .get_skill("skill2")
            .unwrap()
            .contains("Second skill content"));
    }

    #[tokio::test]
    async fn test_external_agent_with_defaults() {
        let dir = tempdir().unwrap();

        // Create minimal external agent file (only required field)
        let agent_content = r#"
system: "You are an agent with defaults."
"#;
        std::fs::write(dir.path().join("minimal.agent.yaml"), agent_content).unwrap();

        let mut agents = FxHashMap::default();
        agents.insert(
            "minimal".to_string(),
            AgentDef::External {
                file: "minimal.agent.yaml".to_string(),
            },
        );

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.6".to_string(),
            provider: "claude".to_string(),
            model: None,
            mcp: None,
            memory: None,
            agents: Some(agents),
            skills: None,
            tasks: vec![],
            flows: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        let agent = assets.get_agent("minimal").unwrap();
        assert_eq!(agent.system, "You are an agent with defaults.");
        assert_eq!(agent.provider, "claude"); // default
        assert!(agent.model.is_none());
        assert!(agent.max_turns.is_none());
        assert!(agent.temperature.is_none());
    }

    #[test]
    fn test_resolved_agent_clone() {
        let agent = ResolvedAgent {
            system: "Test".to_string(),
            provider: "claude".to_string(),
            model: None,
            max_turns: None,
            temperature: None,
            source: AgentSource::Inline,
        };

        let cloned = agent.clone();
        assert_eq!(cloned.system, "Test");
    }

    #[test]
    fn test_resolved_assets_get_methods() {
        let mut assets = ResolvedAssets::new();

        assets.agents.insert(
            "test".to_string(),
            ResolvedAgent {
                system: "Test system".to_string(),
                provider: "claude".to_string(),
                model: None,
                max_turns: None,
                temperature: None,
                source: AgentSource::Inline,
            },
        );
        assets
            .skills
            .insert("skill".to_string(), "Skill content".to_string());

        assert!(assets.get_agent("test").is_some());
        assert!(assets.get_agent("nonexistent").is_none());
        assert_eq!(assets.get_skill("skill"), Some("Skill content"));
        assert!(assets.get_skill("nonexistent").is_none());
        assert!(!assets.is_empty());
    }
}
