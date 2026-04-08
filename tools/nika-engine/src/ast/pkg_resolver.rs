// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Package URI resolver for skill ecosystem
//!
//! Parses `pkg:` URIs and resolves them to filesystem paths.
//!
//! # URI Format
//!
//! ```text
//! pkg:@scope/name@version/path
//! pkg:@scope/name/path          (latest version)
//! pkg:name@version/path         (no scope, uses @default)
//! pkg:name/path                 (minimal form)
//! ```
//!
//! # Resolution
//!
//! URIs resolve to: `~/.nika/packages/@scope/name/version/path`
//!
//! # Examples
//!
//! ```
//! use nika_engine::ast::pkg_resolver::PkgUri;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let uri = PkgUri::parse("pkg:@supernovae/skills@1.0.0/rust.md")?;
//! assert_eq!(uri.scope.as_deref(), Some("supernovae"));
//! assert_eq!(uri.name, "skills");
//! assert_eq!(uri.version.as_deref(), Some("1.0.0"));
//! assert_eq!(uri.path, "rust.md");
//! # Ok(())
//! # }
//! ```

use std::path::PathBuf;

use crate::error::NikaError;

/// Default scope when none is specified
const DEFAULT_SCOPE: &str = "default";

/// Default version when none is specified
const LATEST_VERSION: &str = "latest";

/// Base directory for packages (relative to home)
const PACKAGES_DIR: &str = ".nika/packages";

/// Parsed package URI components
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PkgUri {
    /// Package scope (e.g., "supernovae" from @supernovae/skills)
    /// None means use default scope
    pub scope: Option<String>,

    /// Package name (e.g., "skills")
    pub name: String,

    /// Version (e.g., "1.0.0")
    /// None means use "latest"
    pub version: Option<String>,

    /// Path within the package (e.g., "rust.md" or "skills/rust-core.md")
    pub path: String,
}

impl PkgUri {
    /// Parse a pkg: URI string
    ///
    /// # Supported formats
    ///
    /// - `pkg:@scope/name@version/path` - Full form
    /// - `pkg:@scope/name/path` - Latest version
    /// - `pkg:name@version/path` - Default scope
    /// - `pkg:name/path` - Minimal (default scope, latest version)
    ///
    /// # Errors
    ///
    /// Returns `NikaError::InvalidPkgUri` if the URI is malformed.
    pub fn parse(uri: &str) -> Result<Self, NikaError> {
        // Must start with "pkg:"
        let rest = uri
            .strip_prefix("pkg:")
            .ok_or_else(|| NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "URI must start with 'pkg:'".to_string(),
            })?;

