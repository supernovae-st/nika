// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockFs` — in-memory filesystem backed by `BTreeMap`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

use nika_kernel::fs::{FileMetadata, FsList, FsMeta, FsRead, FsWrite};

/// In-memory filesystem for tests.
///
/// Files stored as `BTreeMap<PathBuf, Vec<u8>>`.
/// Directories are implicit (any prefix of a stored path).
/// Clones share state via `Arc`.
#[derive(Clone, Default)]
pub struct MockFs {
    files: Arc<RwLock<BTreeMap<PathBuf, Vec<u8>>>>,
}

impl MockFs {
    /// Create an empty in-memory filesystem.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Seed a file (builder pattern).
    #[must_use]
    pub fn with_file(self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) -> Self {
        self.files.write().insert(path.into(), content.into());
        self
    }

    /// Seed a file (mutable reference).
    pub fn seed(&self, path: impl Into<PathBuf>, content: impl Into<Vec<u8>>) {
        self.files.write().insert(path.into(), content.into());
    }

    /// List all stored file paths.
    #[must_use]
    pub fn file_paths(&self) -> Vec<PathBuf> {
        self.files.read().keys().cloned().collect()
    }

    /// Number of stored files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.read().len()
    }

    /// Whether the filesystem is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.read().is_empty()
    }
}

impl FsRead for MockFs {
    async fn read(&self, path: &Path) -> std::io::Result<Bytes> {
        let guard = self.files.read();
        guard
            .get(path)
            .map(|v| Bytes::copy_from_slice(v))
            .ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
            })
    }

    async fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        let guard = self.files.read();
        let data = guard.get(path).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
        })?;
        String::from_utf8(data.clone())
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
    }

    async fn exists(&self, path: &Path) -> bool {
        let guard = self.files.read();
        if guard.contains_key(path) {
            return true;
        }
        // Check if path is an implicit directory.
        let prefix = format!("{}/", path.display());
        guard
            .keys()
            .any(|k| k.to_string_lossy().starts_with(&prefix))
    }

    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        Ok(path.to_path_buf())
    }
}

impl FsWrite for MockFs {
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()> {
        self.files
            .write()
            .insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    async fn create_dir_all(&self, _path: &Path) -> std::io::Result<()> {
        // Directories are implicit in the in-memory store.
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        self.files.write().remove(path).map(|_| ()).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
        })
    }
}

impl FsMeta for MockFs {
    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        let guard = self.files.read();
        if let Some(data) = guard.get(path) {
            return Ok(FileMetadata::new(data.len() as u64, true, false));
        }
        // Check implicit directory.
        let prefix = format!("{}/", path.display());
        if guard
            .keys()
            .any(|k| k.to_string_lossy().starts_with(&prefix))
        {
            return Ok(FileMetadata::new(0, false, true));
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{}", path.display()),
        ))
    }
}

