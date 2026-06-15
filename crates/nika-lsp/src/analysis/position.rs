// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! `LineIndex` — byte offset ↔ LSP [`Position`] in UTF-16 columns.
//!
//! LSP positions are zero-based `(line, character)` where `character`
//! counts **UTF-16 code units** (the default `PositionEncodingKind`). The
//! engine's spans are byte offsets into a UTF-8 string. This module is the
//! one place that bridges the two encodings ·
//!
//! - `é` (U+00E9) is 2 bytes in UTF-8 but **1** UTF-16 unit.
//! - `🦋` (U+1F98B) is 4 bytes in UTF-8 but **2** UTF-16 units (a surrogate
//!   pair).
//!
//! [`LineIndex::new`] precomputes each line's start byte offset once; the
//! per-position conversions then scan only within a single line, counting
//! UTF-16 units over the relevant `&str` slice.

use lsp_types::Position;

/// A precomputed index of line starts over a UTF-8 document, converting
/// between byte offsets and UTF-16 [`Position`]s.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// The full source text (UTF-8).
    text: String,
    /// Byte offset of the start of each line. `line_starts[0] == 0`; the
    /// length equals the number of lines.
    line_starts: Vec<usize>,
}

impl LineIndex {
    /// Build a line index over `text`, precomputing line starts.
    ///
    /// A line starts at byte 0 and at the byte after every `\n`. A
    /// trailing `\n` therefore yields a final empty line (matching how
    /// editors place the cursor on the line after a terminating newline).
    #[must_use]
    pub fn new(text: &str) -> Self {
        let mut line_starts = vec![0usize];
        for (idx, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(idx + 1);
            }
        }
        Self {
            text: text.to_owned(),
            line_starts,
        }
    }

    /// The source text this index was built over.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The number of lines (always ≥ 1).
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.line_starts.len()
    }

    /// Convert a UTF-8 byte offset to an LSP [`Position`] (UTF-16 columns).
    ///
    /// An offset past the end of the text clamps to the end position. An
    /// offset that falls inside a multi-byte char rounds DOWN to that
    /// char's UTF-16 boundary (the conservative, never-past direction).
    #[must_use]
    pub fn position(&self, offset: usize) -> Position {
        let offset = offset.min(self.text.len());
        // The last line whose start is ≤ offset (binary search; line_starts
        // is sorted ascending).
        let line = match self.line_starts.binary_search(&offset) {
            Ok(exact) => exact,
            Err(insert) => insert.saturating_sub(1),
        };
        let line_start = self.line_starts.get(line).copied().unwrap_or(0);
        // Count UTF-16 units from the line start up to the offset, over the
        // slice that is guaranteed to lie on char boundaries (we slice on
        // the floor of `offset` to its char boundary first).
        let slice_end = self.floor_char_boundary(offset);
        let character = self.text.get(line_start..slice_end).map_or(0, utf16_len);
        Position {
            line: u32::try_from(line).unwrap_or(u32::MAX),
            character: u32::try_from(character).unwrap_or(u32::MAX),
        }
    }

    /// Convert an LSP [`Position`] (UTF-16 columns) back to a UTF-8 byte
    /// offset.
    ///
    /// A line beyond the document clamps to the document end. A character
    /// beyond the line's content clamps to the line end (the LSP rule:
    /// « if the character value is greater than the line length it defaults
    /// back to the line length »).
    #[must_use]
    pub fn offset(&self, pos: Position) -> usize {
        let line = pos.line as usize;
        let Some(&line_start) = self.line_starts.get(line) else {
            return self.text.len();
        };
        let line_end = self
            .line_starts
            .get(line + 1)
            .map_or(self.text.len(), |&next| next);
        let line_text = self.text.get(line_start..line_end).unwrap_or("");
        let want = pos.character as usize;
        let mut utf16 = 0usize;
        for (byte_idx, ch) in line_text.char_indices() {
            if utf16 >= want {
                return line_start + byte_idx;
            }
            utf16 += ch.len_utf16();
        }
        // `want` is at or past the line's UTF-16 length → clamp to line end,
        // but never include a trailing '\n' as a reachable column.
        line_start + trim_trailing_newline_len(line_text)
    }

    /// Round an arbitrary byte offset DOWN to the nearest UTF-8 char
    /// boundary (`str::is_char_boundary` is stable; `floor_char_boundary`
    /// is not, so we implement it by hand).
    fn floor_char_boundary(&self, offset: usize) -> usize {
        let mut idx = offset.min(self.text.len());
        while idx > 0 && !self.text.is_char_boundary(idx) {
            idx -= 1;
        }
        idx
    }
}

