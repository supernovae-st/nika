//! Span ↔ LSP Position Conversion
//!
//! Converts between Nika's byte-offset Spans and LSP's line/character positions.

#[cfg(feature = "lsp")]
use tower_lsp::lsp_types::{Position, Range};

use crate::source::Span;

/// Convert a Nika Span to an LSP Range
///
/// # Arguments
///
/// * `span` - The source span with byte offsets
/// * `source` - The full source text (needed to compute line/col from offset)
///
/// # Example
///
/// ```ignore
/// let span = Span { start: 8, end: 25, file_id: FileId(0) };
/// let range = span_to_range(&span, "schema: nika/workflow@0.12\ntasks:");
/// assert_eq!(range.start.line, 0);
/// assert_eq!(range.start.character, 8);
/// ```
#[cfg(feature = "lsp")]
pub fn span_to_range(span: &Span, source: &str) -> Range {
    let start = offset_to_position(span.start.into(), source);
    let end = offset_to_position(span.end.into(), source);
    Range { start, end }
}

/// Convert a byte offset to an LSP Position (line, character)
///
/// LSP uses 0-based line numbers and UTF-16 code unit offsets for characters.
/// Characters outside the BMP (emoji, CJK supplementary) require 2 UTF-16 units
/// (a surrogate pair), so we use `ch.len_utf16()` instead of counting code points.
///
/// # Arguments
///
/// * `offset` - Byte offset into the source
/// * `source` - The full source text
#[cfg(feature = "lsp")]
pub fn offset_to_position(offset: usize, source: &str) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else if ch == '\r' {
            // Skip \r — LSP spec treats \r\n as one line break.
            // The \n that follows will handle line increment.
        } else {
            col += ch.len_utf16() as u32;
        }
    }

    Position {
        line,
        character: col,
    }
}

/// Convert an LSP Position to a byte offset
///
/// # Arguments
///
/// * `pos` - LSP position (line, character)
/// * `source` - The full source text
///
/// # Returns
///
/// Byte offset into the source, or `source.len()` if position is past end.
#[cfg(feature = "lsp")]
pub fn position_to_offset(pos: Position, source: &str) -> usize {
    let mut current_line = 0u32;
    let mut current_col = 0u32;

    for (i, ch) in source.char_indices() {
        if current_line == pos.line && current_col == pos.character {
            return i;
        }
        if ch == '\n' {
            // Check if we're at the requested line but past the character
            if current_line == pos.line {
                return i; // Return position at end of line
            }
            current_line += 1;
            current_col = 0;
        } else if ch == '\r' {
            // Skip \r — LSP spec treats \r\n as one line break.
        } else {
            current_col += ch.len_utf16() as u32;
        }
    }

    source.len()
}

// Stub implementations when LSP feature is disabled
#[cfg(not(feature = "lsp"))]
pub fn span_to_range(_span: &Span, _source: &str) -> (usize, usize, usize, usize) {
    (0, 0, 0, 0)
}

#[cfg(not(feature = "lsp"))]
pub fn offset_to_position(_offset: usize, _source: &str) -> (u32, u32) {
    (0, 0)
}

