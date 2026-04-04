//! Package path resolution for local SuperNovae packages.
//!
//! Resolves package references like `@workflows/seo-audit` or `@workflows/seo-audit@1.2.0`
//! to actual filesystem paths in `~/.nika/packages/`.
//!
//! # Performance
//!
//! Uses DashMap for thread-safe caching of resolved packages.

use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::Instant;

use dashmap::DashMap;
use semver::Version;
use thiserror::Error;

use super::lockfile::Lockfile;
use super::types::Manifest;

/// Cache entry TTL (5 minutes).
const CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(300);

/// Global package resolution cache
///
/// Thread-safe cache using DashMap to avoid repeated filesystem lookups.
/// Entries expire after 5 minutes to pick up newly installed packages.
///
/// Key: package reference (e.g., "@workflows/seo-audit@1.2.0")
/// Value: (ResolvedPackage, insertion time) for TTL expiration
static PACKAGE_CACHE: LazyLock<DashMap<String, (Arc<ResolvedPackage>, Instant)>> =
    LazyLock::new(DashMap::new);

/// Errors that can occur during package resolution.
#[derive(Error, Debug)]
pub enum ResolverError {
    #[error("Invalid package reference format: {0}")]
    InvalidFormat(String),

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("No version specified and multiple versions available: {0}")]
    AmbiguousVersion(String),

    #[error("Manifest parse error: {0}")]
    ManifestError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// A parsed package reference.
///
/// Examples:
/// - `@workflows/seo-audit` → scope="@workflows", name="seo-audit", version=None
/// - `@workflows/seo-audit@1.2.0` → scope="@workflows", name="seo-audit", version=Some("1.2.0")
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageRef {
    /// Package scope (e.g., "@workflows")
    pub scope: String,

    /// Package name (e.g., "seo-audit")
    pub name: String,

    /// Optional version (e.g., "1.2.0")
    pub version: Option<String>,
}

impl PackageRef {
    /// Get the full package name without version (e.g., "@workflows/seo-audit")
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.scope, self.name)
    }

    /// Get the full package reference with version if present
    pub fn full_ref(&self) -> String {
        match &self.version {
            Some(v) => format!("{}@{}", self.full_name(), v),
            None => self.full_name(),
        }
    }
}

/// A resolved package with filesystem path and loaded manifest.
#[derive(Debug, Clone)]
pub struct ResolvedPackage {
    /// Full path to package directory
    pub path: PathBuf,

    /// Loaded package manifest
    pub manifest: Manifest,

    /// Resolved version
    pub version: String,
}

/// Parse a package reference string into a `PackageRef`.
///
/// # Formats
///
/// - `@workflows/seo-audit` → scope + name only
/// - `@workflows/seo-audit@1.2.0` → scope + name + version
///
/// # Examples
///
/// ```ignore
/// use nika::registry::resolver::parse_package_ref;
///
/// let ref1 = parse_package_ref("@workflows/seo-audit").unwrap();
/// assert_eq!(ref1.scope, "@workflows");
/// assert_eq!(ref1.name, "seo-audit");
/// assert_eq!(ref1.version, None);
///
/// let ref2 = parse_package_ref("@workflows/seo-audit@1.2.0").unwrap();
/// assert_eq!(ref2.version, Some("1.2.0".to_string()));
/// ```
pub fn parse_package_ref(input: &str) -> Result<PackageRef, ResolverError> {
    if input.is_empty() {
        return Err(ResolverError::InvalidFormat(
            "Empty package reference".to_string(),
        ));
    }

    // Validate starts with @
    if !input.starts_with('@') {
        return Err(ResolverError::InvalidFormat(format!(
            "Package reference must start with @: {}",
            input
        )));
    }

    // Find version separator (@) - search for last @ to handle nested paths
    let (scope_and_name, version) = if let Some(pos) = input.rfind('@') {
        if pos == 0 {
            // Only the initial @, no version
            (input, None)
        } else {
            // Split at the last @
            let (sn, v) = input.split_at(pos);
            (sn, Some(v[1..].to_string())) // Skip the @ character
        }
    } else {
        (input, None)
    };

    // Split scope and name at /
    let name_parts: Vec<&str> = scope_and_name.splitn(2, '/').collect();

    if name_parts.len() != 2 {
        return Err(ResolverError::InvalidFormat(format!(
            "Package reference must be in format @scope/name: {}",
            input
        )));
    }

    let scope = name_parts[0].to_string();
    let name = name_parts[1].to_string();

    // Validate name is not empty
    if name.is_empty() {
        return Err(ResolverError::InvalidFormat(format!(
            "Package name cannot be empty: {}",
            input
        )));
    }

    Ok(PackageRef {
        scope,
        name,
        version,
    })
}

