// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Multi-file source registry for tracking included files.
//!
//! The SourceRegistry maintains a collection of source files and provides
//! utilities for converting byte offsets to line:col positions for error
//! reporting.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::span::{ByteOffset, FileId, Span};

/// A single source file with its content and path.
#[derive(Debug)]
pub struct SourceFile {
    /// Unique identifier for this file.
    pub id: FileId,
    /// Path to the source file.
    pub path: PathBuf,
    /// Source content (shared for efficiency).
    pub content: Arc<String>,
    /// Line start byte offsets for fast line:col lookup.
    /// line_starts[i] is the byte offset where line (i+1) starts.
    line_starts: Vec<u32>,
}

impl SourceFile {
    /// Create a new source file.
    pub fn new(id: FileId, path: PathBuf, content: String) -> Self {
        let line_starts = Self::compute_line_starts(&content);

        Self {
            id,
            path,
            content: Arc::new(content),
            line_starts,
        }
    }

    /// Compute line start offsets for a source string.
    fn compute_line_starts(content: &str) -> Vec<u32> {
        std::iter::once(0)
            .chain(content.match_indices('\n').map(|(i, _)| i as u32 + 1))
            .collect()
    }

    /// Get the number of lines in the file.
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Convert a byte offset to (line, column), both 1-indexed.
    ///
    /// Returns (1, 1) for offset 0, and so on.
    pub fn offset_to_line_col(&self, offset: ByteOffset) -> (usize, usize) {
        let offset = offset.as_usize();

        // Binary search for the line containing this offset
        let line = self
            .line_starts
            .partition_point(|&start| (start as usize) <= offset)
            .saturating_sub(1);

        let line_start = self.line_starts.get(line).copied().unwrap_or(0) as usize;
        let column = offset.saturating_sub(line_start);

        (line + 1, column + 1) // 1-indexed
    }

    /// Convert (line, column) to byte offset, both 1-indexed.
    ///
    /// Returns None if the line/column is out of bounds.
    pub fn line_col_to_offset(&self, line: usize, column: usize) -> Option<ByteOffset> {
        if line == 0 || column == 0 {
            return None;
        }

        let line_start = self.line_starts.get(line - 1)?;
        let offset = *line_start as usize + column - 1;

        if offset <= self.content.len() {
            Some(ByteOffset::from(offset))
        } else {
            None
        }
    }

    /// Get the text at a span.
    ///
    /// Returns an empty string if the span is out of bounds.
    pub fn text_at(&self, span: Span) -> &str {
        let range = span.range();
        if range.end <= self.content.len() {
            &self.content[range]
        } else {
            ""
        }
    }

    /// Get the text of a specific line (1-indexed).
    ///
    /// Returns the line without the trailing newline.
    pub fn line_text(&self, line: usize) -> Option<&str> {
        if line == 0 || line > self.line_starts.len() {
            return None;
        }

        let start = self.line_starts[line - 1] as usize;
        let end = self
            .line_starts
            .get(line)
            .map(|&e| e as usize)
            .unwrap_or(self.content.len());

        Some(self.content[start..end].trim_end_matches('\n'))
    }

    /// Get a snippet of source code around a span for error display.
    ///
    /// Returns up to `context_lines` lines before and after the span.
    pub fn snippet(&self, span: Span, context_lines: usize) -> SourceSnippet {
        let (start_line, start_col) = self.offset_to_line_col(span.start);
        let (end_line, end_col) = self.offset_to_line_col(span.end);

        let first_line = start_line.saturating_sub(context_lines).max(1);
        let last_line = (end_line + context_lines).min(self.line_count());

        let lines: Vec<(usize, String)> = (first_line..=last_line)
            .filter_map(|n| self.line_text(n).map(|text| (n, text.to_string())))
            .collect();

        SourceSnippet {
            path: self.path.clone(),
            lines,
            highlight_start: (start_line, start_col),
            highlight_end: (end_line, end_col),
        }
    }
}

/// A snippet of source code for error display.
#[derive(Debug, Clone)]
pub struct SourceSnippet {
    /// Path to the source file.
    pub path: PathBuf,
    /// Lines of source code: (line_number, content).
    pub lines: Vec<(usize, String)>,
    /// Start of highlight region (line, column), 1-indexed.
    pub highlight_start: (usize, usize),
    /// End of highlight region (line, column), 1-indexed.
    pub highlight_end: (usize, usize),
}

/// Registry of all source files in a compilation unit.
///
/// Tracks multiple files (main workflow + pkg: includes) and provides
/// utilities for error reporting across files.
#[derive(Debug, Default)]
pub struct SourceRegistry {
    /// All registered source files.
    files: Vec<SourceFile>,
    /// Map from path to file ID for deduplication.
    path_to_id: HashMap<PathBuf, FileId>,
    /// Next file ID to assign.
    next_id: u32,
}

impl SourceRegistry {
    /// Create a new empty source registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a source file and return its FileId.
    ///
    /// If the file was already added (same path), returns the existing FileId.
    pub fn add_file(&mut self, path: impl AsRef<Path>, content: String) -> FileId {
        let path = path.as_ref().to_path_buf();

        // Check for existing file with same path
        if let Some(&id) = self.path_to_id.get(&path) {
            return id;
        }

        let id = FileId(self.next_id);
        self.next_id += 1;

        let file = SourceFile::new(id, path.clone(), content);
        self.files.push(file);
        self.path_to_id.insert(path, id);

        id
    }

    /// Add a source file from a string (e.g., for tests).
    pub fn add_string(&mut self, name: &str, content: String) -> FileId {
        self.add_file(PathBuf::from(name), content)
    }

