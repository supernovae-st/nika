// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `nika-fs` — the production filesystem implementation for the Nika diamond.
//!
//! This crate sits at **L1** (effect crate): it implements the L0.5
//! `nika_kernel::fs` trait family (`FsRead` · `FsWrite` · `FsMeta` ·
//! `FsList`) using real filesystem I/O via `tokio::fs`. Every crate that
//! touches files injects the kernel traits and receives [`TokioFs`] in
//! production, `MockFs` in tests — the kernel contract keeps the engine
//! hermetic (Invariant #27).
//!
//! [`TokioFs`] implements the **`*Dyn` trait-variant companions**
//! (`FsReadDyn` etc. — the `Send`-future forms), so the base traits come
//! for free via the `trait_variant` blanket impls AND the futures are
//! `Send`: consumers may `tokio::spawn` filesystem work directly.
//!
//! ```rust,no_run
//! use nika_fs::TokioFs;
//! use nika_kernel::FsRead;
//! use nika_kernel::fs::FsError;
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), FsError> {
//! let fs = TokioFs;
//! let content = fs.read_to_string(Path::new("workflow.nika.yaml")).await?;
//! println!("{content}");
//! # Ok(())
//! # }
//! ```
//!
//! # Atomic writes (the Diamond upgrade vs brouillon)
//!
//! [`FsWrite::write`](nika_kernel::FsWrite) targets atomic semantics per
//! the kernel CANCEL SAFETY contract: contents land in a hidden temp file
//! in the destination directory, then a single `rename` makes them
//! visible (POSIX guarantees rename atomicity within one filesystem).
//! Readers never observe a half-written file. A future dropped between
//! the temp write and the rename may leave a stale `.nika-tmp.*` or `..nika-tmp.*` file —
//! exactly the failure mode the kernel trait documents — and the error
//! path cleans its temp file best-effort. Durability (`fsync`) is
//! intentionally NOT provided at this layer; callers needing
//! crash-durability gate it at the policy/engine layer.
//!
//! **Rename-replace semantics** (differ from a plain in-place write):
//! an existing destination's permissions/xattrs are REPLACED by the
//! fresh temp file's default mode (a `0600` file becomes umask-default
//! after overwrite); other hardlinks to the old inode keep the OLD
//! content; a symlink destination is REPLACED by a regular file, not
//! followed. Callers needing preserved modes or follow-symlink writes
//! gate that at the policy layer.
//!
//! # Security posture
//!
//! `nika-fs` is a **mechanism** crate: it performs exactly the I/O it is
//! asked to perform. Path capability gating (allow-lists, sandbox roots,
//! traversal policy) is the job of `nika-policy` (L1.5) — keeping the
//! effect crate policy-free is what lets the policy layer reason about
//! ALL filesystem access in one place.
//!
//! [`OwnedDir`] is the synchronous crash-durable exception for state machines:
//! it holds a directory descriptor, admits only contained child components,
//! and refuses symlinks at every directory and file open. Callers still choose
//! the root, names, and lifecycle policy.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use bytes::Bytes;
use globset::GlobMatcher;
use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

mod owned_dir;
pub use owned_dir::OwnedDir;
mod write_new;

#[cfg(test)]
mod write_new_tests;

/// Monotonic discriminator for temp-file names: two concurrent writes to
/// the same destination must never collide on the same temp path.
static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Production filesystem backed by `tokio::fs`.
///
/// Zero-size — no allocation, no state, trivially `Copy`/`Default`. The
/// only production site touching `tokio::fs`; pure crates (L0) and the
/// kernel (L0.5) stay filesystem-free.
///
/// Implements the four `*Dyn` kernel companions (`Send` futures), which
/// blanket-provide the base `FsRead`/`FsWrite`/`FsMeta`/`FsList` — and
/// therefore the umbrella `Fs` super-trait.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioFs;

impl FsReadDyn for TokioFs {
    /// Read a file's entire contents as bytes.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        let data = tokio::fs::read(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))?;
        Ok(Bytes::from(data))
    }

    /// Read a file's entire contents as a UTF-8 string.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        tokio::fs::read_to_string(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))
    }

    /// Check whether a path exists. I/O errors (permission, broken
    /// symlink) report `false` — existence is a query, not an assertion.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn exists(&self, path: &Path) -> bool {
        tokio::fs::try_exists(path).await.unwrap_or(false)
    }

    /// Canonicalize a path (resolve symlinks and relative components).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        tokio::fs::canonicalize(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))
    }
}

