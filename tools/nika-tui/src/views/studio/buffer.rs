// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! TextBuffer -- simple line-based text buffer for the Studio editor.
//!
//! Provides cursor movement, editing operations (insert, delete, backspace),
//! selection management (single and multi-cursor), and scroll tracking.

use std::cell::Cell;

use crate::selection::{Position, Selection, SelectionSet};

/// Simple line-based text buffer for the editor
#[derive(Debug)]
pub struct TextBuffer {
    /// Lines of text
    pub(super) lines: Vec<String>,
    /// Cursor row (0-indexed)
    pub(super) cursor_row: usize,
    /// Cursor column (0-indexed)
    pub(super) cursor_col: usize,
    /// Scroll offset (first visible line)
    pub(super) scroll_offset: usize,
    /// Last known viewport height — Cell allows update via &self from render path
    pub(super) visible_height: Cell<usize>,
    /// Track if buffer has unsaved changes
    modified: bool,
    /// Selection state (anchor/head model)
    /// Upgraded to SelectionSet for multi-cursor support
    selections: SelectionSet,
}

impl Clone for TextBuffer {
    fn clone(&self) -> Self {
        Self {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            scroll_offset: self.scroll_offset,
            visible_height: Cell::new(self.visible_height.get()),
            modified: self.modified,
            selections: self.selections.clone(),
        }
    }
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            visible_height: Cell::new(20),
            modified: false,
            selections: SelectionSet::origin(),
        }
    }
}

