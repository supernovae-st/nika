//! Document state management for LSP.
//!
//! Tracks open documents and their content with efficient incremental updates.

use ropey::Rope;

/// State of an open document in the editor.
#[derive(Debug, Clone)]
pub struct DocumentState {
    /// Document content as a rope (efficient for large texts and incremental edits)
    rope: Rope,
    /// Document version (incremented on each change)
    pub version: i32,
    /// Last successfully parsed raw content (fallback when current parse fails).
    /// Kept as a String snapshot for simplicity — the AST can be re-parsed from this.
    last_valid_content: Option<String>,
}

impl DocumentState {
    /// Create a new document state from initial content.
    pub fn new(content: String, version: i32) -> Self {
        Self {
            rope: Rope::from_str(&content),
            version,
            last_valid_content: None,
        }
    }

    /// Record a successful parse — saves the current content as fallback.
    pub fn mark_valid(&mut self) {
        self.last_valid_content = Some(self.rope.to_string());
    }

    /// Get the full document content as a string.
    pub fn content(&self) -> String {
        self.rope.to_string()
    }

    /// Apply an incremental change from the LSP protocol.
    pub fn apply_change(
        &mut self,
        change: &tower_lsp_server::ls_types::TextDocumentContentChangeEvent,
    ) {
        if let Some(range) = change.range {
            // Incremental change
            let start_idx = self.position_to_char_idx(range.start);
            let end_idx = self.position_to_char_idx(range.end);

            // Remove old text
            if start_idx < end_idx && end_idx <= self.rope.len_chars() {
                self.rope.remove(start_idx..end_idx);
            }

            // Insert new text
            if start_idx <= self.rope.len_chars() {
                self.rope.insert(start_idx, &change.text);
            }
        } else {
            // Full document replace
            self.rope = Rope::from_str(&change.text);
        }
    }

    /// Convert LSP Position to character index.
    fn position_to_char_idx(&self, position: tower_lsp_server::ls_types::Position) -> usize {
        let line = position.line as usize;
        let col = position.character as usize;

        if line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }

        let line_start = self.rope.line_to_char(line);
        let line_len = self.rope.line(line).len_chars();

        line_start + col.min(line_len)
    }

    /// Convert byte offset to LSP Position.
    pub fn byte_offset_to_position(
        &self,
        byte_offset: usize,
    ) -> tower_lsp_server::ls_types::Position {
        // Convert byte offset to char index
        let char_idx = self
            .rope
            .byte_to_char(byte_offset.min(self.rope.len_bytes()));
        self.char_idx_to_position(char_idx)
    }

    /// Convert character index to LSP Position.
    pub fn char_idx_to_position(&self, char_idx: usize) -> tower_lsp_server::ls_types::Position {
        let char_idx = char_idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(char_idx);
        let line_start = self.rope.line_to_char(line);
        let col = char_idx - line_start;

        tower_lsp_server::ls_types::Position {
            line: line as u32,
            character: col as u32,
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp_server::ls_types::{Position, Range, TextDocumentContentChangeEvent};

    #[test]
    fn test_document_creation() {
        let doc = DocumentState::new("hello\nworld".to_string(), 1);
        assert_eq!(doc.content(), "hello\nworld");
        assert_eq!(doc.version, 1);

    }

    #[test]
    fn test_incremental_change() {
        let mut doc = DocumentState::new("hello world".to_string(), 1);

        // Replace "world" with "rust"
        doc.apply_change(&TextDocumentContentChangeEvent {
            range: Some(Range {
                start: Position {
                    line: 0,
                    character: 6,
                },
                end: Position {
                    line: 0,
                    character: 11,
                },
            }),
            range_length: None,
            text: "rust".to_string(),
        });

        assert_eq!(doc.content(), "hello rust");
    }

    #[test]
    fn test_full_replace() {
        let mut doc = DocumentState::new("old content".to_string(), 1);

        doc.apply_change(&TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "new content".to_string(),
        });

        assert_eq!(doc.content(), "new content");
    }

    #[test]
    fn test_position_conversion() {
        let doc = DocumentState::new("line1\nline2\nline3".to_string(), 1);

        // Start of line 2 (0-indexed)
        let pos = doc.byte_offset_to_position(6);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // Middle of line 2
        let pos = doc.byte_offset_to_position(9);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 3);
    }
}
