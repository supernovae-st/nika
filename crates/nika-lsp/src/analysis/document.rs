// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `Document` — an open text buffer with its [`LineIndex`].
//!
//! A document holds the current text plus a precomputed [`LineIndex`] for
//! position conversion. It applies LSP content changes: a range-based
//! incremental edit (splice the new text into the byte range the range
//! spans) or a full replace (a change with no `range`, per the LSP spec:
//! « If range and rangeLength are omitted the new text is considered to be
//! the full content of the document »).
//!
//! v0.1 advertises FULL sync (so clients send full replacements), but the
//! range-based path is implemented and tested too — it is the same splice
//! logic, and supporting it costs nothing and makes the document robust to
//! a client that sends incremental changes anyway.

use lsp_types::TextDocumentContentChangeEvent;

use super::position::LineIndex;

/// An open document: its text and the line index over it.
#[derive(Debug, Clone)]
pub struct Document {
    index: LineIndex,
}

impl Document {
    /// Open a document with the given initial text.
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            index: LineIndex::new(text),
        }
    }

    /// The current text.
    #[must_use]
    pub fn text(&self) -> &str {
        self.index.text()
    }

    /// The line index over the current text.
    #[must_use]
    pub fn line_index(&self) -> &LineIndex {
        &self.index
    }

    /// Replace the whole document text.
    pub fn set_text(&mut self, text: &str) {
        self.index = LineIndex::new(text);
    }

    /// Apply one LSP content change.
    ///
    /// - `change.range == None` → full replace (the new text is the whole
    ///   document).
    /// - `change.range == Some(range)` → splice `change.text` into the byte
    ///   span the range covers (incremental edit).
    ///
    /// After each change the [`LineIndex`] is rebuilt so subsequent changes
    /// in the same batch resolve their ranges against the up-to-date text
    /// (the LSP contract: changes in one notification apply in order).
    pub fn apply_change(&mut self, change: &TextDocumentContentChangeEvent) {
        match change.range {
            None => self.set_text(&change.text),
            Some(range) => {
                let start = self.index.offset(range.start);
                let end = self.index.offset(range.end);
                // Guard against an inverted range (start > end): clamp end
                // up to start so the splice is a pure insertion, never a
                // panic on a backwards slice.
                let (lo, hi) = if start <= end {
                    (start, end)
                } else {
                    (start, start)
                };
                let mut next =
                    String::with_capacity(self.text().len() - (hi - lo) + change.text.len());
                next.push_str(self.text().get(..lo).unwrap_or(""));
                next.push_str(&change.text);
                next.push_str(self.text().get(hi..).unwrap_or(""));
                self.set_text(&next);
            }
        }
    }

    /// Apply a batch of content changes in order.
    pub fn apply_changes(&mut self, changes: &[TextDocumentContentChangeEvent]) {
        for change in changes {
            self.apply_change(change);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::{Position, Range};

    fn full(text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: text.to_owned(),
        }
    }

    fn ranged(range: Range, text: &str) -> TextDocumentContentChangeEvent {
        TextDocumentContentChangeEvent {
            range: Some(range),
            range_length: None,
            text: text.to_owned(),
        }
    }

    fn at(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn full_replace() {
        let mut doc = Document::new("old");
        doc.apply_change(&full("new content"));
        assert_eq!(doc.text(), "new content");
    }

    #[test]
    fn incremental_middle_splice() {
        let mut doc = Document::new("hello world");
        // replace "world" (chars 6..11) with "there"
        let range = Range::new(at(0, 6), at(0, 11));
        doc.apply_change(&ranged(range, "there"));
        assert_eq!(doc.text(), "hello there");
    }

    #[test]
    fn incremental_insertion_empty_range() {
        let mut doc = Document::new("ac");
        // insert "b" at column 1 (zero-width range)
        let range = Range::new(at(0, 1), at(0, 1));
        doc.apply_change(&ranged(range, "b"));
        assert_eq!(doc.text(), "abc");
    }

    #[test]
    fn incremental_across_lines() {
        let mut doc = Document::new("line1\nline2\nline3");
        // delete from (0,5) to (2,0): joins line1 + line3
        let range = Range::new(at(0, 5), at(2, 0));
        doc.apply_change(&ranged(range, ""));
        assert_eq!(doc.text(), "line1line3");
    }

    #[test]
    fn incremental_with_multibyte() {
        let mut doc = Document::new("café");
        // append "🦋" at end (column 4 = after é)
        let range = Range::new(at(0, 4), at(0, 4));
        doc.apply_change(&ranged(range, "🦋"));
        assert_eq!(doc.text(), "café🦋");
    }

    #[test]
    fn batch_applies_in_order() {
        let mut doc = Document::new("aaa");
        let changes = [
            ranged(Range::new(at(0, 0), at(0, 0)), "X"), // -> "Xaaa"
            ranged(Range::new(at(0, 4), at(0, 4)), "Y"), // -> "XaaaY"
        ];
        doc.apply_changes(&changes);
        assert_eq!(doc.text(), "XaaaY");
    }

    #[test]
    fn inverted_range_does_not_panic() {
        let mut doc = Document::new("hello");
        // start (0,3) after end (0,1) — clamp to insertion at start
        let range = Range::new(at(0, 3), at(0, 1));
        doc.apply_change(&ranged(range, "Z"));
        // splice is a pure insertion at byte 3: "helZlo"
        assert_eq!(doc.text(), "helZlo");
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use lsp_types::{Position, Range};
    use proptest::prelude::*;

    // A full-replace change sequence must leave the document equal to the
    // last replacement — apply_change(full) ≡ set_text.
    proptest! {
        #[test]
        fn full_replace_sequence_equals_last(
            texts in proptest::collection::vec(".*", 1..6)
        ) {
            let mut doc = Document::new("");
            for t in &texts {
                doc.apply_change(&TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: t.clone(),
                });
            }
            let last = texts.last().cloned().unwrap_or_default();
            prop_assert_eq!(doc.text(), last);
        }
    }

    // An incremental edit and the equivalent full replacement produce the
    // same text: splicing `ins` into byte range [a, b) of `base` equals
    // the precomputed full string.
    proptest! {
        #[test]
        fn incremental_splice_equals_full_replace(
            base in ".*",
            ins in ".*",
            a in 0usize..200,
            b in 0usize..200,
        ) {
            // Build a document and compute a valid char-boundary range
            // [lo, hi) from the proptest offsets.
            let mut doc = Document::new(&base);
            let lo = floor_boundary(&base, a.min(base.len()));
            let hi = floor_boundary(&base, b.min(base.len()));
            let (lo, hi) = (lo.min(hi), lo.max(hi));

            // The expected full result of splicing `ins` into [lo, hi).
            let mut expected = String::new();
            expected.push_str(&base[..lo]);
            expected.push_str(&ins);
            expected.push_str(&base[hi..]);

            // Apply the same edit as an incremental range change, using the
            // document's own position mapping to express [lo, hi).
            let li = doc.line_index().clone();
            let range = Range::new(byte_to_pos(&li, lo), byte_to_pos(&li, hi));
            doc.apply_change(&TextDocumentContentChangeEvent {
                range: Some(range),
                range_length: None,
                text: ins.clone(),
            });
            prop_assert_eq!(doc.text(), expected);
        }
    }

    fn floor_boundary(s: &str, mut idx: usize) -> usize {
        idx = idx.min(s.len());
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }

    fn byte_to_pos(li: &super::LineIndex, byte: usize) -> Position {
        li.position(byte)
    }
}
