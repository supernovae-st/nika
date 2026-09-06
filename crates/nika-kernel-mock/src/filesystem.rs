// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `MockFs` — in-memory filesystem backed by `BTreeMap`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bytes::Bytes;
use parking_lot::RwLock;

use nika_kernel::fs::{FileMetadata, FsError, FsListDyn, FsMetaDyn, FsReadDyn, FsWriteDyn};

/// In-memory filesystem for tests.
///
/// Files stored as `BTreeMap<PathBuf, Vec<u8>>`.
/// Directories are implicit (any prefix of a stored path).
/// Clones share state via `Arc`.
#[derive(Clone, Default)]
#[non_exhaustive]
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

impl FsReadDyn for MockFs {
    async fn read(&self, path: &Path) -> Result<Bytes, FsError> {
        let guard = self.files.read();
        guard
            .get(path)
            .map(|v| Bytes::copy_from_slice(v))
            .ok_or_else(|| FsError::NotFound {
                path: path.display().to_string(),
            })
    }

    async fn read_to_string(&self, path: &Path) -> Result<String, FsError> {
        let guard = self.files.read();
        let data = guard.get(path).ok_or_else(|| FsError::NotFound {
            path: path.display().to_string(),
        })?;
        String::from_utf8(data.clone()).map_err(|e| FsError::InvalidData {
            path: path.display().to_string(),
            reason: e.to_string(),
        })
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

    async fn canonicalize(&self, path: &Path) -> Result<PathBuf, FsError> {
        Ok(path.to_path_buf())
    }
}

impl FsWriteDyn for MockFs {
    async fn write_new(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        let mut files = self.files.write();
        let prefix = format!("{}/", path.display());
        if files
            .keys()
            .any(|child| child.to_string_lossy().starts_with(&prefix))
        {
            return Err(FsError::AlreadyExists {
                path: path.display().to_string(),
            });
        }
        match files.entry(path.to_path_buf()) {
            std::collections::btree_map::Entry::Occupied(_) => Err(FsError::AlreadyExists {
                path: path.display().to_string(),
            }),
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(contents.to_vec());
                Ok(())
            }
        }
    }

    async fn write(&self, path: &Path, contents: &[u8]) -> Result<(), FsError> {
        self.files
            .write()
            .insert(path.to_path_buf(), contents.to_vec());
        Ok(())
    }

    async fn create_dir_all(&self, _path: &Path) -> Result<(), FsError> {
        // Directories are implicit in the in-memory store.
        Ok(())
    }

    async fn remove_file(&self, path: &Path) -> Result<(), FsError> {
        self.files
            .write()
            .remove(path)
            .map(|_| ())
            .ok_or_else(|| FsError::NotFound {
                path: path.display().to_string(),
            })
    }
}

impl FsMetaDyn for MockFs {
    async fn metadata(&self, path: &Path) -> Result<FileMetadata, FsError> {
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
        Err(FsError::NotFound {
            path: path.display().to_string(),
        })
    }
}

/// Match a glob `pattern` against a `/`-separated `path` that has already been
/// made relative to the glob root — **pure**. Standard subset (gitignore/POSIX):
/// - `?` matches exactly one non-`/` character,
/// - `*` matches any run (incl. empty) of non-`/` chars WITHIN one segment,
/// - `**` matches any run of whole path segments (incl. zero).
///
/// So `*.rs` matches `foo.rs` but NOT `a/foo.rs`, while `**/*.rs` matches both.
/// This is the contract the real `nika-fs` glob must satisfy. The mock used to
/// do a bare `contains(pattern)` substring test, which silently returned the
/// wrong set (`"*.rs"` is never a substring of `/x/foo.rs`, so it matched
/// nothing) and gave consumers false confidence.
fn glob_match(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let txt: Vec<&str> = path.split('/').collect();
    match_segments(&pat, &txt)
}

/// Match glob pattern-segments against path-segments. `**` consumes zero or more
/// whole segments (with backtracking). **Pure**.
fn match_segments(pat: &[&str], txt: &[&str]) -> bool {
    match pat.split_first() {
        None => txt.is_empty(),
        Some((&"**", rest)) => (0..=txt.len()).any(|skip| match_segments(rest, &txt[skip..])),
        Some((&seg, rest)) => {
            !txt.is_empty()
                && segment_match(seg.as_bytes(), txt[0].as_bytes())
                && match_segments(rest, &txt[1..])
        }
    }
}