        // Empty after prefix
        if rest.is_empty() {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "URI is empty after 'pkg:' prefix".to_string(),
            });
        }

        // Parse scope if present (@scope/...)
        let (scope, after_scope) = if rest.starts_with('@') {
            // Find the slash after scope
            let slash_pos = rest.find('/').ok_or_else(|| NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "Scoped package must have format @scope/name".to_string(),
            })?;

            let scope_str = &rest[1..slash_pos]; // Skip '@'
            if scope_str.is_empty() {
                return Err(NikaError::InvalidPkgUri {
                    uri: uri.to_string(),
                    reason: "Scope cannot be empty".to_string(),
                });
            }

            Self::validate_identifier(scope_str, "scope", uri)?;
            (Some(scope_str.to_string()), &rest[slash_pos + 1..])
        } else {
            (None, rest)
        };

        // Now parse name[@version]/path from after_scope
        // Find the first slash (separates name[@version] from path)
        let first_slash = after_scope
            .find('/')
            .ok_or_else(|| NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "URI must include a path after package name".to_string(),
            })?;

        let name_version = &after_scope[..first_slash];
        let path = &after_scope[first_slash + 1..];

        if path.is_empty() {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "Path cannot be empty".to_string(),
            });
        }

        // Parse name and optional version from name[@version]
        let (name, version) = if let Some(at_pos) = name_version.find('@') {
            let name_str = &name_version[..at_pos];
            let version_str = &name_version[at_pos + 1..];

            if name_str.is_empty() {
                return Err(NikaError::InvalidPkgUri {
                    uri: uri.to_string(),
                    reason: "Package name cannot be empty".to_string(),
                });
            }

            if version_str.is_empty() {
                return Err(NikaError::InvalidPkgUri {
                    uri: uri.to_string(),
                    reason: "Version cannot be empty after '@'".to_string(),
                });
            }

            Self::validate_identifier(name_str, "name", uri)?;
            Self::validate_version(version_str, uri)?;

            (name_str.to_string(), Some(version_str.to_string()))
        } else {
            Self::validate_identifier(name_version, "name", uri)?;
            (name_version.to_string(), None)
        };

        // Validate path doesn't escape package
        Self::validate_path(path, uri)?;

        Ok(Self {
            scope,
            name,
            version,
            path: path.to_string(),
        })
    }

    /// Resolve the URI to a filesystem path
    ///
    /// Uses `~/.nika/packages/@scope/name/version/path`
    pub fn resolve(&self) -> Result<PathBuf, NikaError> {
        let home = dirs::home_dir().ok_or_else(|| NikaError::InvalidPkgUri {
            uri: self.to_string(),
            reason: "Could not determine home directory".to_string(),
        })?;

        let scope = self.scope.as_deref().unwrap_or(DEFAULT_SCOPE);
        let version = self.version.as_deref().unwrap_or(LATEST_VERSION);

        let mut path = home;
        path.push(PACKAGES_DIR);
        path.push(format!("@{}", scope));
        path.push(&self.name);
        path.push(version);
        path.push(&self.path);

        Ok(path)
    }

    /// Get the effective scope (returns default if none specified)
    pub fn effective_scope(&self) -> &str {
        self.scope.as_deref().unwrap_or(DEFAULT_SCOPE)
    }

    /// Get the effective version (returns "latest" if none specified)
    pub fn effective_version(&self) -> &str {
        self.version.as_deref().unwrap_or(LATEST_VERSION)
    }

    /// Check if this is a scoped package
    pub fn is_scoped(&self) -> bool {
        self.scope.is_some()
    }

    /// Check if this has an explicit version
    pub fn has_version(&self) -> bool {
        self.version.is_some()
    }

    /// Validate an identifier (scope or name)
    fn validate_identifier(id: &str, kind: &str, uri: &str) -> Result<(), NikaError> {
        // Must be alphanumeric with hyphens (no underscores for npm compat)
        if id.is_empty() {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: format!("{kind} cannot be empty"),
            });
        }

        // First char must be alphanumeric
        // SAFETY: id.is_empty() check above guarantees at least one char
        let first_char = id.chars().next().expect("id is non-empty");
        if !first_char.is_ascii_alphanumeric() {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: format!("{kind} must start with alphanumeric character"),
            });
        }

        // All chars must be alphanumeric or hyphen
        for c in id.chars() {
            if !c.is_ascii_alphanumeric() && c != '-' {
                return Err(NikaError::InvalidPkgUri {
                    uri: uri.to_string(),
                    reason: format!(
                        "{kind} contains invalid character '{}' (only alphanumeric and hyphens allowed)",
                        c
                    ),
                });
            }
        }

        // Cannot end with hyphen
        if id.ends_with('-') {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: format!("{kind} cannot end with hyphen"),
            });
        }

        Ok(())
    }

    /// Validate a semver-like version string
    fn validate_version(version: &str, uri: &str) -> Result<(), NikaError> {
        // Accept semver-like: major.minor.patch[-prerelease][+build]
        // Also accept simple: "latest", "1", "1.0"

        if version == "latest" {
            return Ok(());
        }

        // Check for valid characters
        for c in version.chars() {
            if !c.is_ascii_alphanumeric() && c != '.' && c != '-' && c != '+' {
                return Err(NikaError::InvalidPkgUri {
                    uri: uri.to_string(),
                    reason: format!(
                        "version contains invalid character '{}' (use semver format)",
                        c
                    ),
                });
            }
        }

        // Must start with a digit
        if !version.starts_with(|c: char| c.is_ascii_digit()) {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "version must start with a digit".to_string(),
            });
        }

        Ok(())
    }

    /// Validate path doesn't escape the package directory
    fn validate_path(path: &str, uri: &str) -> Result<(), NikaError> {
        // No path traversal
        if path.contains("..") {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "path traversal (..) is not allowed".to_string(),
            });
        }

        // No absolute paths
        if path.starts_with('/') {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "path must be relative (cannot start with '/')".to_string(),
            });
        }

        // No backslashes (Windows compat)
        if path.contains('\\') {
            return Err(NikaError::InvalidPkgUri {
                uri: uri.to_string(),
                reason: "use forward slashes in paths".to_string(),
            });
        }

        Ok(())
    }
}

