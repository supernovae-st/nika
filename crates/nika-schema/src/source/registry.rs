// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source file registry — intern files and convert byte offsets to line:col.

use std::path::PathBuf;
use std::sync::Arc;

use super::span::{ByteOffset, FileId, LineCol};

/// A registered source file.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SourceFile {
    /// The interned file identifier.
    pub id: FileId,
    /// Path of the source file (may be synthetic, e.g., `<stdin>`).
    pub path: PathBuf,
    /// Full content of the file.
    pub content: Arc<String>,
    /// Byte offsets where each line starts (for fast line:col lookup).
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Compute line start offsets from content.
    fn compute_line_starts(content: &str) -> Vec<u32> {
        let mut starts = vec![0];
        for (i, byte) in content.bytes().enumerate() {
            if byte == b'\n' {
                // Next line starts at i+1 (may be past end — that's OK).
                // u32 is sufficient: max workflow file < 4 GiB.
                #[allow(clippy::cast_possible_truncation)]
                starts.push((i + 1) as u32);
            }
        }
        starts
    }
}

/// Registry of source files with fast offset-to-line:col conversion.
///
/// Files are interned: each `add()` returns a unique [`FileId`].
/// The registry is the single owner of source content, shared via
/// `Arc<String>` for zero-copy references.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SourceRegistry {
    files: Vec<SourceFile>,
}

impl SourceRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    /// Intern a source file and return its ID.
    pub fn add(&mut self, path: impl Into<PathBuf>, content: String) -> FileId {
        // u32 is sufficient: we'll never intern > 4 billion files.
        #[allow(clippy::cast_possible_truncation)]
        let id = FileId(self.files.len() as u32);
        let line_starts = SourceFile::compute_line_starts(&content);
        self.files.push(SourceFile {
            id,
            path: path.into(),
            content: Arc::new(content),
            line_starts,
        });
        id
    }

    /// Look up a source file by ID.
    #[must_use]
    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        self.files.get(id.0 as usize)
    }

    /// Convert a byte offset to a 1-based line:col position.
    ///
    /// Returns `None` if the file ID is unknown or the offset is past the end.
    #[must_use]
    pub fn line_col(&self, file: FileId, offset: ByteOffset) -> Option<LineCol> {
        let f = self.get(file)?;
        if offset.as_usize() > f.content.len() {
            return None;
        }

        // Binary search for the line containing this offset.
        let line_idx = match f.line_starts.binary_search(&offset.0) {
            Ok(exact) => exact,
            Err(insert) => insert.saturating_sub(1),
        };

        let line_start = f.line_starts[line_idx];
        let col = offset.0 - line_start;

        // u32 is sufficient: source files < 4 billion lines.
        #[allow(clippy::cast_possible_truncation)]
        Some(LineCol {
            line: (line_idx as u32) + 1, // 1-based
            col: col + 1,                // 1-based
        })
    }

    /// Number of registered files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_returns_sequential_ids() {
        let mut reg = SourceRegistry::new();
        let a = reg.add("a.yaml", "hello".into());
        let b = reg.add("b.yaml", "world".into());
        assert_eq!(a, FileId(0));
        assert_eq!(b, FileId(1));
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn get_returns_content() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("test.yaml", "name: hello".into());
        let file = reg.get(id).expect("file exists");
        assert_eq!(&*file.content, "name: hello");
        assert_eq!(file.path, PathBuf::from("test.yaml"));
    }

    #[test]
    fn get_unknown_returns_none() {
        let reg = SourceRegistry::new();
        assert!(reg.get(FileId(42)).is_none());
    }

    #[test]
    fn line_col_single_line() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("<stdin>", "hello world".into());

        assert_eq!(reg.line_col(id, ByteOffset(0)), Some(LineCol::new(1, 1)));
        assert_eq!(reg.line_col(id, ByteOffset(5)), Some(LineCol::new(1, 6)));
        assert_eq!(reg.line_col(id, ByteOffset(10)), Some(LineCol::new(1, 11)));
    }

    #[test]
    fn line_col_multi_line() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("test.yaml", "abc\ndef\nghi".into());
        // line 1: abc\n  (offsets 0..3, newline at 3)
        // line 2: def\n  (offsets 4..6, newline at 7)
        // line 3: ghi    (offsets 8..10)

        assert_eq!(reg.line_col(id, ByteOffset(0)), Some(LineCol::new(1, 1))); // 'a'
        assert_eq!(reg.line_col(id, ByteOffset(3)), Some(LineCol::new(1, 4))); // '\n'
        assert_eq!(reg.line_col(id, ByteOffset(4)), Some(LineCol::new(2, 1))); // 'd'
        assert_eq!(reg.line_col(id, ByteOffset(8)), Some(LineCol::new(3, 1))); // 'g'
        assert_eq!(reg.line_col(id, ByteOffset(10)), Some(LineCol::new(3, 3))); // 'i'
    }

    #[test]
    fn line_col_past_end_returns_none() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("test.yaml", "hi".into());
        assert!(reg.line_col(id, ByteOffset(100)).is_none());
    }

    #[test]
    fn line_col_at_end_of_content() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("test.yaml", "ab".into());
        // Offset 2 == content.len(), should work (points to "just past end")
        assert_eq!(reg.line_col(id, ByteOffset(2)), Some(LineCol::new(1, 3)));
    }

    #[test]
    fn line_col_empty_content() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("empty.yaml", String::new());
        assert_eq!(reg.line_col(id, ByteOffset(0)), Some(LineCol::new(1, 1)));
        assert!(reg.line_col(id, ByteOffset(1)).is_none());
    }

    #[test]
    fn line_col_unknown_file() {
        let reg = SourceRegistry::new();
        assert!(reg.line_col(FileId(0), ByteOffset(0)).is_none());
    }

    #[test]
    fn empty_registry() {
        let reg = SourceRegistry::new();
        assert!(reg.is_empty());
        assert_eq!(reg.len(), 0);
    }

    #[test]
    fn source_file_content_is_shared() {
        let mut reg = SourceRegistry::new();
        let id = reg.add("test.yaml", "shared content".into());
        let file = reg.get(id).expect("exists");
        let arc1 = Arc::clone(&file.content);
        let arc2 = Arc::clone(&file.content);
        assert!(Arc::ptr_eq(&arc1, &arc2));
    }
}
