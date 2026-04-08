// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Edit History - Undo/Redo for text input
//!
//! Provides undo/redo functionality for text editing in the TUI.
//! Coalesces rapid keystrokes into single undo steps to avoid
//! stepping through every character.
//!
//! # Usage
//!
//! ```rust,ignore
//! let mut history = EditHistory::new(100); // max 100 states
//! history.push("hello", 5);  // Save state
//! history.push("hello!", 6); // Save another state
//! let (text, cursor) = history.undo().unwrap(); // "hello", 5
//! let (text, cursor) = history.redo().unwrap(); // "hello!", 6
//! ```

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How long between keystrokes to consider them part of the same "edit group"
/// If user types rapidly, we coalesce into one undo step.
const COALESCE_TIMEOUT: Duration = Duration::from_millis(500);

/// Minimum change size to trigger a new undo checkpoint
const MIN_CHANGE_SIZE: usize = 1;

/// A snapshot of edit state (text + cursor position)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditState {
    pub text: String,
    pub cursor: usize,
    timestamp: Instant,
}

impl EditState {
    /// Create a new edit state snapshot
    pub fn new(text: String, cursor: usize) -> Self {
        Self {
            text,
            cursor,
            timestamp: Instant::now(),
        }
    }
}

/// Edit history manager supporting undo/redo
///
/// Maintains two stacks:
/// - `undo_stack`: Past states we can undo to
/// - `redo_stack`: States we undid and can redo to
///
/// Redo stack is cleared on any new edit.
#[derive(Debug, Clone)]
pub struct EditHistory {
    /// Stack of past states (undo targets)
    undo_stack: VecDeque<EditState>,
    /// Stack of undone states (redo targets)
    redo_stack: VecDeque<EditState>,
    /// Maximum number of states to keep
    max_size: usize,
    /// Current state (not in either stack)
    current: Option<EditState>,
}

impl Default for EditHistory {
    fn default() -> Self {
        Self::new(100)
    }
}

impl EditHistory {
    /// Create a new edit history with given max size
    pub fn new(max_size: usize) -> Self {
        Self {
            undo_stack: VecDeque::new(),
            redo_stack: VecDeque::new(),
            max_size,
            current: None,
        }
    }

    /// Initialize history with starting state
    pub fn init(&mut self, text: &str, cursor: usize) {
        self.current = Some(EditState::new(text.to_string(), cursor));
        self.undo_stack.clear();
        self.redo_stack.clear();
    }

    /// Push a new state onto the history
    ///
    /// This clears the redo stack (new edits invalidate redo).
    /// Coalesces rapid edits into single undo steps.
    pub fn push(&mut self, text: &str, cursor: usize) {
        let new_state = EditState::new(text.to_string(), cursor);

        // Check if we should coalesce with current state
        if let Some(ref current) = self.current {
            // If text hasn't changed, just update cursor
            if current.text == text {
                self.current = Some(new_state);
                return;
            }

            // If within coalesce timeout and small change, update in place
            let elapsed = new_state.timestamp.duration_since(current.timestamp);
            let change_size = text.len().abs_diff(current.text.len());

            if elapsed < COALESCE_TIMEOUT && change_size <= MIN_CHANGE_SIZE {
                // Coalesce: update current without pushing to undo stack
                self.current = Some(new_state);
                // Clear redo on any edit
                self.redo_stack.clear();
                return;
            }

            // Not coalescing: push current to undo stack
            self.undo_stack.push_back(current.clone());

            // Enforce max size - O(1) with VecDeque
            if self.undo_stack.len() > self.max_size {
                self.undo_stack.pop_front();
            }
        }

        // Update current state
        self.current = Some(new_state);

        // Clear redo stack on new edit
        self.redo_stack.clear();
    }

    /// Force a checkpoint regardless of coalescing rules
    ///
    /// Use this before operations like paste or clear.
    pub fn checkpoint(&mut self, text: &str, cursor: usize) {
        if let Some(ref current) = self.current {
            self.undo_stack.push_back(current.clone());
            // Enforce max size - O(1) with VecDeque
            if self.undo_stack.len() > self.max_size {
                self.undo_stack.pop_front();
            }
        }
        self.current = Some(EditState::new(text.to_string(), cursor));
        self.redo_stack.clear();
    }

    /// Undo to previous state
    ///
    /// Returns the state to restore, or None if nothing to undo.
    pub fn undo(&mut self) -> Option<(String, usize)> {
        let previous = self.undo_stack.pop_back()?;

        // Push current to redo stack
        if let Some(current) = self.current.take() {
            self.redo_stack.push_back(current);
        }

        // Make previous the current
        let result = (previous.text.clone(), previous.cursor);
        self.current = Some(previous);

        Some(result)
    }

