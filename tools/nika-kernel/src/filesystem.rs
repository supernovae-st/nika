// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem trait — async FS abstraction replacing 30+ raw std::fs/tokio::fs sites.
//!
//! Wire points: artifact_processor, context_loader, skill_injector, vault,
//! daemon lifecycle, course, init, showcase, media store, trace writer.
//!
//! The trait is split into two splinter traits so verb crates can depend
//! only on the capabilities they actually use:
//!
//! - [`FsRead`] — read-only operations (context loaders, skill injectors)
//! - [`FsWrite`] — write-only operations (artifact writer, trace writer)
//!
//! The [`Filesystem`] umbrella trait is a blanket alias: any type that
//! implements both splinters automatically satisfies it.

use std::path::{Path, PathBuf};

use bytes::Bytes;

/// Metadata about a filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMetadata {
    pub len: u64,
    pub is_file: bool,
    pub is_dir: bool,
}

/// Read-only filesystem operations.
///
/// Verbs that only consume files (context loading, skill injection)
/// should depend on this splinter instead of the full [`Filesystem`].
#[async_trait::async_trait]
pub trait FsRead: Send + Sync {
    /// Read a file's entire contents as bytes.
    async fn read(&self, path: &Path) -> std::io::Result<Bytes>;

    /// Read a file's entire contents as a UTF-8 string.
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// Get metadata for a path.
    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata>;

    /// Check whether a path exists.
    async fn exists(&self, path: &Path) -> bool;

    /// List files matching a glob pattern relative to a root directory.
    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>>;

    /// Canonicalize a path (resolve symlinks and relative components).
    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}

/// Write-only filesystem operations.
///
/// Verbs that only emit files (artifact writer, trace writer) should
/// depend on this splinter instead of the full [`Filesystem`].
#[async_trait::async_trait]
pub trait FsWrite: Send + Sync {
    /// Write bytes to a file, creating it if it doesn't exist.
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()>;

    /// Create a directory and all parent directories.
    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;

    /// Remove a file.
    async fn remove_file(&self, path: &Path) -> std::io::Result<()>;
}

/// Convenience umbrella trait — any type implementing both splinters satisfies this.
///
/// Production: `TokioFs` via `tokio::fs`.
/// Tests: `InMemoryFs` backed by `HashMap<PathBuf, Vec<u8>>`.
pub trait Filesystem: FsRead + FsWrite {}
impl<T: FsRead + FsWrite + ?Sized> Filesystem for T {}

#[cfg(test)]
mod narrowing_tests {
    use super::*;

    fn take_read_only<R: FsRead + ?Sized>(_r: &R) {}
    fn take_both<F: Filesystem + ?Sized>(_f: &F) {}

    struct OnlyRead;
    #[async_trait::async_trait]
    impl FsRead for OnlyRead {
        async fn read(&self, _: &Path) -> std::io::Result<Bytes> {
            Ok(Bytes::new())
        }
        async fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }
        async fn metadata(&self, _: &Path) -> std::io::Result<FileMetadata> {
            Ok(FileMetadata {
                len: 0,
                is_file: true,
                is_dir: false,
            })
        }
        async fn exists(&self, _: &Path) -> bool {
            false
        }
        async fn glob(&self, _: &Path, _: &str) -> std::io::Result<Vec<PathBuf>> {
            Ok(vec![])
        }
        async fn canonicalize(&self, _: &Path) -> std::io::Result<PathBuf> {
            Ok(PathBuf::new())
        }
    }

    struct ReadWrite;
    #[async_trait::async_trait]
    impl FsRead for ReadWrite {
        async fn read(&self, _: &Path) -> std::io::Result<Bytes> {
            Ok(Bytes::new())
        }
        async fn read_to_string(&self, _: &Path) -> std::io::Result<String> {
            Ok(String::new())
        }
        async fn metadata(&self, _: &Path) -> std::io::Result<FileMetadata> {
            Ok(FileMetadata {
                len: 0,
                is_file: true,
                is_dir: false,
            })
        }
        async fn exists(&self, _: &Path) -> bool {
            false
        }
        async fn glob(&self, _: &Path, _: &str) -> std::io::Result<Vec<PathBuf>> {
            Ok(vec![])
        }
        async fn canonicalize(&self, _: &Path) -> std::io::Result<PathBuf> {
            Ok(PathBuf::new())
        }
    }
    #[async_trait::async_trait]
    impl FsWrite for ReadWrite {
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

    #[test]
    fn only_read_compiles_as_fs_read() {
        take_read_only(&OnlyRead);
    }

    #[test]
    fn read_write_satisfies_filesystem_umbrella() {
        take_read_only(&ReadWrite);
        take_both(&ReadWrite);
    }
}