impl std::fmt::Display for PkgUri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pkg:")?;

        if let Some(ref scope) = self.scope {
            write!(f, "@{}/", scope)?;
        }

        write!(f, "{}", self.name)?;

        if let Some(ref version) = self.version {
            write!(f, "@{}", version)?;
        }

        write!(f, "/{}", self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // PARSING TESTS (8 tests)
    // =========================================================================

    #[test]
    fn test_parse_full_uri() {
        let uri = PkgUri::parse("pkg:@supernovae/skills@1.0.0/rust.md").unwrap();
        assert_eq!(uri.scope, Some("supernovae".to_string()));
        assert_eq!(uri.name, "skills");
        assert_eq!(uri.version, Some("1.0.0".to_string()));
        assert_eq!(uri.path, "rust.md");
    }

    #[test]
    fn test_parse_scoped_no_version() {
        let uri = PkgUri::parse("pkg:@supernovae/skills/rust.md").unwrap();
        assert_eq!(uri.scope, Some("supernovae".to_string()));
        assert_eq!(uri.name, "skills");
        assert_eq!(uri.version, None);
        assert_eq!(uri.path, "rust.md");
        assert_eq!(uri.effective_version(), "latest");
    }

    #[test]
    fn test_parse_unscoped_with_version() {
        let uri = PkgUri::parse("pkg:rust-skills@2.0.0/async.md").unwrap();
        assert_eq!(uri.scope, None);
        assert_eq!(uri.name, "rust-skills");
        assert_eq!(uri.version, Some("2.0.0".to_string()));
        assert_eq!(uri.path, "async.md");
        assert_eq!(uri.effective_scope(), "default");
    }

    #[test]
    fn test_parse_minimal() {
        let uri = PkgUri::parse("pkg:my-skills/README.md").unwrap();
        assert_eq!(uri.scope, None);
        assert_eq!(uri.name, "my-skills");
        assert_eq!(uri.version, None);
        assert_eq!(uri.path, "README.md");
    }

    #[test]
    fn test_parse_nested_path() {
        let uri = PkgUri::parse("pkg:@nika/writing@1.2.3/skills/mermaid/SKILL.md").unwrap();
        assert_eq!(uri.scope, Some("nika".to_string()));
        assert_eq!(uri.name, "writing");
        assert_eq!(uri.version, Some("1.2.3".to_string()));
        assert_eq!(uri.path, "skills/mermaid/SKILL.md");
    }

    #[test]
    fn test_parse_prerelease_version() {
        let uri = PkgUri::parse("pkg:@test/pkg@1.0.0-beta.1/file.md").unwrap();
        assert_eq!(uri.version, Some("1.0.0-beta.1".to_string()));
    }

    #[test]
    fn test_parse_build_metadata_version() {
        let uri = PkgUri::parse("pkg:@test/pkg@1.0.0+build.123/file.md").unwrap();
        assert_eq!(uri.version, Some("1.0.0+build.123".to_string()));
    }

    #[test]
    fn test_parse_latest_version() {
        let uri = PkgUri::parse("pkg:@test/pkg@latest/file.md").unwrap();
        assert_eq!(uri.version, Some("latest".to_string()));
        assert_eq!(uri.effective_version(), "latest");
    }

    // =========================================================================
    // INVALID URI TESTS (7 tests)
    // =========================================================================

    #[test]
    fn test_invalid_no_prefix() {
        let err = PkgUri::parse("@supernovae/skills@1.0.0/rust.md").unwrap_err();
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    #[test]
    fn test_invalid_empty_after_prefix() {
        let err = PkgUri::parse("pkg:").unwrap_err();
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    #[test]
    fn test_invalid_empty_scope() {
        let err = PkgUri::parse("pkg:@/skills/file.md").unwrap_err();
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    #[test]
    fn test_invalid_empty_name() {
        let err = PkgUri::parse("pkg:@scope/@1.0.0/file.md").unwrap_err();
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    #[test]
    fn test_invalid_empty_path() {
        let err = PkgUri::parse("pkg:@scope/name@1.0.0/").unwrap_err();
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    #[test]
    fn test_invalid_path_traversal() {
        let err = PkgUri::parse("pkg:@scope/name/../../etc/passwd").unwrap_err();
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    #[test]
    fn test_invalid_absolute_path() {
        let err = PkgUri::parse("pkg:@scope/name//etc/passwd").unwrap_err();
        // This results in empty path segment, which is caught
        assert!(matches!(err, NikaError::InvalidPkgUri { .. }));
    }

    // =========================================================================
    // RESOLUTION TESTS (3 tests)
    // =========================================================================

    #[test]
    fn test_resolve_full_uri() {
        let uri = PkgUri::parse("pkg:@supernovae/skills@1.0.0/rust.md").unwrap();
        let resolved = uri.resolve().unwrap();

        let home = dirs::home_dir().unwrap();
        let expected = home.join(".nika/packages/@supernovae/skills/1.0.0/rust.md");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_resolve_uses_defaults() {
        let uri = PkgUri::parse("pkg:my-skills/file.md").unwrap();
        let resolved = uri.resolve().unwrap();

        let home = dirs::home_dir().unwrap();
        let expected = home.join(".nika/packages/@default/my-skills/latest/file.md");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn test_resolve_nested_path() {
        let uri = PkgUri::parse("pkg:@nika/writing@1.0.0/skills/mermaid/SKILL.md").unwrap();
        let resolved = uri.resolve().unwrap();

        let home = dirs::home_dir().unwrap();
        let expected = home.join(".nika/packages/@nika/writing/1.0.0/skills/mermaid/SKILL.md");
        assert_eq!(resolved, expected);
    }

    // =========================================================================
    // DISPLAY/ROUNDTRIP TESTS (2 tests)
    // =========================================================================

    #[test]
    fn test_display_full_uri() {
        let uri = PkgUri::parse("pkg:@supernovae/skills@1.0.0/rust.md").unwrap();
        assert_eq!(uri.to_string(), "pkg:@supernovae/skills@1.0.0/rust.md");
    }

    #[test]
    fn test_display_minimal_uri() {
        let uri = PkgUri::parse("pkg:skills/file.md").unwrap();
        assert_eq!(uri.to_string(), "pkg:skills/file.md");
    }

    // =========================================================================
    // HELPER METHOD TESTS (2 tests)
    // =========================================================================

    #[test]
    fn test_is_scoped() {
        let scoped = PkgUri::parse("pkg:@scope/name/file.md").unwrap();
        let unscoped = PkgUri::parse("pkg:name/file.md").unwrap();

        assert!(scoped.is_scoped());
        assert!(!unscoped.is_scoped());
    }

    #[test]
    fn test_has_version() {
        let versioned = PkgUri::parse("pkg:name@1.0.0/file.md").unwrap();
        let unversioned = PkgUri::parse("pkg:name/file.md").unwrap();

        assert!(versioned.has_version());
        assert!(!unversioned.has_version());
    }
}