impl FsWriteDyn for TokioFs {
    /// Publish a completed sibling file with an exclusive hard link.
    /// No fsync is added. The owned blocking operation can finish after its
    /// future is dropped; cleanup of the temporary name is best-effort.
    async fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        let path = path.to_path_buf();
        let contents = contents.to_vec();
        tokio::task::spawn_blocking(move || {
            let staged = write_new::StagedFile::create(&path, &contents)?;
            staged.publish(&path)
        })
        .await
        .map_err(|error| FsError::Io {
            reason: format!("exclusive write worker failed: {error}"),
        })?
    }

    /// Write contents to a file atomically (creates or overwrites).
    ///
    /// Parent directories are created automatically (brouillon parity).
    /// Contents land in a temp file beside the destination, then one
    /// `rename` publishes them — readers never see a partial file.
    /// Rename-replace does NOT preserve an existing destination's
    /// permissions/xattrs, diverges other hardlinks, and replaces (does
    /// not follow) a symlink destination — see the crate docs.
    ///
    /// # Errors
    ///
    /// `InvalidInput` when `path` has no file name (e.g. `/`); otherwise
    /// any underlying I/O error. On error the temp file is removed
    /// best-effort.
    ///
    /// CANCEL SAFETY: `tokio::fs` operations detach (not abort) on drop —
    /// a cancelled write may still complete in the background, either
    /// leaving a stale `.nika-tmp.*` or `..nika-tmp.*` file or fully publishing the
    /// destination, but NEVER a partial destination (the
    /// kernel-documented trade).
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        if path.file_name().is_none() {
            return Err(FsError::InvalidData {
                path: path.display().to_string(),
                reason: "write path has no file name".to_owned(),
            });
        }

        let (parent_to_create, tmp) = tmp_sibling(path);
        if let Some(p) = parent_to_create {
            tokio::fs::create_dir_all(p)
                .await
                .map_err(|e| FsError::from_io(&e, p))?;
        }

        // Write the temp, then publish it with one rename. ANY failure
        // after the temp path is chosen removes it best-effort at a SINGLE
        // site — a partial `tokio::fs::write` (e.g. ENOSPC mid-stream) would
        // otherwise leak a `.nika-tmp.*` file, and the crate docs promise the
        // error path cleans up. On rename success the temp no longer exists,
        // so the guard only fires on a real write-or-rename failure.
        let publish = async {
            tokio::fs::write(&tmp, contents)
                .await
                .map_err(|e| FsError::from_io(&e, &tmp))?;
            tokio::fs::rename(&tmp, path)
                .await
                .map_err(|e| FsError::from_io(&e, path))
        };
        let result = publish.await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
        result
    }

    /// Create a directory and all parent directories (idempotent).
    ///
    /// CANCEL SAFETY: partial cancel-safe — each mkdir is atomic; a drop
    /// mid-chain may leave a partial prefix. Idempotent retry OK.
    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError> {
        tokio::fs::create_dir_all(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))
    }

    /// Remove a file.
    ///
    /// CANCEL SAFETY: cancel-safe — single unlink syscall.
    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))
    }
}

impl FsMetaDyn for TokioFs {
    /// Get metadata for a path (follows symlinks, like `stat`).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
        let meta = tokio::fs::metadata(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))?;
        Ok(FileMetadata::new(meta.len(), meta.is_file(), meta.is_dir()))
    }
}

impl FsListDyn for TokioFs {
    /// List entries in a directory as full paths, deterministically
    /// sorted (byte order) so downstream DAG planning is reproducible.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        let mut entries = tokio::fs::read_dir(path)
            .await
            .map_err(|e| FsError::from_io(&e, path))?;
        let mut results = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| FsError::from_io(&e, path))?
        {
            results.push(entry.path());
        }
        results.sort();
        Ok(results)
    }

    /// Find files matching a glob pattern relative to a root directory.
    ///
    /// Pattern semantics (brouillon parity): `literal_separator(true)`,
    /// so `*` never crosses `/` while `**` matches zero or more
    /// components. Matching runs against the path RELATIVE to `root`.
    /// Hidden directories (name starting with `.`) are not traversed;
    /// symlinked directories are not followed (`file_type` does not
    /// follow links), so cycles terminate. Results are sorted.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for an invalid glob pattern; otherwise any
    /// underlying I/O error (e.g. `NotFound` for a missing root).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only directory walk).
    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        let matcher = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| FsError::InvalidData {
                path: root.display().to_string(),
                reason: format!("invalid glob pattern '{pattern}': {e}"),
            })?
            .compile_matcher();

        let mut results = walk_matches(root, &matcher).await?;
        results.sort();
        Ok(results)
    }
}

