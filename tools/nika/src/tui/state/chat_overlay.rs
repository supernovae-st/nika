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
    /// Edit history for undo/redo (v0.8)
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
        // Detect initial model from environment
        let initial_model = if std::env::var("ANTHROPIC_API_KEY").is_ok() {
            "claude-sonnet-4".to_string()
        } else if std::env::var("OPENAI_API_KEY").is_ok() {
            "gpt-4o".to_string()
        } else {
            "No API Key".to_string()
        };

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
        self.cursor += 1;
        // Track for undo (coalesces rapid keystrokes)
        self.edit_history.push(&self.input, self.cursor);
    }

    /// Delete character before cursor (backspace)
    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.input.remove(self.cursor);
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

    /// Move cursor left
    pub fn cursor_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    /// Move cursor right
    pub fn cursor_right(&mut self) {
        if self.cursor < self.input.len() {
            self.cursor += 1;
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
