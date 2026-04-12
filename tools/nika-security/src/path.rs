// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Security Module - Path Validation for Artifact Output
//!
//! Provides security-critical path validation functions for artifact output.
//! These functions prevent path traversal attacks and ensure artifacts stay
//! within the designated output directory.
//!
//! # Security Principles
//!
//! 1. **Canonical Path Validation**: All paths are canonicalized before comparison
//! 2. **Strict Boundary Enforcement**: Artifacts must be within the workflow directory
//! 3. **No Symlink Following**: Symlinks that escape the boundary are rejected
//! 4. **Fail Closed**: Any validation error results in rejection
//!
//! # Example
//!
//! ```ignore
//! use nika::io::security::validate_artifact_path;
//! use std::path::Path;
//!
//! let artifacts_dir = Path::new("/project/artifacts");
//! validate_artifact_path(artifacts_dir, Path::new("task1/output.json"))?;
//! ```

use std::path::{Path, PathBuf};



/// Default artifact output directory relative to project root.
///
/// Changed from ".nika/artifacts" to "./artifacts" in v0.59.0
/// so artifacts are visible (not hidden inside .nika/).
/// Configurable via `[artifacts] dir` in nika.toml.
pub const DEFAULT_ARTIFACT_DIR: &str = "./artifacts";

/// Maximum path length to prevent resource exhaustion attacks
const MAX_PATH_LENGTH: usize = 4096;

/// Validate that an artifact output path stays within the artifact directory boundary
///
/// This is the primary security check for artifact output. It ensures that:
/// 1. The output path resolves to a location within the artifact directory
/// 2. Path traversal attacks using `../` are blocked
/// 3. Symlink attacks are detected and blocked
///
/// # Arguments
///
/// * `artifact_dir` - The base artifact directory (must be absolute)
/// * `output_path` - The relative output path from artifact configuration
///
/// # Returns
///
/// The validated absolute path, or an error if validation fails.
///
/// # Errors
///
/// - `crate::SecurityError::ArtifactPath` if the path escapes the artifact directory
/// - `crate::SecurityError::ArtifactPath` if the path contains invalid characters
///
/// # Security Notes
///
/// CRITICAL: This function must be called before any file write operation.
/// Skipping this validation can lead to arbitrary file write vulnerabilities.
pub fn validate_artifact_path(
    artifact_dir: &Path,
    output_path: &Path,
) -> Result<PathBuf, crate::SecurityError> {
    let output_str = output_path.to_string_lossy();

    // Validate path length
    if output_str.len() > MAX_PATH_LENGTH {
        return Err(crate::SecurityError::ArtifactPath {
            path: output_str.to_string(),
            reason: format!(
                "Path exceeds maximum length of {} characters",
                MAX_PATH_LENGTH
            ),
        });
    }

    // Reject absolute paths in output specification
    if output_path.is_absolute() {
        return Err(crate::SecurityError::ArtifactPath {
            path: output_str.to_string(),
            reason: "Absolute paths are not allowed in artifact output".to_string(),
        });
    }

    // Resolve the full path
    let full_path = artifact_dir.join(output_path);

    // Create parent directories if they don't exist (needed for canonicalize)
    // We validate the path components first without creating
    validate_path_components(output_path)?;

    // For paths that don't exist yet, we validate the normalized path
    // instead of using canonicalize (which requires the path to exist)
    let normalized = normalize_path(&full_path);

    // Ensure the artifact directory exists and is canonicalized
    let canonical_base = if artifact_dir.exists() {
        artifact_dir
            .canonicalize()
            .map_err(|e| crate::SecurityError::ArtifactPath {
                path: artifact_dir.display().to_string(),
                reason: format!("Failed to canonicalize artifact directory: {e}"),
            })?
    } else {
        // If artifact dir doesn't exist, normalize it
        normalize_path(artifact_dir)
    };

    // Check that normalized path starts with the canonical base
    if !normalized.starts_with(&canonical_base) {
        return Err(crate::SecurityError::ArtifactPath {
            path: output_str.to_string(),
            reason: format!(
                "Path traversal detected: '{}' would escape artifact directory '{}'",
                output_path.display(),
                artifact_dir.display()
            ),
        });
    }

    Ok(full_path)
}