/// Clear the package resolution cache
///
/// Useful for testing or when packages are installed/updated.
pub fn clear_cache() {
    PACKAGE_CACHE.clear();
}

/// Invalidate a specific package in the cache
///
/// Removes all cached entries starting with the given package name.
/// More efficient than clear_cache() when only one package changed.
///
/// # Examples
///
/// ```ignore
/// use nika::registry::resolver;
///
/// // After `nika add @workflows/seo-audit`, invalidate just that package
/// resolver::invalidate_package("@workflows/seo-audit");
/// ```
pub fn invalidate_package(name: &str) {
    PACKAGE_CACHE.retain(|key, _value| !key.starts_with(name));
}

/// Get cache statistics
pub fn cache_stats() -> (usize, usize) {
    let size = PACKAGE_CACHE.len();
    let capacity = PACKAGE_CACHE.capacity();
    (size, capacity)
}

/// Resolve a package reference to its filesystem path and manifest.
///
/// This function:
/// 1. Checks the cache first
/// 2. Parses the package reference
/// 3. Looks up the package in `~/.nika/packages/`
/// 4. If no version specified, finds the latest installed version
/// 5. Loads and validates the package manifest
/// 6. Caches the result
///
/// # Examples
///
/// ```ignore
/// use nika::registry::resolver::resolve_package_path;
///
/// let pkg = resolve_package_path("@workflows/seo-audit").unwrap();
/// println!("Found package at: {}", pkg.path.display());
/// println!("Version: {}", pkg.version);
/// ```
pub fn resolve_package_path(reference: &str) -> Result<ResolvedPackage, ResolverError> {
    // Check cache first (with TTL expiration)
    if let Some(cached) = PACKAGE_CACHE.get(reference) {
        let (pkg, inserted_at) = cached.value();
        if inserted_at.elapsed() < CACHE_TTL {
            return Ok(Arc::unwrap_or_clone(Arc::clone(pkg)));
        }
        // TTL expired — drop ref before removing
        drop(cached);
        PACKAGE_CACHE.remove(reference);
    }

    // Cache miss - resolve and cache
    let resolved = resolve_package_path_uncached(reference)?;
    let arc_resolved = Arc::new(resolved);

    PACKAGE_CACHE.insert(
        reference.to_string(),
        (Arc::clone(&arc_resolved), Instant::now()),
    );

    Ok(Arc::unwrap_or_clone(arc_resolved))
}

/// Internal uncached resolution function
fn resolve_package_path_uncached(reference: &str) -> Result<ResolvedPackage, ResolverError> {
    let pkg_ref = parse_package_ref(reference)?;

    // Get base packages directory (~/.nika/packages/)
    let packages_dir = get_packages_dir()?;

    // Construct package base path (@workflows/seo-audit)
    let package_base = packages_dir
        .join(pkg_ref.scope.trim_start_matches('@'))
        .join(&pkg_ref.name);

    if !package_base.exists() {
        return Err(ResolverError::PackageNotFound(pkg_ref.full_name()));
    }

    // Determine version
    let version = match &pkg_ref.version {
        Some(v) => {
            // Explicit version - check it exists
            let version_dir = package_base.join(v);
            if !version_dir.exists() {
                return Err(ResolverError::PackageNotFound(pkg_ref.full_ref()));
            }
            v.clone()
        }
        None => {
            // No version - try lockfile first, then find latest
            if let Ok(lockfile) = Lockfile::load(None) {
                if let Some(locked_version) = lockfile.find_version(&pkg_ref.full_name()) {
                    // Verify locked version exists
                    let version_dir = package_base.join(locked_version);
                    if version_dir.exists() {
                        locked_version.to_string()
                    } else {
                        // Lockfile version missing, fall back to latest
                        find_latest_version(&package_base)?
                    }
                } else {
                    // Not in lockfile, find latest
                    find_latest_version(&package_base)?
                }
            } else {
                // Lockfile load failed, find latest
                find_latest_version(&package_base)?
            }
        }
    };

    let package_path = package_base.join(&version);

    // Load manifest
    let manifest_path = package_path.join("manifest.yaml");
    if !manifest_path.exists() {
        return Err(ResolverError::ManifestError(format!(
            "Manifest not found at: {}",
            manifest_path.display()
        )));
    }

    let manifest_content = std::fs::read_to_string(&manifest_path)?;
    let manifest: Manifest = crate::util::parse_yaml_budgeted(&manifest_content)
        .map_err(|e| ResolverError::ManifestError(e.to_string()))?;

    Ok(ResolvedPackage {
        path: package_path,
        manifest,
        version,
    })
}

