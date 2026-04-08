// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Multi-format loader for agents and skills
//!
//! Supports loading agent and skill definitions from multiple formats:
//! - `.agent.yaml` / `.agent.yml` - YAML format
//! - `.skill.yaml` / `.skill.yml` - YAML format
//! - `.md` files with YAML frontmatter (Claude Code style)
//! - Folders containing the above
//!
//! # Examples
//!
//! ```yaml
//! # In workflow
//! agents:
//!   researcher:
//!     from: ./agents/researcher          # Folder or file (auto-detect)
//!   helper:
//!     from: ./agents/helper.agent.yaml   # Explicit YAML
//!   reviewer:
//!     from: ./agents/reviewer.md         # Markdown with frontmatter
//! ```

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::error::NikaError;

/// Parsed agent/skill definition from any format
#[derive(Debug, Clone)]
pub struct LoadedDefinition {
    /// Name of the agent/skill (from filename or frontmatter)
    pub name: String,

    /// Description (from frontmatter or first paragraph)
    pub description: Option<String>,

    /// System prompt / instructions (body content)
    pub system: String,

    /// Provider (claude, openai, etc.)
    pub provider: Option<String>,

    /// Model override
    pub model: Option<String>,

    /// Maximum turns for agent loops
    pub max_turns: Option<u32>,

    /// Temperature for generation
    pub temperature: Option<f32>,

    /// Source path (for debugging)
    pub source_path: PathBuf,
}

/// YAML frontmatter structure (Claude Code style)
#[derive(Debug, Deserialize)]
struct Frontmatter {
    name: Option<String>,
    description: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    max_turns: Option<u32>,
    temperature: Option<f32>,
}

/// YAML-only definition format
#[derive(Debug, Deserialize)]
struct YamlDefinition {
    name: Option<String>,
    description: Option<String>,
    system: String,
    provider: Option<String>,
    model: Option<String>,
    max_turns: Option<u32>,
    temperature: Option<f32>,
}

/// Definition kind for detection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefinitionKind {
    Agent,
    Skill,
}

impl DefinitionKind {
    /// Get file extensions for this kind
    pub fn extensions(&self) -> &[&str] {
        match self {
            DefinitionKind::Agent => &[".agent.yaml", ".agent.yml", ".md"],
            DefinitionKind::Skill => &[".skill.yaml", ".skill.yml", ".md"],
        }
    }

    /// Get standard filename for this kind
    pub fn standard_filename(&self) -> &str {
        match self {
            DefinitionKind::Agent => "AGENT.md",
            DefinitionKind::Skill => "SKILL.md",
        }
    }
}

/// Load a definition from a path (file or folder)
///
/// Auto-detects format based on extension or folder contents.
pub fn load_definition(path: &Path, kind: DefinitionKind) -> Result<LoadedDefinition, NikaError> {
    if path.is_dir() {
        load_from_folder(path, kind)
    } else if path.is_file() {
        load_from_file(path, kind)
    } else {
        // Try adding extensions
        try_load_with_extensions(path, kind)
    }
}

/// Try loading with various extensions
fn try_load_with_extensions(
    path: &Path,
    kind: DefinitionKind,
) -> Result<LoadedDefinition, NikaError> {
    let base = path.to_string_lossy();

    for ext in kind.extensions() {
        let with_ext = PathBuf::from(format!("{}{}", base, ext));
        if with_ext.is_file() {
            return load_from_file(&with_ext, kind);
        }
    }

    // Try as folder
    if path.is_dir() {
        return load_from_folder(path, kind);
    }

    // Try without extension (might be folder)
    let as_folder = path.to_path_buf();
    if as_folder.is_dir() {
        return load_from_folder(&as_folder, kind);
    }

    Err(NikaError::WorkflowNotFound {
        path: path.to_string_lossy().to_string(),
    })
}

/// Load definition from a folder
fn load_from_folder(path: &Path, kind: DefinitionKind) -> Result<LoadedDefinition, NikaError> {
    // Look for standard file (AGENT.md or SKILL.md)
    let standard = path.join(kind.standard_filename());
    if standard.is_file() {
        return load_from_file(&standard, kind);
    }

    // Look for any matching file in folder
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            if entry_path.is_file() {
                let filename = entry_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("");
                for ext in kind.extensions() {
                    if filename.ends_with(ext) {
                        return load_from_file(&entry_path, kind);
                    }
                }
            }
        }
    }

    // Fallback: look for index files
    for name in &["index.md", "README.md", "main.md"] {
        let index = path.join(name);
        if index.is_file() {
            return load_from_file(&index, kind);
        }
    }

    Err(NikaError::WorkflowNotFound {
        path: path.to_string_lossy().to_string(),
    })
}