/// Compute the parent directory to create (None when the destination is
/// a bare relative name — the cwd already exists) and the unique temp
/// sibling path for an atomic write. Pure — unit-testable without cwd.
fn tmp_sibling(path: &Path) -> (Option<&Path>, PathBuf) {
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let dir = parent.unwrap_or_else(|| Path::new("."));
    // Discriminator-derived temp name: embedding the file name
    // would risk ENAMETOOLONG near the 255-byte limit and lossy
    // collisions on non-UTF-8 names — pid+counter alone guarantees
    // in-process uniqueness (review swarm P2 · 2026-06-10).
    // A destination may itself look like a temporary name. Distinguish it
    // with punctuation, so case folding cannot alias staging to destination.
    let name = path.file_name().map(std::ffi::OsStr::as_encoded_bytes);
    let prefix = if name.is_some_and(|name| name.starts_with(b".") && !name.starts_with(b"..")) {
        "..nika-tmp"
    } else {
        ".nika-tmp"
    };
    let tmp = dir.join(format!(
        "{prefix}.{}.{}",
        std::process::id(),
        TMP_COUNTER.fetch_add(1, Ordering::Relaxed),
    ));
    (parent, tmp)
}

/// Iteratively walk `root`, collecting non-directory entries whose
/// root-relative path matches `matcher`. A stack replaces recursion: no
/// `Box::pin` per directory, no recursion-depth concern on deep trees.
///
/// Fail-closed: any `read_dir`/`file_type` error mid-walk (e.g. EACCES
/// on one subdirectory) aborts the WHOLE glob — a workflow engine must
/// not silently return partial file sets.
async fn walk_matches(root: &Path, matcher: &GlobMatcher) -> Result<Vec<PathBuf>, FsError> {
    let mut results = Vec::new();
    let mut stack = vec![root.to_path_buf()];

    while let Some(dir) = stack.pop() {
        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| FsError::from_io(&e, &dir))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| FsError::from_io(&e, &dir))?
        {
            let path = entry.path();
            if entry
                .file_type()
                .await
                .map_err(|e| FsError::from_io(&e, &path))?
                .is_dir()
            {
                // `file_type` does not follow symlinks, so a symlinked
                // dir reports !is_dir and is treated as a leaf below —
                // cycles cannot occur.
                //
                // Byte-level hidden check: `.`-prefixed names hide even
                // when the rest of the name is not valid UTF-8 (the
                // ASCII prefix byte survives WTF-8 on Windows too) —
                // a `to_str()`-based check would fail OPEN (review P2).
                let hidden = entry.file_name().as_encoded_bytes().first() == Some(&b'.');
                if !hidden {
                    stack.push(path);
                }
            } else {
                // Structurally, every entry descends from `root` by
                // construction (paths are built by join from the stack),
                // so strip_prefix cannot fail — but if it ever did,
                // silently matching against an ABSOLUTE path would be
                // wrong. Skip explicitly instead (review swarm P2).
                let Ok(relative) = path.strip_prefix(root) else {
                    continue;
                };
                if matcher.is_match(relative) {
                    results.push(path);
                }
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Type-level assertions on the public type live in
    // `tests/fs_contract.rs` — here only crate-private invariants.

    #[test]
    fn tmp_counter_is_monotonic() {
        let a = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let b = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        assert!(b > a, "temp discriminator must never repeat in-process");
    }

    #[test]
    fn tmp_sibling_bare_relative_maps_to_cwd() {
        let (parent, tmp) = tmp_sibling(Path::new("bare.txt"));
        assert!(parent.is_none(), "bare name: nothing to create");
        assert_eq!(tmp.parent(), Some(Path::new(".")));
        assert!(
            tmp.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with(".nika-tmp.")),
            "temp must be a hidden .nika-tmp sibling, got {tmp:?}"
        );
    }

    #[test]
    fn tmp_sibling_nested_path_stays_beside_destination() {
        let dest = Path::new("/abs/dir/file.txt");
        let (parent, tmp) = tmp_sibling(dest);
        assert_eq!(parent, Some(Path::new("/abs/dir")));
        assert_eq!(
            tmp.parent(),
            dest.parent(),
            "temp and destination must share a directory (same-fs rename)"
        );
    }

    #[test]
    fn tmp_sibling_never_repeats() {
        let a = tmp_sibling(Path::new("/d/f")).1;
        let b = tmp_sibling(Path::new("/d/f")).1;
        assert_ne!(a, b, "two writes to one destination need distinct temps");
    }
}
