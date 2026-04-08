// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Registry type definitions for the SuperNovae package system.
//!
//! This module defines the core types for the `~/.nika/packages/` registry:
//! - `Manifest` - Package metadata from manifest.yaml
//! - `SkillEntry` - Individual skill definition within a package
//! - `RegistryIndex` - Installed packages index (registry.yaml)
//! - `InstalledPackage` - Metadata for an installed package

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Package manifest loaded from `manifest.yaml`.
///
/// Located at: `~/.nika/packages/@scope/name/version/manifest.yaml`
///
/// # Example
///
/// ```yaml
/// name: "@supernovae/workflows"
/// version: "1.0.0"
/// description: "Production workflow templates"
/// authors:
///   - "SuperNovae Team"
/// license: "MIT"
/// repository: "https://github.com/supernovae/workflows"
/// skills:
///   brainstorm:
///     path: "skills/brainstorm.skill.md"
///     description: "Collaborative ideation skill"
///   review:
///     path: "skills/review.skill.md"
///     description: "Code review skill"
/// dependencies:
///   "@supernovae/core": "^0.8.0"
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Package name (e.g., "@supernovae/workflows")
    pub name: String,

    /// Semantic version (e.g., "1.0.0")
    pub version: String,

    /// Human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Package authors
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authors: Option<Vec<String>>,

    /// SPDX license identifier (e.g., "MIT", "Apache-2.0")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Repository URL
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// Skills provided by this package
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub skills: HashMap<String, SkillEntry>,

    /// Package dependencies (name -> version constraint)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dependencies: Option<HashMap<String, String>>,
}

impl Manifest {
    /// Create a new manifest with required fields.
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            description: None,
            authors: None,
            license: None,
            repository: None,
            skills: HashMap::new(),
            dependencies: None,
        }
    }

    /// Get the scoped package identifier (e.g., "@scope/name@1.0.0").
    pub fn identifier(&self) -> String {
        format!("{}@{}", self.name, self.version)
    }

    /// Check if this package has any skills.
    pub fn has_skills(&self) -> bool {
        !self.skills.is_empty()
    }

    /// Get skill count.
    pub fn skill_count(&self) -> usize {
        self.skills.len()
    }
}

/// Individual skill entry within a package manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillEntry {
    /// Relative path to skill file from package root
    pub path: String,

    /// Human-readable description
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl SkillEntry {
    /// Create a new skill entry with required path.
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            description: None,
        }
    }

    /// Create a skill entry with path and description.
    pub fn with_description(path: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            description: Some(description.into()),
        }
    }
}

/// Registry index tracking all installed packages.
///
/// Located at: `~/.nika/registry.yaml`
///
/// # Example
///
/// ```yaml
/// packages:
///   "@supernovae/workflows":
///     version: "1.0.0"
///     installed_at: "2026-03-01T10:30:00Z"
///     manifest_path: "packages/@supernovae/workflows/1.0.0/manifest.yaml"
///   "@supernovae/core":
///     version: "0.8.0"
///     installed_at: "2026-02-28T15:45:00Z"
///     manifest_path: "packages/@supernovae/core/0.8.0/manifest.yaml"
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RegistryIndex {
    /// Map of package name -> installed package info
    #[serde(default)]
    pub packages: HashMap<String, InstalledPackage>,
}

impl RegistryIndex {
    /// Create an empty registry index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a package is installed.
    pub fn is_installed(&self, name: &str) -> bool {
        self.packages.contains_key(name)
    }

    /// Get installed package info.
    pub fn get(&self, name: &str) -> Option<&InstalledPackage> {
        self.packages.get(name)
    }

    /// Add or update an installed package.
    pub fn insert(&mut self, name: impl Into<String>, package: InstalledPackage) {
        self.packages.insert(name.into(), package);
    }

    /// Remove a package from the index.
    pub fn remove(&mut self, name: &str) -> Option<InstalledPackage> {
        self.packages.remove(name)
    }

    /// Get the number of installed packages.
    pub fn len(&self) -> usize {
        self.packages.len()
    }

    /// Check if the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }

    /// Iterate over installed packages.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &InstalledPackage)> {
        self.packages.iter()
    }
}

/// Metadata for an installed package in the registry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstalledPackage {
    /// Installed version
    pub version: String,

    /// ISO 8601 timestamp of installation
    pub installed_at: String,

    /// Relative path to manifest.yaml from ~/.nika/
    pub manifest_path: String,
}

impl InstalledPackage {
    /// Create new installed package metadata.
    pub fn new(
        version: impl Into<String>,
        installed_at: impl Into<String>,
        manifest_path: impl Into<String>,
    ) -> Self {
        Self {
            version: version.into(),
            installed_at: installed_at.into(),
            manifest_path: manifest_path.into(),
        }
    }

