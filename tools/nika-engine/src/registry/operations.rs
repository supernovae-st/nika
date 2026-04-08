// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Registry operations for the SuperNovae package system.
//!
//! This module provides file system operations for the `~/.nika/` registry:
//! - Package directory management
//! - Registry index loading/saving
//! - Manifest loading
//! - Installation status checking
//!
//! Path operations delegate to `core::paths` for the unified home directory.

use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

#[cfg(test)]
use crate::registry::types::InstalledPackage;
use crate::registry::types::{Manifest, RegistryIndex};
use crate::serde_yaml;
use crate::NikaError;

// Re-export from core::paths
pub use crate::core::paths::{NIKA_DIR_NAME, NIKA_HOME_ENV};

/// Registry index filename.
pub const REGISTRY_INDEX_FILE: &str = "registry.yaml";

/// Registry index cache TTL (5 minutes).
///
/// Avoids repeated filesystem reads for `is_installed()`, `installed_version()`, etc.
/// Invalidated on `save_registry()` so writes are immediately visible.
const REGISTRY_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// In-memory cache for the registry index.
///
/// Stores (RegistryIndex, load_time). Entries older than REGISTRY_CACHE_TTL are stale.
/// Protected by Mutex (not RwLock) because reads are fast and contention is minimal.
static REGISTRY_CACHE: LazyLock<Mutex<Option<(RegistryIndex, Instant)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Packages directory name.
pub const PACKAGES_DIR_NAME: &str = "packages";

/// Manifest filename within a package.
pub const MANIFEST_FILE: &str = "manifest.yaml";

/// Get the packages directory.
///
/// Returns `~/.nika/packages/` (or `$NIKA_HOME/packages/`).
pub fn packages_dir() -> Result<PathBuf, NikaError> {
    Ok(crate::core::paths::nika_home().join(PACKAGES_DIR_NAME))
}

/// Get the registry index file path.
///
/// Returns `~/.nika/registry.yaml` (or `$NIKA_HOME/registry.yaml`).
pub fn registry_index_path() -> Result<PathBuf, NikaError> {
    Ok(crate::core::paths::nika_home().join(REGISTRY_INDEX_FILE))
}

/// Get the directory path for a specific package version.
///
/// Returns `~/.nika/packages/@scope/name/version/`.
///
/// # Arguments
///
/// * `name` - Package name (e.g., "@supernovae/workflows")
/// * `version` - Package version (e.g., "1.0.0")
///
/// # Examples
///
/// ```ignore
/// use nika::registry::operations::package_dir;
///
/// let dir = package_dir("@supernovae/workflows", "1.0.0")?;
/// // Returns PathBuf like "~/.nika/packages/@supernovae/workflows/1.0.0/"
/// # Ok::<(), nika::NikaError>(())
/// ```
pub fn package_dir(name: &str, version: &str) -> Result<PathBuf, NikaError> {
    let packages = packages_dir()?;

    // Handle scoped packages (@scope/name → @scope/name)
    let package_path = if name.starts_with('@') {
        // @scope/name → packages/@scope/name/version
        packages.join(name).join(version)
    } else {
        // Non-scoped: name → packages/name/version
        packages.join(name).join(version)
    };

    Ok(package_path)
}

/// Get the manifest file path for a specific package version.
///
/// Returns `~/.nika/packages/@scope/name/version/manifest.yaml`.
pub fn manifest_path(name: &str, version: &str) -> Result<PathBuf, NikaError> {
    Ok(package_dir(name, version)?.join(MANIFEST_FILE))
}

