// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Selection Model for Text Editor
//!
//! Provides VS Code-like text selection support with:
//! - Anchor/head selection model (anchor = start, head = cursor)
//! - Character, word, and line selection modes
//! - Selection expansion (Shift+Arrow, Shift+Ctrl+Arrow)
//! - Multi-cursor foundation
//!
//! # Architecture
//!
//! ```text
//! Selection:
//! ├── anchor: Position  (where selection started)
//! ├── head: Position    (where cursor is now)
//! └── mode: SelectionMode (char/word/line)
//!
//! When anchor == head, there is no selection (just a cursor).
//! When anchor != head, the selection spans from anchor to head.
//! ```

use std::cmp::{max, min, Ordering};

/// A position in a text buffer (line, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Position {
    /// Line number (0-indexed)
    pub line: usize,
    /// Column offset (0-indexed, in grapheme clusters)
    pub col: usize,
}

impl Position {
    /// Create a new position
    pub fn new(line: usize, col: usize) -> Self {
        Self { line, col }
    }

    /// Create a position at the start of the document
    pub fn origin() -> Self {
        Self { line: 0, col: 0 }
    }
}

impl Ord for Position {
    fn cmp(&self, other: &Self) -> Ordering {
        match self.line.cmp(&other.line) {
            Ordering::Equal => self.col.cmp(&other.col),
            ord => ord,
        }
    }
}

impl PartialOrd for Position {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Selection mode determines expansion behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Character-by-character selection (default)
    #[default]
    Character,
    /// Word-based selection (double-click or Ctrl+Shift+Arrow)
    Word,
    /// Line-based selection (triple-click or line number click)
    Line,
}

/// A text selection with anchor and head positions
///
/// The anchor is where the selection started (immobile).
/// The head is where the cursor is now (moves with cursor).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    /// Starting position of selection (immobile)
    pub anchor: Position,
    /// Current cursor position (mobile)
    pub head: Position,
    /// Selection mode
    pub mode: SelectionMode,
}

impl Selection {
    /// Create a new selection at a position (no selection, just cursor)
    pub fn new(pos: Position) -> Self {
        Self {
            anchor: pos,
            head: pos,
            mode: SelectionMode::Character,
        }
    }

    /// Create a selection at origin (0, 0)
    pub fn origin() -> Self {
        Self::new(Position::origin())
    }

    /// Create a selection spanning from anchor to head
    pub fn spanning(anchor: Position, head: Position) -> Self {
        Self {
            anchor,
            head,
            mode: SelectionMode::Character,
        }
    }

    /// Check if there is an active selection (anchor != head)
    pub fn is_active(&self) -> bool {
        self.anchor != self.head
    }

    /// Get the start of the selection (min of anchor/head)
    pub fn start(&self) -> Position {
        min(self.anchor, self.head)
    }

    /// Get the end of the selection (max of anchor/head)
    pub fn end(&self) -> Position {
        max(self.anchor, self.head)
    }

    /// Get the cursor position (same as head)
    pub fn cursor(&self) -> Position {
        self.head
    }

    /// Move cursor without extending selection
    pub fn move_to(&mut self, pos: Position) {
        self.anchor = pos;
        self.head = pos;
        self.mode = SelectionMode::Character;
    }

    /// Extend selection to a new position (keeps anchor, moves head)
    pub fn extend_to(&mut self, pos: Position) {
        self.head = pos;
    }

    /// Collapse selection to cursor position
    pub fn collapse(&mut self) {
        self.anchor = self.head;
        self.mode = SelectionMode::Character;
    }

    /// Collapse selection to start
    pub fn collapse_to_start(&mut self) {
        let start = self.start();
        self.anchor = start;
        self.head = start;
    }

    /// Collapse selection to end
    pub fn collapse_to_end(&mut self) {
        let end = self.end();
        self.anchor = end;
        self.head = end;
    }

    /// Select a range (sets both anchor and head)
    pub fn select_range(&mut self, start: Position, end: Position) {
        self.anchor = start;
        self.head = end;
    }