    /// Create installed package with current timestamp.
    pub fn now(version: impl Into<String>, manifest_path: impl Into<String>) -> Self {
        Self {
            version: version.into(),
            installed_at: chrono::Utc::now().to_rfc3339(),
            manifest_path: manifest_path.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_saphyr;

    #[test]
    fn test_manifest_new() {
        let manifest = Manifest::new("@supernovae/test", "1.0.0");
        assert_eq!(manifest.name, "@supernovae/test");
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.description.is_none());
        assert!(manifest.skills.is_empty());
    }

    #[test]
    fn test_manifest_identifier() {
        let manifest = Manifest::new("@supernovae/workflows", "2.1.0");
        assert_eq!(manifest.identifier(), "@supernovae/workflows@2.1.0");
    }

    #[test]
    fn test_manifest_has_skills() {
        let mut manifest = Manifest::new("@test/pkg", "1.0.0");
        assert!(!manifest.has_skills());

        manifest
            .skills
            .insert("test".to_string(), SkillEntry::new("skills/test.md"));
        assert!(manifest.has_skills());
        assert_eq!(manifest.skill_count(), 1);
    }

    #[test]
    fn test_manifest_yaml_roundtrip() {
        let mut manifest = Manifest::new("@supernovae/workflows", "1.0.0");
        manifest.description = Some("Test package".to_string());
        manifest.authors = Some(vec!["Author One".to_string()]);
        manifest.license = Some("MIT".to_string());
        manifest.skills.insert(
            "brainstorm".to_string(),
            SkillEntry::with_description("skills/brainstorm.md", "Brainstorm skill"),
        );

        let yaml = serde_saphyr::to_string(&manifest).unwrap();
        let parsed: Manifest = serde_saphyr::from_str(&yaml).unwrap();

        assert_eq!(manifest, parsed);
    }

    #[test]
    fn test_skill_entry_new() {
        let entry = SkillEntry::new("skills/test.skill.md");
        assert_eq!(entry.path, "skills/test.skill.md");
        assert!(entry.description.is_none());
    }

    #[test]
    fn test_skill_entry_with_description() {
        let entry = SkillEntry::with_description("skills/review.md", "Code review skill");
        assert_eq!(entry.path, "skills/review.md");
        assert_eq!(entry.description.as_deref(), Some("Code review skill"));
    }

    #[test]
    fn test_registry_index_operations() {
        let mut index = RegistryIndex::new();
        assert!(index.is_empty());
        assert!(!index.is_installed("@test/pkg"));

        let pkg = InstalledPackage::new(
            "1.0.0",
            "2026-03-01T10:00:00Z",
            "packages/@test/pkg/1.0.0/manifest.yaml",
        );
        index.insert("@test/pkg", pkg.clone());

        assert!(!index.is_empty());
        assert_eq!(index.len(), 1);
        assert!(index.is_installed("@test/pkg"));
        assert_eq!(index.get("@test/pkg"), Some(&pkg));

        let removed = index.remove("@test/pkg");
        assert_eq!(removed, Some(pkg));
        assert!(index.is_empty());
    }

    #[test]
    fn test_registry_index_yaml_roundtrip() {
        let mut index = RegistryIndex::new();
        index.insert(
            "@supernovae/workflows",
            InstalledPackage::new(
                "1.0.0",
                "2026-03-01T10:30:00Z",
                "packages/@supernovae/workflows/1.0.0/manifest.yaml",
            ),
        );
        index.insert(
            "@supernovae/core",
            InstalledPackage::new(
                "0.8.0",
                "2026-02-28T15:45:00Z",
                "packages/@supernovae/core/0.8.0/manifest.yaml",
            ),
        );

        let yaml = serde_saphyr::to_string(&index).unwrap();
        let parsed: RegistryIndex = serde_saphyr::from_str(&yaml).unwrap();

        assert_eq!(index.len(), parsed.len());
        assert!(parsed.is_installed("@supernovae/workflows"));
        assert!(parsed.is_installed("@supernovae/core"));
    }

    #[test]
    fn test_installed_package_new() {
        let pkg = InstalledPackage::new(
            "2.0.0",
            "2026-03-01T12:00:00Z",
            "packages/@test/pkg/2.0.0/manifest.yaml",
        );
        assert_eq!(pkg.version, "2.0.0");
        assert_eq!(pkg.installed_at, "2026-03-01T12:00:00Z");
        assert_eq!(pkg.manifest_path, "packages/@test/pkg/2.0.0/manifest.yaml");
    }

    #[test]
    fn test_installed_package_now() {
        let pkg = InstalledPackage::now("1.0.0", "packages/@test/pkg/1.0.0/manifest.yaml");
        assert_eq!(pkg.version, "1.0.0");
        assert!(!pkg.installed_at.is_empty());
        // Check ISO 8601 format
        assert!(pkg.installed_at.contains('T'));
    }

    #[test]
    fn test_registry_index_iter() {
        let mut index = RegistryIndex::new();
        index.insert(
            "@pkg/a",
            InstalledPackage::new("1.0.0", "2026-01-01T00:00:00Z", "a/manifest.yaml"),
        );
        index.insert(
            "@pkg/b",
            InstalledPackage::new("2.0.0", "2026-01-02T00:00:00Z", "b/manifest.yaml"),
        );

        let names: Vec<_> = index.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names.len(), 2);
        assert!(names.contains(&"@pkg/a"));
        assert!(names.contains(&"@pkg/b"));
    }
}