/// Ensure the Nika home directory exists.
///
/// Creates `~/.nika/` and `~/.nika/packages/` if they don't exist.
pub fn ensure_nika_home() -> Result<PathBuf, NikaError> {
    let home = crate::core::paths::nika_home();

    if !home.exists() {
        fs::create_dir_all(&home).map_err(|e| NikaError::ValidationError {
            reason: format!("Failed to create directory '{}': {}", home.display(), e),
        })?;
    }

    let packages = home.join(PACKAGES_DIR_NAME);
    if !packages.exists() {
        fs::create_dir_all(&packages).map_err(|e| NikaError::ValidationError {
            reason: format!("Failed to create directory '{}': {}", packages.display(), e),
        })?;
    }

    Ok(home)
}

/// Load the registry index from disk.
///
/// Returns an empty index if the file doesn't exist.
///
/// # Examples
///
/// ```ignore
/// use nika::registry::operations::load_registry;
///
/// let index = load_registry()?;
/// println!("Installed packages: {}", index.len());
/// # Ok::<(), nika::NikaError>(())
/// ```
pub fn load_registry() -> Result<RegistryIndex, NikaError> {
    // Check cache first (TTL = 5 minutes)
    if let Ok(guard) = REGISTRY_CACHE.lock() {
        if let Some((ref cached, ref loaded_at)) = *guard {
            if loaded_at.elapsed() < REGISTRY_CACHE_TTL {
                return Ok(cached.clone());
            }
        }
    }

    let path = registry_index_path()?;

    if !path.exists() {
        return Ok(RegistryIndex::new());
    }

    let content = fs::read_to_string(&path).map_err(|e| NikaError::ValidationError {
        reason: format!("Failed to read registry file '{}': {}", path.display(), e),
    })?;

    let index: RegistryIndex =
        crate::util::parse_yaml_budgeted(&content).map_err(|e| NikaError::ParseError {
            details: format!("Failed to parse registry YAML: {}", e),
        })?;

    // Cache the result
    if let Ok(mut guard) = REGISTRY_CACHE.lock() {
        *guard = Some((index.clone(), Instant::now()));
    }

    Ok(index)
}

/// Save the registry index to disk.
///
/// Creates parent directories if needed.
///
/// # Examples
///
/// ```ignore
/// use nika::registry::operations::save_registry;
/// use nika::registry::types::{RegistryIndex, InstalledPackage};
///
/// let mut index = RegistryIndex::new();
/// index.insert("@test/pkg", InstalledPackage::now("1.0.0", "packages/@test/pkg/1.0.0/manifest.yaml"));
///
/// save_registry(&index)?;
/// # Ok::<(), nika::NikaError>(())
/// ```
pub fn save_registry(index: &RegistryIndex) -> Result<(), NikaError> {
    let path = registry_index_path()?;

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| NikaError::ValidationError {
                reason: format!("Failed to create directory '{}': {}", parent.display(), e),
            })?;
        }
    }

    let content = serde_yaml::to_string(index).map_err(|e| NikaError::ParseError {
        details: format!("Failed to serialize registry: {}", e),
    })?;

    fs::write(&path, content).map_err(|e| NikaError::ValidationError {
        reason: format!("Failed to write registry file '{}': {}", path.display(), e),
    })?;

    // Invalidate cache so subsequent reads pick up the new data immediately
    invalidate_registry_cache();

    Ok(())
}

/// Invalidate the registry index cache.
///
/// Called automatically by `save_registry()`. Can also be called manually
/// after external changes to `registry.yaml` (e.g., after `nika add`).
pub fn invalidate_registry_cache() {
    if let Ok(mut guard) = REGISTRY_CACHE.lock() {
        *guard = None;
    }
}

