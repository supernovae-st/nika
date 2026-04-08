// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! SuperNovae Package Registry
//!
//! This module provides the registry system for managing installed skill packages.
//! Packages are stored in `~/.nika/packages/@scope/name/version/` following the
//! SuperNovae Skill Ecosystem design.
//!
//! # Architecture
//!
//! ```text
//! ~/.nika/
//! ├── registry.yaml           # Index of installed packages
//! └── packages/               # Package storage
//!     ├── @scope/             # Scoped packages
//!     │   └── name/
//!     │       └── version/
//!     │           ├── manifest.yaml
//!     │           └── skills/
//!     └── name/               # Unscoped packages
//!         └── version/
//!             ├── manifest.yaml
//!             └── skills/
//! ```
//!
//! # Usage
//!
//! ```rust,ignore
//! use nika::registry::{load_registry, is_installed, resolve_skill_path};
//!
//! // Check if a package is installed
//! if is_installed("@supernovae/core")? {
//!     // Resolve skill path
//!     let path = resolve_skill_path("@supernovae/core", "1.0.0", "skills/main.md")?;
//! }
//! ```

pub mod api;
pub mod lockfile;
pub mod operations;
pub mod resolver;
pub mod types;

// Re-export core types
pub use types::{InstalledPackage, Manifest, RegistryIndex, SkillEntry};

// Re-export operations
pub use operations::{
    ensure_nika_home, installed_version, invalidate_registry_cache, is_installed,
    is_version_installed, list_installed, load_manifest, load_registry, manifest_path, package_dir,
    packages_dir, registry_index_path, resolve_skill_path, save_registry,
};

// Re-export resolver
pub use resolver::{
    cache_stats, clear_cache, invalidate_package, parse_package_ref, resolve_package_path,
    PackageRef, ResolvedPackage,
};

// Re-export lockfile
pub use lockfile::{LockEntry, Lockfile, LockfileError};

// Re-export API client
pub use api::{
    PackageInfo, RegistryApiError, RegistryClient, SearchResponse, SearchResult, SkillInfo,
    VersionInfo,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_exports_types() {
        // Verify types are accessible
        let _manifest = Manifest::new("@test/pkg", "1.0.0");
        let _skill = SkillEntry::new("skills/test.md");
        let _index = RegistryIndex::default();
        let _pkg = InstalledPackage::now("1.0.0".to_string(), "/path".to_string());
    }

    #[test]
    fn test_module_exports_operations() {
        // Verify operations are accessible (they return Results)
        let _ = crate::core::paths::nika_home();
        let _ = packages_dir();
        let _ = registry_index_path();
    }
}