/// The UTF-16 code-unit length of a `&str`.
#[must_use]
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// The byte length of `line_text` with its trailing line terminator
/// excluded — the cursor cannot sit on the newline itself. Only a real
/// terminator is stripped: `\r\n` or a lone `\n`. A LONE `\r` is NOT a
/// line break here (lines are split on `\n` only), so it stays a reachable
/// column — stripping it would break the offset↔position round-trip.
fn trim_trailing_newline_len(line_text: &str) -> usize {
    match line_text.strip_suffix('\n') {
        Some(without_lf) => without_lf.strip_suffix('\r').unwrap_or(without_lf).len(),
        None => line_text.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pos(line: u32, character: u32) -> Position {
        Position { line, character }
    }

    #[test]
    fn ascii_single_line() {
        let idx = LineIndex::new("hello");
        assert_eq!(idx.position(0), pos(0, 0));
        assert_eq!(idx.position(5), pos(0, 5));
        assert_eq!(idx.offset(pos(0, 3)), 3);
    }

    #[test]
    fn multibyte_e_acute_is_one_utf16_unit() {
        // "é" = 2 UTF-8 bytes, 1 UTF-16 unit. "éx": byte offset of 'x' is 2,
        // but its UTF-16 column is 1.
        let text = "éx";
        let idx = LineIndex::new(text);
        assert_eq!(text.len(), 3, "é(2) + x(1)");
        let x_byte = text.find('x').expect("x present");
        assert_eq!(x_byte, 2);
        assert_eq!(idx.position(x_byte), pos(0, 1), "é counts as 1 UTF-16 unit");
        assert_eq!(idx.offset(pos(0, 1)), 2, "column 1 maps back to byte 2");
    }

    #[test]
    fn emoji_butterfly_is_two_utf16_units() {
        // "🦋" = 4 UTF-8 bytes, 2 UTF-16 units (surrogate pair). "🦋x":
        // 'x' is at byte 4, UTF-16 column 2.
        let text = "🦋x";
        let idx = LineIndex::new(text);
        assert_eq!(text.len(), 5, "🦋(4) + x(1)");
        let x_byte = text.find('x').expect("x present");
        assert_eq!(x_byte, 4);
        assert_eq!(
            idx.position(x_byte),
            pos(0, 2),
            "🦋 counts as 2 UTF-16 units"
        );
        assert_eq!(idx.offset(pos(0, 2)), 4, "column 2 maps back to byte 4");
    }

    #[test]
    fn multiline_positions() {
        let text = "a\nbb\nccc";
        let idx = LineIndex::new(text);
        assert_eq!(idx.line_count(), 3);
        // line 1 ("bb") starts at byte 2
        assert_eq!(idx.position(2), pos(1, 0));
        assert_eq!(idx.position(3), pos(1, 1));
        // line 2 ("ccc") starts at byte 5
        assert_eq!(idx.position(5), pos(2, 0));
        assert_eq!(idx.position(7), pos(2, 2));
        assert_eq!(idx.offset(pos(2, 2)), 7);
    }

    #[test]
    fn multibyte_across_lines() {
        let text = "café\n🦋!";
        let idx = LineIndex::new(text);
        // 'c' 'a' 'f' = 3 bytes, é = 2 bytes, \n = 1 → line 1 starts at 6
        let bang_byte = text.find('!').expect("!");
        // line 1 is "🦋!": 🦋 = 2 UTF-16 units, ! at column 2
        assert_eq!(idx.position(bang_byte), pos(1, 2));
        assert_eq!(idx.offset(pos(1, 2)), bang_byte);
    }

    #[test]
    fn offset_past_end_clamps() {
        let idx = LineIndex::new("ab");
        assert_eq!(idx.position(999), pos(0, 2));
        assert_eq!(idx.offset(pos(99, 99)), 2);
    }

    #[test]
    fn character_past_line_clamps_to_line_end_not_newline() {
        let text = "ab\ncd";
        let idx = LineIndex::new(text);
        // asking for column 99 on line 0 clamps to byte 2 (before the \n)
        assert_eq!(idx.offset(pos(0, 99)), 2);
    }

    #[test]
    fn trailing_newline_yields_final_empty_line() {
        let idx = LineIndex::new("x\n");
        assert_eq!(idx.line_count(), 2);
        assert_eq!(idx.position(2), pos(1, 0));
    }

    #[test]
    fn offset_inside_multibyte_rounds_down() {
        // byte 1 is inside the 2-byte 'é' — must not panic, rounds to col 0
        let idx = LineIndex::new("é");
        assert_eq!(idx.position(1), pos(0, 0));
    }

    #[test]
    fn offset_inside_second_multibyte_floors_to_prior_boundary() {
        // "éé" = 4 bytes (2 + 2). byte 3 is INSIDE the second 'é'.
        // `floor_char_boundary` must walk DOWN from 3 to the boundary at 2,
        // so the UTF-16 column is 1 (the first 'é' precedes the cursor).
        // If the floor loop fails to decrement (`>` → `==`/`<`), slice_end
        // stays at byte 3 (mid-char), `text.get(0..3)` is None, and the
        // column wrongly collapses to 0.
        let idx = LineIndex::new("éé");
        assert_eq!(idx.text().len(), 4, "two 2-byte chars");
        assert!(!"éé".is_char_boundary(3), "byte 3 is mid-char");
        assert_eq!(
            idx.position(3),
            pos(0, 1),
            "floors byte 3 down to the boundary at byte 2 → column 1"
        );
    }
}

#[cfg(test)]
mod proptests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        // Gate 6 — round-trip: for any char boundary in arbitrary UTF-8,
        // offset(position(b)) == b. Positions only land on char
        // boundaries, so the round-trip is exact there.
        #[test]
        fn offset_position_round_trip(text in ".*") {
            let idx = LineIndex::new(&text);
            for (byte_idx, _) in text.char_indices() {
                let pos = idx.position(byte_idx);
                let back = idx.offset(pos);
                prop_assert_eq!(
                    back, byte_idx,
                    "round-trip failed at byte {} in {:?}", byte_idx, text
                );
            }
            // end-of-text boundary also round-trips
            let end = text.len();
            prop_assert_eq!(idx.offset(idx.position(end)), end);
        }

        // position() never panics and never reports a column past the line.
        #[test]
        fn position_is_total(text in ".*", offset in 0usize..10_000) {
            let idx = LineIndex::new(&text);
            let _ = idx.position(offset); // must not panic
        }
    }
}