/// Load a package manifest from disk.
///
/// # Arguments
///
/// * `name` - Package name (e.g., "@supernovae/workflows")
/// * `version` - Package version (e.g., "1.0.0")
///
/// # Examples
///
/// ```ignore
/// use nika::registry::operations::load_manifest;
///
/// let manifest = load_manifest("@supernovae/workflows", "1.0.0")?;
/// println!("Package: {} v{}", manifest.name, manifest.version);
/// # Ok::<(), nika::NikaError>(())
/// ```
pub fn load_manifest(name: &str, version: &str) -> Result<Manifest, NikaError> {
    let path = manifest_path(name, version)?;

    if !path.exists() {
        return Err(NikaError::PackageNotFound {
            name: name.to_string(),
            version: version.to_string(),
        });
    }

    let content = fs::read_to_string(&path).map_err(|e| NikaError::ValidationError {
        reason: format!("Failed to read manifest file '{}': {}", path.display(), e),
    })?;

    crate::util::parse_yaml_budgeted(&content).map_err(|e| NikaError::ParseError {
        details: format!("Failed to parse manifest YAML: {}", e),
    })
}

/// Check if a package is installed.
///
/// Uses the registry index for quick lookup.
pub fn is_installed(name: &str) -> Result<bool, NikaError> {
    let index = load_registry()?;
    Ok(index.is_installed(name))
}

/// Check if a specific version of a package is installed.
pub fn is_version_installed(name: &str, version: &str) -> Result<bool, NikaError> {
    let index = load_registry()?;
    match index.get(name) {
        Some(pkg) => Ok(pkg.version == version),
        None => Ok(false),
    }
}

/// Get the installed version of a package.
///
/// Returns `None` if not installed.
pub fn installed_version(name: &str) -> Result<Option<String>, NikaError> {
    let index = load_registry()?;
    Ok(index.get(name).map(|pkg| pkg.version.clone()))
}

/// List all installed packages.
///
/// Returns a vector of (name, version) tuples.
pub fn list_installed() -> Result<Vec<(String, String)>, NikaError> {
    let index = load_registry()?;
    Ok(index
        .iter()
        .map(|(name, pkg)| (name.clone(), pkg.version.clone()))
        .collect())
}

