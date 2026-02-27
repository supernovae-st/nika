//! Skill definition types for workflow (v0.6)
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

use serde::Deserialize;

/// Skill definition (v0.6)
///
/// A skill is a path to a skill file (.skill.md) containing prompt augmentation.
pub type SkillDef = String;

/// Skill reference for agent tasks (v0.6)
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
}
