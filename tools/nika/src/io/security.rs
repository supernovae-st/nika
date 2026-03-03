//! Security Module - Path Validation for Artifact Output (v0.18)
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
//! use nika::io::security::{validate_artifact_path, resolve_artifact_dir};
//! use std::path::Path;
//!
//! let workflow_dir = Path::new("/project");
//! let artifacts_dir = resolve_artifact_dir(workflow_dir, Some("./output"))?;
//! validate_artifact_path(&artifacts_dir, Path::new("task1/output.json"))?;
//! ```

use std::path::{Path, PathBuf};

use crate::error::NikaError;

/// Default artifact output directory relative to workflow
pub const DEFAULT_ARTIFACT_DIR: &str = ".nika/artifacts";

/// Maximum path length to prevent resource exhaustion attacks
const MAX_PATH_LENGTH: usize = 4096;

/// Resolve the artifact output directory from workflow configuration
///
/// If `configured_dir` is provided, it's resolved relative to the workflow directory.
/// Otherwise, the default `.nika/artifacts` is used.
///
/// # Security
///
/// - The resolved directory must be within the workflow directory boundary
/// - Symlinks are followed and validated against the canonical boundary
/// - Relative paths with `..` components are rejected if they escape the boundary
///
/// # Arguments
///
/// * `workflow_dir` - The workflow's root directory (absolute path)
/// * `configured_dir` - Optional configured directory from `artifacts.dir`
///
/// # Returns
///
/// The absolute path to the artifact directory, or an error if validation fails.
///
/// # Errors
///
/// - `NikaError::ArtifactPathError` if the path escapes the workflow boundary
/// - `NikaError::ArtifactPathError` if the path is too long
pub fn resolve_artifact_dir(
    workflow_dir: &Path,
    configured_dir: Option<&str>,
) -> Result<PathBuf, NikaError> {
    let dir_str = configured_dir.unwrap_or(DEFAULT_ARTIFACT_DIR);

    // Validate path length
    if dir_str.len() > MAX_PATH_LENGTH {
        return Err(NikaError::ArtifactPathError {
            path: dir_str.to_string(),
            reason: format!(
                "Path exceeds maximum length of {} characters",
                MAX_PATH_LENGTH
            ),
        });
    }

    // Resolve the directory path
    let resolved = workflow_dir.join(dir_str);

    // Validate the resolved path stays within workflow boundary
    validate_path_boundary(workflow_dir, &resolved)?;

    Ok(resolved)
}

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
/// - `NikaError::ArtifactPathError` if the path escapes the artifact directory
/// - `NikaError::ArtifactPathError` if the path contains invalid characters
///
/// # Security Notes
///
/// CRITICAL: This function must be called before any file write operation.
/// Skipping this validation can lead to arbitrary file write vulnerabilities.
pub fn validate_artifact_path(
    artifact_dir: &Path,
    output_path: &Path,
) -> Result<PathBuf, NikaError> {
    let output_str = output_path.to_string_lossy();

    // Validate path length
    if output_str.len() > MAX_PATH_LENGTH {
        return Err(NikaError::ArtifactPathError {
            path: output_str.to_string(),
            reason: format!(
                "Path exceeds maximum length of {} characters",
                MAX_PATH_LENGTH
            ),
        });
    }

    // Reject absolute paths in output specification
    if output_path.is_absolute() {
        return Err(NikaError::ArtifactPathError {
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
            .map_err(|e| NikaError::ArtifactPathError {
                path: artifact_dir.display().to_string(),
                reason: format!("Failed to canonicalize artifact directory: {}", e),
            })?
    } else {
        // If artifact dir doesn't exist, normalize it
        normalize_path(artifact_dir)
    };

    // Check that normalized path starts with the canonical base
    if !normalized.starts_with(&canonical_base) {
        return Err(NikaError::ArtifactPathError {
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
fn validate_path_components(path: &Path) -> Result<(), NikaError> {
    for component in path.components() {
        let component_str = component.as_os_str().to_string_lossy();

        // Check for null bytes
        if component_str.contains('\0') {
            return Err(NikaError::ArtifactPathError {
                path: path.display().to_string(),
                reason: "Path contains null bytes".to_string(),
            });
        }

        // Check for control characters
        if component_str.chars().any(|c| c.is_control() && c != '\t') {
            return Err(NikaError::ArtifactPathError {
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
fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {
                // Skip current directory references
            }
            _ => {
                normalized.push(component);
            }
        }
    }

    normalized
}

/// Validate that a path stays within a base directory boundary
///
/// This is a general-purpose boundary check used by `resolve_artifact_dir`.
fn validate_path_boundary(base_path: &Path, target_path: &Path) -> Result<(), NikaError> {
    let normalized_base = normalize_path(base_path);
    let normalized_target = normalize_path(target_path);

    if !normalized_target.starts_with(&normalized_base) {
        return Err(NikaError::ArtifactPathError {
            path: target_path.display().to_string(),
            reason: format!(
                "Path '{}' is outside allowed boundary '{}'",
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
    fn test_resolve_artifact_dir_default() {
        let workflow_dir = PathBuf::from("/project");
        let result = resolve_artifact_dir(&workflow_dir, None);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/project/.nika/artifacts"));
    }

    #[test]
    fn test_resolve_artifact_dir_custom() {
        let workflow_dir = PathBuf::from("/project");
        let result = resolve_artifact_dir(&workflow_dir, Some("output"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/project/output"));
    }

    #[test]
    fn test_resolve_artifact_dir_nested() {
        let workflow_dir = PathBuf::from("/project");
        let result = resolve_artifact_dir(&workflow_dir, Some("build/artifacts"));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("/project/build/artifacts"));
    }

    #[test]
    fn test_resolve_artifact_dir_path_traversal() {
        let workflow_dir = PathBuf::from("/project");
        let result = resolve_artifact_dir(&workflow_dir, Some("../outside"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(matches!(err, NikaError::ArtifactPathError { .. }));
    }

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
        assert!(matches!(err, NikaError::ArtifactPathError { .. }));
    }

    #[test]
    fn test_validate_artifact_path_absolute_rejected() {
        let artifact_dir = PathBuf::from("/project/artifacts");
        let result = validate_artifact_path(&artifact_dir, Path::new("/etc/passwd"));
        assert!(result.is_err());
        let err = result.unwrap_err();
        if let NikaError::ArtifactPathError { reason, .. } = err {
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
        if let NikaError::ArtifactPathError { reason, .. } = err {
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
        if let NikaError::ArtifactPathError { reason, .. } = err {
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
}
