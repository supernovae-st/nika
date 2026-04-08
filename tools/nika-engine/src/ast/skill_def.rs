// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Skill definition types for workflow
//!
//! The `skills:` block in a workflow allows loading prompt augmentation files.
//! Skills are loaded at workflow start and injected into agent system prompts.
//!
//! # Example
//!
//! ```yaml
//! skills:
//!   seo: ./skills/seo-writer.skill.md       # Single skill file
//!   brand: ./skills/brand-voice.skill.md    # Another skill
//! ```
//!
//! Skills can be referenced in agent tasks:
//!
//! ```yaml
//! tasks:
//!   - id: generate_seo
//!     agent:
//!       prompt: "Write SEO content"
//!       skill: seo           # Single skill
//!       # OR
//!       skills: [seo, brand] # Multiple skills
//! ```
//!
//! ## pkg: URI Support
//!
//! Skills can also be loaded from the package registry using `pkg:` URIs:
//!
//! ```yaml
//! skills:
//!   rust: pkg:@supernovae/skills@1.0.0/rust.md
//!   seo: pkg:skills/seo-writer.md  # Uses default scope and latest version
//! ```

use serde::Deserialize;
use std::path::{Path, PathBuf};

use super::pkg_resolver::PkgUri;
use crate::error::NikaError;

/// Resolve a skill path, handling both local paths and pkg: URIs
///
/// # Arguments
/// * `skill_path` - The skill path from YAML (local path or `pkg:` URI)
/// * `base_dir` - Base directory for resolving relative local paths
///
/// # Returns
/// * `Ok(PathBuf)` - Resolved absolute path to skill file
/// * `Err(NikaError)` - If pkg: URI is invalid or path resolution fails
///
/// # Examples
/// ```
/// use std::path::{Path, PathBuf};
/// use nika_engine::ast::skill_def::resolve_skill_path;
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// // Local relative path (resolved against base_dir)
/// let path = resolve_skill_path("./skills/seo.skill.md", Path::new("/project"))?;
/// assert_eq!(path, PathBuf::from("/project/./skills/seo.skill.md"));
///
/// // Path traversal is blocked:
/// assert!(resolve_skill_path("../../../../etc/passwd", Path::new("/project")).is_err());
/// # Ok(())
/// # }
/// ```
pub fn resolve_skill_path(skill_path: &str, base_dir: &Path) -> Result<PathBuf, NikaError> {
    if skill_path.starts_with("pkg:") {
        // Parse and resolve pkg: URI
        let uri = PkgUri::parse(skill_path)?;
        uri.resolve()
    } else {
        // Local path - resolve relative to base_dir
        let path = Path::new(skill_path);
        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base_dir.join(path)
        };

        // SECURITY: Block path traversal via .. components.
        // Check both the raw skill_path (before join) and the resolved path.
        // This prevents attacks like `skills: { evil: "../../../../etc/passwd" }`.
        validate_skill_path_boundary(skill_path, &full_path, base_dir)?;

        Ok(full_path)
    }
}

/// Validate that a skill path does not escape the project boundary.
///
/// Uses component-based traversal detection (no file existence required).
/// Blocks:
/// - `..` components in the skill path
/// - Absolute paths pointing outside the project
fn validate_skill_path_boundary(
    skill_path: &str,
    full_path: &Path,
    base_dir: &Path,
) -> Result<(), NikaError> {
    use std::path::Component;

    // Check for parent directory components in the raw path
    let has_parent_traversal = Path::new(skill_path)
        .components()
        .any(|c| matches!(c, Component::ParentDir));

    if has_parent_traversal {
        return Err(NikaError::SkillLoadError {
            skill: skill_path.to_string(),
            reason: format!(
                "Path traversal blocked: '{}' contains '..' components (must stay within project boundary '{}')",
                skill_path,
                base_dir.display()
            ),
        });
    }

    // For absolute paths: verify they start with the base directory
    if full_path.is_absolute() && !full_path.starts_with(base_dir) {
        // Allow pkg: paths (handled separately) and absolute paths within project
        return Err(NikaError::SkillLoadError {
            skill: skill_path.to_string(),
            reason: format!(
                "Path traversal blocked: '{}' is outside project boundary '{}'",
                full_path.display(),
                base_dir.display()
            ),
        });
    }

    Ok(())
}