impl TextBuffer {
    /// Create a new text buffer from content
    pub fn from_content(content: &str) -> Self {
        let lines: Vec<String> = if content.is_empty() {
            vec![String::new()]
        } else {
            content.lines().map(String::from).collect()
        };
        Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_offset: 0,
            visible_height: Cell::new(20),
            modified: false, // Fresh content is not modified
            selections: SelectionSet::origin(),
        }
    }

    /// Check if buffer has unsaved changes
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Mark buffer as modified (called after any edit)
    pub fn mark_modified(&mut self) {
        self.modified = true;
    }

    /// Clear modified flag (called after save)
    pub fn clear_modified(&mut self) {
        self.modified = false;
    }

    /// Update the known viewport height (called by renderer each frame via &self)
    pub fn set_visible_height(&self, height: usize) {
        if height > 0 {
            self.visible_height.set(height);
        }
    }

    /// Get all lines as a joined string
    pub fn content(&self) -> String {
        self.lines.join("\n")
    }

    /// Get lines slice
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// Get cursor position (row, col) - 0-indexed
    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    /// Move cursor up
    pub fn cursor_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.clamp_cursor_col();
            self.adjust_scroll();
        }
    }

    /// Move cursor down
    pub fn cursor_down(&mut self) {
        if self.cursor_row < self.lines.len().saturating_sub(1) {
            self.cursor_row += 1;
            self.clamp_cursor_col();
            self.adjust_scroll();
        }
    }

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
            self.adjust_scroll();
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        let line_len = self.current_line_len();
        if self.cursor_col < line_len {
            self.cursor_col += 1;
        } else if self.cursor_row < self.lines.len().saturating_sub(1) {
            self.cursor_row += 1;
            self.cursor_col = 0;
            self.adjust_scroll();
        }
    }

    /// Move cursor to beginning of line
    pub fn cursor_home(&mut self) {
        self.cursor_col = 0;
    }

    /// Move cursor to end of line (char count, not byte length)
    pub fn cursor_end(&mut self) {
        if self.cursor_row < self.lines.len() {
            self.cursor_col = self.lines[self.cursor_row].chars().count();
        }
    }

    /// Move cursor one page up
    pub fn page_up(&mut self) {
        let page = self.visible_height.get().saturating_sub(2);
        self.cursor_row = self.cursor_row.saturating_sub(page);
        self.clamp_cursor_col();
        self.adjust_scroll();
    }

    /// Move cursor one page down
    pub fn page_down(&mut self) {
        let page = self.visible_height.get().saturating_sub(2);
        self.cursor_row = (self.cursor_row + page).min(self.lines.len().saturating_sub(1));
        self.clamp_cursor_col();
        self.adjust_scroll();
    }

    /// Move cursor one word left (stop at word boundaries)
    pub fn cursor_word_left(&mut self) {
        if self.cursor_col == 0 {
            if self.cursor_row > 0 {
                self.cursor_row -= 1;
                if let Some(line) = self.lines.get(self.cursor_row) {
                    self.cursor_col = line.chars().count();
                }
            }
            return;
        }
        let Some(line) = self.lines.get(self.cursor_row) else {
            return;
        };
        let chars: Vec<char> = line.chars().collect();
        let mut col = self.cursor_col.min(chars.len());
        // Skip trailing whitespace
        while col > 0 && chars[col - 1].is_whitespace() {
            col -= 1;
        }
        // Skip word characters
        while col > 0 && !chars[col - 1].is_whitespace() && !chars[col - 1].is_ascii_punctuation() {
            col -= 1;
        }
        self.cursor_col = col;
    }

    /// Move cursor one word right (stop at word boundaries)
    pub fn cursor_word_right(&mut self) {
        let Some(line) = self.lines.get(self.cursor_row) else {
            return;
        };
        let chars: Vec<char> = line.chars().collect();
        if self.cursor_col >= chars.len() {
            if self.cursor_row + 1 < self.lines.len() {
                self.cursor_row += 1;
                self.cursor_col = 0;
            }
            return;
        }
        let mut col = self.cursor_col;
        // Skip word characters
        while col < chars.len() && !chars[col].is_whitespace() && !chars[col].is_ascii_punctuation()
        {
            col += 1;
        }
        // Skip whitespace/punctuation
        while col < chars.len() && (chars[col].is_whitespace() || chars[col].is_ascii_punctuation())
        {
            col += 1;
        }
        self.cursor_col = col;
    }

    /// Insert a character at cursor (cursor_col is char index)
    pub fn insert_char(&mut self, c: char) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let char_count = line.chars().count();
            let char_idx = self.cursor_col.min(char_count);
            let byte_idx = Self::char_to_byte(line, char_idx);
            line.insert(byte_idx, c);
            self.cursor_col = char_idx + 1;
            self.modified = true;
        }
    }

    /// Insert a newline at cursor with smart indent (preserves indentation,
    /// adds extra indent after lines ending with ':' for YAML convenience)
    pub fn insert_newline(&mut self) {
        if self.cursor_row >= self.lines.len() {
            return;
        }
        let current_line = self.lines[self.cursor_row].clone();
        let char_count = current_line.chars().count();
        let char_idx = self.cursor_col.min(char_count);
        let byte_idx = Self::char_to_byte(&current_line, char_idx);

        // Detect indent of current line (in chars for cursor_col)
        let indent: String = current_line
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        // Extra indent after lines ending with ':'
        let trimmed = current_line[..byte_idx].trim_end();
        let extra = if trimmed.ends_with(':') { "  " } else { "" };

        // Split current line at cursor (byte-safe)
        let after_cursor = &current_line[byte_idx..];
        self.lines[self.cursor_row].truncate(byte_idx);

        // Create new line with indent
        let new_line = format!("{}{}{}", indent, extra, after_cursor.trim_start());
        self.lines.insert(self.cursor_row + 1, new_line);

        self.cursor_row += 1;
        // cursor_col in chars: indent chars + extra chars
        self.cursor_col = indent.chars().count() + extra.len();
        self.modified = true;
        self.adjust_scroll();
    }

    /// Delete character before cursor (backspace, char-boundary safe)
    pub fn backspace(&mut self) {
        if self.cursor_col > 0 {
            if let Some(line) = self.lines.get_mut(self.cursor_row) {
                let char_count = line.chars().count();
                let char_idx = self.cursor_col.min(char_count);
                if char_idx > 0 {
                    let byte_idx = Self::char_to_byte(line, char_idx - 1);
                    line.remove(byte_idx);
                    self.cursor_col = char_idx - 1;
                    self.modified = true;
                }
            }
        } else if self.cursor_row > 0 {
            // Merge with previous line
            let current_line = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].chars().count();
            self.lines[self.cursor_row].push_str(&current_line);
            self.modified = true;
            self.adjust_scroll();
        }
    }

    /// Delete character at cursor (char-boundary safe)
    pub fn delete(&mut self) {
        if let Some(line) = self.lines.get_mut(self.cursor_row) {
            let char_count = line.chars().count();
            let char_idx = self.cursor_col.min(char_count);
            if char_idx < char_count {
                let byte_idx = Self::char_to_byte(line, char_idx);
                line.remove(byte_idx);
                self.modified = true;
            } else if self.cursor_row + 1 < self.lines.len() {
                // Merge with next line
                let next_line = self.lines.remove(self.cursor_row + 1);
                self.lines[self.cursor_row].push_str(&next_line);
                self.modified = true;
            }
        }
    }

    /// Get current line length in characters (not bytes)
    fn current_line_len(&self) -> usize {
        self.lines
            .get(self.cursor_row)
            .map(|l| l.chars().count())
            .unwrap_or(0)
    }

    /// Clamp cursor column to line length (in chars)
    fn clamp_cursor_col(&mut self) {
        let line_len = self.current_line_len();
        self.cursor_col = self.cursor_col.min(line_len);
    }

    /// Convert a char index to a byte offset within a string.
    /// Handles multi-byte UTF-8 correctly.
    pub(super) fn char_to_byte(s: &str, char_idx: usize) -> usize {
        s.char_indices()
            .nth(char_idx)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(s.len())
    }

    /// Adjust scroll to keep cursor visible (both up and down)
    fn adjust_scroll(&mut self) {
        let height = self.visible_height.get();
        // Scroll up: cursor above viewport
        if self.cursor_row < self.scroll_offset {
            self.scroll_offset = self.cursor_row;
        }
        // Scroll down: cursor below viewport
        if height > 0 && self.cursor_row >= self.scroll_offset + height {
            self.scroll_offset = self.cursor_row - height + 1;
        }
    }

    /// Get scroll offset
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Get current selection (primary selection from SelectionSet)
    pub fn selection(&self) -> &Selection {
        self.selections.primary()
    }

    /// Get mutable selection (primary selection from SelectionSet)
    pub fn selection_mut(&mut self) -> &mut Selection {
        self.selections.primary_mut()
    }

    /// Get all selections (for multi-cursor support)
    pub fn selections(&self) -> &SelectionSet {
        &self.selections
    }

    /// Get mutable selections (for multi-cursor support)
    pub fn selections_mut(&mut self) -> &mut SelectionSet {
        &mut self.selections
    }

    /// Check if there is an active selection (any selection is active)
    pub fn has_selection(&self) -> bool {
        self.selections.has_active_selection()
    }

    /// Check if multi-cursor mode is active
    pub fn is_multi_cursor(&self) -> bool {
        self.selections.is_multi_cursor()
    }

    /// Get number of cursors
    pub fn cursor_count(&self) -> usize {
        self.selections.cursor_count()
    }

    /// Clear all selections (collapse to primary cursor position)
    pub fn clear_selection(&mut self) {
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections.move_all_to(pos);
    }

    /// Clear additional cursors, keep primary
    pub fn clear_additional_cursors(&mut self) {
        self.selections.clear_additional();
    }

    /// Start a new selection at current cursor position
    pub fn start_selection(&mut self) {
        // Reset to single cursor mode with fresh selection
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections = SelectionSet::new(pos);
    }

    /// Extend selection to current cursor position
    pub fn extend_selection(&mut self) {
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections.primary_mut().extend_to(pos);
    }

    /// Sync selection head with cursor (call after cursor movement when extending)
    pub fn sync_selection_to_cursor(&mut self) {
        let pos = Position::new(self.cursor_row, self.cursor_col);
        self.selections.primary_mut().extend_to(pos);
    }

    /// Add a cursor at position (multi-cursor support)
    pub fn add_cursor(&mut self, line: usize, col: usize) {
        let pos = Position::new(line, col);
        self.selections.add_selection(pos);
    }

    /// Get the selected text as a string (primary selection)
    pub fn get_selected_text(&self) -> Option<String> {
        if !self.has_selection() {
            return None;
        }

        let selection = self.selections.primary();
        let start = selection.start();
        let end = selection.end();

        if start.line == end.line {
            // Single line selection
            let line = self.lines.get(start.line)?;
            let start_byte = Self::char_to_byte(line, start.col);
            let end_byte = Self::char_to_byte(line, end.col);
            Some(line[start_byte..end_byte].to_string())
        } else {
            // Multi-line selection
            let mut result = String::new();

            // First line (from start_col to end)
            if let Some(first_line) = self.lines.get(start.line) {
                let start_byte = Self::char_to_byte(first_line, start.col);
                result.push_str(&first_line[start_byte..]);
            }

            // Middle lines (full lines)
            for line_idx in (start.line + 1)..end.line {
                result.push('\n');
                if let Some(line) = self.lines.get(line_idx) {
                    result.push_str(line);
                }
            }

            // Last line (from start to end_col)
            if let Some(last_line) = self.lines.get(end.line) {
                result.push('\n');
                let end_byte = Self::char_to_byte(last_line, end.col);
                result.push_str(&last_line[..end_byte]);
            }

            Some(result)
        }
    }

    /// Delete selected text and collapse selection (primary selection)
    pub fn delete_selection(&mut self) -> bool {
        if !self.has_selection() {
            return false;
        }

        let selection = self.selections.primary();
        let start = selection.start();
        let end = selection.end();

        if start.line == end.line {
            // Single line deletion
            if let Some(line) = self.lines.get_mut(start.line) {
                let start_byte = Self::char_to_byte(line, start.col);
                let end_byte = Self::char_to_byte(line, end.col);
                line.replace_range(start_byte..end_byte, "");
            }
        } else {
            // Multi-line deletion
            // Keep start of first line + end of last line
            let first_part = self
                .lines
                .get(start.line)
                .map(|l| {
                    let byte = Self::char_to_byte(l, start.col);
                    l[..byte].to_string()
                })
                .unwrap_or_default();

            let last_part = self
                .lines
                .get(end.line)
                .map(|l| {
                    let byte = Self::char_to_byte(l, end.col);
                    l[byte..].to_string()
                })
                .unwrap_or_default();

            // Remove lines from start.line to end.line (inclusive)
            self.lines.drain(start.line..=end.line);

            // Insert merged line
            let merged = format!("{}{}", first_part, last_part);
            self.lines.insert(start.line, merged);
        }

        // Move cursor to selection start
        self.cursor_row = start.line;
        self.cursor_col = start.col;
        self.clear_selection();
        self.modified = true;

        true
    }

    /// Select all text
    pub fn select_all(&mut self) {
        let start = Position::new(0, 0);
        let last_line = self.lines.len().saturating_sub(1);
        let last_col = self
            .lines
            .get(last_line)
            .map(|l| l.chars().count())
            .unwrap_or(0);
        let end = Position::new(last_line, last_col);
        self.selections.primary_mut().select_range(start, end);
        self.cursor_row = last_line;
        self.cursor_col = last_col;
    }

    /// Select next occurrence of current selection (Ctrl+D - VS Code style)
    /// Returns true if a new occurrence was found and selected
    pub fn select_next_occurrence(&mut self) -> bool {
        // Get the current selected text
        let selected = match self.get_selected_text() {
            Some(text) if !text.is_empty() => text,
            _ => {
                // No selection - select word under cursor
                return self.select_word_under_cursor();
            }
        };

        // Get current primary selection end position
        let primary = self.selections.primary();
        let search_start_line = primary.end().line;
        let search_start_col = primary.end().col;

        // Search for next occurrence after the current selection
        for line_idx in search_start_line..self.lines.len() {
            if let Some(line) = self.lines.get(line_idx) {
                let start_col = if line_idx == search_start_line {
                    search_start_col
                } else {
                    0
                };

                // Search in this line from start_col
                if start_col < line.len() {
                    if let Some(found_col) = line[start_col..].find(&selected) {
                        let abs_col = start_col + found_col;
                        // Add new cursor at this occurrence
                        let new_anchor = Position::new(line_idx, abs_col);
                        let new_head = Position::new(line_idx, abs_col + selected.len());

                        // Add a new selection spanning the occurrence
                        let mut new_selection = Selection::new(new_anchor);
                        new_selection.extend_to(new_head);
                        self.selections.add_selection_spanning(new_anchor, new_head);

                        // Move cursor to the new selection
                        self.cursor_row = line_idx;
                        self.cursor_col = abs_col + selected.len();

                        return true;
                    }
                }
            }
        }

        // Wrap around and search from beginning
        for line_idx in 0..=search_start_line {
            if let Some(line) = self.lines.get(line_idx) {
                let end_col = if line_idx == search_start_line {
                    search_start_col.saturating_sub(selected.len())
                } else {
                    line.len()
                };

                if let Some(found_col) = line[..end_col].find(&selected) {
                    let new_anchor = Position::new(line_idx, found_col);
                    let new_head = Position::new(line_idx, found_col + selected.len());
                    self.selections.add_selection_spanning(new_anchor, new_head);
                    self.cursor_row = line_idx;
                    self.cursor_col = found_col + selected.len();
                    return true;
                }
            }
        }

        false
    }

    /// Select the word under cursor (helper for select_next_occurrence)
    fn select_word_under_cursor(&mut self) -> bool {
        if let Some(line) = self.lines.get(self.cursor_row) {
            if line.is_empty() || self.cursor_col >= line.len() {
                return false;
            }

            let chars: Vec<char> = line.chars().collect();
            let col = self.cursor_col.min(chars.len().saturating_sub(1));

            // Check if current char is a word char
            if !chars
                .get(col)
                .map(|c| c.is_alphanumeric() || *c == '_')
                .unwrap_or(false)
            {
                return false;
            }

            // Find word start
            let mut word_start = col;
            while word_start > 0 {
                if !chars
                    .get(word_start - 1)
                    .map(|c| c.is_alphanumeric() || *c == '_')
                    .unwrap_or(false)
                {
                    break;
                }
                word_start -= 1;
            }

            // Find word end
            let mut word_end = col;
            while word_end < chars.len() {
                if !chars
                    .get(word_end)
                    .map(|c| c.is_alphanumeric() || *c == '_')
                    .unwrap_or(false)
                {
                    break;
                }
                word_end += 1;
            }

            // Select the word
            let start = Position::new(self.cursor_row, word_start);
            let end = Position::new(self.cursor_row, word_end);
            self.selections.primary_mut().select_range(start, end);
            self.cursor_col = word_end;

            true
        } else {
            false
        }
    }

    /// Get cursor position as linear index (for EditHistory)
    /// Get cursor position as byte offset in the full text.
    /// cursor_col is a char index, so we convert to bytes for LSP compatibility.
    pub fn cursor_position(&self) -> usize {
        let mut pos = 0;
        for (i, line) in self.lines.iter().enumerate() {
            if i == self.cursor_row {
                return pos + Self::char_to_byte(line, self.cursor_col);
            }
            pos += line.len() + 1; // +1 for newline (always 1 byte)
        }
        pos
    }

    /// Set content from string (clears buffer and reloads)
    pub fn set_content(&mut self, content: &str) {
        *self = Self::from_content(content);
    }

    /// Set cursor to linear position (byte offset → row/col in chars)
    pub fn set_cursor_position(&mut self, pos: usize) {
        let mut remaining = pos;
        for (row, line) in self.lines.iter().enumerate() {
            let line_len = line.len() + 1; // +1 for newline
            if remaining < line_len || row + 1 >= self.lines.len() {
                self.cursor_row = row;
                // Convert byte offset within line to char count
                let byte_col = remaining.min(line.len());
                self.cursor_col = line[..byte_col].chars().count();
                self.adjust_scroll();
                return;
            }
            remaining -= line_len;
        }
        // Fallback: end of document
        self.cursor_row = self.lines.len().saturating_sub(1);
        self.cursor_col = self.lines.last().map(|l| l.chars().count()).unwrap_or(0);
        self.adjust_scroll();
    }
}
