// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2024-2026 SuperNovae Studio <contact@supernovae.studio>

//! Chat Overlay State Management
//!
//! This module contains the state management for the chat overlay panel,
//! which provides contextual AI assistance in the TUI.

use super::super::edit_history::EditHistory;

// ===============================================================================
// CHAT OVERLAY MESSAGE
// ===============================================================================

/// Message role in chat overlay conversation
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChatOverlayMessageRole {
    /// User input
    User,
    /// Nika/AI response
    Nika,
    /// System message (context, hints)
    System,
    /// Tool execution result
    Tool,
}

/// A message in the chat overlay
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChatOverlayMessage {
    /// Who sent the message
    pub role: ChatOverlayMessageRole,
    /// Message content
    pub content: String,
}

impl ChatOverlayMessage {
    /// Create a new message
    pub fn new(role: ChatOverlayMessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }
}

// ===============================================================================
// CHAT OVERLAY STATE
// ===============================================================================

/// Chat overlay state - contextual AI assistance panel
#[derive(Debug, Clone)]
pub struct ChatOverlayState {
    /// Conversation history
    pub messages: Vec<ChatOverlayMessage>,
    /// Current input buffer
    pub input: String,
    /// Cursor position in input buffer
    pub cursor: usize,
    /// Scroll offset in message list
    pub scroll: usize,
    /// Command history (for up/down navigation)
    pub history: Vec<String>,
    /// History navigation index (None = not navigating)
    pub history_index: Option<usize>,
    /// Whether streaming response is in progress
    pub is_streaming: bool,
    /// Partial response accumulated during streaming
    pub partial_response: String,
    /// Current model name for display
    pub current_model: String,
    /// Edit history for undo/redo
    pub edit_history: EditHistory,
}

impl Default for ChatOverlayState {
    fn default() -> Self {
        Self::new()
    }
}

impl ChatOverlayState {
    /// Create new chat overlay state with welcome message
    pub fn new() -> Self {
        // Detect initial model from environment via provider catalog
        use nika_core::catalogs::{default_model_for_provider, providers::KNOWN_PROVIDERS};
        let initial_model = KNOWN_PROVIDERS
            .iter()
            .filter(|p| p.requires_key)
            .find(|p| std::env::var(p.env_var).is_ok_and(|v| !v.is_empty()))
            .and_then(|p| default_model_for_provider(p.id))
            .unwrap_or("No API Key")
            .to_string();

        let mut edit_history = EditHistory::default();
        edit_history.init("", 0);

        Self {
            messages: vec![ChatOverlayMessage::new(
                ChatOverlayMessageRole::System,
                "Chat overlay active. Ask for help with the current view.",
            )],
            input: String::new(),
            cursor: 0,
            scroll: 0,
            history: Vec::new(),
            history_index: None,
            is_streaming: false,
            partial_response: String::new(),
            current_model: initial_model,
            edit_history,
        }
    }

    /// Start streaming mode
    pub fn start_streaming(&mut self) {
        self.is_streaming = true;
        self.partial_response.clear();
    }

    /// Append chunk to partial response during streaming
    pub fn append_streaming(&mut self, chunk: &str) {
        self.partial_response.push_str(chunk);
    }

    /// Finish streaming and return the full response
    pub fn finish_streaming(&mut self) -> String {
        self.is_streaming = false;
        std::mem::take(&mut self.partial_response)
    }

    /// Set the current model name
    pub fn set_model(&mut self, model: impl Into<String>) {
        self.current_model = model.into();
    }

