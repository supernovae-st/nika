// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Source position types for miette diagnostics.
//!
//! These types track where in the YAML source each AST node came from,
//! enabling precise error messages with underlined source ranges.

use serde::{Deserialize, Serialize};

/// An interned file identifier. Used to index into [`super::SourceRegistry`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct FileId(pub u32);

impl FileId {
    /// Create a new file identifier.
    #[must_use]
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for FileId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "file:{}", self.0)
    }
}

/// A byte offset within a source file.
#[derive(
    Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Default, Serialize, Deserialize,
)]
#[non_exhaustive]
pub struct ByteOffset(pub u32);

impl ByteOffset {
    /// Create a new byte offset.
    #[must_use]
    pub fn new(offset: u32) -> Self {
        Self(offset)
    }

    /// The offset as a `usize` for indexing.
    #[must_use]
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl std::fmt::Display for ByteOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A span within a source file: `[start, end)` byte offsets.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Span {
    /// Source file this span belongs to.
    pub file: FileId,
    /// Start byte offset (inclusive).
    pub start: ByteOffset,
    /// End byte offset (exclusive).
    pub end: ByteOffset,
}

impl Span {
    /// Create a new span.
    #[must_use]
    pub fn new(file: FileId, start: ByteOffset, end: ByteOffset) -> Self {
        Self { file, start, end }
    }

    /// Create a zero-length span at the given offset.
    #[must_use]
    pub fn point(file: FileId, offset: ByteOffset) -> Self {
        Self {
            file,
            start: offset,
            end: offset,
        }
    }

    /// The length of this span in bytes.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.end.0.saturating_sub(self.start.0)
    }

    /// Whether this span has zero length.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Merge two spans into one that covers both.
    #[must_use]
    pub fn merge(self, other: Self) -> Self {
        debug_assert_eq!(
            self.file, other.file,
            "cannot merge spans from different files"
        );
        Self {
            file: self.file,
            start: ByteOffset(self.start.0.min(other.start.0)),
            end: ByteOffset(self.end.0.max(other.end.0)),
        }
    }

    /// Whether this span contains the given offset.
    #[must_use]
    pub fn contains(&self, offset: ByteOffset) -> bool {
        offset.0 >= self.start.0 && offset.0 < self.end.0
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}-{}", self.file, self.start, self.end)
    }
}

/// A 1-based line and column position.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct LineCol {
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number (byte offset within line).
    pub col: u32,
}

impl LineCol {
    /// Create a new line:col position.
    #[must_use]
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

impl std::fmt::Display for LineCol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// A value annotated with its source span.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Spanned<T> {
    /// The wrapped value.
    pub value: T,
    /// Where it appeared in the source.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Create a new spanned value.
    #[must_use]
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// Map the inner value while preserving the span.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_id_default_is_zero() {
        assert_eq!(FileId::default(), FileId(0));
    }

    #[test]
    fn file_id_display() {
        assert_eq!(FileId(3).to_string(), "file:3");
    }

    #[test]
    fn byte_offset_as_usize() {
        assert_eq!(ByteOffset(42).as_usize(), 42);
    }

    #[test]
    fn byte_offset_ordering() {
        assert!(ByteOffset(10) < ByteOffset(20));
    }

    #[test]
    fn span_new_and_len() {
        let span = Span::new(FileId(0), ByteOffset(5), ByteOffset(15));
        assert_eq!(span.len(), 10);
        assert!(!span.is_empty());
    }

    #[test]
    fn span_point_is_empty() {
        let span = Span::point(FileId(0), ByteOffset(5));
        assert_eq!(span.len(), 0);
        assert!(span.is_empty());
    }

    #[test]
    fn span_contains() {
        let span = Span::new(FileId(0), ByteOffset(10), ByteOffset(20));
        assert!(span.contains(ByteOffset(10)));
        assert!(span.contains(ByteOffset(15)));
        assert!(!span.contains(ByteOffset(20))); // exclusive end
        assert!(!span.contains(ByteOffset(9)));
    }

    #[test]
    fn span_merge() {
        let a = Span::new(FileId(0), ByteOffset(5), ByteOffset(15));
        let b = Span::new(FileId(0), ByteOffset(10), ByteOffset(25));
        let merged = a.merge(b);
        assert_eq!(merged.start, ByteOffset(5));
        assert_eq!(merged.end, ByteOffset(25));
    }

    #[test]
    fn span_display() {
        let span = Span::new(FileId(1), ByteOffset(10), ByteOffset(20));
        assert_eq!(span.to_string(), "file:1:10-20");
    }

    #[test]
    fn line_col_display() {
        assert_eq!(LineCol::new(5, 12).to_string(), "5:12");
    }

    #[test]
    fn spanned_map() {
        let s = Spanned::new(42_u32, Span::default());
        let doubled = s.map(|v| v * 2);
        assert_eq!(doubled.value, 84);
    }

    #[test]
    fn spanned_eq_ignores_span() {
        let a = Spanned::new("hello", Span::new(FileId(0), ByteOffset(0), ByteOffset(5)));
        let b = Spanned::new(
            "hello",
            Span::new(FileId(1), ByteOffset(10), ByteOffset(20)),
        );
        assert_eq!(a, b); // same value, different span → equal
    }

    #[test]
    fn span_default_is_zero() {
        let span = Span::default();
        assert_eq!(span.file, FileId(0));
        assert_eq!(span.start, ByteOffset(0));
        assert_eq!(span.end, ByteOffset(0));
        assert!(span.is_empty());
    }

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn source_types_are_send_sync() {
        _assert_send_sync::<FileId>();
        _assert_send_sync::<ByteOffset>();
        _assert_send_sync::<Span>();
        _assert_send_sync::<LineCol>();
        _assert_send_sync::<Spanned<String>>();
    }
}