    /// Redo to next state
    ///
    /// Returns the state to restore, or None if nothing to redo.
    pub fn redo(&mut self) -> Option<(String, usize)> {
        let next = self.redo_stack.pop_back()?;

        // Push current to undo stack
        if let Some(current) = self.current.take() {
            self.undo_stack.push_back(current);
        }

        // Make next the current
        let result = (next.text.clone(), next.cursor);
        self.current = Some(next);

        Some(result)
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Get current undo stack depth
    pub fn undo_depth(&self) -> usize {
        self.undo_stack.len()
    }

    /// Get current redo stack depth
    pub fn redo_depth(&self) -> usize {
        self.redo_stack.len()
    }

    /// Clear all history
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_edit_history_new_creates_empty() {
        let history = EditHistory::new(100);
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert_eq!(history.undo_depth(), 0);
        assert_eq!(history.redo_depth(), 0);
    }

    #[test]
    fn test_edit_history_default_has_100_max() {
        let history = EditHistory::default();
        assert_eq!(history.max_size, 100);
    }

    #[test]
    fn test_edit_history_init_sets_current() {
        let mut history = EditHistory::new(100);
        history.init("hello", 5);
        assert!(history.current.is_some());
        assert_eq!(history.current.as_ref().unwrap().text, "hello");
        assert_eq!(history.current.as_ref().unwrap().cursor, 5);
    }

    #[test]
    fn test_edit_history_init_clears_stacks() {
        let mut history = EditHistory::new(100);
        history.push("a", 1);
        history.push("ab", 2);
        history.undo();

        history.init("fresh", 5);
        assert!(!history.can_undo());
        assert!(!history.can_redo());
    }

    #[test]
    fn test_edit_history_push_enables_undo() {
        let mut history = EditHistory::new(100);
        history.init("", 0);

        // Force checkpoint to avoid coalescing
        std::thread::sleep(Duration::from_millis(600));
        history.push("hello", 5);

        assert!(history.can_undo());
    }

    #[test]
    fn test_edit_history_undo_returns_previous() {
        let mut history = EditHistory::new(100);
        history.init("", 0);
        history.checkpoint("hello", 5); // Force checkpoint

        let result = history.undo();
        assert!(result.is_some());
        let (text, cursor) = result.unwrap();
        assert_eq!(text, "");
        assert_eq!(cursor, 0);
    }

    #[test]
    fn test_edit_history_redo_after_undo() {
        let mut history = EditHistory::new(100);
        history.init("", 0);
        history.checkpoint("hello", 5);

        history.undo();
        assert!(history.can_redo());

        let result = history.redo();
        assert!(result.is_some());
        let (text, cursor) = result.unwrap();
        assert_eq!(text, "hello");
        assert_eq!(cursor, 5);
    }

    #[test]
    fn test_edit_history_new_edit_clears_redo() {
        let mut history = EditHistory::new(100);
        history.init("", 0);
        history.checkpoint("hello", 5);
        history.undo();
        assert!(history.can_redo());

        // New edit should clear redo
        history.checkpoint("world", 5);
        assert!(!history.can_redo());
    }

    #[test]
    fn test_edit_history_max_size_enforced() {
        let mut history = EditHistory::new(3);
        history.init("0", 1);
        history.checkpoint("1", 1);
        history.checkpoint("2", 1);
        history.checkpoint("3", 1);
        history.checkpoint("4", 1);

        // Should only have 3 items in undo stack
        assert_eq!(history.undo_depth(), 3);
    }

    #[test]
    fn test_edit_history_undo_empty_returns_none() {
        let mut history = EditHistory::new(100);
        assert!(history.undo().is_none());
    }

    #[test]
    fn test_edit_history_redo_empty_returns_none() {
        let mut history = EditHistory::new(100);
        assert!(history.redo().is_none());
    }

    #[test]
    fn test_edit_history_clear_removes_all() {
        let mut history = EditHistory::new(100);
        history.init("", 0);
        history.checkpoint("hello", 5);
        history.undo();

        history.clear();
        assert!(!history.can_undo());
        assert!(!history.can_redo());
        assert!(history.current.is_none());
    }

    #[test]
    fn test_edit_history_cursor_only_change_no_undo_push() {
        let mut history = EditHistory::new(100);
        history.init("hello", 0);
        history.push("hello", 3); // Same text, different cursor

        // Should not have created an undo entry
        assert!(!history.can_undo());
    }

    #[test]
    fn test_edit_history_multiple_undo_redo() {
        let mut history = EditHistory::new(100);
        history.init("a", 1);
        history.checkpoint("ab", 2);
        history.checkpoint("abc", 3);

        // Undo twice
        let (text1, _) = history.undo().unwrap();
        assert_eq!(text1, "ab");
        let (text2, _) = history.undo().unwrap();
        assert_eq!(text2, "a");

        // Redo twice
        let (text3, _) = history.redo().unwrap();
        assert_eq!(text3, "ab");
        let (text4, _) = history.redo().unwrap();
        assert_eq!(text4, "abc");
    }

    #[test]
    fn test_edit_state_new() {
        let state = EditState::new("test".to_string(), 4);
        assert_eq!(state.text, "test");
        assert_eq!(state.cursor, 4);
    }

    #[test]
    fn test_edit_state_equality() {
        let state1 = EditState::new("test".to_string(), 4);
        std::thread::sleep(Duration::from_millis(1));
        let state2 = EditState::new("test".to_string(), 4);

        // Note: timestamp differs, but we don't include it in equality
        // Actually our PartialEq derives include timestamp, so they won't be equal
        // This is fine for our use case
        assert_ne!(state1, state2); // Different timestamps
    }

    #[test]
    fn test_edit_history_checkpoint_always_pushes() {
        let mut history = EditHistory::new(100);
        history.init("a", 1);

        // Even rapid checkpoints should push
        history.checkpoint("ab", 2);
        history.checkpoint("abc", 3);
        history.checkpoint("abcd", 4);

        assert_eq!(history.undo_depth(), 3);
    }

    #[test]
    fn test_coalesce_timeout_constant() {
        assert_eq!(COALESCE_TIMEOUT, Duration::from_millis(500));
    }

    #[test]
    fn test_min_change_size_constant() {
        assert_eq!(MIN_CHANGE_SIZE, 1);
    }
}
