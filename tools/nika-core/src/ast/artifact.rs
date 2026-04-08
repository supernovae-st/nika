// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Artifact configuration for file output
//!
//! Provides declarative file persistence for workflow task outputs.
//! Artifacts are written atomically and tracked in a manifest.
//!
//! # YAML Syntax
//!
//! ```yaml
//! # Workflow-level defaults
//! artifacts:
//!   dir: ./output/{{context.meta.date}}
//!   format: json
//!   mode: overwrite
//!   manifest: true
//!   max_size: 104857600  # 100MB
//!
//! tasks:
//!   # Simple: use defaults
//!   - id: task1
//!     artifact: true
//!
//!   # Custom path
//!   - id: task2
//!     artifact:
//!       path: ./data/{{context.meta.task_id}}.json
//!
//!   # Multiple outputs
//!   - id: task3
//!     artifact:
//!       - path: ./raw.json
//!         source: raw_data
//!       - path: ./processed.json
//! ```

use serde::{Deserialize, Serialize};

// ═══════════════════════════════════════════════════════════════════════════
// WORKFLOW-LEVEL CONFIG
// ═══════════════════════════════════════════════════════════════════════════

/// Default max artifact size: 100MB
pub const DEFAULT_MAX_ARTIFACT_SIZE: u64 = 100 * 1024 * 1024;

/// Workflow-level artifact defaults
///
/// Configures default behavior for all task artifacts.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct ArtifactsConfig {
    /// Base directory for artifacts (supports context.meta.* templates)
    #[serde(default)]
    pub dir: Option<String>,

    /// Default output format
    #[serde(default)]
    pub format: ArtifactFormat,

    /// Default write mode
    #[serde(default)]
    pub mode: ArtifactMode,

    /// Generate artifacts.json manifest at workflow end
    #[serde(default)]
    pub manifest: bool,

    /// Maximum artifact size in bytes (default: 100MB)
    #[serde(default = "default_max_size")]
    pub max_size: u64,
}

fn default_max_size() -> u64 {
    DEFAULT_MAX_ARTIFACT_SIZE
}

// ═══════════════════════════════════════════════════════════════════════════
// TASK-LEVEL SPEC
// ═══════════════════════════════════════════════════════════════════════════

/// Task-level artifact specification
///
/// Supports three forms:
/// - `artifact: true` — use workflow defaults
/// - `artifact: { path: ... }` — single artifact with options
/// - `artifact: [{ path: ... }, ...]` — multiple artifacts
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum ArtifactSpec {
    /// Simple boolean: use defaults
    Enabled(bool),

    /// Single artifact with options
    Single(ArtifactOutput),

    /// Multiple artifacts
    Multiple(Vec<ArtifactOutput>),
}

impl Default for ArtifactSpec {
    fn default() -> Self {
        Self::Enabled(false)
    }
}

/// Individual artifact output configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ArtifactOutput {
    /// Output path (supports context.meta.* templates)
    pub path: String,

    /// Which binding to save (default: task result)
    #[serde(default)]
    pub source: Option<String>,

    /// Template string to render (supports {{with.*}} bindings)
    /// If set, this is used instead of task output
    #[serde(default)]
    pub template: Option<String>,

    /// Output format override
    #[serde(default)]
    pub format: Option<ArtifactFormat>,

    /// Write mode override
    #[serde(default)]
    pub mode: Option<ArtifactMode>,
}

// ═══════════════════════════════════════════════════════════════════════════
// ENUMS
// ═══════════════════════════════════════════════════════════════════════════

/// Artifact output format
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactFormat {
    /// Plain text (default)
    #[default]
    Text,

    /// JSON (pretty-printed)
    Json,

    /// YAML format
    Yaml,

    /// Binary (raw bytes from CAS store)
    Binary,

    /// Markdown format
    Markdown,
}

impl std::fmt::Display for ArtifactFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Text => write!(f, "text"),
            Self::Json => write!(f, "json"),
            Self::Yaml => write!(f, "yaml"),
            Self::Binary => write!(f, "binary"),
            Self::Markdown => write!(f, "markdown"),
        }
    }
}

