// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! InMemoryFs — in-memory filesystem for tests.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

use nika_kernel::filesystem::{FileMetadata, FsRead, FsWrite};

/// In-memory filesystem backed by a HashMap.
///
/// Thread-safe via RwLock. All paths are stored canonically (no symlinks).
#[derive(Clone, Default)]
pub struct InMemoryFs {
    files: Arc<RwLock<HashMap<PathBuf, Vec<u8>>>>,
}

impl InMemoryFs {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a file into the filesystem.
    pub fn seed(&self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) {
        self.files.write().insert(path.into(), content.into());
    }

    /// Number of files stored.
    pub fn len(&self) -> usize {
        self.files.read().len()
    }

    /// Whether the filesystem is empty.
    pub fn is_empty(&self) -> bool {
        self.files.read().is_empty()
    }
}

#[async_trait::async_trait]
impl FsRead for InMemoryFs {
    async fn read(&self, path: &Path) -> std::io::Result<Bytes> {
        self.files
            .read()
            .get(path)
            .map(|v| Bytes::from(v.clone()))
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, path.display().to_string()))
    }

    async fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        let bytes = self.read(path).await?;
        String::from_utf8(bytes.to_vec())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        let files = self.files.read();
        if let Some(content) = files.get(path) {
            Ok(FileMetadata {
                len: content.len() as u64,
                is_file: true,
                is_dir: false,
            })
        } else {
            // Check if any file has this as a prefix (directory)
            let is_dir = files.keys().any(|k| k.starts_with(path) && k != path);
            if is_dir {
                Ok(FileMetadata {
                    len: 0,
                    is_file: false,
                    is_dir: true,
                })
            } else {
                Err(std::io::Error::new(std::io::ErrorKind::NotFound, path.display().to_string()))
            }
        }
    }

    async fn exists(&self, path: &Path) -> bool {
        let files = self.files.read();
        files.contains_key(path) || files.keys().any(|k| k.starts_with(path) && k != path)
    }

    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>> {
        // Simple glob: support `*.ext` and `**/*.ext` patterns.
        // For full glob semantics, use the `globset` crate (in workspace).
        let suffix = pattern
            .trim_start_matches("**/")
            .trim_start_matches("*/")
            .trim_start_matches('*');
        let files = self.files.read();
        let mut matches: Vec<PathBuf> = files
            .keys()
            .filter(|p| {
                p.starts_with(root)
                    && (suffix.is_empty() || p.to_string_lossy().ends_with(suffix))
            })
            .cloned()
            .collect();
        matches.sort();
        Ok(matches)
    }

    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        // In-memory: no symlinks, just return the path as-is.
        Ok(path.to_path_buf())
    }
}

#[async_trait::async_trait]
impl FsWrite for InMemoryFs {
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()> {
        self.files.write().insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    async fn create_dir_all(&self, _path: &Path) -> std::io::Result<()> {
        // In-memory: directories are implicit from file paths.
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.files
            .write()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, path.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_write_roundtrip() {
        let fs = InMemoryFs::new();
        let path = Path::new("/tmp/test.txt");
        fs.write(path, b"hello").await.unwrap();
        let content = fs.read_to_string(path).await.unwrap();
        assert_eq!(content, "hello");
    }

    #[tokio::test]
    async fn read_missing_file_errors() {
        let fs = InMemoryFs::new();
        let err = fs.read(Path::new("/missing")).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }

    #[tokio::test]
    async fn seed_then_read() {
        let fs = InMemoryFs::new();
        fs.seed("/data/config.json", r#"{"key": "value"}"#);
        let content = fs.read_to_string(Path::new("/data/config.json")).await.unwrap();
        assert!(content.contains("key"));
    }

    #[tokio::test]
    async fn metadata_file() {
        let fs = InMemoryFs::new();
        fs.seed("/file.txt", "abc");
        let meta = fs.metadata(Path::new("/file.txt")).await.unwrap();
        assert!(meta.is_file);
        assert_eq!(meta.len, 3);
    }

    #[tokio::test]
    async fn exists_and_remove() {
        let fs = InMemoryFs::new();
        let path = Path::new("/tmp/ephemeral");
        assert!(!fs.exists(path).await);
        fs.write(path, b"data").await.unwrap();
        assert!(fs.exists(path).await);
        fs.remove_file(path).await.unwrap();
        assert!(!fs.exists(path).await);
    }
}
