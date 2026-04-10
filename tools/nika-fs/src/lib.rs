// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Production [`FsRead`] + [`FsWrite`] implementation via `tokio::fs`.
//!
//! This crate sits at L1 in the dependency graph — it implements the
//! [`nika_kernel::filesystem::FsRead`] and [`nika_kernel::filesystem::FsWrite`]
//! splinter traits (and thus the umbrella [`nika_kernel::filesystem::Filesystem`])
//! using real filesystem I/O.
//!
//! # Example
//!
//! ```rust,no_run
//! use nika_fs::TokioFs;
//! use nika_kernel::filesystem::FsRead;
//! use std::path::Path;
//!
//! # async fn example() -> std::io::Result<()> {
//! let fs = TokioFs;
//! let content = fs.read_to_string(Path::new("README.md")).await?;
//! println!("{}", content);
//! # Ok(())
//! # }
//! ```

use std::path::{Path, PathBuf};

use bytes::Bytes;
use nika_kernel::filesystem::{FileMetadata, FsRead, FsWrite};

/// Production filesystem backed by `tokio::fs`.
///
/// Zero-size type — delegates directly to `tokio::fs` functions.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioFs;

#[async_trait::async_trait]
impl FsRead for TokioFs {
    async fn read(&self, path: &Path) -> std::io::Result<Bytes> {
        let data = tokio::fs::read(path).await?;
        Ok(Bytes::from(data))
    }

    async fn read_to_string(&self, path: &Path) -> std::io::Result<String> {
        tokio::fs::read_to_string(path).await
    }

    async fn metadata(&self, path: &Path) -> std::io::Result<FileMetadata> {
        let meta = tokio::fs::metadata(path).await?;
        Ok(FileMetadata {
            len: meta.len(),
            is_file: meta.is_file(),
            is_dir: meta.is_dir(),
        })
    }

    async fn exists(&self, path: &Path) -> bool {
        tokio::fs::try_exists(path).await.unwrap_or(false)
    }

    async fn glob(&self, root: &Path, pattern: &str) -> std::io::Result<Vec<PathBuf>> {
        let matcher = globset::GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e))?
            .compile_matcher();

        let mut results = Vec::new();
        collect_glob_matches(root, root, &matcher, &mut results).await?;
        results.sort();
        Ok(results)
    }

    async fn canonicalize(&self, path: &Path) -> std::io::Result<PathBuf> {
        tokio::fs::canonicalize(path).await
    }
}

#[async_trait::async_trait]
impl FsWrite for TokioFs {
    async fn write(&self, path: &Path, contents: &[u8]) -> std::io::Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                tokio::fs::create_dir_all(parent).await?;
            }
        }
        tokio::fs::write(path, contents).await
    }

    async fn create_dir_all(&self, path: &Path) -> std::io::Result<()> {
        tokio::fs::create_dir_all(path).await
    }

    async fn remove_file(&self, path: &Path) -> std::io::Result<()> {
        tokio::fs::remove_file(path).await
    }
}

/// Recursively walk a directory and collect files matching a glob pattern.
async fn collect_glob_matches(
    root: &Path,
    dir: &Path,
    matcher: &globset::GlobMatcher,
    results: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    let mut entries = tokio::fs::read_dir(dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let relative = path.strip_prefix(root).unwrap_or(&path);

        if entry.file_type().await?.is_dir() {
            // Recurse into subdirectories (skip hidden dirs)
            if !entry
                .file_name()
                .to_str()
                .is_some_and(|n| n.starts_with('.'))
            {
                Box::pin(collect_glob_matches(root, &path, matcher, results)).await?;
            }
        } else if matcher.is_match(relative) {
            results.push(path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_and_write_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.txt");
        let fs = TokioFs;

        fs.write(&path, b"hello world").await.unwrap();
        let content = fs.read_to_string(&path).await.unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn read_bytes_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("binary.bin");
        let fs = TokioFs;

        let data = vec![0u8, 1, 2, 3, 255];
        fs.write(&path, &data).await.unwrap();
        let read_back = fs.read(&path).await.unwrap();
        assert_eq!(&read_back[..], &data[..]);
    }

    #[tokio::test]
    async fn write_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a").join("b").join("c").join("file.txt");
        let fs = TokioFs;

        fs.write(&path, b"nested").await.unwrap();
        assert!(fs.exists(&path).await);
    }

    #[tokio::test]
    async fn metadata_for_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("meta.txt");
        let fs = TokioFs;

        fs.write(&path, b"12345").await.unwrap();
        let meta = fs.metadata(&path).await.unwrap();
        assert_eq!(meta.len, 5);
        assert!(meta.is_file);
        assert!(!meta.is_dir);
    }

    #[tokio::test]
    async fn metadata_for_dir() {
        let dir = tempfile::tempdir().unwrap();
        let fs = TokioFs;

        let meta = fs.metadata(dir.path()).await.unwrap();
        assert!(meta.is_dir);
        assert!(!meta.is_file);
    }

    #[tokio::test]
    async fn create_dir_all_nested() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("x").join("y").join("z");
        let fs = TokioFs;

        fs.create_dir_all(&nested).await.unwrap();
        assert!(fs.exists(&nested).await);
    }

    #[tokio::test]
    async fn remove_file_works() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("remove_me.txt");
        let fs = TokioFs;

        fs.write(&path, b"doomed").await.unwrap();
        assert!(fs.exists(&path).await);
        fs.remove_file(&path).await.unwrap();
        assert!(!fs.exists(&path).await);
    }

    #[tokio::test]
    async fn exists_returns_false_for_missing() {
        let fs = TokioFs;
        assert!(!fs.exists(Path::new("/nonexistent/path/abc123")).await);
    }

    #[tokio::test]
    async fn glob_finds_matching_files() {
        let dir = tempfile::tempdir().unwrap();
        let fs = TokioFs;

        fs.write(&dir.path().join("a.yaml"), b"").await.unwrap();
        fs.write(&dir.path().join("b.yaml"), b"").await.unwrap();
        fs.write(&dir.path().join("c.txt"), b"").await.unwrap();

        let matches = fs.glob(dir.path(), "*.yaml").await.unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.iter().all(|p| p.extension().unwrap() == "yaml"));
    }

    #[tokio::test]
    async fn glob_recurses_subdirs() {
        let dir = tempfile::tempdir().unwrap();
        let fs = TokioFs;

        fs.write(&dir.path().join("sub").join("deep.nika.yaml"), b"")
            .await
            .unwrap();
        fs.write(&dir.path().join("top.nika.yaml"), b"")
            .await
            .unwrap();

        let matches = fs.glob(dir.path(), "**/*.nika.yaml").await.unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn canonicalize_resolves_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("canon.txt");
        let fs = TokioFs;

        fs.write(&path, b"").await.unwrap();
        let canonical = fs.canonicalize(&path).await.unwrap();
        assert!(canonical.is_absolute());
    }

    #[test]
    fn is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<TokioFs>();
    }

    #[test]
    fn is_zero_sized() {
        assert_eq!(std::mem::size_of::<TokioFs>(), 0);
    }
}
