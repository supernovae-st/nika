// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Artifact configuration — the `artifacts:` section at workflow root.
//!
//! Controls how workflow outputs are captured, stored, and made available
//! after execution (files, reports, exports).

use serde::{Deserialize, Serialize};

/// Artifacts configuration for a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ArtifactsConfig {
    /// List of artifact specifications.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub artifacts: Vec<ArtifactSpec>,
    /// Base directory for artifact output (relative to run dir).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
}

impl ArtifactsConfig {
    /// Create a new empty artifacts configuration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            artifacts: Vec::new(),
            output_dir: None,
        }
    }
}

impl Default for ArtifactsConfig {
    fn default() -> Self {
        Self::new()
    }
}

/// A single artifact to capture from workflow output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ArtifactSpec {
    /// Artifact name (used as filename if no path specified).
    pub name: String,
    /// Source task whose output to capture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    /// Output file path (relative to `output_dir`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Format to write the artifact in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

impl ArtifactSpec {
    /// Create a new artifact spec with the given name.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            from: None,
            path: None,
            format: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn artifacts_config_empty() {
        let ac = ArtifactsConfig::new();
        assert!(ac.artifacts.is_empty());
        assert!(ac.output_dir.is_none());
    }

    #[test]
    fn artifact_spec_new() {
        let spec = ArtifactSpec::new("report");
        assert_eq!(spec.name, "report");
        assert!(spec.from.is_none());
    }

    #[test]
    fn serde_roundtrip() {
        let ac = ArtifactsConfig {
            artifacts: vec![
                ArtifactSpec {
                    name: "summary".into(),
                    from: Some("analyze_task".into()),
                    path: Some("summary.md".into()),
                    format: Some("markdown".into()),
                },
                ArtifactSpec::new("data"),
            ],
            output_dir: Some("./output".into()),
        };
        let json = serde_json::to_string(&ac).expect("serialize");
        let back: ArtifactsConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.artifacts.len(), 2);
        assert_eq!(back.artifacts[0].name, "summary");
        assert_eq!(back.output_dir.as_deref(), Some("./output"));
    }
}