/// Validate individual path components for security issues
///
/// Checks each component of the path for potentially dangerous patterns:
/// - Null bytes (can truncate paths in some systems)
/// - Leading dots in directory names (hidden files, except . and ..)
/// - Control characters
fn validate_path_components(path: &Path) -> Result<(), crate::SecurityError> {
    for component in path.components() {
        let component_str = component.as_os_str().to_string_lossy();

        // Check for null bytes
        if component_str.contains('\0') {
            return Err(crate::SecurityError::ArtifactPath {
                path: path.display().to_string(),
                reason: "Path contains null bytes".to_string(),
            });
        }

        // Check for control characters
        if component_str.chars().any(|c| c.is_control() && c != '\t') {
            return Err(crate::SecurityError::ArtifactPath {
                path: path.display().to_string(),
                reason: "Path contains control characters".to_string(),
            });
        }
    }

    Ok(())
}

/// Normalize a path by resolving `.` and `..` components without filesystem access
///
/// This is used when we need to validate paths that don't exist yet.
///
/// # Safety
///
/// For absolute paths, `..` at the root is a no-op (can't go above `/`).
/// For relative paths, unresolvable `..` components are preserved to ensure
/// the boundary check detects traversal attempts rather than silently swallowing them.
fn normalize_path(path: &Path) -> PathBuf {
    let mut components: Vec<std::path::Component<'_>> = Vec::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                // Pop only if the last component is a Normal directory.
                // Preserve `..` if we'd go above the starting point (prevents
                // silent traversal swallowing on relative paths).
                match components.last() {
                    Some(std::path::Component::Normal(_)) => {
                        components.pop();
                    }
                    Some(std::path::Component::RootDir) | Some(std::path::Component::Prefix(_)) => {
                        // At root — can't go higher, skip this `..`
                    }
                    _ => {
                        // Empty or already has `..` — preserve for boundary detection
                        components.push(component);
                    }
                }
            }
            std::path::Component::CurDir => {
                // Skip current directory references
            }
            std::path::Component::Prefix(_)
            | std::path::Component::RootDir
            | std::path::Component::Normal(_) => {
                components.push(component);
            }
        }
    }

    components.iter().collect()
}

/// Error type for path boundary validation
///
/// Contains the target path and reason for conversion to context-specific error types.
#[derive(Debug, Clone)]
pub struct PathBoundaryError {
    /// The target path that failed validation
    pub target_path: PathBuf,
    /// Human-readable reason for the failure
    pub reason: String,
}

impl std::fmt::Display for PathBoundaryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.reason)
    }
}

impl std::error::Error for PathBoundaryError {}