#[cfg(not(feature = "lsp"))]
pub fn position_to_offset(_line: u32, _character: u32, _source: &str) -> usize {
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::FileId;

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_to_position_first_line() {
        let source = "hello world";
        assert_eq!(
            offset_to_position(0, source),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(5, source),
            Position {
                line: 0,
                character: 5
            }
        );
        assert_eq!(
            offset_to_position(11, source),
            Position {
                line: 0,
                character: 11
            }
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_to_position_multiline() {
        let source = "line1\nline2\nline3";
        // Position after first newline (start of line2)
        assert_eq!(
            offset_to_position(6, source),
            Position {
                line: 1,
                character: 0
            }
        );
        // Position after second newline (start of line3)
        assert_eq!(
            offset_to_position(12, source),
            Position {
                line: 2,
                character: 0
            }
        );
        // Position in middle of line2
        assert_eq!(
            offset_to_position(8, source),
            Position {
                line: 1,
                character: 2
            }
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_span_to_range() {
        let source = "schema: nika/workflow@0.12\ntasks:";
        let span = Span::new(FileId(0), 8, 26);
        let range = span_to_range(&span, source);
        assert_eq!(range.start.line, 0);
        assert_eq!(range.start.character, 8);
        assert_eq!(range.end.line, 0);
        assert_eq!(range.end.character, 26);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_span_to_range_multiline() {
        let source = "tasks:\n  - id: step1";
        let span = Span::new(FileId(0), 10, 20);
        let range = span_to_range(&span, source);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 3); // After "  -"
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_position_to_offset_first_line() {
        let source = "hello world";
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 0
                },
                source
            ),
            0
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 5
                },
                source
            ),
            5
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_position_to_offset_multiline() {
        let source = "line1\nline2\nline3";
        assert_eq!(
            position_to_offset(
                Position {
                    line: 1,
                    character: 0
                },
                source
            ),
            6
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 2,
                    character: 0
                },
                source
            ),
            12
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 1,
                    character: 2
                },
                source
            ),
            8
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_roundtrip_offset_position() {
        let source = "schema: nika/workflow@0.12\ntasks:\n  - id: step1";
        for offset in [0, 5, 10, 27, 36, 46] {
            if offset <= source.len() {
                let pos = offset_to_position(offset, source);
                let back = position_to_offset(pos, source);
                assert_eq!(
                    back, offset,
                    "Roundtrip failed for offset {}: got {}",
                    offset, back
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_past_end() {
        let source = "short";
        let pos = offset_to_position(100, source);
        // Should stop at end of source
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 5);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_position_past_end() {
        let source = "short";
        let offset = position_to_offset(
            Position {
                line: 10,
                character: 0,
            },
            source,
        );
        assert_eq!(offset, source.len());
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_empty_source() {
        let source = "";
        assert_eq!(
            offset_to_position(0, source),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 0
                },
                source
            ),
            0
        );
    }

    // ── UTF-16 surrogate pair tests ──────────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_to_position_emoji() {
        // 🚀 is U+1F680, outside BMP → 2 UTF-16 code units (surrogate pair)
        // In UTF-8: 4 bytes (0xF0 0x9F 0x9A 0x80)
        let source = "a🚀b";
        // 'a' at byte 0 → character 0
        assert_eq!(
            offset_to_position(0, source),
            Position {
                line: 0,
                character: 0
            }
        );
        // '🚀' starts at byte 1 → character 1
        assert_eq!(
            offset_to_position(1, source),
            Position {
                line: 0,
                character: 1
            }
        );
        // 'b' starts at byte 5 → character 3 (1 for 'a' + 2 for '🚀')
        assert_eq!(
            offset_to_position(5, source),
            Position {
                line: 0,
                character: 3
            }
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_position_to_offset_emoji() {
        let source = "a🚀b";
        // character 0 → byte 0 ('a')
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 0
                },
                source
            ),
            0
        );
        // character 1 → byte 1 (start of '🚀')
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 1
                },
                source
            ),
            1
        );
        // character 3 → byte 5 ('b', after 2 UTF-16 units for emoji)
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 3
                },
                source
            ),
            5
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_roundtrip_emoji() {
        let source = "hello 🌍 world\nsecond 🎉 line";
        // Test roundtrip for char-boundary offsets only (mid-emoji offsets can't roundtrip)
        // "hello 🌍 world\nsecond 🎉 line"
        //  0     5 6    10 11   15 16 17     22 23 24     28 29
        for offset in [0, 5, 6, 10, 11, 15, 16, 22, 23, 24, 28, 29] {
            if offset <= source.len() {
                let pos = offset_to_position(offset, source);
                let back = position_to_offset(pos, source);
                assert_eq!(
                    back, offset,
                    "Roundtrip failed for offset {}: pos=({},{}), got {}",
                    offset, pos.line, pos.character, back
                );
            }
        }
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_to_position_cjk_supplementary() {
        // 𠀀 is U+20000 (CJK Unified Ideographs Extension B) → 2 UTF-16 code units
        // In UTF-8: 4 bytes
        let source = "x𠀀y";
        // 'x' at byte 0 → character 0
        assert_eq!(
            offset_to_position(0, source),
            Position {
                line: 0,
                character: 0
            }
        );
        // '𠀀' starts at byte 1 → character 1
        assert_eq!(
            offset_to_position(1, source),
            Position {
                line: 0,
                character: 1
            }
        );
        // 'y' starts at byte 5 → character 3 (1 for 'x' + 2 for '𠀀')
        assert_eq!(
            offset_to_position(5, source),
            Position {
                line: 0,
                character: 3
            }
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_to_position_bmp_non_ascii() {
        // BMP characters like é (U+00E9) and ñ (U+00F1) are 1 UTF-16 code unit
        // but 2 bytes in UTF-8
        let source = "café";
        // 'c' at byte 0 → char 0
        // 'a' at byte 1 → char 1
        // 'f' at byte 2 → char 2
        // 'é' at byte 3 → char 3 (2 UTF-8 bytes, 1 UTF-16 unit)
        assert_eq!(
            offset_to_position(3, source),
            Position {
                line: 0,
                character: 3
            }
        );
        // End of string: byte 5 → char 4
        assert_eq!(
            offset_to_position(5, source),
            Position {
                line: 0,
                character: 4
            }
        );
    }

    // ── Additional UTF-16 coverage ──────────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_greek_alpha() {
        // αβγ: each 2 UTF-8 bytes, 1 UTF-16 unit
        let source = "αβγ";
        // 'γ' at byte 4 → character 2
        assert_eq!(
            offset_to_position(4, source),
            Position {
                line: 0,
                character: 2
            }
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_emoji_multiline() {
        let source = "line1 🎉\nline2 🌍";
        // 🎉 at byte 6 (4 bytes), '\n' at byte 10, 'l' of line2 at byte 11
        let pos = offset_to_position(11, source);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_position_to_offset_after_emoji() {
        let source = "🎉b";
        // '🎉' = 2 UTF-16 units, 'b' at UTF-16 character 2
        let offset = position_to_offset(
            Position {
                line: 0,
                character: 2,
            },
            source,
        );
        assert_eq!(offset, 4); // 'b' at byte 4
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_consecutive_emoji() {
        let source = "🎉🌍🚀x";
        // Each emoji: 4 UTF-8 bytes, 2 UTF-16 units
        // 'x' at byte 12, UTF-16 character 6
        let pos = offset_to_position(12, source);
        assert_eq!(pos.character, 6);
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_yaml_with_emoji_positions() {
        let source = "prompt: \"Hello 🌍!\"\ntasks:";
        let tasks_offset = source.find("tasks").unwrap();
        let pos = offset_to_position(tasks_offset, source);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
    }

    // ── Windows \r\n line ending tests ─────────────────────────────

    #[test]
    #[cfg(feature = "lsp")]
    fn test_offset_to_position_crlf() {
        // "ab\r\ncd" — \r should be invisible to column counting
        let source = "ab\r\ncd";
        // 'a'=byte 0, 'b'=byte 1, '\r'=byte 2, '\n'=byte 3, 'c'=byte 4, 'd'=byte 5
        assert_eq!(
            offset_to_position(0, source),
            Position {
                line: 0,
                character: 0
            } // 'a'
        );
        assert_eq!(
            offset_to_position(1, source),
            Position {
                line: 0,
                character: 1
            } // 'b'
        );
        assert_eq!(
            offset_to_position(2, source),
            Position {
                line: 0,
                character: 2
            } // '\r' — at end of line content
        );
        assert_eq!(
            offset_to_position(4, source),
            Position {
                line: 1,
                character: 0
            } // 'c'
        );
        assert_eq!(
            offset_to_position(5, source),
            Position {
                line: 1,
                character: 1
            } // 'd'
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_position_to_offset_crlf() {
        let source = "ab\r\ncd";
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 0
                },
                source
            ),
            0 // 'a'
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 0,
                    character: 2
                },
                source
            ),
            2 // end of visible content on line 0 (at '\r')
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 1,
                    character: 0
                },
                source
            ),
            4 // 'c'
        );
        assert_eq!(
            position_to_offset(
                Position {
                    line: 1,
                    character: 1
                },
                source
            ),
            5 // 'd'
        );
    }

    #[test]
    #[cfg(feature = "lsp")]
    fn test_roundtrip_crlf() {
        let source = "line1\r\nline2\r\nline3";
        // Only test char-boundary offsets that are NOT the \r itself
        for offset in [0, 1, 5, 7, 8, 12, 14, 15, 18] {
            if offset <= source.len() {
                let pos = offset_to_position(offset, source);
                let back = position_to_offset(pos, source);
                // Offsets on \r (bytes 5, 12) map to the same position as the
                // preceding char, so roundtrip returns the \r's byte offset.
                // For non-\r offsets, roundtrip should be exact.
                if source.as_bytes().get(offset) != Some(&b'\r') {
                    assert_eq!(
                        back, offset,
                        "Roundtrip failed for offset {}: pos=({},{}), got {}",
                        offset, pos.line, pos.character, back
                    );
                }
            }
        }
    }
}
