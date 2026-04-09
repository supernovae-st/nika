// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! FileToolContext — minimal shared state for the 5 builtin file tools.
//!
//! Holds the working directory (security boundary) and the read-file cache
//! required by EditTool (must read a file before editing it).

use crate::BuiltinError;
use dashmap::DashMap;
use std::path::{Path, PathBuf};

/// Shared context for builtin file tools.
///
/// Constructed by the engine's `BuiltinToolRouter::with_file_tools()` from
/// the `working_dir` of `ToolContext`. Does not depend on nika-engine types.
pub struct FileToolContext {
    /// Working directory — all paths must fall within this boundary.
    pub working_dir: PathBuf,
    /// Tracks files that have been read. EditTool enforces a read-before-edit
    /// contract to prevent blind overwrites.
    read_files: DashMap<PathBuf, bool>,
}

impl FileToolContext {
    /// Create a new context with the given working directory.
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            read_files: DashMap::new(),
        }
    }

    /// Mark a path as having been read. Called by ReadTool after a successful read.
    pub fn mark_read(&self, path: &Path) {
        self.read_files.insert(path.to_path_buf(), true);
    }

    /// Check whether a path has been read in this context.
    ///
    /// Canonicalizes the path before lookup so that `/var/...` and
    /// `/private/var/...` (macOS symlink equivalents) match correctly.
    pub fn has_been_read(&self, path: &Path) -> bool {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.read_files.contains_key(&canonical)
    }

    /// Validate that `raw_path` is absolute and lies within the working directory.
    ///
    /// Returns the canonicalized (or lexically-normalized) path on success.
    pub fn validate_path(&self, raw_path: &str) -> Result<PathBuf, BuiltinError> {
        let path = Path::new(raw_path);

        if !path.is_absolute() {
            return Err(BuiltinError::InvalidArgs {
                tool: "file-tool".into(),
                reason: format!("[NIKA-211] Path must be absolute, got: {raw_path}"),
            });
        }

        // Canonicalize to resolve symlinks and `..` components so that
        // `/work/../../etc/passwd` cannot escape the boundary.
        //
        // For non-existing files: canonicalize the deepest existing ancestor,
        // then append the remaining components. This correctly handles macOS
        // where `TempDir` paths like `/var/...` are symlinks to `/private/var/...`.
        let canonical = if path.exists() {
            path.canonicalize().map_err(|e| BuiltinError::Io {
                tool: "file-tool".into(),
                reason: format!("Failed to canonicalize path {raw_path}: {e}"),
            })?
        } else {
            canonicalize_best_effort(path)
        };

        let work_canonical = self
            .working_dir
            .canonicalize()
            .unwrap_or_else(|_| self.working_dir.clone());

        if !canonical.starts_with(&work_canonical) {
            return Err(BuiltinError::InvalidArgs {
                tool: "file-tool".into(),
                reason: format!(
                    "[NIKA-204] Path is outside the working directory. \
                     path={}, working_dir={}",
                    canonical.display(),
                    work_canonical.display()
                ),
            });
        }

        Ok(canonical)
    }
}

/// Canonicalize a non-existing path by resolving its deepest existing ancestor
/// and appending the remaining components.
///
/// This correctly handles macOS where `/var` is a symlink to `/private/var`:
/// `canonicalize_best_effort("/var/foo/new.txt")` = `/private/var/foo/new.txt`.
fn canonicalize_best_effort(path: &Path) -> PathBuf {
    // Find the deepest ancestor that exists
    let mut existing_part = path;
    while !existing_part.exists() {
        match existing_part.parent() {
            Some(p) if p != existing_part => existing_part = p,
            _ => break,
        }
    }

    // Collect remaining components that don't exist yet
    let remainder: Vec<_> = path
        .components()
        .skip(existing_part.components().count())
        .collect();

    // Canonicalize the existing ancestor
    let canonical_base = if existing_part.exists() {
        existing_part
            .canonicalize()
            .unwrap_or_else(|_| existing_part.to_path_buf())
    } else {
        // Nothing exists — return a lexically-normalized version
        return normalize_lexically(path);
    };

    // Append remaining components
    let mut result = canonical_base;
    for comp in remainder {
        result.push(comp);
    }
    result
}

/// Lexically resolve `..` and `.` components without I/O.
fn normalize_lexically(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            Component::Normal(_) | Component::RootDir | Component::Prefix(_) => {
                result.push(component);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn temp_ctx() -> (TempDir, FileToolContext) {
        let d = TempDir::new().unwrap();
        let ctx = FileToolContext::new(d.path().to_path_buf());
        (d, ctx)
    }

    #[test]
    fn test_validate_path_within_boundary() {
        let (d, ctx) = temp_ctx();
        let path = d.path().join("file.txt");
        // File doesn't need to exist for validate_path
        let result = ctx.validate_path(&path.to_string_lossy());
        assert!(result.is_ok(), "{:?}", result);
    }

    #[test]
    fn test_validate_path_outside_boundary_rejected() {
        let (_d, ctx) = temp_ctx();
        let result = ctx.validate_path("/etc/passwd");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("NIKA-204") || msg.contains("outside"), "{msg}");
    }

    #[test]
    fn test_validate_relative_path_rejected() {
        let (_d, ctx) = temp_ctx();
        let result = ctx.validate_path("relative/path.txt");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("NIKA-211") || msg.contains("absolute"), "{msg}");
    }

    #[test]
    fn test_validate_path_traversal_rejected() {
        let (d, ctx) = temp_ctx();
        let evil = format!("{}/../../etc/passwd", d.path().display());
        let result = ctx.validate_path(&evil);
        assert!(result.is_err());
    }

    #[test]
    fn test_mark_read_and_has_been_read() {
        let (d, ctx) = temp_ctx();
        let path = d.path().join("file.txt");
        assert!(!ctx.has_been_read(&path));
        ctx.mark_read(&path);
        assert!(ctx.has_been_read(&path));
    }
}
