//! Position conversion utilities for LSP ↔ AST integration.
//!
//! This module bridges LSP positions (line:col 0-indexed) with Nika's
//! AST position types (ByteOffset, Span) for intelligent completions.
//!
//! # Example
//!
//! ```ignore
//! use nika_lsp::position::{position_to_offset, offset_to_position, span_to_lsp_range};
//!
//! let content = "hello\nworld";
//! let offset = position_to_offset(content, 1, 2).unwrap(); // line 1, col 2 = 'r'
//! let pos = offset_to_position(content, offset);
//! assert_eq!(pos.line, 1);
//! assert_eq!(pos.character, 2);
//! ```

use tower_lsp_server::ls_types::Position;

/// A byte offset in a source file (mirrors nika::source::ByteOffset).
///
/// We use a local copy to avoid depending on the full nika crate,
/// keeping the LSP lightweight.
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

/// Convert an LSP Position (0-indexed line:col) to a ByteOffset.
///
/// Returns `None` if the position is out of bounds.
///
/// # Arguments
///
/// * `content` - The full document content
/// * `line` - 0-indexed line number
/// * `character` - 0-indexed UTF-16 code unit offset (LSP standard)
///
/// # Example
///
/// ```ignore
/// let content = "hello\nworld";
/// assert_eq!(position_to_offset(content, 0, 0), Some(ByteOffset(0)));  // 'h'
/// assert_eq!(position_to_offset(content, 0, 5), Some(ByteOffset(5)));  // '\n'
/// assert_eq!(position_to_offset(content, 1, 0), Some(ByteOffset(6)));  // 'w'
/// assert_eq!(position_to_offset(content, 1, 4), Some(ByteOffset(10))); // 'd'
/// ```
pub fn position_to_offset(content: &str, line: u32, character: u32) -> Option<ByteOffset> {
    // Handle edge case: empty content first
    if content.is_empty() {
        return if line == 0 && character == 0 {
            Some(ByteOffset(0))
        } else {
            None
        };
    }

    let mut current_line = 0u32;
    let mut line_start = 0usize;

    // Find the start of the target line
    for (idx, ch) in content.char_indices() {
        if current_line == line {
            line_start = idx;
            break;
        }
        if ch == '\n' {
            current_line += 1;
            if current_line == line {
                line_start = idx + 1;
                break;
            }
        }
    }

    // Handle case where target line is beyond content
    if current_line < line {
        // Check if we're at the last line (no trailing newline)
        if line == current_line + 1 && !content.ends_with('\n') {
            // Position on the empty line after content
            return Some(ByteOffset(content.len() as u32));
        }
        return None;
    }

    // Find the byte offset at the character position
    // LSP uses UTF-16 code units, but for ASCII/simple YAML this is ~equal to chars
    let line_content = &content[line_start..];
    let mut char_count = 0u32;
    let mut byte_offset = line_start;

    for (idx, ch) in line_content.char_indices() {
        if char_count == character {
            return Some(ByteOffset((line_start + idx) as u32));
        }
        if ch == '\n' {
            // End of line reached before character position
            // Return position at end of line (before newline)
            return Some(ByteOffset((line_start + idx) as u32));
        }
        // Count UTF-16 code units (surrogate pairs count as 2)
        char_count += ch.len_utf16() as u32;
        byte_offset = line_start + idx + ch.len_utf8();
    }

    // Character position is at or beyond end of line content
    Some(ByteOffset(byte_offset as u32))
}