/// Load definition from a file
fn load_from_file(path: &Path, kind: DefinitionKind) -> Result<LoadedDefinition, NikaError> {
    let content = std::fs::read_to_string(path).map_err(|_| NikaError::WorkflowNotFound {
        path: path.to_string_lossy().to_string(),
    })?;

    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");

    match ext {
        "yaml" | "yml" => parse_yaml(&content, path, kind),
        "md" => parse_markdown(&content, path, kind),
        _ => {
            // Try markdown first (more common), then YAML
            parse_markdown(&content, path, kind).or_else(|_| parse_yaml(&content, path, kind))
        }
    }
}

/// Parse YAML format definition
fn parse_yaml(
    content: &str,
    path: &Path,
    _kind: DefinitionKind,
) -> Result<LoadedDefinition, NikaError> {
    let def: YamlDefinition =
        crate::util::parse_yaml_budgeted(content).map_err(|e| NikaError::ParseError {
            details: format!("{}: {}", path.display(), e),
        })?;

    let name = def.name.unwrap_or_else(|| extract_name_from_path(path));

    Ok(LoadedDefinition {
        name,
        description: def.description,
        system: def.system,
        provider: def.provider,
        model: def.model,
        max_turns: def.max_turns,
        temperature: def.temperature,
        source_path: path.to_path_buf(),
    })
}

/// Parse Markdown with YAML frontmatter (Claude Code style)
fn parse_markdown(
    content: &str,
    path: &Path,
    _kind: DefinitionKind,
) -> Result<LoadedDefinition, NikaError> {
    let (frontmatter, body) = extract_frontmatter(content)?;

    let fm: Frontmatter = if let Some(fm_str) = frontmatter {
        crate::util::parse_yaml_budgeted(&fm_str).map_err(|e| NikaError::ParseError {
            details: format!("{}: Invalid frontmatter: {}", path.display(), e),
        })?
    } else {
        Frontmatter {
            name: None,
            description: None,
            provider: None,
            model: None,
            max_turns: None,
            temperature: None,
        }
    };

    let name = fm.name.unwrap_or_else(|| extract_name_from_path(path));

    let description = fm.description.or_else(|| extract_first_paragraph(&body));

    Ok(LoadedDefinition {
        name,
        description,
        system: body.trim().to_string(),
        provider: fm.provider,
        model: fm.model,
        max_turns: fm.max_turns,
        temperature: fm.temperature,
        source_path: path.to_path_buf(),
    })
}

/// Extract YAML frontmatter from markdown content
fn extract_frontmatter(content: &str) -> Result<(Option<String>, String), NikaError> {
    let content = content.trim_start();

    if !content.starts_with("---") {
        return Ok((None, content.to_string()));
    }

    // Find end of frontmatter
    let after_start = &content[3..];
    if let Some(end_pos) = after_start.find("\n---") {
        let frontmatter = after_start[..end_pos].trim().to_string();
        let body = after_start[end_pos + 4..].trim().to_string();
        Ok((Some(frontmatter), body))
    } else {
        Err(NikaError::ParseError {
            details: "Unterminated YAML frontmatter (missing closing ---)".to_string(),
        })
    }
}

/// Extract name from file path
fn extract_name_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown");

    // Remove common suffixes
    let name = stem
        .strip_suffix(".agent")
        .or_else(|| stem.strip_suffix(".skill"))
        .unwrap_or(stem);

    // Handle standard names
    if name.eq_ignore_ascii_case("agent") || name.eq_ignore_ascii_case("skill") {
        // Use parent folder name
        path.parent()
            .and_then(|p| p.file_name())
            .and_then(|s| s.to_str())
            .unwrap_or(name)
            .to_string()
    } else {
        name.to_string()
    }
}