impl FsList for MockFs {
    async fn list_dir(&self, path: &Path) -> std::io::Result<Vec<PathBuf>> {
        let guard = self.files.read();
        let prefix = format!("{}/", path.display());
        let entries: Vec<PathBuf> = guard
            .keys()
            .filter(|k| k.to_string_lossy().starts_with(&prefix))
            .cloned()
            .collect();
        Ok(entries)
    }

    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>> {
        let guard = self.files.read();
        let entries: Vec<PathBuf> = guard
            .keys()
            .filter(|k| k.starts_with(root) && k.to_string_lossy().contains(pattern))
            .cloned()
            .collect();
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_write_roundtrip() {
        let fs = MockFs::new();
        let path = Path::new("/tmp/test.txt");
        fs.write(path, b"hello").await.unwrap();
        let data = fs.read(path).await.unwrap();
        assert_eq!(data.as_ref(), b"hello");
    }

    #[tokio::test]
    async fn read_missing_file_returns_not_found() {
        let fs = MockFs::new();
        let result = fs.read(Path::new("/missing")).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn read_to_string_works() {
        let fs = MockFs::new().with_file("/test.txt", "content");
        let s = fs.read_to_string(Path::new("/test.txt")).await.unwrap();
        assert_eq!(s, "content");
    }

    #[tokio::test]
    async fn exists_true_for_file() {
        let fs = MockFs::new().with_file("/a.txt", "data");
        assert!(fs.exists(Path::new("/a.txt")).await);
    }

    #[tokio::test]
    async fn exists_false_for_missing() {
        let fs = MockFs::new();
        assert!(!fs.exists(Path::new("/nope")).await);
    }

    #[tokio::test]
    async fn metadata_for_file() {
        let fs = MockFs::new().with_file("/x.txt", "12345");
        let meta = fs.metadata(Path::new("/x.txt")).await.unwrap();
        assert_eq!(meta.len, 5);
        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[tokio::test]
    async fn remove_file_works() {
        let fs = MockFs::new().with_file("/del.txt", "data");
        fs.remove_file(Path::new("/del.txt")).await.unwrap();
        assert!(!fs.exists(Path::new("/del.txt")).await);
    }

    #[tokio::test]
    async fn remove_missing_file_errors() {
        let fs = MockFs::new();
        let result = fs.remove_file(Path::new("/missing")).await;
        assert!(result.is_err());
    }

    #[test]
    fn seed_and_len() {
        let fs = MockFs::new();
        fs.seed("/a", "1");
        fs.seed("/b", "2");
        assert_eq!(fs.len(), 2);
        assert!(!fs.is_empty());
    }

    #[test]
    fn with_file_builder() {
        let fs = MockFs::new().with_file("/a", "1").with_file("/b", "2");
        assert_eq!(fs.len(), 2);
    }

    #[test]
    fn clone_shares_state() {
        let fs1 = MockFs::new();
        let fs2 = fs1.clone();
        fs1.seed("/x", "data");
        assert_eq!(fs2.len(), 1);
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn mock_fs_is_send_sync() {
        _assert_send_sync::<MockFs>();
    }

    #[test]
    fn file_paths_returns_correct_paths() {
        let fs = MockFs::new()
            .with_file("/a.txt", "a")
            .with_file("/b.txt", "b");
        let paths = fs.file_paths();
        assert!(paths.contains(&PathBuf::from("/a.txt")));
        assert!(paths.contains(&PathBuf::from("/b.txt")));
    }

    #[test]
    fn is_empty_returns_false_when_files_exist() {
        let fs = MockFs::new().with_file("/x.txt", "data");
        assert!(!fs.is_empty());
    }

    #[tokio::test]
    async fn canonicalize_returns_the_path() {
        let fs = MockFs::new();
        let path = Path::new("/some/path.txt");
        let result = fs.canonicalize(path).await.unwrap();
        assert_eq!(result, path.to_path_buf());
    }

    #[tokio::test]
    async fn list_dir_returns_entries_under_directory() {
        let fs = MockFs::new()
            .with_file("/dir/a.txt", "a")
            .with_file("/dir/b.txt", "b");
        let entries = fs.list_dir(Path::new("/dir")).await.unwrap();
        assert_eq!(entries.len(), 2);
    }

    #[tokio::test]
    async fn glob_returns_matching_files() {
        let fs = MockFs::new()
            .with_file("/root/foo.rs", "fn main() {}")
            .with_file("/root/bar.rs", "fn bar() {}")
            .with_file("/root/readme.md", "# Hello");
        let matches = fs.glob(Path::new("/root"), ".rs").await.unwrap();
        assert!(!matches.is_empty());
        assert_eq!(matches.len(), 2);
    }

    /// Verify blanket Fs trait is satisfied.
    #[test]
    fn mock_fs_satisfies_fs_trait() {
        fn _accepts_fs<T: nika_kernel::Fs>(_: &T) {}
        let fs = MockFs::new();
        _accepts_fs(&fs);
    }
}