/// Check if a skill path is a pkg: URI
///
/// # Examples
/// ```
/// use nika_engine::ast::skill_def::is_pkg_uri;
///
/// assert!(is_pkg_uri("pkg:@supernovae/skills@1.0.0/rust.md"));
/// assert!(is_pkg_uri("pkg:skills/seo.md"));
/// assert!(!is_pkg_uri("./skills/local.skill.md"));
/// ```
pub fn is_pkg_uri(skill_path: &str) -> bool {
    skill_path.starts_with("pkg:")
}

/// Skill definition
///
/// A skill is a path to a skill file (.skill.md) containing prompt augmentation.
pub type SkillDef = String;

/// Skill reference for agent tasks
///
/// Agents can reference skills by name (single or multiple).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum SkillRef {
    /// Single skill reference
    Single(String),

    /// Multiple skill references
    Multiple(Vec<String>),
}

impl SkillRef {
    /// Get all skill names as a vector
    pub fn names(&self) -> Vec<&str> {
        match self {
            SkillRef::Single(name) => vec![name.as_str()],
            SkillRef::Multiple(names) => names.iter().map(|s| s.as_str()).collect(),
        }
    }

    /// Check if this reference includes a specific skill
    pub fn contains(&self, skill_name: &str) -> bool {
        match self {
            SkillRef::Single(name) => name == skill_name,
            SkillRef::Multiple(names) => names.iter().any(|n| n == skill_name),
        }
    }

    /// Get the count of referenced skills
    pub fn len(&self) -> usize {
        match self {
            SkillRef::Single(_) => 1,
            SkillRef::Multiple(names) => names.len(),
        }
    }

    /// Check if no skills are referenced
    pub fn is_empty(&self) -> bool {
        match self {
            SkillRef::Single(_) => false,
            SkillRef::Multiple(names) => names.is_empty(),
        }
    }
}

impl Default for SkillRef {
    fn default() -> Self {
        SkillRef::Multiple(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_skill_def_is_string() {
        let skill: SkillDef = "./skills/seo-writer.skill.md".to_string();
        assert!(skill.ends_with(".skill.md"));
    }

    #[test]
    fn test_skill_ref_single() {
        let yaml = r#""seo""#;
        let skill_ref: SkillRef = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(skill_ref, SkillRef::Single(_)));
        assert_eq!(skill_ref.names(), vec!["seo"]);
        assert!(skill_ref.contains("seo"));
        assert!(!skill_ref.contains("brand"));
        assert_eq!(skill_ref.len(), 1);
        assert!(!skill_ref.is_empty());
    }

    #[test]
    fn test_skill_ref_multiple() {
        let yaml = r#"["seo", "brand", "tone"]"#;
        let skill_ref: SkillRef = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(skill_ref, SkillRef::Multiple(_)));
        assert_eq!(skill_ref.names(), vec!["seo", "brand", "tone"]);
        assert!(skill_ref.contains("seo"));
        assert!(skill_ref.contains("brand"));
        assert!(!skill_ref.contains("unknown"));
        assert_eq!(skill_ref.len(), 3);
        assert!(!skill_ref.is_empty());
    }