/// Get the packages directory (~/.nika/packages/)
fn get_packages_dir() -> Result<PathBuf, ResolverError> {
    let home = dirs::home_dir().ok_or_else(|| {
        ResolverError::IoError(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "Could not determine home directory",
        ))
    })?;

    Ok(home.join(".nika").join("packages"))
}

/// Find the latest installed version in a package directory.
///
/// Scans subdirectories and returns the semantically latest version.
/// Uses semantic versioning (semver) to correctly handle versions like 1.10.0 > 1.9.0.
fn find_latest_version(package_base: &PathBuf) -> Result<String, ResolverError> {
    let mut versions: Vec<(Version, String)> = Vec::new();

    for entry in std::fs::read_dir(package_base)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            if let Some(version_str) = entry.file_name().to_str() {
                // Try to parse as semantic version, skip invalid versions
                if let Ok(version) = Version::parse(version_str) {
                    versions.push((version, version_str.to_string()));
                }
            }
        }
    }

    if versions.is_empty() {
        return Err(ResolverError::PackageNotFound(format!(
            "No valid semantic versions found in {}",
            package_base.display()
        )));
    }

    // Sort by semantic version (correctly handles 1.10.0 > 1.9.0)
    versions.sort_by(|a, b| a.0.cmp(&b.0));

    // Return the latest version string (safe: empty check above guarantees at least one element)
    versions.last().map(|(_, s)| s.clone()).ok_or_else(|| {
        ResolverError::PackageNotFound(format!(
            "No valid semantic versions found in {}",
            package_base.display()
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_package_ref_basic() {
        let pkg = parse_package_ref("@workflows/seo-audit").unwrap();
        assert_eq!(pkg.scope, "@workflows");
        assert_eq!(pkg.name, "seo-audit");
        assert_eq!(pkg.version, None);
        assert_eq!(pkg.full_name(), "@workflows/seo-audit");
    }

    #[test]
    fn test_parse_package_ref_with_version() {
        let pkg = parse_package_ref("@workflows/seo-audit@1.2.0").unwrap();
        assert_eq!(pkg.scope, "@workflows");
        assert_eq!(pkg.name, "seo-audit");
        assert_eq!(pkg.version, Some("1.2.0".to_string()));
        assert_eq!(pkg.full_ref(), "@workflows/seo-audit@1.2.0");
    }

    #[test]
    fn test_parse_package_ref_nested_scope() {
        let pkg = parse_package_ref("@workflows/data/transformer").unwrap();
        assert_eq!(pkg.scope, "@workflows");
        assert_eq!(pkg.name, "data/transformer");
    }

    #[test]
    fn test_parse_package_ref_invalid_no_scope() {
        let result = parse_package_ref("seo-audit");
        assert!(result.is_err());
        assert!(matches!(result, Err(ResolverError::InvalidFormat(_))));
    }

    #[test]
    fn test_parse_package_ref_invalid_no_name() {
        let result = parse_package_ref("@workflows/");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_package_ref_invalid_empty() {
        let result = parse_package_ref("");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_package_ref_invalid_no_slash() {
        let result = parse_package_ref("@workflows");
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_package_ref_with_prerelease() {
        let pkg = parse_package_ref("@workflows/seo-audit@1.2.0-beta.1").unwrap();
        assert_eq!(pkg.version, Some("1.2.0-beta.1".to_string()));
    }

    #[test]
    fn test_package_ref_full_name() {
        let pkg = PackageRef {
            scope: "@workflows".to_string(),
            name: "seo-audit".to_string(),
            version: None,
        };
        assert_eq!(pkg.full_name(), "@workflows/seo-audit");
    }

    #[test]
    fn test_package_ref_full_ref_no_version() {
        let pkg = PackageRef {
            scope: "@workflows".to_string(),
            name: "seo-audit".to_string(),
            version: None,
        };
        assert_eq!(pkg.full_ref(), "@workflows/seo-audit");
    }

    #[test]
    fn test_package_ref_full_ref_with_version() {
        let pkg = PackageRef {
            scope: "@workflows".to_string(),
            name: "seo-audit".to_string(),
            version: Some("1.2.0".to_string()),
        };
        assert_eq!(pkg.full_ref(), "@workflows/seo-audit@1.2.0");
    }

    // ============================================================================
    // CACHE TESTS
    // ============================================================================

    #[test]
    fn test_clear_cache() {
        use super::clear_cache;

        // Clear cache before test
        clear_cache();

        let (size, _) = cache_stats();
        assert_eq!(size, 0, "Cache should be empty after clear");
    }

    #[test]
    fn test_cache_stats() {
        use super::{cache_stats, clear_cache};

        clear_cache();

        let (size, _capacity) = cache_stats();
        assert_eq!(size, 0, "Cache should start empty");
    }
}