/// Convert a ByteOffset to an LSP Position (0-indexed line:col).
///
/// # Arguments
///
/// * `content` - The full document content
/// * `offset` - Byte offset in the content
///
/// # Returns
///
/// An LSP Position with 0-indexed line and character (UTF-16 code units).
pub fn offset_to_position(content: &str, offset: ByteOffset) -> Position {
    let offset = offset.as_usize().min(content.len());
    let prefix = &content[..offset];

    let line = prefix.chars().filter(|&c| c == '\n').count() as u32;

    // Find the start of the current line
    let line_start = prefix.rfind('\n').map_or(0, |pos| pos + 1);
    let line_content = &content[line_start..offset];

    // Count UTF-16 code units
    let character = line_content.chars().map(|c| c.len_utf16() as u32).sum();

    Position { line, character }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_to_offset_simple() {
        let content = "hello\nworld";

        // First line
        assert_eq!(position_to_offset(content, 0, 0), Some(ByteOffset(0))); // 'h'
        assert_eq!(position_to_offset(content, 0, 4), Some(ByteOffset(4))); // 'o'
        assert_eq!(position_to_offset(content, 0, 5), Some(ByteOffset(5))); // '\n'

        // Second line
        assert_eq!(position_to_offset(content, 1, 0), Some(ByteOffset(6))); // 'w'
        assert_eq!(position_to_offset(content, 1, 4), Some(ByteOffset(10))); // 'd'
    }

    #[test]
    fn test_position_to_offset_yaml() {
        let content = "schema: nika/workflow@0.9\ntasks:\n  - id: step1\n    infer: \"Hello\"";

        // Line 0: "schema: nika/workflow@0.9"
        assert_eq!(position_to_offset(content, 0, 0), Some(ByteOffset(0)));
        assert_eq!(position_to_offset(content, 0, 7), Some(ByteOffset(7))); // ' ' after ':'

        // Line 2: "  - id: step1"
        assert_eq!(position_to_offset(content, 2, 0), Some(ByteOffset(33))); // first space
        assert_eq!(position_to_offset(content, 2, 4), Some(ByteOffset(37))); // 'i' in id
    }

    #[test]
    fn test_offset_to_position_simple() {
        let content = "hello\nworld";

        assert_eq!(
            offset_to_position(content, ByteOffset(0)),
            Position {
                line: 0,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(content, ByteOffset(4)),
            Position {
                line: 0,
                character: 4
            }
        );
        assert_eq!(
            offset_to_position(content, ByteOffset(6)),
            Position {
                line: 1,
                character: 0
            }
        );
        assert_eq!(
            offset_to_position(content, ByteOffset(10)),
            Position {
                line: 1,
                character: 4
            }
        );
    }

    #[test]
    fn test_roundtrip() {
        let content = "schema: nika/workflow@0.9\ntasks:\n  - id: step1";

        for line in 0..3 {
            let line_content = content.lines().nth(line as usize).unwrap_or("");
            for col in 0..line_content.len() as u32 {
                let offset = position_to_offset(content, line, col).unwrap();
                let pos = offset_to_position(content, offset);
                assert_eq!(pos.line, line, "line mismatch at {}:{}", line, col);
                assert_eq!(pos.character, col, "col mismatch at {}:{}", line, col);
            }
        }
    }

    #[test]
    fn test_empty_content() {
        let content = "";

        assert_eq!(position_to_offset(content, 0, 0), Some(ByteOffset(0)));
        assert_eq!(position_to_offset(content, 0, 1), None);
        assert_eq!(position_to_offset(content, 1, 0), None);
    }

    #[test]
    fn test_position_beyond_line_end() {
        let content = "short\nline";

        // Position beyond end of line should clamp to end
        let offset = position_to_offset(content, 0, 100);
        assert!(offset.is_some());
        assert!(offset.unwrap().as_usize() <= content.len());
    }

    #[test]
    fn test_utf8_handling() {
        // Test with multi-byte UTF-8 characters
        let content = "héllo"; // é is 2 bytes in UTF-8

        // LSP uses UTF-16, where é is 1 code unit
        let offset = position_to_offset(content, 0, 1); // position after 'h'
        assert_eq!(offset, Some(ByteOffset(1))); // 'h' is 1 byte

        let offset = position_to_offset(content, 0, 2); // position after 'é'
        assert_eq!(offset, Some(ByteOffset(3))); // 'é' is 2 bytes in UTF-8
    }
}
