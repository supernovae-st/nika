// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Span and source position tracking for Nika workflows.
//!
//! This module provides the foundation for error reporting with precise
//! line:col positions, enabling cargo-style diagnostics via miette.

use std::ops::Range;

/// Unique identifier for a source file in the SourceRegistry.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Default)]
pub struct FileId(pub u32);

impl FileId {
    /// The dummy file ID used for synthetic or unknown locations.
    pub const DUMMY: FileId = FileId(u32::MAX);

    /// Check if this is the dummy file ID.
    pub fn is_dummy(self) -> bool {
        self == Self::DUMMY
    }
}

/// A byte offset in a source file.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Default)]
pub struct ByteOffset(pub u32);

impl ByteOffset {
    /// Create a new byte offset.
    pub fn new(offset: u32) -> Self {
        Self(offset)
    }

    /// Get the offset as usize.
    pub fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl From<usize> for ByteOffset {
    fn from(offset: usize) -> Self {
        Self(offset as u32)
    }
}

impl From<ByteOffset> for usize {
    fn from(offset: ByteOffset) -> Self {
        offset.0 as usize
    }
}

/// A span in a source file (byte range + file ID).
///
/// Spans are the foundation for error reporting. Every AST node in the
/// raw phase carries a span, enabling precise error locations.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Default)]
pub struct Span {
    /// The source file this span belongs to.
    pub file: FileId,
    /// Start byte offset (inclusive).
    pub start: ByteOffset,
    /// End byte offset (exclusive).
    pub end: ByteOffset,
}

impl Span {
    /// Create a new span.
    pub fn new(file: FileId, start: u32, end: u32) -> Self {
        Self {
            file,
            start: ByteOffset(start),
            end: ByteOffset(end),
        }
    }

    /// Create a dummy span for synthetic nodes or unknown locations.
    pub const fn dummy() -> Self {
        Self {
            file: FileId::DUMMY,
            start: ByteOffset(0),
            end: ByteOffset(0),
        }
    }

    /// Check if this is a dummy span.
    pub fn is_dummy(&self) -> bool {
        self.file.is_dummy()
    }

    /// Get the byte range as a `Range<usize>`.
    pub fn range(&self) -> Range<usize> {
        self.start.as_usize()..self.end.as_usize()
    }

    /// Get the length in bytes.
    pub fn len(&self) -> usize {
        (self.end.0 - self.start.0) as usize
    }

    /// Check if the span is empty.
    pub fn is_empty(&self) -> bool {
        self.start.0 >= self.end.0
    }

    /// Merge two spans into one that covers both.
    /// Both spans must be from the same file.
    pub fn merge(self, other: Span) -> Span {
        debug_assert!(
            self.file == other.file || self.is_dummy() || other.is_dummy(),
            "Cannot merge spans from different files"
        );

        if self.is_dummy() {
            return other;
        }
        if other.is_dummy() {
            return self;
        }

        Span {
            file: self.file,
            start: ByteOffset(self.start.0.min(other.start.0)),
            end: ByteOffset(self.end.0.max(other.end.0)),
        }
    }

    /// Create a span that points to the start position (zero-length).
    pub fn start_point(self) -> Span {
        Span {
            file: self.file,
            start: self.start,
            end: self.start,
        }
    }

    /// Create a span that points to the end position (zero-length).
    pub fn end_point(self) -> Span {
        Span {
            file: self.file,
            start: self.end,
            end: self.end,
        }
    }
}

/// A value with an associated span.
///
/// This is the core type for the raw AST phase. Every parsed value
/// carries its source location for error reporting.
#[derive(Clone, Debug)]
pub struct Spanned<T> {
    /// The wrapped value.
    pub value: T,
    /// The source location of the value.
    pub span: Span,
}

impl<T> Spanned<T> {
    /// Create a new spanned value.
    pub fn new(value: T, span: Span) -> Self {
        Self { value, span }
    }

    /// Create a spanned value with a dummy span.
    pub fn dummy(value: T) -> Self {
        Self {
            value,
            span: Span::dummy(),
        }
    }

    /// Map the inner value while preserving the span.
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Spanned<U> {
        Spanned {
            value: f(self.value),
            span: self.span,
        }
    }

    /// Get a reference to the inner value.
    pub fn inner(&self) -> &T {
        &self.value
    }

    /// Get a mutable reference to the inner value.
    pub fn inner_mut(&mut self) -> &mut T {
        &mut self.value
    }

    /// Consume self and return the inner value.
    pub fn into_inner(self) -> T {
        self.value
    }

    /// Create a reference to a spanned reference.
    pub fn as_ref(&self) -> Spanned<&T> {
        Spanned {
            value: &self.value,
            span: self.span,
        }
    }
}

impl<T> std::ops::Deref for Spanned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Spanned<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.value
    }
}

impl<T: PartialEq> PartialEq for Spanned<T> {
    fn eq(&self, other: &Self) -> bool {
        // Compare only values, not spans (for semantic equality)
        self.value == other.value
    }
}

impl<T: Eq> Eq for Spanned<T> {}

impl<T: std::hash::Hash> std::hash::Hash for Spanned<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        // Hash only the value, not the span
        self.value.hash(state);
    }
}

impl<T: Default> Default for Spanned<T> {
    fn default() -> Self {
        Self::dummy(T::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_span_creation() {
        let span = Span::new(FileId(1), 10, 20);
        assert_eq!(span.file, FileId(1));
        assert_eq!(span.start.0, 10);
        assert_eq!(span.end.0, 20);
        assert_eq!(span.len(), 10);
        assert!(!span.is_dummy());
    }

    #[test]
    fn test_dummy_span() {
        let span = Span::dummy();
        assert!(span.is_dummy());
        assert!(span.file.is_dummy());
    }

    #[test]
    fn test_span_merge() {
        let file = FileId(1);
        let a = Span::new(file, 10, 20);
        let b = Span::new(file, 15, 30);
        let merged = a.merge(b);

        assert_eq!(merged.start.0, 10);
        assert_eq!(merged.end.0, 30);
    }

    #[test]
    fn test_span_merge_with_dummy() {
        let file = FileId(1);
        let real = Span::new(file, 10, 20);
        let dummy = Span::dummy();

        assert_eq!(real.merge(dummy), real);
        assert_eq!(dummy.merge(real), real);
    }

    #[test]
    fn test_spanned_map() {
        let spanned = Spanned::new(42, Span::new(FileId(1), 0, 5));
        let mapped = spanned.map(|x| x.to_string());

        assert_eq!(mapped.value, "42");
        assert_eq!(mapped.span.start.0, 0);
    }

    #[test]
    fn test_spanned_equality_ignores_span() {
        let a = Spanned::new(42, Span::new(FileId(1), 0, 5));
        let b = Spanned::new(42, Span::new(FileId(2), 100, 200));

        assert_eq!(a, b); // Same value, different spans
    }
}