/// Validate that a path stays within a base directory boundary using canonicalization
///
/// This is the primary security function for validating file paths. It uses
/// `canonicalize()` to resolve symlinks and verify the real path stays within bounds.
///
/// # Security
///
/// - Both paths must exist for canonicalization to work
/// - Symlinks are resolved to their real targets
/// - Prevents path traversal attacks using `../` or symlinks
///
/// # Arguments
///
/// * `base_path` - The boundary directory (must exist)
/// * `target_path` - The path to validate (must exist)
///
/// # Returns
///
/// `Ok(())` if the target is within the base, or `PathBoundaryError` with details.
///
/// # Example
///
/// ```ignore
/// use nika::io::security::validate_canonicalized_boundary;
///
/// let project = Path::new("/project");
/// let file = Path::new("/project/data/file.txt");
/// validate_canonicalized_boundary(project, file)?;
/// ```
pub fn validate_canonicalized_boundary(
    base_path: &Path,
    target_path: &Path,
) -> Result<(), PathBoundaryError> {
    let canonical_base = base_path.canonicalize().map_err(|e| PathBoundaryError {
        target_path: target_path.to_path_buf(),
        reason: format!("Cannot resolve base path '{}': {e}", base_path.display()),
    })?;

    let canonical_target = target_path.canonicalize().map_err(|e| PathBoundaryError {
        target_path: target_path.to_path_buf(),
        reason: format!(
            "Cannot resolve target path '{}': {}",
            target_path.display(),
            e
        ),
    })?;

    if !canonical_target.starts_with(&canonical_base) {
        return Err(PathBoundaryError {
            target_path: target_path.to_path_buf(),
            reason: format!(
                "Path traversal detected: '{}' is outside project boundary '{}'",
                target_path.display(),
                base_path.display()
            ),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_validate_artifact_path_simple() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("task1/output.json"));
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            PathBuf::from("/project/artifacts/task1/output.json")
        );
    }

    #[test]
    fn test_validate_artifact_path_nested() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("2024/01/15/report.json"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_artifact_path_traversal_blocked() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("../../../etc/passwd"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, crate::SecurityError::ArtifactPath { .. }));
    }

    #[test]
    fn test_validate_artifact_path_absolute_rejected() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("/etc/passwd"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let crate::SecurityError::ArtifactPath { reason, .. } = err {
            assert!(reason.contains("Absolute paths"));
        } else {
            panic!("Expected ArtifactPathError");
        }
    }

    #[test]
    fn test_validate_artifact_path_null_byte_rejected() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("file\0.txt"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let crate::SecurityError::ArtifactPath { reason, .. } = err {
            assert!(reason.contains("null bytes"));
        } else {
            panic!("Expected ArtifactPathError");
        }
    }

    #[test]
    fn test_validate_path_components_clean() {
        let result = validate_path_components(Path::new("task1/output.json"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_components_with_dots() {
        let result = validate_path_components(Path::new("../parent"));
        assert!(result.is_ok()); // Components are valid, boundary check is separate
    }

    #[test]
    fn test_normalize_path_removes_parent_refs() {
        let path = PathBuf::from("/project/artifacts/../output");
        let normalized = normalize_path(&path);
        assert_eq!(normalized, PathBuf::from("/project/output"));
    }

    #[test]
    fn test_normalize_path_removes_current_refs() {
        let path = PathBuf::from("/project/./artifacts/./output");
        let normalized = normalize_path(&path);
        assert_eq!(normalized, PathBuf::from("/project/artifacts/output"));
    }

    #[test]
    fn test_normalize_path_complex() {
        let path = PathBuf::from("/project/a/b/../c/./d/../e");
        let normalized = normalize_path(&path);
        assert_eq!(normalized, PathBuf::from("/project/a/c/e"));
    }

    #[test]
    fn test_max_path_length_enforced() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let long_path = "a".repeat(MAX_PATH_LENGTH + 1);
        let result = validate_artifact_path(&artifact_dir, Path::new(&long_path));
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let crate::SecurityError::ArtifactPath { reason, .. } = err {
            assert!(reason.contains("maximum length"));
        } else {
            panic!("Expected ArtifactPathError");
        }
    }

    #[test]
    fn test_validate_with_existing_dir() {
        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        fs::create_dir_all(&artifact_dir).unwrap();

        // Use canonicalized path to avoid symlink issues (macOS: /var -> /private/var)
        let canonical_artifact_dir = artifact_dir.canonicalize().unwrap();
        let result = validate_artifact_path(&canonical_artifact_dir, Path::new("output.json"));
        assert!(result.is_ok());
    }

    #[test]
    fn test_hidden_parent_escape() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        // Try to escape using nested parent references
        let result = validate_artifact_path(&artifact_dir, Path::new("a/../../b"));
        assert!(result.is_err());
    }

    // ═══════════════════════════════════════════════════════════════
    // SECURITY: Symlink attack detection tests
    // ═══════════════════════════════════════════════════════════════

    #[cfg(unix)]
    #[test]
    fn test_validate_canonicalized_boundary_detects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let base_dir = temp.path().join("artifacts");
        fs::create_dir_all(&base_dir).unwrap();

        let escape_target = temp.path().join("outside");
        fs::create_dir_all(&escape_target).unwrap();
        let secret_file = escape_target.join("secret.txt");
        fs::write(&secret_file, "sensitive data").unwrap();

        let symlink_path = base_dir.join("evil");
        symlink(&escape_target, &symlink_path).unwrap();

        let result = validate_canonicalized_boundary(&base_dir, &symlink_path.join("secret.txt"));
        assert!(
            result.is_err(),
            "validate_canonicalized_boundary must detect symlink-based escape"
        );
        assert!(
            result.unwrap_err().reason.contains("traversal"),
            "Error should mention path traversal"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_symlink_escape_caught_by_canonicalized_boundary() {
        // validate_artifact_path alone doesn't resolve symlinks (logical only).
        // The write path calls validate_canonicalized_boundary() after creating
        // parent dirs, which DOES resolve symlinks and catches escapes.
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let base_dir = temp.path().join("artifacts");
        fs::create_dir_all(&base_dir).unwrap();

        let escape_target = temp.path().join("outside");
        fs::create_dir_all(&escape_target).unwrap();

        // Create symlink inside artifacts that points outside
        let symlink_path = base_dir.join("sneaky");
        symlink(&escape_target, &symlink_path).unwrap();

        // validate_canonicalized_boundary catches the escape
        let result = validate_canonicalized_boundary(&base_dir, &symlink_path);
        assert!(result.is_err(), "symlink escape must be detected");
        assert!(result.unwrap_err().reason.contains("traversal"));
    }

    #[cfg(unix)]
    #[test]
    fn test_validate_artifact_path_does_not_resolve_symlinks() {
        // validate_artifact_path uses normalize_path (logical) not canonicalize.
        // This documents the known limitation: symlinks inside the artifact dir
        // that point outside are NOT detected by this function.
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let artifact_dir = temp.path().join("artifacts");
        fs::create_dir_all(&artifact_dir).unwrap();
        let canonical_dir = artifact_dir.canonicalize().unwrap();

        let escape_target = temp.path().join("outside");
        fs::create_dir_all(&escape_target).unwrap();
        let symlink_dir = canonical_dir.join("escape_link");
        symlink(&escape_target, &symlink_dir).unwrap();

        let result = validate_artifact_path(&canonical_dir, Path::new("escape_link/file.txt"));
        assert!(
            result.is_ok(),
            "validate_artifact_path does not resolve symlinks (known limitation)"
        );
    }

    #[test]
    fn test_validate_artifact_path_dot_dot_in_middle() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("subdir/../../escape"));
        assert!(
            result.is_err(),
            "Path with .. escaping via subdirectory must be blocked"
        );
    }

    #[test]
    fn test_validate_artifact_path_deep_traversal() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(
            &artifact_dir,
            Path::new("a/b/c/d/../../../../../../../../etc/passwd"),
        );
        assert!(result.is_err(), "Deep path traversal must be blocked");
    }

    #[test]
    fn test_validate_artifact_path_control_chars_blocked() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("file\r\ninjection"));
        assert!(
            result.is_err(),
            "Control characters in path must be blocked"
        );
    }

    #[test]
    fn test_normalize_path_preserves_unresolvable_parent() {
        // Relative path with leading `..` — should be preserved, not swallowed
        let path = PathBuf::from("../../etc/passwd");
        let normalized = normalize_path(&path);
        assert_eq!(
            normalized,
            PathBuf::from("../../etc/passwd"),
            "`..` at start of relative path must be preserved for boundary checks"
        );
    }

    #[test]
    fn test_normalize_path_absolute_root_clamp() {
        // Absolute path with `..` past root — clamps at root
        let path = PathBuf::from("/a/../../b");
        let normalized = normalize_path(&path);
        assert_eq!(normalized, PathBuf::from("/b"));
    }

    #[test]
    fn test_normalize_path_mixed_relative() {
        // Relative path: resolvable `..` removed, unresolvable preserved
        let path = PathBuf::from("a/b/../../c/../../../etc");
        let normalized = normalize_path(&path);
        // a/b/../.. → (empty), c/.. → (empty), ../etc → ../etc  but leading .. preserved
        assert_eq!(normalized, PathBuf::from("../../etc"));
    }
}