    #[test]
    fn test_skill_ref_empty_multiple() {
        let yaml = r#"[]"#;
        let skill_ref: SkillRef = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(skill_ref, SkillRef::Multiple(_)));
        assert!(skill_ref.names().is_empty());
        assert_eq!(skill_ref.len(), 0);
        assert!(skill_ref.is_empty());
    }

    #[test]
    fn test_skill_ref_default() {
        let skill_ref = SkillRef::default();
        assert!(skill_ref.is_empty());
        assert_eq!(skill_ref.len(), 0);
    }

    #[test]
    fn test_skill_ref_in_context() {
        // Test how it would appear in a task YAML
        #[derive(Debug, Deserialize)]
        struct TestTask {
            skill: Option<SkillRef>,
            skills: Option<SkillRef>,
        }

        // Single skill via 'skill' field
        let yaml = r#"
skill: seo
"#;
        let task: TestTask = serde_yaml::from_str(yaml).unwrap();
        assert!(task.skill.is_some());
        assert!(task.skill.as_ref().unwrap().contains("seo"));

        // Multiple skills via 'skills' field
        let yaml = r#"
skills: [seo, brand]
"#;
        let task: TestTask = serde_yaml::from_str(yaml).unwrap();
        assert!(task.skills.is_some());
        assert_eq!(task.skills.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_is_pkg_uri_with_pkg_prefix() {
        assert!(is_pkg_uri("pkg:@supernovae/skills@1.0.0/rust.md"));
        assert!(is_pkg_uri("pkg:skills/seo.md"));
        assert!(is_pkg_uri("pkg:@scope/name/path.md"));
    }

    #[test]
    fn test_is_pkg_uri_with_local_path() {
        assert!(!is_pkg_uri("./skills/local.skill.md"));
        assert!(!is_pkg_uri("../skills/up.skill.md"));
        assert!(!is_pkg_uri("/absolute/path/skill.md"));
        assert!(!is_pkg_uri("relative/skill.md"));
    }

    #[test]
    fn test_resolve_skill_path_local_relative() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("./skills/seo.skill.md", base_dir).unwrap();
        assert_eq!(result, PathBuf::from("/project/./skills/seo.skill.md"));
    }

    #[test]
    fn test_resolve_skill_path_local_absolute_outside_project_blocked() {
        let base_dir = Path::new("/project");
        // Absolute paths outside the project boundary are now blocked (security fix)
        let result = resolve_skill_path("/absolute/path/skill.md", base_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_path_local_absolute_inside_project_allowed() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("/project/skills/seo.md", base_dir).unwrap();
        assert_eq!(result, PathBuf::from("/project/skills/seo.md"));
    }

    #[test]
    fn test_resolve_skill_path_pkg_uri() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("pkg:@supernovae/skills@1.0.0/rust.md", base_dir).unwrap();
        let expected = dirs::home_dir()
            .unwrap()
            .join(".nika/packages/@supernovae/skills/1.0.0/rust.md");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_resolve_skill_path_pkg_uri_no_version() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("pkg:@supernovae/skills/rust.md", base_dir).unwrap();
        let expected = dirs::home_dir()
            .unwrap()
            .join(".nika/packages/@supernovae/skills/latest/rust.md");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_resolve_skill_path_pkg_uri_no_scope() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("pkg:skills/seo.md", base_dir).unwrap();
        let expected = dirs::home_dir()
            .unwrap()
            .join(".nika/packages/@default/skills/latest/seo.md");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_resolve_skill_path_invalid_pkg_uri() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("pkg:", base_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_path_blocks_path_traversal() {
        let base_dir = Path::new("/project");
        // Parent directory traversal must be blocked
        let result = resolve_skill_path("../../../../etc/passwd", base_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Path traversal blocked"), "Got: {err}");

        // More subtle traversal
        let result = resolve_skill_path("./skills/../../../etc/shadow", base_dir);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_skill_path_allows_normal_relative() {
        let base_dir = Path::new("/project");
        // Normal relative paths without .. are fine
        let result = resolve_skill_path("skills/seo.md", base_dir);
        assert!(result.is_ok());
        let result = resolve_skill_path("./skills/brand.md", base_dir);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_skill_path_blocks_absolute_outside_project() {
        let base_dir = Path::new("/project");
        let result = resolve_skill_path("/etc/passwd", base_dir);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Path traversal blocked"), "Got: {err}");
    }
}
