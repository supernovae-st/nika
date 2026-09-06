// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem traits — ISP decomposition of async FS operations.
//!
//! 4 atomic traits: `FsRead`, `FsWrite`, `FsMeta`, `FsList`.
//! 1 super-trait: `Fs` (blanket for all 4).

use std::path::{Path, PathBuf};

use bytes::Bytes;

#[cfg(test)]
mod legacy_write_tests;

/// Metadata about a filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct FileMetadata {
    /// Size in bytes.
    pub len: u64,
    /// Whether the entry is a regular file.
    pub is_file: bool,
    /// Whether the entry is a directory.
    pub is_dir: bool,
}

impl FileMetadata {
    /// Create new file metadata.
    #[must_use]
    pub fn new(len: u64, is_file: bool, is_dir: bool) -> Self {
        Self {
            len,
            is_file,
            is_dir,
        }
    }
}

/// Filesystem errors — the typed taxonomy the `Fs*` traits return.
///
/// Carries the NIKA `FileIo` code range (110–119, sibling of `BlobError`'s
/// 100–102) via `NikaErrorCode` in `crate::errors`. The traits return this
/// instead of `std::io::Result` so the typed Nika error survives the contract
/// boundary (forward-compat: a `std::io::Error` return would discard the code +
/// freeze the un-typed surface once an `Fs` impl is admitted). Mirror the
/// `BlobError`/`ShellError` pattern.
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
#[non_exhaustive]
pub enum FsError {
    /// Path does not exist.
    #[error("path not found: {path}")]
    NotFound {
        /// The path that was not found.
        path: String,
    },

    /// Permission denied for the path.
    #[error("permission denied: {path}")]
    PermissionDenied {
        /// The path access was denied for.
        path: String,
    },

    /// Path already exists (e.g. an exclusive create).
    #[error("path already exists: {path}")]
    AlreadyExists {
        /// The path that already exists.
        path: String,
    },

    /// Content was not valid for the operation (e.g. non-UTF-8 in
    /// `read_to_string`, or an invalid glob pattern).
    #[error("invalid data for {path}: {reason}")]
    InvalidData {
        /// The path (or pattern) involved.
        path: String,
        /// What was invalid.
        reason: String,
    },

    /// Any other I/O failure.
    #[error("filesystem I/O error: {reason}")]
    Io {
        /// Description of the I/O failure.
        reason: String,
    },
}

impl FsError {
    /// Map a `std::io::Error` (with the path the caller operated on) into the
    /// typed taxonomy. The real L1 `Fs` impl wraps `tokio::fs`/`std::fs` calls
    /// with this: `… .map_err(|e| FsError::from_io(&e, path))`.
    #[must_use]
    pub fn from_io(err: &std::io::Error, path: &Path) -> Self {
        let p = path.display().to_string();
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound { path: p },
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied { path: p },
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists { path: p },
            std::io::ErrorKind::InvalidData => Self::InvalidData {
                path: p,
                reason: err.to_string(),
            },
            _ => Self::Io {
                reason: err.to_string(),
            },
        }
    }
}

/// Read-only filesystem operations.
///
/// CANCEL SAFETY: every read method is cancel-safe. Read ops have no
/// side effects, so dropping the future abandons the RESULT — note
/// that `tokio::fs`-style impls detach (not abort) the underlying
/// blocking syscall, which runs to completion in the background; for
/// read-only ops that is unobservable.
#[trait_variant::make(FsReadDyn: Send)]
pub trait FsRead: Send + Sync {
    /// Read a file's entire contents as bytes.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read(&self, path: &Path) -> Result<Bytes, FsError>;

    /// Read a file's entire contents as a UTF-8 string.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read_to_string(&self, path: &Path) -> Result<String, FsError>;

    /// Check whether a path exists.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn exists(&self, path: &Path) -> bool;

    /// Canonicalize a path (resolve symlinks and relative components).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError>;
}

/// Write filesystem operations.
///
/// CANCEL SAFETY: NOT cancel-safe at the raw-syscall level. Impls
/// targeting atomic semantics MUST use temp-file + rename (POSIX
/// guarantees rename atomicity within the same filesystem). A
/// dropped future may leave partial writes OR a stale temp file —
/// and `tokio::fs`-style impls detach (not abort) on drop, so a
/// cancelled write may still fully PUBLISH the destination in the
/// background. Callers on non-atomic impls MUST retry cautiously.
#[trait_variant::make(FsWriteDyn: Send)]
pub trait FsWrite: Send + Sync {
    /// Write contents to a file (creates or overwrites).
    ///
    /// CANCEL SAFETY: NOT cancel-safe unless impl uses atomic
    /// temp-file + rename. Partial writes possible.
    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError>;