    /// Check if a position is within the selection
    pub fn contains(&self, pos: Position) -> bool {
        if !self.is_active() {
            return false;
        }
        let start = self.start();
        let end = self.end();
        pos >= start && pos < end
    }

    /// Check if a line has any selected content
    pub fn contains_line(&self, line: usize) -> bool {
        if !self.is_active() {
            return false;
        }
        let start = self.start();
        let end = self.end();
        line >= start.line && line <= end.line
    }

    /// Get selected range on a specific line
    /// Returns (start_col, end_col) or None if line is not selected
    pub fn line_range(&self, line: usize) -> Option<(usize, Option<usize>)> {
        if !self.contains_line(line) {
            return None;
        }

        let start = self.start();
        let end = self.end();

        let start_col = if line == start.line { start.col } else { 0 };
        let end_col = if line == end.line {
            Some(end.col)
        } else {
            None // Extends to end of line
        };

        Some((start_col, end_col))
    }

    /// Calculate number of lines in selection
    pub fn line_count(&self) -> usize {
        if !self.is_active() {
            return 0;
        }
        self.end().line - self.start().line + 1
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Multi-Cursor Support
// ═══════════════════════════════════════════════════════════════════════════════

/// Collection of selections for multi-cursor support
#[derive(Debug, Clone, Default)]
pub struct SelectionSet {
    /// Primary selection (the main cursor)
    primary: Selection,
    /// Additional selections (for multi-cursor)
    additional: Vec<Selection>,
}

impl SelectionSet {
    /// Create a new selection set with a single cursor
    pub fn new(pos: Position) -> Self {
        Self {
            primary: Selection::new(pos),
            additional: Vec::new(),
        }
    }

    /// Create at origin
    pub fn origin() -> Self {
        Self::new(Position::origin())
    }

    /// Get the primary selection
    pub fn primary(&self) -> &Selection {
        &self.primary
    }

    /// Get mutable primary selection
    pub fn primary_mut(&mut self) -> &mut Selection {
        &mut self.primary
    }

    /// Get cursor position (primary selection head)
    pub fn cursor(&self) -> Position {
        self.primary.head
    }

    /// Move all cursors
    pub fn move_all_to(&mut self, pos: Position) {
        self.primary.move_to(pos);
        self.additional.clear();
    }

    /// Add a new selection at position (cursor only, no selection)
    pub fn add_selection(&mut self, pos: Position) {
        self.additional.push(Selection::new(pos));
    }

    /// Add a new selection spanning from anchor to head
    pub fn add_selection_spanning(&mut self, anchor: Position, head: Position) {
        self.additional.push(Selection::spanning(anchor, head));
    }

    /// Clear all additional selections, keep primary
    pub fn clear_additional(&mut self) {
        self.additional.clear();
    }

    /// Get all selections (primary + additional)
    pub fn all(&self) -> impl Iterator<Item = &Selection> {
        std::iter::once(&self.primary).chain(self.additional.iter())
    }

    /// Get all selections mutably
    pub fn all_mut(&mut self) -> impl Iterator<Item = &mut Selection> {
        std::iter::once(&mut self.primary).chain(self.additional.iter_mut())
    }

    /// Check if any selection is active
    pub fn has_active_selection(&self) -> bool {
        self.primary.is_active() || self.additional.iter().any(|s| s.is_active())
    }

    /// Number of cursors
    pub fn cursor_count(&self) -> usize {
        1 + self.additional.len()
    }

    /// Check if multi-cursor mode
    pub fn is_multi_cursor(&self) -> bool {
        !self.additional.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_position_ordering() {
        let p1 = Position::new(0, 5);
        let p2 = Position::new(0, 10);
        let p3 = Position::new(1, 0);

        assert!(p1 < p2);
        assert!(p2 < p3);
        assert!(p1 < p3);
    }

    #[test]
    fn test_selection_new_no_selection() {
        let sel = Selection::new(Position::new(5, 10));
        assert!(!sel.is_active());
        assert_eq!(sel.cursor(), Position::new(5, 10));
    }

    #[test]
    fn test_selection_spanning() {
        let sel = Selection::spanning(Position::new(1, 5), Position::new(3, 10));
        assert!(sel.is_active());
        assert_eq!(sel.start(), Position::new(1, 5));
        assert_eq!(sel.end(), Position::new(3, 10));
    }

    #[test]
    fn test_selection_reversed() {
        // Selection where head is before anchor
        let sel = Selection::spanning(Position::new(3, 10), Position::new(1, 5));
        assert!(sel.is_active());
        assert_eq!(sel.start(), Position::new(1, 5));
        assert_eq!(sel.end(), Position::new(3, 10));
    }

    #[test]
    fn test_selection_extend() {
        let mut sel = Selection::new(Position::new(1, 0));
        sel.extend_to(Position::new(1, 10));

        assert!(sel.is_active());
        assert_eq!(sel.anchor, Position::new(1, 0));
        assert_eq!(sel.head, Position::new(1, 10));
    }

    #[test]
    fn test_selection_collapse() {
        let mut sel = Selection::spanning(Position::new(1, 0), Position::new(3, 10));
        sel.collapse();

        assert!(!sel.is_active());
        assert_eq!(sel.anchor, Position::new(3, 10));
        assert_eq!(sel.head, Position::new(3, 10));
    }

    #[test]
    fn test_selection_contains() {
        let sel = Selection::spanning(Position::new(1, 5), Position::new(3, 10));

        assert!(sel.contains(Position::new(2, 0)));
        assert!(sel.contains(Position::new(1, 5)));
        assert!(!sel.contains(Position::new(3, 10))); // End is exclusive
        assert!(!sel.contains(Position::new(0, 0)));
    }

    #[test]
    fn test_selection_contains_line() {
        let sel = Selection::spanning(Position::new(1, 5), Position::new(3, 10));

        assert!(!sel.contains_line(0));
        assert!(sel.contains_line(1));
        assert!(sel.contains_line(2));
        assert!(sel.contains_line(3));
        assert!(!sel.contains_line(4));
    }

    #[test]
    fn test_selection_line_range() {
        let sel = Selection::spanning(Position::new(1, 5), Position::new(3, 10));

        assert_eq!(sel.line_range(0), None);
        assert_eq!(sel.line_range(1), Some((5, None)));
        assert_eq!(sel.line_range(2), Some((0, None)));
        assert_eq!(sel.line_range(3), Some((0, Some(10))));
        assert_eq!(sel.line_range(4), None);
    }

    #[test]
    fn test_selection_line_count() {
        let sel1 = Selection::new(Position::new(1, 5));
        assert_eq!(sel1.line_count(), 0);

        let sel2 = Selection::spanning(Position::new(1, 0), Position::new(1, 10));
        assert_eq!(sel2.line_count(), 1);

        let sel3 = Selection::spanning(Position::new(1, 5), Position::new(3, 10));
        assert_eq!(sel3.line_count(), 3);
    }

    #[test]
    fn test_selection_set_single_cursor() {
        let set = SelectionSet::new(Position::new(5, 10));
        assert_eq!(set.cursor_count(), 1);
        assert!(!set.is_multi_cursor());
    }

    #[test]
    fn test_selection_set_multi_cursor() {
        let mut set = SelectionSet::new(Position::new(0, 0));
        set.add_selection(Position::new(1, 0));
        set.add_selection(Position::new(2, 0));

        assert_eq!(set.cursor_count(), 3);
        assert!(set.is_multi_cursor());
    }

    #[test]
    fn test_selection_set_clear_additional() {
        let mut set = SelectionSet::new(Position::new(0, 0));
        set.add_selection(Position::new(1, 0));
        set.clear_additional();

        assert_eq!(set.cursor_count(), 1);
        assert!(!set.is_multi_cursor());
    }
}