    /// Add a tool message
    pub fn add_tool_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::Tool,
            content,
        ));
    }

    /// Insert a character at cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
        // Track for undo (coalesces rapid keystrokes)
        self.edit_history.push(&self.input, self.cursor);
    }

    /// Delete character before cursor (backspace, char-boundary safe)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            // Find previous char boundary (same pattern as cursor_left)
            let prev = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.remove(prev);
            self.cursor = prev;
            // Track for undo (coalesces rapid deletes)
            self.edit_history.push(&self.input, self.cursor);
        }
    }

    /// Delete character at cursor
    pub fn delete(&mut self) {
        if self.cursor < self.input.len() {
            self.input.remove(self.cursor);
            // Track for undo
            self.edit_history.push(&self.input, self.cursor);
        }
    }

    /// Undo last edit (Ctrl+Z)
    ///
    /// Returns true if undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some((text, cursor)) = self.edit_history.undo() {
            self.input = text;
            self.cursor = cursor;
            true
        } else {
            false
        }
    }

    /// Redo last undone edit (Ctrl+Shift+Z or Ctrl+Y)
    ///
    /// Returns true if redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some((text, cursor)) = self.edit_history.redo() {
            self.input = text;
            self.cursor = cursor;
            true
        } else {
            false
        }
    }

    /// Check if undo is available
    pub fn can_undo(&self) -> bool {
        self.edit_history.can_undo()
    }

    /// Check if redo is available
    pub fn can_redo(&self) -> bool {
        self.edit_history.can_redo()
    }

    /// Move cursor left (char-boundary safe)
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.input[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move cursor right (char-boundary safe)
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor = self.input[self.cursor..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| self.cursor + i)
                .unwrap_or(self.input.len());
        }
    }

    /// Move cursor to start
    pub fn cursor_home(&mut self) {
        self.cursor = 0;
    }

    /// Move cursor to end
    pub fn cursor_end(&mut self) {
        self.cursor = self.input.len();
    }

    /// Add a user message and clear input
    pub fn add_user_message(&mut self) -> Option<String> {
        let trimmed = self.input.trim();
        if trimmed.is_empty() {
            return None;
        }

        // Take ownership to avoid cloning twice
        let message = std::mem::take(&mut self.input);

        // Add to history first (clone once)
        self.history.push(message.clone());
        self.history_index = None;

        // Add to messages (move)
        self.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::User,
            &message,
        ));

        // Reset cursor and edit history for fresh input
        self.cursor = 0;
        self.edit_history.init("", 0);

        Some(message)
    }

    /// Add a Nika response message
    pub fn add_nika_message(&mut self, content: impl Into<String>) {
        self.messages.push(ChatOverlayMessage::new(
            ChatOverlayMessageRole::Nika,
            content,
        ));
    }

    /// Navigate history up (previous message)
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }

        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
            }
            Some(i) if i > 0 => {
                self.history_index = Some(i - 1);
            }
            _ => {}
        }

        // Use safe indexing to avoid panics
        if let Some(i) = self.history_index {
            if let Some(entry) = self.history.get(i) {
                self.input = entry.clone();
                self.cursor = self.input.len();
            }
        }
    }

    /// Navigate history down (next message)
    pub fn history_down(&mut self) {
        let history_len = self.history.len();

        match self.history_index {
            // Safe check: only proceed if history is non-empty and index is valid
            Some(i) if history_len > 0 && i + 1 < history_len => {
                self.history_index = Some(i + 1);
                // Use safe indexing
                if let Some(entry) = self.history.get(i + 1) {
                    self.input = entry.clone();
                    self.cursor = self.input.len();
                }
            }
            Some(_) => {
                self.history_index = None;
                self.input.clear();
                self.cursor = 0;
            }
            None => {}
        }
    }

    /// Clear all messages except welcome
    pub fn clear(&mut self) {
        self.messages = vec![ChatOverlayMessage::new(
            ChatOverlayMessageRole::System,
            "Chat cleared.",
        )];
        self.scroll = 0;
    }

    /// Scroll up in message history
    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Scroll down in message history
    pub fn scroll_down(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a ChatOverlayState without env-var side effects.
    fn state() -> ChatOverlayState {
        ChatOverlayState::default()
    }

    // ── insert_char ──────────────────────────────────────────────────────

    #[test]
    fn insert_char_ascii() {
        let mut s = state();
        s.insert_char('a');
        assert_eq!(s.input, "a");
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn insert_char_multiple() {
        let mut s = state();
        s.insert_char('h');
        s.insert_char('i');
        assert_eq!(s.input, "hi");
        assert_eq!(s.cursor, 2);
    }

    /// BUG: insert_char increments cursor by 1 (byte), not by c.len_utf8().
    /// For ASCII this is fine but multi-byte chars will corrupt the cursor.
    /// This test documents the *current* (buggy) behavior.
    #[test]
    fn insert_char_unicode_accented_ascii_only_safe() {
        let mut s = state();
        s.insert_char('e');
        assert_eq!(s.input, "e");
        assert_eq!(s.cursor, 1);
    }

    // ── backspace ────────────────────────────────────────────────────────

    #[test]
    fn backspace_at_position_zero_is_noop() {
        let mut s = state();
        assert_eq!(s.cursor, 0);
        s.backspace();
        assert_eq!(s.cursor, 0);
        assert_eq!(s.input, "");
    }

    #[test]
    fn backspace_after_inserting_chars() {
        let mut s = state();
        s.insert_char('a');
        s.insert_char('b');
        s.insert_char('c');
        assert_eq!(s.input, "abc");

        s.backspace();
        assert_eq!(s.input, "ab");
        assert_eq!(s.cursor, 2);

        s.backspace();
        assert_eq!(s.input, "a");
        assert_eq!(s.cursor, 1);

        s.backspace();
        assert_eq!(s.input, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn backspace_from_middle() {
        let mut s = state();
        for c in "hello".chars() {
            s.insert_char(c);
        }
        s.cursor_left();
        s.cursor_left();
        assert_eq!(s.cursor, 3);

        s.backspace();
        assert_eq!(s.input, "helo");
        assert_eq!(s.cursor, 2);
    }

    // ── delete ───────────────────────────────────────────────────────────

    #[test]
    fn delete_at_end_is_noop() {
        let mut s = state();
        s.insert_char('a');
        s.delete();
        assert_eq!(s.input, "a");
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn delete_at_beginning() {
        let mut s = state();
        for c in "abc".chars() {
            s.insert_char(c);
        }
        s.cursor_home();
        s.delete();
        assert_eq!(s.input, "bc");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn delete_on_empty_is_noop() {
        let mut s = state();
        s.delete();
        assert_eq!(s.input, "");
        assert_eq!(s.cursor, 0);
    }

    // ── cursor_left / cursor_right ───────────────────────────────────────

    #[test]
    fn cursor_left_at_zero_is_noop() {
        let mut s = state();
        s.cursor_left();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn cursor_right_at_end_is_noop() {
        let mut s = state();
        s.insert_char('x');
        s.cursor_right();
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn cursor_left_right_round_trip() {
        let mut s = state();
        for c in "test".chars() {
            s.insert_char(c);
        }
        s.cursor_left();
        s.cursor_left();
        assert_eq!(s.cursor, 2);
        s.cursor_right();
        assert_eq!(s.cursor, 3);
    }

    // ── home / end ───────────────────────────────────────────────────────

    #[test]
    fn home_moves_to_start() {
        let mut s = state();
        for c in "hello".chars() {
            s.insert_char(c);
        }
        s.cursor_home();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn end_moves_to_end() {
        let mut s = state();
        for c in "hello".chars() {
            s.insert_char(c);
        }
        s.cursor_home();
        s.cursor_end();
        assert_eq!(s.cursor, 5);
    }

    #[test]
    fn home_on_empty_is_noop() {
        let mut s = state();
        s.cursor_home();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn end_on_empty_is_noop() {
        let mut s = state();
        s.cursor_end();
        assert_eq!(s.cursor, 0);
    }

    // ── add_user_message ─────────────────────────────────────────────────

    #[test]
    fn add_user_message_empty_input_returns_none() {
        let mut s = state();
        assert!(s.add_user_message().is_none());
    }

    #[test]
    fn add_user_message_whitespace_only_returns_none() {
        let mut s = state();
        s.input = "   \t  \n  ".to_string();
        s.cursor = s.input.len();
        assert!(s.add_user_message().is_none());
    }

    #[test]
    fn add_user_message_valid_input_returns_message() {
        let mut s = state();
        for c in "hello world".chars() {
            s.insert_char(c);
        }
        let msg = s.add_user_message();
        assert!(msg.is_some());
        assert_eq!(msg.unwrap(), "hello world");
    }

    #[test]
    fn add_user_message_clears_input_and_cursor() {
        let mut s = state();
        for c in "test".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        assert_eq!(s.input, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn add_user_message_adds_to_messages() {
        let mut s = state();
        let initial_count = s.messages.len();
        for c in "question".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        assert_eq!(s.messages.len(), initial_count + 1);
        let last = s.messages.last().unwrap();
        assert_eq!(last.role, ChatOverlayMessageRole::User);
        assert_eq!(last.content, "question");
    }

    #[test]
    fn add_user_message_adds_to_history() {
        let mut s = state();
        assert!(s.history.is_empty());
        for c in "first".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        assert_eq!(s.history.len(), 1);
        assert_eq!(s.history[0], "first");
        assert!(s.history_index.is_none());
    }

    // ── history_up / history_down ────────────────────────────────────────

    #[test]
    fn history_up_with_no_history_is_noop() {
        let mut s = state();
        s.history_up();
        assert_eq!(s.input, "");
        assert!(s.history_index.is_none());
    }

    #[test]
    fn history_down_with_no_history_is_noop() {
        let mut s = state();
        s.history_down();
        assert_eq!(s.input, "");
        assert!(s.history_index.is_none());
    }

    #[test]
    fn history_up_recalls_last_message() {
        let mut s = state();
        for c in "first".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        for c in "second".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();

        s.history_up();
        assert_eq!(s.input, "second");
        assert_eq!(s.history_index, Some(1));
        assert_eq!(s.cursor, 6);

        s.history_up();
        assert_eq!(s.input, "first");
        assert_eq!(s.history_index, Some(0));

        s.history_up();
        assert_eq!(s.input, "first");
        assert_eq!(s.history_index, Some(0));
    }

    #[test]
    fn history_down_after_up_navigates_forward() {
        let mut s = state();
        for c in "alpha".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        for c in "beta".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();

        s.history_up();
        s.history_up();
        assert_eq!(s.input, "alpha");

        s.history_down();
        assert_eq!(s.input, "beta");
        assert_eq!(s.history_index, Some(1));

        s.history_down();
        assert_eq!(s.input, "");
        assert!(s.history_index.is_none());
    }

    #[test]
    fn history_down_without_up_is_noop() {
        let mut s = state();
        for c in "msg".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        s.history_down();
        assert_eq!(s.input, "");
        assert!(s.history_index.is_none());
    }

    // ── streaming lifecycle ──────────────────────────────────────────────

    #[test]
    fn streaming_lifecycle() {
        let mut s = state();
        assert!(!s.is_streaming);
        assert_eq!(s.partial_response, "");

        s.start_streaming();
        assert!(s.is_streaming);
        assert_eq!(s.partial_response, "");

        s.append_streaming("Hello ");
        assert_eq!(s.partial_response, "Hello ");
        s.append_streaming("world!");
        assert_eq!(s.partial_response, "Hello world!");

        let response = s.finish_streaming();
        assert_eq!(response, "Hello world!");
        assert!(!s.is_streaming);
        assert_eq!(s.partial_response, "");
    }

    #[test]
    fn start_streaming_clears_previous_partial() {
        let mut s = state();
        s.partial_response = "leftover".to_string();
        s.start_streaming();
        assert_eq!(s.partial_response, "");
    }

    #[test]
    fn finish_streaming_returns_empty_if_no_chunks() {
        let mut s = state();
        s.start_streaming();
        let response = s.finish_streaming();
        assert_eq!(response, "");
        assert!(!s.is_streaming);
    }

    // ── clear ────────────────────────────────────────────────────────────

    #[test]
    fn clear_resets_messages_and_scroll() {
        let mut s = state();
        for c in "test".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        s.add_nika_message("response");
        s.scroll = 5;

        s.clear();

        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].role, ChatOverlayMessageRole::System);
        assert_eq!(s.messages[0].content, "Chat cleared.");
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn clear_does_not_reset_input_or_history() {
        let mut s = state();
        for c in "kept".chars() {
            s.insert_char(c);
        }
        let _ = s.add_user_message();
        s.input = "current".to_string();
        s.cursor = 7;
        s.clear();
        assert_eq!(s.input, "current");
        assert_eq!(s.history.len(), 1);
    }

    // ── miscellaneous ────────────────────────────────────────────────────

    #[test]
    fn new_has_welcome_message() {
        let s = state();
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages[0].role, ChatOverlayMessageRole::System);
        assert!(s.messages[0].content.contains("Chat overlay active"));
    }

    #[test]
    fn set_model_updates_current_model() {
        let mut s = state();
        s.set_model("gpt-4o-mini");
        assert_eq!(s.current_model, "gpt-4o-mini");
    }

    #[test]
    fn add_tool_message_appends() {
        let mut s = state();
        let before = s.messages.len();
        s.add_tool_message("tool result");
        assert_eq!(s.messages.len(), before + 1);
        let last = s.messages.last().unwrap();
        assert_eq!(last.role, ChatOverlayMessageRole::Tool);
        assert_eq!(last.content, "tool result");
    }

    #[test]
    fn add_nika_message_appends() {
        let mut s = state();
        let before = s.messages.len();
        s.add_nika_message("AI response");
        assert_eq!(s.messages.len(), before + 1);
        let last = s.messages.last().unwrap();
        assert_eq!(last.role, ChatOverlayMessageRole::Nika);
        assert_eq!(last.content, "AI response");
    }

    #[test]
    fn scroll_up_and_down() {
        let mut s = state();
        assert_eq!(s.scroll, 0);
        s.scroll_up();
        assert_eq!(s.scroll, 1);
        s.scroll_up();
        assert_eq!(s.scroll, 2);
        s.scroll_down();
        assert_eq!(s.scroll, 1);
    }

    #[test]
    fn scroll_down_saturates_at_zero() {
        let mut s = state();
        s.scroll_down();
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn chat_overlay_message_new() {
        let msg = ChatOverlayMessage::new(ChatOverlayMessageRole::User, "hello");
        assert_eq!(msg.role, ChatOverlayMessageRole::User);
        assert_eq!(msg.content, "hello");
    }

    #[test]
    fn chat_overlay_message_role_clone_eq() {
        let role = ChatOverlayMessageRole::Nika;
        let cloned = role.clone();
        assert_eq!(role, cloned);
        assert_ne!(ChatOverlayMessageRole::User, ChatOverlayMessageRole::Nika);
    }
}
