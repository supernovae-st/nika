// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Filesystem trait — async FS abstraction replacing 30+ raw std::fs/tokio::fs sites.
//!
//! Wire points: artifact_processor, context_loader, skill_injector, vault,
//! daemon lifecycle, course, init, showcase, media store, trace writer.

use std::path::{Path, PathBuf};

use bytes::Bytes;

/// Metadata about a filesystem entry.
#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub len: u64,
    pub is_file: bool,
    pub is_dir: bool,
}

/// Async filesystem operations.
///
/// Production: `TokioFs` via `tokio::fs`.
/// Tests: `InMemoryFs` backed by `HashMap<PathBuf, Vec<u8>>`.
#[async_trait::async_trait]
pub trait Filesystem: Send + Sync {
    /// Read a file's entire contents as bytes.
    async fn read(&self, path: &Path) -> std::io::Result<Bytes>;

    /// Read a file's entire contents as a UTF-8 string.
    async fn read_to_string(&self, path: &Path) -> std::io::Result<String>;

    /// Write bytes to a file, creating it if it doesn't exist.
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()>;

    /// Get metadata for a path.
    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata>;

    /// Create a directory and all parent directories.
    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()>;

    /// Remove a file.
    async fn remove_file(&self, path: &Path) -> std::io::Result<()>;

    /// Check whether a path exists.
    async fn exists(&self, path: &Path) -> bool;

    /// List files matching a glob pattern relative to a root directory.
    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>>;

    /// Canonicalize a path (resolve symlinks and relative components).
    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf>;
}