    /// Publish complete contents only if the destination is absent at commit.
    /// An occupied name, including a symlink, returns `AlreadyExists` without
    /// replacing it. Durability is not implied. Cancellation can leave a
    /// temporary file or a completed publication; dropping a future does not
    /// prove that a detached filesystem operation has stopped.
    ///
    /// Existing backends remain source-compatible but must implement this
    /// operation before serving exclusive writes. The default refuses without
    /// I/O; it must never approximate exclusivity with `exists` then `write`.
    fn write_new(
        &self,
        path: &Path,
        contents: &[u8],
    ) -> impl std::future::Future<Output = Result<(), FsError>> + Send {
        let _ = (path, contents);
        async {
            Err(FsError::Io {
                reason: "exclusive publication unsupported by this filesystem backend".to_owned(),
            })
        }
    }

    /// Create a directory and all parent directories.
    ///
    /// CANCEL SAFETY: partial cancel-safe — each mkdir is atomic, but
    /// a drop mid-chain may leave a partial prefix. Idempotent retry OK.
    async fn create_dir_all(&self, path: &Path) -> Result<(), FsError>;

    /// Remove a file.
    ///
    /// CANCEL SAFETY: cancel-safe — single unlink syscall.
    async fn remove_file(&self, path: &Path) -> Result<(), FsError>;
}

/// Filesystem metadata operations.
///
/// CANCEL SAFETY: cancel-safe (read-only stat syscall).
#[trait_variant::make(FsMetaDyn: Send)]
pub trait FsMeta: Send + Sync {
    /// Get metadata for a path.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError>;
}

/// Filesystem listing operations.
///
/// CANCEL SAFETY: cancel-safe (read-only — opendir/readdir/closedir).
#[trait_variant::make(FsListDyn: Send)]
pub trait FsList: Send + Sync {
    /// List entries in a directory.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError>;

    /// Find files matching a glob pattern relative to a root directory.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only directory walk).
    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError>;
}

/// Full filesystem access — blanket super-trait.
///
/// Any type implementing all 4 atomic traits automatically satisfies `Fs`.
pub trait Fs: FsRead + FsWrite + FsMeta + FsList {}
impl<T: FsRead + FsWrite + FsMeta + FsList> Fs for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_metadata_new() {
        let meta = FileMetadata::new(1024, true, false);
        assert_eq!(meta.len, 1024);
        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[test]
    fn file_metadata_directory() {
        let meta = FileMetadata::new(0, false, true);
        assert!(meta.is_dir);
        assert!(!meta.is_file);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn file_metadata_is_send_sync() {
        _assert_send_sync::<FileMetadata>();
    }

    /// Verify blanket super-trait: a type implementing all 4 atomics gets Fs for free.
    struct DummyFs;

    impl FsRead for DummyFs {
        async fn read(&self, _: &Path) -> Result<Bytes, FsError> {
            Ok(Bytes::new())
        }
        async fn read_to_string(&self, _: &Path) -> Result<String, FsError> {
            Ok(String::new())
        }
        async fn exists(&self, _: &Path) -> bool {
            false
        }
        async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
            Ok(path.to_path_buf())
        }
    }

    impl FsWrite for DummyFs {
        async fn write(&self, _: &Path, _: &[u8]) -> Result<(), FsError> {
            Ok(())
        }
        async fn create_dir_all(&self, _: &Path) -> Result<(), FsError> {
            Ok(())
        }
        async fn remove_file(&self, _: &Path) -> Result<(), FsError> {
            Ok(())
        }
    }

    impl FsMeta for DummyFs {
        async fn metadata(&self, _: &Path) -> Result<FileMetadata, FsError> {
            Ok(FileMetadata::new(0, false, false))
        }
    }

    impl FsList for DummyFs {
        async fn list_dir(&self, _: &Path) -> Result<Vec<PathBuf>, FsError> {
            Ok(vec![])
        }
        async fn glob(&self, _: &Path, _: &str) -> Result<Vec<PathBuf>, FsError> {
            Ok(vec![])
        }
    }

    #[test]
    fn blanket_fs_impl() {
        fn _accepts_fs<T: Fs>(_: &T) {}
        let fs = DummyFs;
        _accepts_fs(&fs);
    }
}