/// Match ONE path segment against one pattern segment (`*`/`?`, no `/`) with `*`
/// backtracking. **Pure**.
fn segment_match(pat: &[u8], txt: &[u8]) -> bool {
    match pat.split_first() {
        None => txt.is_empty(),
        // `*` matches zero chars (advance pattern) OR one char (advance text).
        Some((&b'*', rest)) => {
            segment_match(rest, txt) || (!txt.is_empty() && segment_match(pat, &txt[1..]))
        }
        Some((&b'?', rest)) => !txt.is_empty() && segment_match(rest, &txt[1..]),
        Some((&c, rest)) => txt.first() == Some(&c) && segment_match(rest, &txt[1..]),
    }
}

impl FsListDyn for MockFs {
    async fn list_dir(&self, path: &Path) -> Result<Vec<PathBuf>, FsError> {
        let guard = self.files.read();
        let prefix = format!("{}/", path.display());
        let entries: Vec<PathBuf> = guard
            .keys()
            .filter(|k| k.to_string_lossy().starts_with(&prefix))
            .cloned()
            .collect();
        Ok(entries)
    }

    async fn glob(&self, root: &Path, pattern: &str) -> Result<Vec<PathBuf>, FsError> {
        let guard = self.files.read();
        let root_prefix = format!("{}/", root.display());
        let entries: Vec<PathBuf> = guard
            .keys()
            .filter(|k| {
                // Glob-match the path RELATIVE to `root` (the pattern is relative).
                k.to_string_lossy()
                    .strip_prefix(&root_prefix)
                    .is_some_and(|rel| glob_match(pattern, rel))
            })
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
        // `*.rs` matches the two top-level .rs files, not the .md.
        let matches = fs.glob(Path::new("/root"), "*.rs").await.unwrap();
        assert_eq!(matches.len(), 2);
    }

    #[tokio::test]
    async fn glob_star_is_single_segment_double_star_is_recursive() {
        let fs = MockFs::new()
            .with_file("/root/top.rs", "")
            .with_file("/root/a/mid.rs", "")
            .with_file("/root/a/b/deep.rs", "")
            .with_file("/root/a/note.md", "");
        // `*.rs` is ONE segment — only the top-level file.
        let top = fs.glob(Path::new("/root"), "*.rs").await.unwrap();
        assert_eq!(top.len(), 1, "*.rs must NOT descend into subdirs");
        // `**/*.rs` is recursive — all three .rs files, no .md.
        let all = fs.glob(Path::new("/root"), "**/*.rs").await.unwrap();
        assert_eq!(all.len(), 3, "**/*.rs must match every nested .rs");
        // A nested-dir pattern.
        let mid = fs.glob(Path::new("/root"), "a/*.rs").await.unwrap();
        assert_eq!(mid.len(), 1, "a/*.rs matches only /root/a/mid.rs");
    }

    #[tokio::test]
    async fn glob_substring_trap_is_gone() {
        // The old `contains()` impl would have returned this file for the
        // substring `oo` and for the literal `foo` regardless of extension.
        // Real glob: a bare literal must match the WHOLE segment, and `*` is
        // required for partial matches.
        let fs = MockFs::new().with_file("/root/foo.rs", "");
        // bare substring no longer matches (would under the old bug).
        assert!(fs.glob(Path::new("/root"), "oo").await.unwrap().is_empty());
        assert!(fs.glob(Path::new("/root"), "foo").await.unwrap().is_empty());
        // proper patterns do match.
        assert_eq!(
            fs.glob(Path::new("/root"), "foo.rs").await.unwrap().len(),
            1
        );
        assert_eq!(fs.glob(Path::new("/root"), "f*.rs").await.unwrap().len(), 1);
        assert_eq!(
            fs.glob(Path::new("/root"), "fo?.rs").await.unwrap().len(),
            1
        );
    }

    #[test]
    fn glob_match_unit_cases() {
        // segment-scoped `*`
        assert!(glob_match("*.rs", "foo.rs"));
        assert!(!glob_match("*.rs", "a/foo.rs"));
        // recursive `**`
        assert!(glob_match("**/*.rs", "foo.rs"));
        assert!(glob_match("**/*.rs", "a/b/foo.rs"));
        assert!(!glob_match("**/*.rs", "a/foo.md"));
        // `?` single char
        assert!(glob_match("fo?.rs", "foo.rs"));
        assert!(!glob_match("fo?.rs", "fooo.rs"));
        // literal whole-segment (no substring)
        assert!(glob_match("foo.rs", "foo.rs"));
        assert!(!glob_match("foo", "foo.rs"));
        // `**` matches zero segments
        assert!(glob_match("**/foo.rs", "foo.rs"));
    }

    /// Verify blanket Fs trait is satisfied.
    #[test]
    fn mock_fs_satisfies_fs_trait() {
        fn _accepts_fs<T: FsReadDyn + FsWriteDyn + FsMetaDyn + FsListDyn>(_: &T) {}
        let fs = MockFs::new();
        _accepts_fs(&fs);
    }
}
