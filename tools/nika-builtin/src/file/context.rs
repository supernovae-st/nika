// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! FileToolContext — minimal shared state for the 5 builtin file tools.
//!
//! Holds the working directory (security boundary) and the read-file cache
//! required by EditTool (must read a file before editing it).

use crate::BuiltinError;
use dashmap::DashMap;
use nika_kernel::task_local::current_working_dir;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Shared context for builtin file tools.
///
/// Constructed by the engine's `BuiltinToolRouter::with_file_tools()` from
/// the `working_dir` of `ToolContext`. Does not depend on nika-engine types.
pub struct FileToolContext {
    /// Working directory — all paths must fall within this boundary.
    pub working_dir: PathBuf,
    /// Cached canonical form of `working_dir` (S12.E1). Populated once on
    /// first `validate_path` call. Only used when the `CURRENT_WORKING_DIR`
    /// task_local is `None`; when set, `with_base_path()` rotates the
    /// effective dir at runtime and the cache would be stale.
    working_dir_canonical: OnceLock<PathBuf>,
    /// Tracks files that have been read. EditTool enforces a read-before-edit
    /// contract to prevent blind overwrites.
    read_files: DashMap<PathBuf, bool>,
}

impl FileToolContext {
    /// Create a new context with the given working directory.
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            working_dir_canonical: OnceLock::new(),
            read_files: DashMap::new(),
        }
    }

    /// Return the cached canonical form of `self.working_dir`.
    ///
    /// S12.D3 — failure is a hard error (`BuiltinError::Io`), never a
    /// silent fall-back to the non-canonical path. The previous
    /// `unwrap_or_else(_ → effective_dir.clone())` let the boundary check
    /// compare against a non-canonical path, which defeated symlink
    /// resolution on macOS `/var` ↔ `/private/var`.
    ///
    /// S12.E1 — populated on first use via `OnceLock::get_or_init` style
    /// (manual check + set so we can return a `BuiltinError` rather than
    /// requiring nightly `get_or_try_init`).
    fn cached_work_canonical(&self) -> Result<PathBuf, BuiltinError> {
        if let Some(cached) = self.working_dir_canonical.get() {
            return Ok(cached.clone());
        }
        let canonical =
            self.working_dir.canonicalize().map_err(|e| BuiltinError::Io {
                tool: "file-tool".into(),
                reason: format!(
                    "[NIKA-200] Failed to canonicalize working_dir {}: {e}",
                    self.working_dir.display()
                ),
            })?;
        // Best-effort set — if another thread won the race, both values
        // are identical, so returning our own clone is fine.
        let _ = self.working_dir_canonical.set(canonical.clone());
        Ok(canonical)
    }

    /// Test-only accessor: is the canonical form cached yet?
    #[cfg(test)]
    pub(crate) fn is_work_canonical_cached(&self) -> bool {
        self.working_dir_canonical.get().is_some()
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

    /// Validate that `raw_path` lies within the working directory.
    ///
    /// Accepts both absolute and relative paths. Relative paths are resolved
    /// against the effective working directory, which is:
    /// 1. `CURRENT_WORKING_DIR` task_local (set by TaskExecutor before each dispatch),
    ///    so that `with_base_path()` changes after router construction are respected.
    /// 2. `self.working_dir` as fallback (used outside a runner context, e.g. tests).
    ///
    /// Returns the canonicalized (or lexically-normalized) path on success.
    pub fn validate_path(&self, raw_path: &str) -> Result<PathBuf, BuiltinError> {
        let input = Path::new(raw_path);

        // Use task_local working dir if available (keeps in sync with with_base_path()).
        // `tl` is `Some` when a runner has set a per-task override; in that
        // case we cannot use the cached canonical form because the override
        // rotates between tasks.
        let tl = current_working_dir();
        let effective_dir = tl.clone().unwrap_or_else(|| self.working_dir.clone());

        // Resolve relative paths against effective_dir
        let path = if input.is_absolute() {
            input.to_path_buf()
        } else {
            effective_dir.join(input)
        };

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
            canonicalize_best_effort(&path)
        };

        // S12.D3 + E1 — cached hard-error canonicalization of the working dir.
        // The task-local override path is not cached (it rotates).
        let work_canonical = if tl.is_none() {
            self.cached_work_canonical()?
        } else {
            effective_dir.canonicalize().map_err(|e| BuiltinError::Io {
                tool: "file-tool".into(),
                reason: format!(
                    "[NIKA-200] Failed to canonicalize task working_dir {}: {e}",
                    effective_dir.display()
                ),
            })?
        };

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
    fn test_validate_relative_path_resolves_in_working_dir() {
        let (d, ctx) = temp_ctx();
        // Relative paths are resolved against working_dir — should succeed (no NIKA-211)
        let result = ctx.validate_path("relative/path.txt");
        assert!(result.is_ok(), "relative path should resolve within working_dir: {:?}", result);
        // The resolved path should be within the working dir
        assert!(result.unwrap().starts_with(d.path().canonicalize().unwrap()));
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

    // ── S12.E1 + S12.D3 — canonicalize cache + hard error ──

    #[test]
    fn test_working_dir_canonical_cached_after_first_validate() {
        let (_d, ctx) = temp_ctx();
        assert!(
            !ctx.is_work_canonical_cached(),
            "cache must start empty"
        );
        // Any validate_path call against the struct-level working_dir
        // (no task_local override) populates the cache.
        let _ = ctx.validate_path("file.txt");
        assert!(
            ctx.is_work_canonical_cached(),
            "cache must be populated after first validate_path"
        );
    }

    #[test]
    fn test_working_dir_canonical_hard_error_when_missing() {
        // S12.D3: canonicalize failure on the struct-level working_dir is a
        // hard error (BuiltinError::Io), never a silent fall-back.
        // Build a tempdir, capture its path, then destroy it so the
        // canonicalize call hits ENOENT.
        let tmp = TempDir::new().unwrap();
        let dead_dir = tmp.path().to_path_buf();
        drop(tmp);
        let ctx = FileToolContext::new(dead_dir.clone());

        let result = ctx.validate_path("anything.txt");
        match result {
            Err(BuiltinError::Io { tool, reason }) => {
                assert_eq!(tool, "file-tool");
                assert!(
                    reason.contains("NIKA-200")
                        && reason.contains("canonicalize")
                        && reason.contains("working_dir"),
                    "unexpected reason: {reason}"
                );
            }
            other => panic!("expected BuiltinError::Io hard error, got {other:?}"),
        }
        // A hard error must NOT populate the cache.
        assert!(
            !ctx.is_work_canonical_cached(),
            "cache must not be populated on canonicalize failure"
        );
    }
}
