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
/// For simplicity, we use character counts (which works for ASCII and most UTF-8).
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
        } else {
            col += 1;
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
        } else {
            current_col += 1;
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
}
