// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem traits — ISP decomposition of async FS operations.
//!
//! 4 atomic traits: `FsRead`, `FsWrite`, `FsMeta`, `FsList`.
//! 1 super-trait: `Fs` (blanket for all 4).

use std::path::{Path, PathBuf};

use bytes::Bytes;

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

/// Read-only filesystem operations.
///
/// CANCEL SAFETY: every read method is cancel-safe. Read ops have no
/// side effects, so dropping the future simply abandons the syscall.
#[trait_variant::make(FsReadDyn: Send)]
pub trait FsRead: Send + Sync {
    /// Read a file's entire contents as bytes.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read(&self, path: &Path) -> std::io::Result<Bytes>;

    /// Read a file's entire contents as a UTF-8 string.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// Check whether a path exists.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn exists(&self, path: &Path) -> bool;

    /// Canonicalize a path (resolve symlinks and relative components).
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}

/// Write filesystem operations.
///
/// CANCEL SAFETY: NOT cancel-safe at the raw-syscall level. Impls
/// targeting atomic semantics MUST use temp-file + rename (POSIX
/// guarantees rename atomicity within the same filesystem). A
/// dropped future may leave partial writes OR a stale temp file.
/// Callers on non-atomic impls MUST retry cautiously.
#[trait_variant::make(FsWriteDyn: Send)]
pub trait FsWrite: Send + Sync {
    /// Write contents to a file (creates or overwrites).
    ///
    /// CANCEL SAFETY: NOT cancel-safe unless impl uses atomic
    /// temp-file + rename. Partial writes possible.
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()>;

    /// Create a directory and all parent directories.
    ///
    /// CANCEL SAFETY: partial cancel-safe — each mkdir is atomic, but
    /// a drop mid-chain may leave a partial prefix. Idempotent retry OK.
    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;

    /// Remove a file.
    ///
    /// CANCEL SAFETY: cancel-safe — single unlink syscall.
    async fn remove_file(&self, path: &Path) -> std::io::Result<()>;
}

/// Filesystem metadata operations.
///
/// CANCEL SAFETY: cancel-safe (read-only stat syscall).
#[trait_variant::make(FsMetaDyn: Send)]
pub trait FsMeta: Send + Sync {
    /// Get metadata for a path.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata>;
}

/// Filesystem listing operations.
///
/// CANCEL SAFETY: cancel-safe (read-only — opendir/readdir/closedir).
#[trait_variant::make(FsListDyn: Send)]
pub trait FsList: Send + Sync {
    /// List entries in a directory.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only).
    async fn list_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>>;

    /// Find files matching a glob pattern relative to a root directory.
    ///
    /// CANCEL SAFETY: cancel-safe (read-only directory walk).
    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>>;
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
        async fn read(&self, _: &Path) -> std::io::Result<Bytes> {
            Ok(Bytes::new())
        }
        async fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }
        async fn exists(&self, _: &Path) -> bool {
            false
        }
        async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
            Ok(path.to_path_buf())
        }
    }

    impl FsWrite for DummyFs {
        async fn write(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
            Ok(())
        }
        async fn create_dir_all(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }
        async fn remove_file(&self, _: &Path) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl FsMeta for DummyFs {
        async fn metadata(&self, _: &Path) -> std::io::Result<FileMetadata> {
            Ok(FileMetadata::new(0, false, false))
        }
    }

    impl FsList for DummyFs {
        async fn list_dir(&self, _: &Path) -> std::io::Result<Vec<PathBuf>> {
            Ok(vec![])
        }
        async fn glob(&self, _: &Path, _: &str) -> std::io::Result<Vec<PathBuf>> {
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