impl ArtifactFormat {
    /// Get file extension for this format
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Text => "txt",
            Self::Json => "json",
            Self::Yaml => "yaml",
            Self::Binary => "bin",
            Self::Markdown => "md",
        }
    }
}

/// File write mode
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum ArtifactMode {
    /// Overwrite existing file (default)
    #[default]
    Overwrite,

    /// Append to existing file
    Append,

    /// Create unique filename if exists (file.json → file-1.json)
    Unique,

    /// Fail if file already exists
    Fail,
}

impl std::fmt::Display for ArtifactMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Overwrite => write!(f, "overwrite"),
            Self::Append => write!(f, "append"),
            Self::Unique => write!(f, "unique"),
            Self::Fail => write!(f, "fail"),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// MANIFEST ENTRY
// ═══════════════════════════════════════════════════════════════════════════

/// Artifact metadata for manifest and events
///
/// Returned by ArtifactWriter after successful write.
/// Accumulated by Runner for manifest generation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactEntry {
    /// Task that produced this artifact
    pub task_id: String,

    /// Absolute path where artifact was written
    pub path: String,

    /// File size in bytes
    pub size: u64,

    /// Format used (text, json, yaml)
    pub format: String,

    /// xxHash3 checksum (not cryptographic, for integrity)
    pub checksum: String,

    /// Creation timestamp (UTC)
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// ═══════════════════════════════════════════════════════════════════════════
// TESTS
// ═══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serde_yaml;

    #[test]
    fn test_parse_artifact_enabled() {
        let yaml = r#"true"#;
        let spec: ArtifactSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(spec, ArtifactSpec::Enabled(true)));
    }

    #[test]
    fn test_parse_artifact_disabled() {
        let yaml = r#"false"#;
        let spec: ArtifactSpec = serde_yaml::from_str(yaml).unwrap();
        assert!(matches!(spec, ArtifactSpec::Enabled(false)));
    }

    #[test]
    fn test_parse_artifact_single() {
        let yaml = r#"
path: ./output/{{context.meta.task_id}}.json
mode: unique
"#;
        let spec: ArtifactSpec = serde_yaml::from_str(yaml).unwrap();
        match spec {
            ArtifactSpec::Single(output) => {
                assert!(output.path.contains("context.meta.task_id"));
                assert_eq!(output.mode, Some(ArtifactMode::Unique));
            }
            _ => panic!("Expected Single artifact"),
        }
    }

    #[test]
    fn test_parse_artifact_multiple() {
        let yaml = r#"
- path: ./raw.json
  source: raw_data
- path: ./processed.json
  format: json
"#;
        let spec: ArtifactSpec = serde_yaml::from_str(yaml).unwrap();
        match spec {
            ArtifactSpec::Multiple(outputs) => {
                assert_eq!(outputs.len(), 2);
                assert_eq!(outputs[0].source, Some("raw_data".to_string()));
                assert_eq!(outputs[1].format, Some(ArtifactFormat::Json));
            }
            _ => panic!("Expected Multiple artifacts"),
        }
    }

    #[test]
    fn test_parse_artifacts_config() {
        let yaml = r#"
dir: ./output/{{context.meta.date}}
format: json
mode: overwrite
manifest: true
max_size: 52428800
"#;
        let config: ArtifactsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            config.dir,
            Some("./output/{{context.meta.date}}".to_string())
        );
        assert_eq!(config.format, ArtifactFormat::Json);
        assert_eq!(config.mode, ArtifactMode::Overwrite);
        assert!(config.manifest);
        assert_eq!(config.max_size, 50 * 1024 * 1024); // 50MB
    }

    #[test]
    fn test_artifacts_config_defaults() {
        let yaml = "{}";
        let config: ArtifactsConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.dir, None);
        assert_eq!(config.format, ArtifactFormat::Text);
        assert_eq!(config.mode, ArtifactMode::Overwrite);
        assert!(!config.manifest);
        assert_eq!(config.max_size, DEFAULT_MAX_ARTIFACT_SIZE);
    }

    #[test]
    fn test_artifact_format_display() {
        assert_eq!(ArtifactFormat::Text.to_string(), "text");
        assert_eq!(ArtifactFormat::Json.to_string(), "json");
        assert_eq!(ArtifactFormat::Yaml.to_string(), "yaml");
        assert_eq!(ArtifactFormat::Binary.to_string(), "binary");
        assert_eq!(ArtifactFormat::Markdown.to_string(), "markdown");
    }

    #[test]
    fn test_artifact_format_extension() {
        assert_eq!(ArtifactFormat::Text.extension(), "txt");
        assert_eq!(ArtifactFormat::Json.extension(), "json");
        assert_eq!(ArtifactFormat::Yaml.extension(), "yaml");
        assert_eq!(ArtifactFormat::Binary.extension(), "bin");
        assert_eq!(ArtifactFormat::Markdown.extension(), "md");
    }

    #[test]
    fn test_artifact_mode_display() {
        assert_eq!(ArtifactMode::Overwrite.to_string(), "overwrite");
        assert_eq!(ArtifactMode::Append.to_string(), "append");
        assert_eq!(ArtifactMode::Unique.to_string(), "unique");
        assert_eq!(ArtifactMode::Fail.to_string(), "fail");
    }

    #[test]
    fn test_artifact_format_binary_serde() {
        // Deserialize from YAML
        let yaml = r#""binary""#;
        let format: ArtifactFormat = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(format, ArtifactFormat::Binary);

        // Serialize to JSON
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, r#""binary""#);

        // Deserialize from JSON
        let back: ArtifactFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ArtifactFormat::Binary);

        // Extension
        assert_eq!(ArtifactFormat::Binary.extension(), "bin");

        // Display
        assert_eq!(ArtifactFormat::Binary.to_string(), "binary");
    }

    #[test]
    fn test_artifact_format_binary_in_artifact_spec() {
        let yaml = r#"
path: ./output/image.bin
format: binary
"#;
        let spec: ArtifactSpec = serde_yaml::from_str(yaml).unwrap();
        match spec {
            ArtifactSpec::Single(output) => {
                assert_eq!(output.format, Some(ArtifactFormat::Binary));
                assert_eq!(output.path, "./output/image.bin");
            }
            _ => panic!("Expected Single artifact"),
        }
    }

    #[test]
    fn test_artifact_format_markdown_serde() {
        // Deserialize from YAML
        let yaml = r#""markdown""#;
        let format: ArtifactFormat = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(format, ArtifactFormat::Markdown);

        // Serialize to JSON
        let json = serde_json::to_string(&format).unwrap();
        assert_eq!(json, r#""markdown""#);

        // Deserialize from JSON
        let back: ArtifactFormat = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ArtifactFormat::Markdown);

        // Extension
        assert_eq!(ArtifactFormat::Markdown.extension(), "md");

        // Display
        assert_eq!(ArtifactFormat::Markdown.to_string(), "markdown");
    }

    #[test]
    fn test_artifact_format_markdown_in_artifact_spec() {
        let yaml = r#"
path: report.md
format: markdown
"#;
        let spec: ArtifactSpec = serde_yaml::from_str(yaml).unwrap();
        match spec {
            ArtifactSpec::Single(output) => {
                assert_eq!(output.format, Some(ArtifactFormat::Markdown));
                assert_eq!(output.path, "report.md");
            }
            _ => panic!("Expected Single artifact"),
        }
    }

    #[test]
    fn test_artifact_entry_serialization() {
        let entry = ArtifactEntry {
            task_id: "test_task".to_string(),
            path: "/tmp/output.json".to_string(),
            size: 1234,
            format: "json".to_string(),
            checksum: "abc123".to_string(),
            created_at: chrono::Utc::now(),
        };

        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("test_task"));
        assert!(json.contains("1234"));
    }
}
