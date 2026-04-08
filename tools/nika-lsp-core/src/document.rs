// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Rope-backed document for efficient incremental edits.
//!
//! [`Document`] wraps a [`ropey::Rope`] and provides the operations
//! needed for LSP `textDocument/didChange` handling.

use ropey::Rope;

/// A document backed by a Rope for efficient incremental edits.
#[derive(Debug, Clone)]
pub struct Document {
    rope: Rope,
    version: i32,
}

impl Document {
    /// Create a new document from full text and initial version.
    pub fn new(text: String, version: i32) -> Self {
        Self {
            rope: Rope::from_str(&text),
            version,
        }
    }

    /// Full text content.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Current version.
    pub fn version(&self) -> i32 {
        self.version
    }

    /// Apply an incremental edit (from LSP `didChange`).
    ///
    /// The `range` uses LSP coordinates (0-based line/character).
    /// The `new_text` replaces the content in that range.
    pub fn apply_edit(&mut self, range: ls_types::Range, new_text: &str, new_version: i32) {
        let start = self.lsp_position_to_char_idx(&range.start);
        let end = self.lsp_position_to_char_idx(&range.end);
        self.rope.remove(start..end);
        self.rope.insert(start, new_text);
        self.version = new_version;
    }

    /// Apply a full content change (replace everything).
    pub fn set_text(&mut self, text: String, new_version: i32) {
        self.rope = Rope::from_str(&text);
        self.version = new_version;
    }

    /// Total length in characters.
    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    /// Total length in bytes.
    pub fn len_bytes(&self) -> usize {
        self.rope.len_bytes()
    }

    /// Number of lines.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Convert an LSP position to a char index in the rope.
    fn lsp_position_to_char_idx(&self, pos: &ls_types::Position) -> usize {
        let line = (pos.line as usize).min(self.rope.len_lines().saturating_sub(1));
        let line_start = self.rope.line_to_char(line);
        let line_len = self.rope.line(line).len_chars();
        let col = (pos.character as usize).min(line_len);
        line_start + col
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn pos(line: u32, character: u32) -> ls_types::Position {
        ls_types::Position::new(line, character)
    }

    fn range(sl: u32, sc: u32, el: u32, ec: u32) -> ls_types::Range {
        ls_types::Range::new(pos(sl, sc), pos(el, ec))
    }

    #[test]
    fn new_document() {
        let doc = Document::new("hello\nworld".into(), 1);
        assert_eq!(doc.text(), "hello\nworld");
        assert_eq!(doc.version(), 1);
    }

    #[test]
    fn empty_document() {
        let doc = Document::new(String::new(), 0);
        assert_eq!(doc.text(), "");
        assert_eq!(doc.version(), 0);
        assert_eq!(doc.len_chars(), 0);
        assert_eq!(doc.len_bytes(), 0);
    }

    #[test]
    fn line_count() {
        let doc = Document::new("a\nb\nc".into(), 1);
        assert_eq!(doc.line_count(), 3);
    }

    #[test]
    fn set_text_replaces_everything() {
        let mut doc = Document::new("old".into(), 1);
        doc.set_text("new content".into(), 2);
        assert_eq!(doc.text(), "new content");
        assert_eq!(doc.version(), 2);
    }

    #[test]
    fn apply_edit_insert_at_start() {
        let mut doc = Document::new("world".into(), 1);
        doc.apply_edit(range(0, 0, 0, 0), "hello ", 2);
        assert_eq!(doc.text(), "hello world");
        assert_eq!(doc.version(), 2);
    }

    #[test]
    fn apply_edit_insert_at_end() {
        let mut doc = Document::new("hello".into(), 1);
        doc.apply_edit(range(0, 5, 0, 5), " world", 2);
        assert_eq!(doc.text(), "hello world");
    }

    #[test]
    fn apply_edit_replace_single_line() {
        let mut doc = Document::new("hello world".into(), 1);
        doc.apply_edit(range(0, 6, 0, 11), "rust", 2);
        assert_eq!(doc.text(), "hello rust");
    }

    #[test]
    fn apply_edit_replace_across_lines() {
        let mut doc = Document::new("aaa\nbbb\nccc".into(), 1);
        // Replace from line 0, col 1 to line 2, col 2 -> "aXXc"
        doc.apply_edit(range(0, 1, 2, 2), "XX", 2);
        assert_eq!(doc.text(), "aXXc");
    }

    #[test]
    fn apply_edit_delete() {
        let mut doc = Document::new("hello world".into(), 1);
        doc.apply_edit(range(0, 5, 0, 11), "", 2);
        assert_eq!(doc.text(), "hello");
    }

    #[test]
    fn apply_edit_multiline_insert() {
        let mut doc = Document::new("ab".into(), 1);
        doc.apply_edit(range(0, 1, 0, 1), "\n", 2);
        assert_eq!(doc.text(), "a\nb");
        assert_eq!(doc.line_count(), 2);
    }

    #[test]
    fn sequential_edits() {
        let mut doc = Document::new("schema: '@0.12'".into(), 1);

        // Type a newline + "tasks:"
        doc.apply_edit(range(0, 15, 0, 15), "\ntasks:", 2);
        assert_eq!(doc.text(), "schema: '@0.12'\ntasks:");

        // Add a newline + "  summarize:"
        doc.apply_edit(range(1, 6, 1, 6), "\n  summarize:", 3);
        assert_eq!(doc.text(), "schema: '@0.12'\ntasks:\n  summarize:");
        assert_eq!(doc.version(), 3);
    }

    #[test]
    fn unicode_edit() {
        let mut doc = Document::new("caf\u{00e9}".into(), 1);
        assert_eq!(doc.len_chars(), 4); // c, a, f, e-acute
                                        // Replace the e-acute with "e"
        doc.apply_edit(range(0, 3, 0, 4), "e", 2);
        assert_eq!(doc.text(), "cafe");
    }

    #[test]
    fn len_bytes_vs_chars() {
        let doc = Document::new("caf\u{00e9}".into(), 1);
        assert_eq!(doc.len_chars(), 4);
        assert_eq!(doc.len_bytes(), 5); // e-acute is 2 bytes
    }

    #[test]
    fn clone_is_independent() {
        let doc = Document::new("hello".into(), 1);
        let mut cloned = doc.clone();
        cloned.set_text("world".into(), 2);
        assert_eq!(doc.text(), "hello");
        assert_eq!(cloned.text(), "world");
    }

    #[test]
    fn edit_empty_document() {
        let mut doc = Document::new(String::new(), 0);
        doc.apply_edit(range(0, 0, 0, 0), "new content", 1);
        assert_eq!(doc.text(), "new content");
    }

    #[test]
    fn edit_clamps_out_of_bounds_position() {
        let mut doc = Document::new("ab".into(), 1);
        // Col 99 on line 0 should clamp to col 2
        doc.apply_edit(range(0, 99, 0, 99), "!", 2);
        assert_eq!(doc.text(), "ab!");
    }
}