/// Resolve a skill path within an installed package.
///
/// # Arguments
///
/// * `name` - Package name
/// * `version` - Package version
/// * `skill_path` - Relative path to skill file (e.g., "skills/brainstorm.skill.md")
///
/// # Returns
///
/// Absolute path to the skill file.
///
/// # Security
///
/// This function validates that the resolved path stays within the package
/// directory to prevent path traversal attacks (e.g., `../../../etc/passwd`).
pub fn resolve_skill_path(
    name: &str,
    version: &str,
    skill_path: &str,
) -> Result<PathBuf, NikaError> {
    // Reject obvious traversal attempts early (before filesystem access)
    if skill_path.contains("..") {
        return Err(NikaError::SkillLoadError {
            skill: format!("{}:{}", name, skill_path),
            reason: "Skill path contains path traversal sequence (..)".into(),
        });
    }

    let pkg_dir = package_dir(name, version)?;
    let full_path = pkg_dir.join(skill_path);

    if !full_path.exists() {
        return Err(NikaError::SkillLoadError {
            skill: format!("{}:{}", name, skill_path),
            reason: format!("Skill file not found at '{}'", full_path.display()),
        });
    }

    // Canonicalize paths to resolve symlinks and validate boundaries
    let canonical_pkg = pkg_dir
        .canonicalize()
        .map_err(|e| NikaError::SkillLoadError {
            skill: format!("{}:{}", name, skill_path),
            reason: format!("Failed to canonicalize package directory: {}", e),
        })?;

    let canonical_skill = full_path
        .canonicalize()
        .map_err(|e| NikaError::SkillLoadError {
            skill: format!("{}:{}", name, skill_path),
            reason: format!("Failed to canonicalize skill path: {}", e),
        })?;

    // Ensure skill file is within package directory (prevents symlink attacks)
    if !canonical_skill.starts_with(&canonical_pkg) {
        return Err(NikaError::SkillLoadError {
            skill: format!("{}:{}", name, skill_path),
            reason: format!(
                "Path traversal detected: skill file '{}' is outside package directory '{}'",
                canonical_skill.display(),
                canonical_pkg.display()
            ),
        });
    }

    Ok(canonical_skill)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use std::path::Path;
    use tempfile::TempDir;

    fn with_temp_nika_home<F, T>(f: F) -> T
    where
        F: FnOnce(&Path) -> T,
    {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Invalidate registry cache before switching home directory
        // (prevents stale cache from previous test leaking into this one)
        invalidate_registry_cache();

        // Set NIKA_HOME to temp directory
        env::set_var(NIKA_HOME_ENV, &temp_path);

        let result = f(&temp_path);

        // Clean up env var and cache
        env::remove_var(NIKA_HOME_ENV);
        invalidate_registry_cache();

        result
    }

    #[test]
    #[serial]
    fn test_nika_home_uses_env_var() {
        with_temp_nika_home(|temp_path| {
            let home = crate::core::paths::nika_home();
            assert_eq!(home, temp_path);
        });
    }

    #[test]
    #[serial]
    fn test_packages_dir() {
        with_temp_nika_home(|temp_path| {
            let dir = packages_dir().unwrap();
            assert_eq!(dir, temp_path.join("packages"));
        });
    }

    #[test]
    #[serial]
    fn test_registry_index_path() {
        with_temp_nika_home(|temp_path| {
            let path = registry_index_path().unwrap();
            assert_eq!(path, temp_path.join("registry.yaml"));
        });
    }

    #[test]
    #[serial]
    fn test_package_dir_scoped() {
        with_temp_nika_home(|temp_path| {
            let dir = package_dir("@supernovae/workflows", "1.0.0").unwrap();
            assert_eq!(
                dir,
                temp_path
                    .join("packages")
                    .join("@supernovae/workflows")
                    .join("1.0.0")
            );
        });
    }

    #[test]
    #[serial]
    fn test_package_dir_unscoped() {
        with_temp_nika_home(|temp_path| {
            let dir = package_dir("my-package", "2.0.0").unwrap();
            assert_eq!(
                dir,
                temp_path.join("packages").join("my-package").join("2.0.0")
            );
        });
    }

    #[test]
    #[serial]
    fn test_manifest_path() {
        with_temp_nika_home(|temp_path| {
            let path = manifest_path("@test/pkg", "1.0.0").unwrap();
            assert_eq!(
                path,
                temp_path
                    .join("packages")
                    .join("@test/pkg")
                    .join("1.0.0")
                    .join("manifest.yaml")
            );
        });
    }

    #[test]
    #[serial]
    fn test_ensure_nika_home() {
        with_temp_nika_home(|temp_path| {
            // Remove any existing directories
            let _ = fs::remove_dir_all(temp_path);

            let home = ensure_nika_home().unwrap();
            assert!(home.exists());
            assert!(home.join("packages").exists());
        });
    }

    #[test]
    #[serial]
    fn test_load_registry_empty() {
        with_temp_nika_home(|_| {
            let index = load_registry().unwrap();
            assert!(index.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_save_and_load_registry() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            let mut index = RegistryIndex::new();
            index.insert(
                "@test/pkg",
                InstalledPackage::new(
                    "1.0.0",
                    "2026-03-01T10:00:00Z",
                    "packages/@test/pkg/1.0.0/manifest.yaml",
                ),
            );

            save_registry(&index).unwrap();

            let loaded = load_registry().unwrap();
            assert_eq!(loaded.len(), 1);
            assert!(loaded.is_installed("@test/pkg"));
            assert_eq!(loaded.get("@test/pkg").unwrap().version, "1.0.0");
        });
    }

    #[test]
    #[serial]
    fn test_is_installed() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            // Initially not installed
            assert!(!is_installed("@test/pkg").unwrap());

            // Install
            let mut index = RegistryIndex::new();
            index.insert(
                "@test/pkg",
                InstalledPackage::new("1.0.0", "2026-03-01T10:00:00Z", "path/to/manifest.yaml"),
            );
            save_registry(&index).unwrap();

            // Now installed
            assert!(is_installed("@test/pkg").unwrap());
        });
    }

    #[test]
    #[serial]
    fn test_is_version_installed() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            let mut index = RegistryIndex::new();
            index.insert(
                "@test/pkg",
                InstalledPackage::new("1.0.0", "2026-03-01T10:00:00Z", "path"),
            );
            save_registry(&index).unwrap();

            assert!(is_version_installed("@test/pkg", "1.0.0").unwrap());
            assert!(!is_version_installed("@test/pkg", "2.0.0").unwrap());
            assert!(!is_version_installed("@other/pkg", "1.0.0").unwrap());
        });
    }

    #[test]
    #[serial]
    fn test_installed_version() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            // Not installed
            assert_eq!(installed_version("@test/pkg").unwrap(), None);

            // Install
            let mut index = RegistryIndex::new();
            index.insert(
                "@test/pkg",
                InstalledPackage::new("2.1.0", "2026-03-01T10:00:00Z", "path"),
            );
            save_registry(&index).unwrap();

            assert_eq!(
                installed_version("@test/pkg").unwrap(),
                Some("2.1.0".to_string())
            );
        });
    }

    #[test]
    #[serial]
    fn test_list_installed() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            let mut index = RegistryIndex::new();
            index.insert(
                "@pkg/a",
                InstalledPackage::new("1.0.0", "2026-03-01T10:00:00Z", "a"),
            );
            index.insert(
                "@pkg/b",
                InstalledPackage::new("2.0.0", "2026-03-01T10:00:00Z", "b"),
            );
            save_registry(&index).unwrap();

            let list = list_installed().unwrap();
            assert_eq!(list.len(), 2);

            let names: Vec<_> = list.iter().map(|(n, _)| n.as_str()).collect();
            assert!(names.contains(&"@pkg/a"));
            assert!(names.contains(&"@pkg/b"));
        });
    }

    #[test]
    #[serial]
    fn test_load_manifest_not_found() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            let result = load_manifest("@nonexistent/pkg", "1.0.0");
            assert!(result.is_err());
        });
    }

    #[test]
    #[serial]
    fn test_load_manifest_success() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            // Create package directory with manifest
            let pkg_dir = package_dir("@test/pkg", "1.0.0").unwrap();
            fs::create_dir_all(&pkg_dir).unwrap();

            let manifest = Manifest::new("@test/pkg", "1.0.0");
            let manifest_content = serde_yaml::to_string(&manifest).unwrap();
            fs::write(pkg_dir.join("manifest.yaml"), manifest_content).unwrap();

            let loaded = load_manifest("@test/pkg", "1.0.0").unwrap();
            assert_eq!(loaded.name, "@test/pkg");
            assert_eq!(loaded.version, "1.0.0");
        });
    }

    #[test]
    #[serial]
    fn test_resolve_skill_path() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            // Create package directory with skill file
            let pkg_dir = package_dir("@test/pkg", "1.0.0").unwrap();
            let skills_dir = pkg_dir.join("skills");
            fs::create_dir_all(&skills_dir).unwrap();
            fs::write(skills_dir.join("test.skill.md"), "# Test Skill").unwrap();

            let path = resolve_skill_path("@test/pkg", "1.0.0", "skills/test.skill.md").unwrap();
            assert!(path.exists());
            assert!(path.ends_with("skills/test.skill.md"));
        });
    }

    #[test]
    #[serial]
    fn test_resolve_skill_path_not_found() {
        with_temp_nika_home(|_| {
            ensure_nika_home().unwrap();

            let pkg_dir = package_dir("@test/pkg", "1.0.0").unwrap();
            fs::create_dir_all(&pkg_dir).unwrap();

            let result = resolve_skill_path("@test/pkg", "1.0.0", "skills/nonexistent.md");
            assert!(result.is_err());
        });
    }

    // ═══════════════════════════════════════════════════════════════
    // REGISTRY CACHE TTL TESTS
    // ═══════════════════════════════════════════════════════════════

    #[test]
    #[serial]
    fn test_registry_cache_returns_cached_result() {
        with_temp_nika_home(|_| {
            // Start with a clean cache
            invalidate_registry_cache();
            ensure_nika_home().unwrap();

            // Save a registry with one package
            let mut index = RegistryIndex::new();
            index.insert(
                "@cache/test",
                InstalledPackage::new("1.0.0", "2026-01-01T00:00:00Z", "path"),
            );
            save_registry(&index).unwrap();

            // First load — reads from disk, populates cache
            let loaded1 = load_registry().unwrap();
            assert_eq!(loaded1.len(), 1);
            assert!(loaded1.is_installed("@cache/test"));

            // Modify the file directly (bypassing save_registry to skip cache invalidation)
            let path = registry_index_path().unwrap();
            fs::write(&path, "packages: {}").unwrap();

            // Second load — should return cached result (not the modified file)
            let loaded2 = load_registry().unwrap();
            assert_eq!(
                loaded2.len(),
                1,
                "Second load should return cached result, not the modified file"
            );

            // Clean up
            invalidate_registry_cache();
        });
    }

    #[test]
    #[serial]
    fn test_registry_cache_invalidated_on_save() {
        with_temp_nika_home(|_| {
            invalidate_registry_cache();
            ensure_nika_home().unwrap();

            // Save initial registry
            let mut index1 = RegistryIndex::new();
            index1.insert(
                "@pkg/a",
                InstalledPackage::new("1.0.0", "2026-01-01T00:00:00Z", "a"),
            );
            save_registry(&index1).unwrap();

            // Load to populate cache
            let loaded1 = load_registry().unwrap();
            assert_eq!(loaded1.len(), 1);

            // Save a different registry (this should invalidate cache)
            let mut index2 = RegistryIndex::new();
            index2.insert(
                "@pkg/a",
                InstalledPackage::new("1.0.0", "2026-01-01T00:00:00Z", "a"),
            );
            index2.insert(
                "@pkg/b",
                InstalledPackage::new("2.0.0", "2026-01-01T00:00:00Z", "b"),
            );
            save_registry(&index2).unwrap();

            // Load again — should see the new data (cache was invalidated)
            let loaded2 = load_registry().unwrap();
            assert_eq!(
                loaded2.len(),
                2,
                "After save_registry(), load should return fresh data"
            );

            // Clean up
            invalidate_registry_cache();
        });
    }

    #[test]
    #[serial]
    fn test_invalidate_registry_cache_clears_cache() {
        with_temp_nika_home(|_| {
            invalidate_registry_cache();
            ensure_nika_home().unwrap();

            // Save and load to populate cache
            let mut index = RegistryIndex::new();
            index.insert(
                "@pkg/c",
                InstalledPackage::new("1.0.0", "2026-01-01T00:00:00Z", "c"),
            );
            save_registry(&index).unwrap();
            let _ = load_registry().unwrap();

            // Modify file directly
            let path = registry_index_path().unwrap();
            fs::write(&path, "packages: {}").unwrap();

            // Without invalidation, cache would return old data
            // With invalidation, it reads the (now empty) file
            invalidate_registry_cache();
            let loaded = load_registry().unwrap();
            assert_eq!(
                loaded.len(),
                0,
                "After invalidation, load should read from disk (now empty)"
            );

            // Clean up
            invalidate_registry_cache();
        });
    }

    #[test]
    #[serial]
    fn test_registry_cache_ttl_constant_is_5_minutes() {
        assert_eq!(
            REGISTRY_CACHE_TTL,
            std::time::Duration::from_secs(300),
            "Registry cache TTL should be 5 minutes (300 seconds)"
        );
    }
}
