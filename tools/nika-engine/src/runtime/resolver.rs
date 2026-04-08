// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Agent and Skill Resolver
//!
//! Resolves external agent definitions and loads skill files at workflow start.
//! This module handles the loading and resolution of:
//! - External agent definition files (.agent.yaml)
//! - Package agent references (@agents/name)
//! - Package prompt references (@prompts/name)
//! - Package skill references (@skills/name)
//! - Skill files (.skill.md) for prompt augmentation
//!
//! # Example
//!
//! ```yaml
//! agents:
//!   researcher:
//!     from: "@agents/researcher"              # From package
//!   local:
//!     file: ./agents/local.agent.yaml        # Local file
//!   translator:
//!     system: "You are a translator..."      # Inline definition
//!
//! skills:
//!   seo: "@prompts/seo-meta"                  # From package
//!   local: ./skills/seo-writer.skill.md      # Local file
//! ```

use crate::ast::analyzed::AnalyzedWorkflow;
use crate::ast::{AgentDef, SkillDef, Workflow};
use crate::error::NikaError;
use crate::registry::resolver; // Package resolution
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};
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
    /// Provider to use (None = inherit from workflow default)
    pub provider: Option<String>,
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
    /// Built-in default preset
    Builtin,
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

// ═══════════════════════════════════════════════════════════════════════════
// DEFAULT PRESETS — 8 built-in agent presets available without agents: block
// ═══════════════════════════════════════════════════════════════════════════