    /// Get a source file by ID.
    pub fn get(&self, id: FileId) -> Option<&SourceFile> {
        if id.is_dummy() {
            return None;
        }
        self.files.get(id.0 as usize)
    }

    /// Get the file path for a FileId.
    pub fn path(&self, id: FileId) -> Option<&Path> {
        self.get(id).map(|f| f.path.as_path())
    }

    /// Get the source content for a FileId.
    pub fn content(&self, id: FileId) -> Option<&str> {
        self.get(id).map(|f| f.content.as_str())
    }

    /// Get the number of registered files.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// Format a span as "path:line:col".
    pub fn format_location(&self, span: Span) -> String {
        if span.is_dummy() {
            return "<unknown>".to_string();
        }

        if let Some(file) = self.get(span.file) {
            let (line, col) = file.offset_to_line_col(span.start);
            format!("{}:{}:{}", file.path.display(), line, col)
        } else {
            "<unknown>".to_string()
        }
    }

    /// Format a span with just "line:col" (no path).
    pub fn format_position(&self, span: Span) -> String {
        if span.is_dummy() {
            return "<unknown>".to_string();
        }

        if let Some(file) = self.get(span.file) {
            let (line, col) = file.offset_to_line_col(span.start);
            format!("{}:{}", line, col)
        } else {
            "<unknown>".to_string()
        }
    }

    /// Get the text at a span.
    pub fn text_at(&self, span: Span) -> &str {
        if span.is_dummy() {
            return "";
        }

        self.get(span.file).map(|f| f.text_at(span)).unwrap_or("")
    }

    /// Create a NamedSource for miette error reporting.
    pub fn named_source(&self, id: FileId) -> Option<miette::NamedSource<String>> {
        self.get(id)
            .map(|f| miette::NamedSource::new(f.path.display().to_string(), f.content.to_string()))
    }

    /// Convert our Span to miette::SourceSpan.
    pub fn to_miette_span(&self, span: Span) -> miette::SourceSpan {
        miette::SourceSpan::new(
            miette::SourceOffset::from(span.start.as_usize()),
            span.len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SOURCE: &str =
        "schema: \"nika/workflow@0.12\"\nworkflow: test\ntasks:\n  - id: foo\n";

    #[test]
    fn test_source_file_line_col() {
        let file = SourceFile::new(
            FileId(0),
            PathBuf::from("test.yaml"),
            SAMPLE_SOURCE.to_string(),
        );

        // Line 1, column 1 (start of file)
        assert_eq!(file.offset_to_line_col(ByteOffset(0)), (1, 1));

        // Line 1, column 8 ("nika...)
        assert_eq!(file.offset_to_line_col(ByteOffset(7)), (1, 8));

        // Line 2, column 1 (after first newline at offset 28)
        assert_eq!(file.offset_to_line_col(ByteOffset(29)), (2, 1));
    }

    #[test]
    fn test_source_file_line_text() {
        let file = SourceFile::new(
            FileId(0),
            PathBuf::from("test.yaml"),
            SAMPLE_SOURCE.to_string(),
        );

        assert_eq!(file.line_text(1), Some("schema: \"nika/workflow@0.12\""));
        assert_eq!(file.line_text(2), Some("workflow: test"));
        assert_eq!(file.line_text(3), Some("tasks:"));
        assert_eq!(file.line_text(4), Some("  - id: foo"));
        // Line 5 exists (empty line after trailing \n)
        assert_eq!(file.line_text(5), Some(""));
        // Line 6 doesn't exist
        assert_eq!(file.line_text(6), None);
        // Line 0 is invalid (1-indexed)
        assert_eq!(file.line_text(0), None);
    }

    #[test]
    fn test_registry_add_file() {
        let mut registry = SourceRegistry::new();

        let id1 = registry.add_file("a.yaml", "content a".to_string());
        let id2 = registry.add_file("b.yaml", "content b".to_string());
        let id3 = registry.add_file("a.yaml", "ignored".to_string()); // duplicate

        assert_eq!(id1, FileId(0));
        assert_eq!(id2, FileId(1));
        assert_eq!(id3, id1); // Same file, same ID
        assert_eq!(registry.file_count(), 2);
    }

    #[test]
    fn test_registry_format_location() {
        let mut registry = SourceRegistry::new();
        let id = registry.add_file("workflow.yaml", SAMPLE_SOURCE.to_string());

        let span = Span::new(id, 29, 40);
        let location = registry.format_location(span);

        assert!(location.starts_with("workflow.yaml:2:1"));
    }

    #[test]
    fn test_registry_dummy_span() {
        let registry = SourceRegistry::new();
        let span = Span::dummy();

        assert_eq!(registry.format_location(span), "<unknown>");
        assert_eq!(registry.text_at(span), "");
    }

    #[test]
    fn test_line_col_to_offset() {
        let file = SourceFile::new(
            FileId(0),
            PathBuf::from("test.yaml"),
            SAMPLE_SOURCE.to_string(),
        );

        // Line 1, column 1 -> offset 0
        assert_eq!(file.line_col_to_offset(1, 1), Some(ByteOffset(0)));

        // Line 2, column 1 -> offset 29 (after first line + newline)
        assert_eq!(file.line_col_to_offset(2, 1), Some(ByteOffset(29)));

        // Invalid inputs
        assert_eq!(file.line_col_to_offset(0, 1), None);
        assert_eq!(file.line_col_to_offset(1, 0), None);
        assert_eq!(file.line_col_to_offset(100, 1), None);
    }
}