/// Extract first paragraph as description
fn extract_first_paragraph(content: &str) -> Option<String> {
    let content = content.trim();
    if content.is_empty() {
        return None;
    }

    // Skip any headers at the start
    let content = content
        .lines()
        .skip_while(|l| l.starts_with('#') || l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    // Take until blank line
    let paragraph: String = content
        .lines()
        .take_while(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join(" ");

    if paragraph.is_empty() {
        None
    } else {
        Some(paragraph)
    }
}

/// Discover all agents/skills in a directory
pub fn discover_definitions(
    dir: &Path,
    kind: DefinitionKind,
) -> Result<Vec<LoadedDefinition>, NikaError> {
    if !dir.is_dir() {
        return Ok(vec![]);
    }

    let mut definitions = Vec::new();

    let entries = std::fs::read_dir(dir).map_err(|_| NikaError::WorkflowNotFound {
        path: dir.to_string_lossy().to_string(),
    })?;

    for entry in entries.flatten() {
        let path = entry.path();

        // Try to load as definition — warn on parse failures so users know
        match load_definition(&path, kind) {
            Ok(def) => definitions.push(def),
            Err(e) => {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "Skipping unparseable definition file"
                );
            }
        }
    }

    Ok(definitions)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_frontmatter_with_frontmatter() {
        let content = r#"---
name: test-agent
description: A test agent
model: claude
---

This is the body content.
"#;
        let (fm, body) = extract_frontmatter(content).unwrap();
        assert!(fm.is_some());
        let fm = fm.unwrap();
        assert!(fm.contains("name: test-agent"));
        assert!(body.contains("This is the body content"));
    }

    #[test]
    fn test_extract_frontmatter_without_frontmatter() {
        let content = "Just plain markdown content.";
        let (fm, body) = extract_frontmatter(content).unwrap();
        assert!(fm.is_none());
        assert_eq!(body, content);
    }

    #[test]
    fn test_extract_frontmatter_unterminated() {
        let content = "---\nname: test\nNo closing delimiter";
        let result = extract_frontmatter(content);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_name_from_path_agent_yaml() {
        let path = Path::new("/foo/bar/researcher.agent.yaml");
        assert_eq!(extract_name_from_path(path), "researcher");
    }

    #[test]
    fn test_extract_name_from_path_skill_yaml() {
        let path = Path::new("/foo/bar/brainstorm.skill.yaml");
        assert_eq!(extract_name_from_path(path), "brainstorm");
    }

    #[test]
    fn test_extract_name_from_path_md() {
        let path = Path::new("/foo/bar/reviewer.md");
        assert_eq!(extract_name_from_path(path), "reviewer");
    }

    #[test]
    fn test_extract_name_from_path_standard_file() {
        let path = Path::new("/foo/my-agent/AGENT.md");
        assert_eq!(extract_name_from_path(path), "my-agent");
    }

    #[test]
    fn test_extract_first_paragraph() {
        let content = r#"# Header

First paragraph content here.
More of the same paragraph.

Second paragraph.
"#;
        let result = extract_first_paragraph(content);
        assert_eq!(
            result,
            Some("First paragraph content here. More of the same paragraph.".to_string())
        );
    }

    #[test]
    fn test_extract_first_paragraph_empty() {
        assert_eq!(extract_first_paragraph(""), None);
        assert_eq!(extract_first_paragraph("# Just a header"), None);
    }

    #[test]
    fn test_definition_kind_extensions() {
        assert!(DefinitionKind::Agent.extensions().contains(&".agent.yaml"));
        assert!(DefinitionKind::Agent.extensions().contains(&".md"));
        assert!(DefinitionKind::Skill.extensions().contains(&".skill.yaml"));
    }

    #[test]
    fn test_definition_kind_standard_filename() {
        assert_eq!(DefinitionKind::Agent.standard_filename(), "AGENT.md");
        assert_eq!(DefinitionKind::Skill.standard_filename(), "SKILL.md");
    }

    #[test]
    fn test_parse_yaml_definition() {
        let yaml = r#"
name: test-agent
description: A test agent
system: You are a helpful assistant.
provider: claude
model: claude-sonnet-4-6
max_turns: 5
"#;
        let path = Path::new("test.agent.yaml");
        let def = parse_yaml(yaml, path, DefinitionKind::Agent).unwrap();

        assert_eq!(def.name, "test-agent");
        assert_eq!(def.description, Some("A test agent".to_string()));
        assert_eq!(def.system, "You are a helpful assistant.");
        assert_eq!(def.provider, Some("claude".to_string()));
        assert_eq!(def.model, Some("claude-sonnet-4-6".to_string()));
        assert_eq!(def.max_turns, Some(5));
    }

    #[test]
    fn test_parse_markdown_with_frontmatter() {
        let md = r#"---
name: code-reviewer
description: Reviews code quality
model: sonnet
---

You are a Senior Code Reviewer with expertise in software architecture.

Your role is to review completed project steps.
"#;
        let path = Path::new("code-reviewer.md");
        let def = parse_markdown(md, path, DefinitionKind::Agent).unwrap();

        assert_eq!(def.name, "code-reviewer");
        assert_eq!(def.description, Some("Reviews code quality".to_string()));
        assert!(def.system.contains("Senior Code Reviewer"));
        assert_eq!(def.model, Some("sonnet".to_string()));
    }

    #[test]
    fn test_parse_markdown_without_frontmatter() {
        let md = "You are a simple assistant.\n\nHelp the user.";
        let path = Path::new("simple-agent.md");
        let def = parse_markdown(md, path, DefinitionKind::Agent).unwrap();

        assert_eq!(def.name, "simple-agent");
        assert!(def.system.contains("simple assistant"));
    }

    #[test]
    fn test_loaded_definition_has_source_path() {
        let md = "---\nname: test\n---\nBody";
        let path = Path::new("/foo/bar/test.md");
        let def = parse_markdown(md, path, DefinitionKind::Agent).unwrap();

        assert_eq!(def.source_path, path);
    }
}