/// Get built-in default presets. Available via `preset: think` without `agents:` block.
pub fn default_presets() -> ResolvedAgents {
    let mut presets = FxHashMap::default();

    presets.insert(
        "think".to_string(),
        ResolvedAgent {
            system: "You are a deep reasoning assistant. Think step by step through complex problems. Show your reasoning process.".to_string(),
            provider: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            max_turns: Some(5),
            temperature: Some(0.3),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "lite".to_string(),
        ResolvedAgent {
            system: "You are a fast, concise assistant. Give brief, direct answers.".to_string(),
            provider: None,
            model: Some("claude-haiku-4-5".to_string()),
            max_turns: Some(3),
            temperature: Some(0.5),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "search".to_string(),
        ResolvedAgent {
            system: "You are a research assistant with web search capabilities. Find accurate, up-to-date information. Cite sources.".to_string(),
            provider: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            max_turns: Some(10),
            temperature: Some(0.3),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "vision".to_string(),
        ResolvedAgent {
            system: "You are a vision analysis assistant. Describe images in detail, identify objects, read text, and analyze visual content.".to_string(),
            provider: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            max_turns: Some(3),
            temperature: Some(0.3),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "judge".to_string(),
        ResolvedAgent {
            system: "You are an impartial judge. Evaluate the quality, accuracy, and completeness of content. Provide a structured assessment with PASS or FAIL verdict.".to_string(),
            provider: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            max_turns: Some(3),
            temperature: Some(0.1),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "coder".to_string(),
        ResolvedAgent {
            system: "You are an expert programmer. Write clean, efficient, well-tested code. Follow best practices and explain your approach.".to_string(),
            provider: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            max_turns: Some(8),
            temperature: Some(0.2),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "summary".to_string(),
        ResolvedAgent {
            system: "You are a summarization specialist. Extract key points, themes, and insights from text. Be concise but comprehensive.".to_string(),
            provider: None,
            model: Some("claude-haiku-4-5".to_string()),
            max_turns: Some(3),
            temperature: Some(0.3),
            source: AgentSource::Builtin,
        },
    );

    presets.insert(
        "creative".to_string(),
        ResolvedAgent {
            system: "You are a creative writing assistant. Generate imaginative, engaging content with vivid language and original ideas.".to_string(),
            provider: None,
            model: Some("claude-sonnet-4-20250514".to_string()),
            max_turns: Some(5),
            temperature: Some(0.9),
            source: AgentSource::Builtin,
        },
    );

    presets
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

    // Seed with default presets — workflow agents: block overrides these
    for (name, preset) in default_presets() {
        assets.agents.insert(name, preset);
    }

    // Resolve user-defined agents (override defaults with same name)
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

/// Resolve agents from an AnalyzedWorkflow.
///
/// AnalyzedWorkflow has `agents` but no `skills` (skills are resolved during
/// analysis and merged via include_loader). This function only resolves agents.
pub async fn resolve_assets_analyzed(
    workflow: &AnalyzedWorkflow,
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

    // No skills on AnalyzedWorkflow — resolved during analysis

    debug!(
        agents = assets.agents.len(),
        "Resolved workflow assets (analyzed)"
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
            // Check built-in presets first (think, lite, search, vision, judge, coder, summary, creative)
            let presets = default_presets();
            if let Some(preset) = presets.get(from.as_str()) {
                debug!(
                    agent = name,
                    preset = from,
                    "Resolved agent from built-in preset"
                );
                return Ok(preset.clone());
            }

            // Support package references (@agents/name)
            use crate::ast::loader::{load_definition, DefinitionKind};

            let source_path: PathBuf = if from.starts_with('@') {
                // Package reference - resolve via registry
                debug!(agent = name, package = from, "Resolving agent from package");

                let resolved = resolver::resolve_package_path(from).map_err(|e| {
                    NikaError::ContextLoadError {
                        alias: name.to_string(),
                        path: from.clone(),
                        reason: format!("Package not found: {}. Try: nika add {}", e, from),
                    }
                })?;

                // Agent packages should contain agent.md or agent.yaml
                let agent_md = resolved.path.join("agent.md");
                let agent_yaml = resolved.path.join("agent.yaml");

                if agent_md.exists() {
                    agent_md
                } else if agent_yaml.exists() {
                    agent_yaml
                } else {
                    return Err(NikaError::ContextLoadError {
                        alias: name.to_string(),
                        path: from.clone(),
                        reason: format!(
                            "Package {} exists but missing agent.md or agent.yaml at {}",
                            from,
                            resolved.path.display()
                        ),
                    });
                }
            } else {
                // Regular filesystem path
                base_path.join(from)
            };

            debug!(agent = name, path = ?source_path, "Loading agent via multi-format loader");

            let loaded = load_definition(&source_path, DefinitionKind::Agent)?;

            Ok(ResolvedAgent {
                system: loaded.system,
                provider: loaded.provider,
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
                    .map_err(|e| NikaError::ContextLoadError {
                        alias: name.to_string(),
                        path: file_path.display().to_string(),
                        reason: e.to_string(),
                    })?;

            // Parse the external file as an inline agent definition
            let parsed: ExternalAgentFile =
                crate::util::parse_yaml_budgeted(&content).map_err(|e| NikaError::ParseError {
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
            skills: _, // agent-level skills handled by skill injector
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
    /// Provider to use (None = inherit from workflow default)
    provider: Option<String>,
    /// Model to use (optional)
    model: Option<String>,
    /// Maximum turns for the agent (optional)
    max_turns: Option<u32>,
    /// Temperature for generation (optional)
    temperature: Option<f32>,
}

/// Load a skill file content.
async fn load_skill(name: &str, path: &SkillDef, base_path: &Path) -> Result<String, NikaError> {
    // Support package references (@prompts/name, @skills/name)
    let file_path: PathBuf = if path.starts_with('@') {
        // Package reference - resolve via registry
        debug!(
            skill = name,
            package = path,
            "Resolving skill/prompt from package"
        );

        let resolved =
            resolver::resolve_package_path(path).map_err(|e| NikaError::ContextLoadError {
                alias: name.to_string(),
                path: path.to_string(),
                reason: format!("Package not found: {}. Try: nika add {}", e, path),
            })?;

        // Skill/Prompt packages should contain skill.md or prompt.md
        let skill_md = resolved.path.join("skill.md");
        let prompt_md = resolved.path.join("prompt.md");

        if skill_md.exists() {
            skill_md
        } else if prompt_md.exists() {
            prompt_md
        } else {
            return Err(NikaError::ContextLoadError {
                alias: name.to_string(),
                path: path.to_string(),
                reason: format!(
                    "Package {} exists but missing skill.md or prompt.md at {}",
                    path,
                    resolved.path.display()
                ),
            });
        }
    } else {
        // Regular filesystem path
        base_path.join(path)
    };

    debug!(skill = name, path = ?file_path, "Loading skill file");

    let content =
        fs::read_to_string(&file_path)
            .await
            .map_err(|e| NikaError::ContextLoadError {
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
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let dir = tempdir().unwrap();
        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        // Default presets are always present; skills should be empty
        assert_eq!(assets.agents.len(), 8, "Should have 8 default presets");
        assert!(assets.skills.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_inline_agent() {
        let mut agents = FxHashMap::default();
        agents.insert(
            "test_agent".to_string(),
            AgentDef::Inline {
                system: "You are a test agent.".to_string(),
                provider: Some("openai".to_string()),
                model: Some("gpt-4o".to_string()),
                max_turns: Some(5),
                temperature: Some(0.7),
                skills: None, // agent-level skills
            },
        );

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: Some(agents),
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let dir = tempdir().unwrap();
        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        assert_eq!(assets.agents.len(), 9, "8 defaults + 1 user-defined");
        let agent = assets.get_agent("test_agent").unwrap();
        assert_eq!(agent.system, "You are a test agent.");
        assert_eq!(agent.provider, Some("openai".to_string()));
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
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: Some(agents),
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        assert_eq!(assets.agents.len(), 9, "8 defaults + 1 user-defined");
        let agent = assets.get_agent("ext_agent").unwrap();
        assert_eq!(agent.system, "You are an external agent.");
        assert_eq!(agent.provider, Some("mistral".to_string()));
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
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: Some(agents),
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let result = resolve_assets(&workflow, dir.path()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::ContextLoadError { .. }));
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
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: Some(skills),
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
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
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: None,
            skills: Some(skills),
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let result = resolve_assets(&workflow, dir.path()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::ContextLoadError { .. }));
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
                provider: Some("claude".to_string()),
                model: None,
                max_turns: None,
                temperature: None,
                skills: None, // agent-level skills
            },
        );

        let mut skills = FxHashMap::default();
        skills.insert("skill1".to_string(), "skills/skill1.skill.md".to_string());
        skills.insert("skill2".to_string(), "skills/skill2.skill.md".to_string());

        let workflow = crate::ast::Workflow {
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: Some(agents),
            skills: Some(skills),
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        // Check agents (8 defaults + 2 user-defined)
        assert_eq!(assets.agents.len(), 10);
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
            schema: "nika/workflow@0.12".to_string(),
            name: None,
            provider: nika_core::ProviderName::Anthropic,
            model: None,
            mcp: None,
            context: None,
            include: None,
            agents: Some(agents),
            skills: None,
            artifacts: None,
            log: None,
            inputs: None,
            tasks: vec![],
        };

        let assets = resolve_assets(&workflow, dir.path()).await.unwrap();

        let agent = assets.get_agent("minimal").unwrap();
        assert_eq!(agent.system, "You are an agent with defaults.");
        assert_eq!(agent.provider, None); // no default — inherits from workflow
        assert!(agent.model.is_none());
        assert!(agent.max_turns.is_none());
        assert!(agent.temperature.is_none());
    }

    #[test]
    fn test_resolved_agent_clone() {
        let agent = ResolvedAgent {
            system: "Test".to_string(),
            provider: Some("claude".to_string()),
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
                provider: Some("claude".to_string()),
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

    // ─────────────────────────────────────────────────────────────
    // Default presets
    // ─────────────────────────────────────────────────────────────

    #[test]
    fn default_presets_has_8_entries() {
        let presets = default_presets();
        assert_eq!(presets.len(), 8, "Expected 8 default presets");
    }

    #[test]
    fn default_presets_names() {
        let presets = default_presets();
        let expected = [
            "think", "lite", "search", "vision", "judge", "coder", "summary", "creative",
        ];
        for name in &expected {
            assert!(
                presets.contains_key(*name),
                "Missing default preset: {}",
                name
            );
        }
    }

    #[test]
    fn default_presets_all_builtin_source() {
        let presets = default_presets();
        for (name, preset) in &presets {
            assert_eq!(
                preset.source,
                AgentSource::Builtin,
                "Preset '{}' should have Builtin source",
                name
            );
        }
    }

    #[test]
    fn default_presets_temperature_ranges() {
        let presets = default_presets();
        for (name, preset) in &presets {
            if let Some(temp) = preset.temperature {
                assert!(
                    (0.0..=2.0).contains(&temp),
                    "Preset '{}' temperature {} out of range",
                    name,
                    temp
                );
            }
        }
    }

    #[test]
    fn default_presets_think_is_reasoning() {
        let presets = default_presets();
        let think = &presets["think"];
        assert_eq!(think.temperature, Some(0.3));
        assert!(think.system.contains("reasoning") || think.system.contains("step by step"));
    }

    #[test]
    fn default_presets_creative_high_temp() {
        let presets = default_presets();
        let creative = &presets["creative"];
        assert!(creative.temperature.unwrap() > 0.7);
    }
}
